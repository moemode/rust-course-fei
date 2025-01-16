//! You can use this file for experiments.
//use the lib

use std::net::TcpListener;

use week08::{ClientToServerMsg, MessageReader, MessageWriter, ServerToClientMsg};

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let stream = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = stream.local_addr().unwrap().port();
    log::info!("Listening on port {port}");
    let mut clients = vec![];
    loop {
        let (mut client, addr) = stream.accept()?;
        if clients.len() > 2 {
            log::error!("Too many clients");
            continue;
        }
        let handle: std::thread::JoinHandle<Result<(), anyhow::Error>> =
            std::thread::spawn(move || -> anyhow::Result<()> {
                log::info!("Client connected from {}", addr);
                let mut reader: MessageReader<ClientToServerMsg, &std::net::TcpStream> =
                    MessageReader::new(&client);
                let mut writer = MessageWriter::new(&client);
                let msg = reader.read().expect("");
                let msg = msg?;
                match msg {
                    ClientToServerMsg::Join { name } => {
                        log::info!("{name} wants to join");
                        writer.write(ServerToClientMsg::Welcome)?;
                    }
                    _ => {
                        writer.write(ServerToClientMsg::Error(
                            "Unexpected message received".to_owned(),
                        ))?;
                        return Ok(());
                    }
                }
                for msg in reader {
                    let msg = msg?;
                    log::info!("{msg:?}");
                    writer.write(ServerToClientMsg::Pong)?;
                }
                log::info!("Client disconnected from {}", addr);
                Ok(())
            });
        clients.push(handle);
    }
}
