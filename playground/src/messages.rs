#[derive(serde::Serialize, Debug)]
pub enum ServerToClientMsg {
    Pong,
}

#[derive(serde::Deserialize, Debug)]
pub enum ClientToServerMsg {
    Ping,
}
