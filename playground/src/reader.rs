use std::{io::Read, net::TcpStream, path::Iter};

use crate::messages::ClientToServerMsg;

pub struct MessageReader<'a> {
    buffer: Vec<u8>,
    loaded: usize,
    client: &'a TcpStream,
}

impl<'a> MessageReader<'a> {
    pub fn new(client: &'a TcpStream) -> Self {
        Self {
            buffer: vec![0; 50],
            loaded: 0,
            client,
        }
    }
    pub fn recv(&mut self) -> Option<anyhow::Result<ClientToServerMsg>> {
        loop {
            /* if self.loaded == 50 {
                self.buffer = vec![0; 50];
                self.loaded = 0;
            } */
            let read_bytes = match self.client.read(&mut self.buffer[self.loaded..]) {
                Ok(b) => b,
                Err(err) => return Some(Err(err.into())),
            };
            if read_bytes == 0 {
                break;
            }
            self.loaded += read_bytes;
            if let Some(position) = self.buffer.iter().position(|c| *c == b'\n') {
                let msg = &self.buffer[..position];
                let msg: ClientToServerMsg = match serde_json::from_slice(msg) {
                    Ok(msg) => msg,
                    Err(error) => return Some(Err(error.into())),
                };
                self.buffer.copy_within(position + 1.., 0);
                self.loaded -= position + 1;
                return Some(Ok(msg));
            }
        }
        None
    }
}

impl<'a> Iterator for MessageReader<'a> {
    type Item = anyhow::Result<ClientToServerMsg>;

    fn next(&mut self) -> Option<Self::Item> {
        self.recv()
    }
}
