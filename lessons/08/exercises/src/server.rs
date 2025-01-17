use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use crate::{ClientToServerMsg, MessageReader, MessageWriter, ServerToClientMsg};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

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

struct SharedWriter {
    writer: Mutex<MessageWriter<ServerToClientMsg, SocketWrapper>>,
}

pub struct RunningServer {
    pub port: u16,
    max_clients: usize,
    terminated: Arc<AtomicBool>,
    clients: Arc<RwLock<HashMap<String, SharedWriter>>>,
    listener: TcpListener,
    acceptor_handle: Option<JoinHandle<anyhow::Result<()>>>,
    client_handles: Arc<Mutex<Vec<JoinHandle<anyhow::Result<()>>>>>,
}

impl RunningServer {
    pub fn port(&self) -> u16 {
        self.port
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
            clients: Arc::new(RwLock::new(HashMap::new())),
            listener,
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
        clients: Arc<RwLock<HashMap<String, SharedWriter>>>,
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
        clients: &Arc<RwLock<HashMap<String, SharedWriter>>>,
    ) -> anyhow::Result<Option<(MessageReader<ClientToServerMsg, SocketWrapper>, String)>> {
        let stream = Arc::new(stream);
        let mut reader = MessageReader::new(SocketWrapper(stream.clone()));
        let mut writer = MessageWriter::new(SocketWrapper(stream));

        let msg = reader.read().expect("No initial msg received")?;
        match msg {
            ClientToServerMsg::Join { name } => {
                if clients.read().unwrap().contains_key(&name) {
                    writer.write(ServerToClientMsg::Error(
                        "Username already taken".to_owned(),
                    ))?;
                    return Ok(None);
                }
                log::info!("User {name} joined");
                writer.write(ServerToClientMsg::Welcome)?;
                clients.write().unwrap().insert(
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
        clients: Arc<RwLock<HashMap<String, SharedWriter>>>,
    ) -> anyhow::Result<()> {
        for msg in reader {
            let msg = msg?;
            match msg {
                ClientToServerMsg::Join { .. } => {
                    let clients = clients.read().unwrap();
                    if let Some(client) = clients.get(&name) {
                        client
                            .writer
                            .lock()
                            .unwrap()
                            .write(ServerToClientMsg::Error(
                                "Unexpected message received".to_owned(),
                            ))?;
                    }
                    break;
                }
                ClientToServerMsg::Ping => {
                    let clients = clients.read().unwrap();
                    if let Some(client) = clients.get(&name) {
                        client
                            .writer
                            .lock()
                            .unwrap()
                            .write(ServerToClientMsg::Pong)?;
                    }
                }
                ClientToServerMsg::ListUsers => {
                    let clients = clients.read().unwrap();
                    let users = clients.keys().cloned().collect();
                    if let Some(client) = clients.get(&name) {
                        client
                            .writer
                            .lock()
                            .unwrap()
                            .write(ServerToClientMsg::UserList { users })?;
                    }
                }
                ClientToServerMsg::SendDM { to, message } => {
                    let clients = clients.read().unwrap();
                    if let Some(sender) = clients.get(&name) {
                        let mut writer = sender.writer.lock().unwrap();
                        if to == name {
                            writer.write(ServerToClientMsg::Error(
                                "Cannot send a DM to yourself".to_owned(),
                            ))?;
                        } else if let Some(recipient) = clients.get(&to) {
                            recipient
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
                }
                ClientToServerMsg::Broadcast { message } => {
                    let clients = clients.read().unwrap();
                    for (recipient_name, recipient) in clients.iter() {
                        if recipient_name != &name {
                            recipient
                                .writer
                                .lock()
                                .unwrap()
                                .write(ServerToClientMsg::Message {
                                    from: name.clone(),
                                    message: message.clone(),
                                })?;
                        }
                    }
                }
            }
        }
        // Clean up when client disconnects
        clients.write().unwrap().remove(&name);
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
        let clients = self.clients.read().unwrap();
        for (_, client) in clients.iter() {
            client
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
