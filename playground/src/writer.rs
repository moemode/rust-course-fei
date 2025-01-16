use std::{io::Write, net::TcpStream};

use serde::Serialize;

use crate::messages::ServerToClientMsg;

pub struct MessageWriter<'a> {
    stream: &'a TcpStream,
}

impl<'a> MessageWriter<'a> {
    pub fn new(stream: &'a TcpStream) -> Self {
        Self { stream }
    }
    pub fn send(&mut self, msg: ServerToClientMsg) -> anyhow::Result<()> {
        let serialized = serde_json::to_vec(&msg)?;
        self.stream.write_all(&serialized)?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;
        Ok(())
    }
}
