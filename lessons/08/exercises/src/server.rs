use dashmap::DashMap;

use crate::{ClientToServerMsg, MessageReader, MessageWriter, ServerToClientMsg};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

struct ConnectedClient {
    writer: MessageWriter<ServerToClientMsg, SocketWrapper>,
}

pub struct Server {
    port: u16,
    max_clients: usize,
    clients: Arc<DashMap<String, Mutex<ConnectedClient>>>,
    listener: TcpListener,
    acceptor_handle: Option<JoinHandle<anyhow::Result<()>>>,
    client_threads: Arc<Mutex<Vec<JoinHandle<anyhow::Result<()>>>>>,
}

pub struct RunningServer {
    pub port: u16,
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

impl Server {
    pub fn new(max_clients: usize) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        log::info!("Listening on port {port}");
        Ok(Self {
            port,
            max_clients,
            clients: Arc::new(DashMap::new()),
            listener,
            acceptor_handle: None,
            client_threads: Arc::new(Mutex::new(vec![])),
        })
    }

    pub fn run(&mut self) -> RunningServer {
        let port = self.port;
        let listener = self.listener.try_clone().unwrap();
        let clients = self.clients.clone();
        let max_clients = self.max_clients;
        let client_handles = self.client_threads.clone();
        let h = std::thread::spawn(move || -> anyhow::Result<()> {
            loop {
                let (client, addr) = listener.accept()?;
                {
                    let mut handles = client_handles.lock().unwrap();
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
                }
                let clients = clients.clone();
                let h = std::thread::spawn(move || Self::handle_client(client, addr, clients));
                client_handles.lock().unwrap().push(h);
            }
        });
        self.acceptor_handle = Some(h);
        RunningServer { port }
    }

    fn handle_client(
        stream: std::net::TcpStream,
        addr: std::net::SocketAddr,
        clients: Arc<DashMap<String, Mutex<ConnectedClient>>>,
    ) -> anyhow::Result<()> {
        log::info!("Client connected from {}", addr);

        // Clone the stream for the writer
        let stream = Arc::new(stream);
        let mut reader = MessageReader::new(SocketWrapper(stream.clone()));
        let mut writer = MessageWriter::new(SocketWrapper(stream.clone()));

        let msg = reader.read().expect("")?;
        match msg {
            ClientToServerMsg::Join { name } => {
                if clients.contains_key(&name) {
                    writer.write(ServerToClientMsg::Error(
                        "Username already taken".to_owned(),
                    ))?;
                    return Ok(());
                }

                log::info!("{name} joined");
                writer.write(ServerToClientMsg::Welcome)?;

                // Create a new writer for the connected client
                clients.insert(name.clone(), Mutex::new(ConnectedClient { writer }));
                /*                 // Handle client messages loop
                for msg in reader {
                    let msg = msg?;
                    // TODO: Handle other message types
                } */
                // Clean up when client disconnects
                clients.remove(&name);
            }
            _ => {
                writer.write(ServerToClientMsg::Error(
                    "Unexpected message received".to_owned(),
                ))?;
            }
        }

        log::info!("Client disconnected from {}", addr);
        Ok(())
    }
}
