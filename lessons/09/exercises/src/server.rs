use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::{
    messages::ClientToServerMsg, messages::ServerToClientMsg, reader::MessageReader,
    writer::MessageWriter,
};
use epoll::{Event, Events};
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

fn add_fd_to_epoll(epoll: RawFd, fd: RawFd, events: Events) -> std::io::Result<()> {
    epoll::ctl(
        epoll,
        epoll::ControlOptions::EPOLL_CTL_ADD,
        fd,
        Event::new(events, fd as u64),
    )?;
    Ok(())
}

struct SharedWriter {
    writer: Mutex<MessageWriter<ServerToClientMsg, SocketWrapper>>,
}

struct Client {
    name: Option<String>,
    address: SocketAddr,
    reader: MessageReader<ClientToServerMsg, SocketWrapper>,
    writer: MessageWriter<ServerToClientMsg, SocketWrapper>,
    last_activity: Instant,
    connected: bool,
}

pub struct RunningServer {
    pub port: u16,
    poller_thread: Option<JoinHandle<anyhow::Result<()>>>,
    terminated: Arc<AtomicBool>,
}

impl RunningServer {
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn new(max_clients: usize) -> anyhow::Result<Self> {
        let terminated = Arc::new(AtomicBool::new(false));
        let mut server = Server::new()?;
        let port = server.port;
        let terminated_clone = terminated.clone();
        let h = std::thread::spawn(move || -> anyhow::Result<()> {
            while !terminated_clone.load(Ordering::SeqCst) {
                server.process_events(100)?;
            }
            Ok(())
        });

        Ok(Self {
            port,
            poller_thread: Some(h),
            terminated,
        })
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.terminated.store(true, Ordering::SeqCst);
        if let Some(thread) = self.poller_thread.take() {
            thread.join().unwrap().unwrap();
        }
    }
}

struct Server {
    clients: HashMap<RawFd, Client>,
    client_writers: HashMap<String, MessageWriter<ServerToClientMsg, SocketWrapper>>,
    listener: TcpListener,
    epoll: RawFd,
    port: u16,
}

impl Server {
    pub fn new() -> anyhow::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let epoll = epoll::create(false)?;
        add_fd_to_epoll(epoll, listener.as_raw_fd(), Events::EPOLLIN)?;
        log::info!("Listening on port {port}");
        Ok(Self {
            clients: HashMap::new(),
            client_writers: HashMap::new(),
            listener,
            epoll,
            port,
        })
    }

    fn process_events(&mut self, timeout_ms: i32) -> anyhow::Result<()> {
        let mut events = [Event::new(Events::empty(), 0); 1024];
        let event_count = epoll::wait(self.epoll, timeout_ms, &mut events)?;
        for event in &events[..event_count] {
            let fd = event.data as RawFd;
            if fd == self.listener.as_raw_fd() {
                self.accept_and_setup_client()?;
                continue;
            }
            if let Some(client) = self.clients.get_mut(&fd) {
                if let Some(msg) = Self::try_recv_msg(client) {
                    if let Err(e) = Self::handle_client_msg(client, &mut self.client_writers, msg) {
                        log::error!("Error handling message from {}: {}", client.address, e);
                        client.connected = false;
                    }
                }
            }
        }
        self.cleanup_disconnected();
        Ok(())
    }

    fn accept_and_setup_client(&mut self) -> anyhow::Result<()> {
        let (stream, address) = match self.listener.accept() {
            Ok(ret) => ret,
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        println!("Connection from {}", address);
        stream.set_nonblocking(true)?;
        add_fd_to_epoll(self.epoll, stream.as_raw_fd(), Events::EPOLLIN)?;
        let stream = Arc::new(stream);
        self.clients.insert(
            stream.as_raw_fd(),
            Client {
                name: None,
                address,
                reader: MessageReader::<ClientToServerMsg, SocketWrapper>::new(SocketWrapper(
                    stream.clone(),
                )),
                writer: MessageWriter::<ServerToClientMsg, SocketWrapper>::new(SocketWrapper(
                    stream,
                )),
                last_activity: Instant::now(),
                connected: true,
            },
        );
        Ok(())
    }

    fn try_recv_msg(client: &mut Client) -> Option<ClientToServerMsg> {
        let msg = match client.reader.recv() {
            Some(Ok(msg)) => msg,
            Some(Err(error)) if error.kind() == ErrorKind::WouldBlock => {
                return None;
            }
            Some(Err(error)) => {
                println!("Error reading from {}: {}", client.address, error);
                client.connected = false;
                return None;
            }
            None => {
                println!("Client {} disconnected", client.address);
                client.connected = false;
                return None;
            }
        };
        client.last_activity = Instant::now();
        eprintln!("Received message from {}: {:?}", client.address, msg);
        Some(msg)
    }

    fn handle_client_msg(
        client: &mut Client,
        client_writers: &mut HashMap<String, MessageWriter<ServerToClientMsg, SocketWrapper>>,
        msg: ClientToServerMsg,
    ) -> anyhow::Result<()> {
        match client.name.clone() {
            None => Self::handle_unregistered_client_msg(client, client_writers, msg),
            Some(name) => Self::handle_registered_client(name, client, client_writers, msg),
        }
    }

    fn handle_unregistered_client_msg(
        client: &mut Client,
        client_writers: &mut HashMap<String, MessageWriter<ServerToClientMsg, SocketWrapper>>,
        msg: ClientToServerMsg,
    ) -> anyhow::Result<()> {
        match msg {
            ClientToServerMsg::Join { name } => {
                if client_writers.contains_key(&name) {
                    client.writer.send(ServerToClientMsg::Error(
                        "Username already taken".to_owned(),
                    ))?;
                }
                log::info!("User {name} joined");
                client.name = Some(name.clone());
                client_writers.insert(
                    name,
                    MessageWriter::new(SocketWrapper(client.writer.inner().0.clone())),
                );
                client.writer.send(ServerToClientMsg::Welcome)?;
                Ok(())
            }
            _ => {
                client.writer.send(ServerToClientMsg::Error(
                    "Unexpected message received".to_owned(),
                ))?;
                Err(anyhow::anyhow!("Unexpected message received"))
            }
        }
    }

    fn handle_registered_client(
        name: String,
        client: &mut Client,
        client_writers: &mut HashMap<String, MessageWriter<ServerToClientMsg, SocketWrapper>>,
        msg: ClientToServerMsg,
    ) -> anyhow::Result<()> {
        match msg {
            ClientToServerMsg::Join { .. } => {
                client.writer.send(ServerToClientMsg::Error(
                    "Unexpected message received".to_owned(),
                ))?;
                Err(anyhow::anyhow!("Unexpected message received"))
            }
            ClientToServerMsg::Ping => {
                client.writer.send(ServerToClientMsg::Pong)?;
                Ok(())
            }
            ClientToServerMsg::ListUsers => {
                let users = client_writers.keys().cloned().collect();
                client.writer.send(ServerToClientMsg::UserList { users })?;
                Ok(())
            }
            ClientToServerMsg::SendDM { to, message } => {
                if to == name {
                    client.writer.send(ServerToClientMsg::Error(
                        "Cannot send a DM to yourself".to_owned(),
                    ))?;
                } else if let Some(recipient) = client_writers.get_mut(&to) {
                    recipient.send(ServerToClientMsg::Message {
                        from: name.clone(),
                        message,
                    })?;
                } else {
                    client.writer.send(ServerToClientMsg::Error(format!(
                        "User {to} does not exist"
                    )))?;
                }
                Ok(())
            }
            ClientToServerMsg::Broadcast { message } => {
                for (recipient_name, recipient) in client_writers {
                    if recipient_name != &name {
                        recipient.send(ServerToClientMsg::Message {
                            from: name.clone(),
                            message: message.clone(),
                        })?;
                    }
                }
                Ok(())
            }
        }
    }

    fn cleanup_disconnected(&mut self) {
        self.clients.retain(|_, client| {
            if !client.connected {
                if let Some(name) = &client.name {
                    self.client_writers.remove(name);
                }
                epoll::ctl(
                    self.epoll,
                    epoll::ControlOptions::EPOLL_CTL_DEL,
                    client.reader.inner().0.as_raw_fd(),
                    Event::new(Events::EPOLLIN, client.reader.inner().0.as_raw_fd() as u64),
                )
                .unwrap();
                client
                    .reader
                    .inner()
                    .0
                    .shutdown(std::net::Shutdown::Both)
                    .unwrap();
                eprintln!("Client disconnected: {}", client.address);
            }
            client.connected
        });
    }
}

/*
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
        reader: &mut MessageReader<ClientToServerMsg, SocketWrapper>,
        writer: &mut MessageWriter<ServerToClientMsg, SocketWrapper>,
    ) -> anyhow::Result<Option<(MessageReader<ClientToServerMsg, SocketWrapper>, String)>> {
        let msg = reader.recv().expect("No initial msg received")?;
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
    */

/*
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
        let clients = self.writers.read().unwrap();
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
 */
