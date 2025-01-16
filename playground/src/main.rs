use std::io::Read;
use std::net::TcpListener;

use reader::MessageReader;
use writer::MessageWriter;

mod messages;
mod reader;
mod writer;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let s = TcpListener::bind(("127.0.0.1", 5555))?;
    let (client, addr) = s.accept()?;
    let mut reader = MessageReader::new(&client);
    let mut writer = MessageWriter::new(&client);
    log::info!("Client connected from {addr}");
    for msg in reader {
        let msg = msg?;
        log::info!("{msg:?}");
        writer.send(messages::ServerToClientMsg::Pong)?;
    }
    log::info!("Client disconnected from {addr}");
    Ok(())
}
