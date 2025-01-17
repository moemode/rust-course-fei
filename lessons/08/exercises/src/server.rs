use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{ClientToServerMsg, MessageReader, MessageWriter, ServerToClientMsg};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

struct SharedWriter {
    writer: Mutex<MessageWriter<ServerToClientMsg, SocketWrapper>>,
}

/* pub struct Server {
    port: u16,
    max_clients: usize,
    clients: Arc<DashMap<String, Mutex<ConnectedClient>>>,
    listener: Option<Arc<TcpListener>>,
    acceptor_handle: Option<JoinHandle<anyhow::Result<()>>>,
    client_threads: Arc<Mutex<Vec<JoinHandle<anyhow::Result<()>>>>>,
    terminated: Arc<AtomicBool>,
} */

pub struct RunningServer {
    pub port: u16,
    max_clients: usize,
    clients: Arc<DashMap<String, SharedWriter>>,
    listener: TcpListener,
    acceptor_handle: Option<JoinHandle<anyhow::Result<()>>>,
    client_handles: Arc<Mutex<Vec<JoinHandle<anyhow::Result<()>>>>>,
    terminated: Arc<AtomicBool>,
}

impl RunningServer {
    pub fn port(&self) -> u16 {
        self.port
    }
}

struct SocketWrapper(Arc<TcpStream>);

impl Read for SocketWrapper {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.as_ref().read(buf)
    }
}

impl Write for SocketWrapper {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.as_ref().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.as_ref().flush()
    }
}

impl RunningServer {
    pub fn new(max_clients: usize) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        log::info!("Listening on port {port}");
        Ok(Self {
            port,
            max_clients,
            clients: Arc::new(DashMap::new()),
            listener: listener,
            acceptor_handle: None,
            client_handles: Arc::new(Mutex::new(vec![])),
            terminated: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn run(&mut self) {
        let port = self.port;
        let listener = self.listener.try_clone().unwrap();
        let clients = self.clients.clone();
        let max_clients = self.max_clients;
        let client_threads = self.client_handles.clone();
        let terminated = self.terminated.clone();

        let h = std::thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let (client, addr) = listener.accept()?;
                if terminated.load(Ordering::SeqCst) {
                    return Ok(());
                }

                let mut handles = client_threads.lock().unwrap();
                let mut i = 0;
                while i < handles.len() {
                    if handles[i].is_finished() {
                        if let Ok(Err(e)) = handles.swap_remove(i).join() {
                            log::error!("Client thread error: {}", e);
                        }
                    } else {
                        i += 1;
                    }
                }
                if handles.len() >= max_clients {
                    let mut writer = MessageWriter::new(client);
                    writer.write(ServerToClientMsg::Error("Server is full".to_owned()))?;
                    continue;
                }
                let clients = clients.clone();
                let h = std::thread::spawn(move || Self::handle_client(client, addr, clients));
                handles.push(h);
            }
        });
        self.acceptor_handle = Some(h);
    }

    fn handle_client(
        stream: std::net::TcpStream,
        addr: std::net::SocketAddr,
        clients: Arc<DashMap<String, SharedWriter>>,
    ) -> anyhow::Result<()> {
        log::info!("Client connected from {}", addr);
        if let Some((mut reader, name)) = Self::join_client(stream, &clients)? {
            // Handle messages from joined client
            Self::handle_joined(name, reader, clients)?;
        }
        log::info!("Client disconnected from {}", addr);
        Ok(())
    }

    fn join_client(
        stream: TcpStream,
        clients: &Arc<DashMap<String, SharedWriter>>,
    ) -> anyhow::Result<Option<(MessageReader<ClientToServerMsg, SocketWrapper>, String)>> {
        let stream = Arc::new(stream);
        let mut reader = MessageReader::new(SocketWrapper(stream.clone()));
        let mut writer = MessageWriter::new(SocketWrapper(stream));

        let msg = reader.read().expect("No initial msg received")?;
        match msg {
            ClientToServerMsg::Join { name } => {
                if clients.contains_key(&name) {
                    writer.write(ServerToClientMsg::Error(
                        "Username already taken".to_owned(),
                    ))?;
                    return Ok(None);
                }
                log::info!("User {name} joined");
                writer.write(ServerToClientMsg::Welcome)?;
                clients.insert(
                    name.clone(),
                    SharedWriter {
                        writer: Mutex::new(writer),
                    },
                );
                Ok(Some((reader, name)))
            }
            _ => {
                writer.write(ServerToClientMsg::Error(
                    "Unexpected message received".to_owned(),
                ))?;
                Ok(None)
            }
        }
    }

    fn handle_joined(
        name: String,
        reader: MessageReader<ClientToServerMsg, SocketWrapper>,
        clients: Arc<DashMap<String, SharedWriter>>,
    ) -> anyhow::Result<()> {
        for msg in reader {
            let msg = msg?;
            match msg {
                ClientToServerMsg::Join { .. } => {
                    clients.get(&name).unwrap().writer.lock().unwrap().write(
                        ServerToClientMsg::Error("Unexpected message received".to_owned()),
                    )?;
                    break;
                }
                ClientToServerMsg::Ping => {
                    clients
                        .get(&name)
                        .unwrap()
                        .writer
                        .lock()
                        .unwrap()
                        .write(ServerToClientMsg::Pong)?;
                }
                ClientToServerMsg::ListUsers => {
                    let users = clients.iter().map(|entry| entry.key().clone()).collect();
                    clients
                        .get(&name)
                        .unwrap()
                        .writer
                        .lock()
                        .unwrap()
                        .write(ServerToClientMsg::UserList { users })?;
                }
                ClientToServerMsg::SendDM { to, message } => {
                    let client = clients.get_mut(&name).unwrap();
                    let mut writer = client.writer.lock().unwrap();
                    if to == name {
                        writer.write(ServerToClientMsg::Error(
                            "Cannot send a DM to yourself".to_owned(),
                        ))?;
                    } else if let Some(client) = clients.get(&to) {
                        client
                            .writer
                            .lock()
                            .unwrap()
                            .write(ServerToClientMsg::Message {
                                from: name.clone(),
                                message,
                            })?;
                    } else {
                        writer.write(ServerToClientMsg::Error(format!(
                            "User {to} does not exist"
                        )))?;
                    }
                }
                ClientToServerMsg::Broadcast { message } => {
                    let client = clients.get(&name).unwrap();
                    let mut writer = client.writer.lock().unwrap();
                    for entry in clients.iter() {
                        if entry.key() != &name {
                            entry.value().writer.lock().unwrap().write(
                                ServerToClientMsg::Message {
                                    from: name.clone(),
                                    message: message.clone(),
                                },
                            )?;
                        }
                    }
                }

                _ => {}
            }
        }
        // Clean up when client disconnects
        clients.remove(&name);
        Ok(())
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        // Signal termination
        self.terminated.store(true, Ordering::SeqCst);
        TcpStream::connect(("127.0.0.1", self.port)).expect("cannot connect to server");

        if let Some(h) = self.acceptor_handle.take() {
            if let Ok(Err(e)) = h.join() {
                log::error!("Acceptor thread error: {e}");
            }
        }

        // Shutdown all client connections
        for entry in self.clients.iter() {
            entry
                .value()
                .writer
                .lock()
                .unwrap()
                .inner()
                .0
                .shutdown(std::net::Shutdown::Both)
                .ok();
        }

        let mut handles = self.client_handles.lock().unwrap();
        while let Some(h) = handles.pop() {
            if let Ok(Err(e)) = h.join() {
                log::error!("Client thread error: {e}");
            }
        }
    }
}
