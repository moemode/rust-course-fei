//! You can use this file for experiments.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
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

struct Client {
    reader: MessageReader<ClientToServerMsg, SocketWrapper>,
    address: SocketAddr,
    connected: bool,
}

fn main() -> anyhow::Result<()> {
    let stream = TcpListener::bind(("127.0.0.1", 5555))?;
    stream.set_nonblocking(true)?;
    let mut clients: Vec<Client> = vec![];
    loop {
        for client in &mut clients {
            let msg = match client.reader.recv() {
                Some(Ok(msg)) => msg,
                Some(Err(error)) if error.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                Some(Err(error)) => {
                    println!("Error reading from {}: {}", client.address, error);
                    client.connected = false;
                    continue;
                }
                None => {
                    println!("Client {} disconnected", client.address);
                    client.connected = false;
                    continue;
                }
            };
            eprintln!("Received message from {}: {:?}", client.address, msg);
        }
        clients.retain(|client| client.connected);
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
        let client = Arc::new(client);
        clients.push(Client {
            reader: MessageReader::<ClientToServerMsg, SocketWrapper>::new(SocketWrapper(
                client.clone(),
            )),
            address,
            connected: true,
        });
    }

    Ok(())
}
