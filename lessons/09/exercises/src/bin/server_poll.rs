//! You can use this file for experiments.

use epoll::{ctl, Event, Events};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::fd::{AsRawFd, RawFd};
use std::time::Instant;
use std::vec;
use std::{io::ErrorKind, net::TcpListener, sync::Arc, time::Duration};
use week09::messages::ClientToServerMsg;
use week09::reader::MessageReader;

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

impl SocketWrapper {
    fn inner(&self) -> &TcpStream {
        self.0.as_ref()
    }
}

struct Client {
    reader: MessageReader<ClientToServerMsg, SocketWrapper>,
    address: SocketAddr,
    connected: bool,
    last_activity: Instant,
}

impl Client {
    fn until_timeout(&self) -> Duration {
        let since_last_activity = self.last_activity.elapsed();
        TIMEOUT_DURATION.saturating_sub(since_last_activity)
    }
}

const TIMEOUT_DURATION: Duration = Duration::from_secs(3);

fn main() -> anyhow::Result<()> {
    let stream = TcpListener::bind(("127.0.0.1", 5555))?;
    stream.set_nonblocking(true)?;
    let mut clients: Vec<Client> = vec![];

    let epoll = epoll::create(false)?;
    epoll::ctl(
        epoll,
        epoll::ControlOptions::EPOLL_CTL_ADD,
        stream.as_raw_fd(),
        Event::new(Events::EPOLLIN, stream.as_raw_fd() as u64),
    )?;
    loop {
        let mut events = [Event::new(Events::empty(), 0); 1024];
        let time_until_timeout = clients
            .iter()
            .map(|client| client.until_timeout().as_millis() as i32)
            .min()
            .unwrap_or(-1);
        let event_count = epoll::wait(epoll, time_until_timeout, &mut events)?;
        println!("Woken up");
        for event in &events[..event_count] {
            let fd = event.data as RawFd;
            if fd == stream.as_raw_fd() {
                let (client, address) = match stream.accept() {
                    Ok(ret) => ret,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                println!("Connection from {}", address);
                client.set_nonblocking(true)?;
                epoll::ctl(
                    epoll,
                    epoll::ControlOptions::EPOLL_CTL_ADD,
                    client.as_raw_fd(),
                    Event::new(Events::EPOLLIN, client.as_raw_fd() as u64),
                )?;
                let client = Arc::new(client);
                clients.push(Client {
                    reader: MessageReader::<ClientToServerMsg, SocketWrapper>::new(SocketWrapper(
                        client,
                    )),
                    address,
                    connected: true,
                    last_activity: Instant::now(),
                });
            }
            for client in &mut clients {
                if fd == client.reader.inner().inner().as_raw_fd() {
                    handle_client(client);
                }
            }
        }
        for client in &mut clients {
            if client.until_timeout() == Duration::ZERO {
                client.connected = false;
            }
        }

        clients.retain(|client| {
            if !client.connected {
                epoll::ctl(
                    epoll,
                    epoll::ControlOptions::EPOLL_CTL_DEL,
                    client.reader.inner().inner().as_raw_fd(),
                    Event::new(
                        Events::EPOLLIN,
                        client.reader.inner().inner().as_raw_fd() as u64,
                    ),
                )
                .unwrap();
                client
                    .reader
                    .inner()
                    .inner()
                    .shutdown(std::net::Shutdown::Both)
                    .unwrap();
                eprintln!("Client disconnected: {}", client.address);
            }
            client.connected
        });
    }

    Ok(())
}

fn handle_client(client: &mut Client) {
    let msg = match client.reader.recv() {
        Some(Ok(msg)) => msg,
        Some(Err(error)) if error.kind() == ErrorKind::WouldBlock => {
            return;
        }
        Some(Err(error)) => {
            println!("Error reading from {}: {}", client.address, error);
            client.connected = false;
            return;
        }
        None => {
            println!("Client {} disconnected", client.address);
            client.connected = false;
            return;
        }
    };
    client.last_activity = Instant::now();
    eprintln!("Received message from {}: {:?}", client.address, msg);
}

fn handle_client_message(msg: ClientToServerMsg) {
    match msg {
        ClientToServerMsg::Join { name } => todo!(),
        ClientToServerMsg::Ping => todo!(),
        ClientToServerMsg::ListUsers => todo!(),
        ClientToServerMsg::SendDM { to, message } => todo!(),
        ClientToServerMsg::Broadcast { message } => todo!(),
    }
}
