use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::{
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{Mutex, RwLock},
};

use tokio::task::JoinSet;

use crate::{
    messages::{ClientToServerMsg, ServerToClientMsg},
    reader::MessageReader,
    writer::MessageWriter,
};

type SharedWriter = Arc<Mutex<MessageWriter<ServerToClientMsg, OwnedWriteHalf>>>;
type ClientMap = Arc<RwLock<HashMap<String, SharedWriter>>>;

/// Representation of a running server
pub struct RunningServer {
    /// Maximum number of clients that can be connected to the server
    max_clients: usize,
    /// Port on which the server is running
    pub port: u16,
    /// Main future of the server
    pub future: Pin<Box<dyn Future<Output = anyhow::Result<()>>>>,
    /// Channel that can be used to tell the server to stop
    pub tx: tokio::sync::oneshot::Sender<()>,
}

async fn run_server(
    mut rx: tokio::sync::oneshot::Receiver<()>,
    listener: tokio::net::TcpListener,
    max_clients: usize,
) -> anyhow::Result<()> {
    let clients: ClientMap = Arc::new(RwLock::new(HashMap::new()));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut rx => break,
            Ok((client, _)) = listener.accept() => {
                let (rx, tx) = client.into_split();
                let reader = MessageReader::<ClientToServerMsg, _>::new(rx);
                let mut writer = MessageWriter::<ServerToClientMsg, _>::new(tx);
                if tasks.len() >= max_clients {
                    writer.send(ServerToClientMsg::Error("Server is full".to_owned())).await?;
                    continue;
                }
                tasks.spawn(handle_client(reader, Arc::new(Mutex::new(writer)), clients.clone()));
            }
            task_res = tasks.join_next(), if !tasks.is_empty() => {
                if let Some(Err(e)) = task_res {
                    println!("Error in client task: {e}");
                }
            }
        }
    }
    Ok(())
}

/// Handles the initial connection handshake with a client.
///
/// # Protocol
/// The client must send a `Join` message as its first message. Upon receiving this message,
/// the server will respond with a `Welcome` message if the join is successful.
///
/// # Errors
/// This function will return an error if:
/// - The client does not send a `Join` message within 2 seconds
/// - The client sends any message other than `Join` as its first message
/// - The connection is closed before receiving a message
///
/// # Returns
/// - `Ok(String)` containing the client's username if join is successful
/// - `Err` if the join fails for any reason listed above
async fn join_client(
    reader: &mut MessageReader<ClientToServerMsg, OwnedReadHalf>,
    writer: &SharedWriter,
    clients: &ClientMap,
) -> anyhow::Result<String> {
    tokio::select! {
        msg = reader.recv() => match msg {
            Some(Ok(ClientToServerMsg::Join { name })) => {
                if clients.read().await.contains_key(&name) {
                    writer.lock().await.send(ServerToClientMsg::Error("Username already taken".to_owned())).await?;
                    return Err(anyhow::anyhow!("Username already taken"));
                }
                writer.lock().await.send(ServerToClientMsg::Welcome).await?;
                Ok(name)
            }
            Some(Ok(_)) => {
                writer.lock().await.send(ServerToClientMsg::Error("Unexpected message received".to_owned())).await?;
                Err(anyhow::anyhow!("Unexpected message received"))
            }
            Some(Err(e)) => Err(e.into()),
            None => {
                writer.lock().await.send(ServerToClientMsg::Error("Connection closed".to_owned())).await?;
                Err(anyhow::anyhow!("Connection closed"))
            }
        },
        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
            writer.lock().await.send(ServerToClientMsg::Error("Timed out waiting for Join".to_owned())).await?;
            Err(anyhow::anyhow!("Timed out waiting for Join"))
        }
    }
}

/// Then it should start receiving requests from the client.
/// - If the client ever sends the `Join` message again, the server should respond with an error
/// "Unexpected message received" and disconnect the client immediately.
async fn handle_client(
    mut reader: MessageReader<ClientToServerMsg, OwnedReadHalf>,
    writer: SharedWriter,
    clients: ClientMap,
) -> anyhow::Result<()> {
    let name = join_client(&mut reader, &writer, &clients).await?;
    {
        let mut clients_guard = clients.write().await;
        clients_guard.insert(name.clone(), writer.clone());
    }
    while let Some(Ok(msg)) = reader.recv().await {
        if react_client_msg(msg, &name, &clients).await.is_err() {
            break;
        }
    }
    clients.write().await.remove(&name);
    Ok(())
}

/// pub enum ClientToServerMsg {
/// This is the first message in the communication, which should be sent by the client.
/// When some other client with the same name already exists, the server should respond
/// with an error "Username already taken" and disconnect the new client.
/// Join { name: String },
/// This message checks that the connection is OK.
/// The server should respond with [ServerToClientMsg::Pong].
/// Ping,
/// Send a request to list the usernames of users currently connected to the server.
/// The order of the usernames is not important.
/// ListUsers,
/// Sends a direct message to the user with the given name (`to`).
/// If the user does not exist, the server responds with an error "User <to> does not exist".
/// If the client tries to send a message to themselves, the server responds with an error
/// "Cannot send a DM to yourself".
/// SendDM { to: String, message: String },
/// Sends a message to all currently connected users (except for the sender of the broadcast).
/// Broadcast { message: String },
/// }
async fn react_client_msg(
    msg: ClientToServerMsg,
    name: &str,
    clients: &ClientMap,
) -> anyhow::Result<()> {
    match msg {
        ClientToServerMsg::Ping => {
            let clients_guard = clients.read().await;
            if let Some(writer) = clients_guard.get(name) {
                writer.lock().await.send(ServerToClientMsg::Pong).await?;
            }
        }
        ClientToServerMsg::ListUsers => {
            let clients_guard = clients.read().await;
            let users = clients_guard.keys().cloned().collect();
            if let Some(writer) = clients_guard.get(name) {
                writer
                    .lock()
                    .await
                    .send(ServerToClientMsg::UserList { users })
                    .await?;
            }
        }
        ClientToServerMsg::Broadcast { message } => {
            let clients_guard = clients.read().await;
            for (client_name, writer) in clients_guard.iter() {
                if client_name != name {
                    writer
                        .lock()
                        .await
                        .send(ServerToClientMsg::Message {
                            from: name.into(),
                            message: message.clone(),
                        })
                        .await?;
                }
            }
        }
        ClientToServerMsg::SendDM { to, message } => {
            let clients_guard = clients.read().await;
            let sender_writer = clients_guard.get(name);
            let target_writer = clients_guard.get(&to);

            match (sender_writer, target_writer) {
                (Some(writer), _) if to == *name => {
                    writer
                        .lock()
                        .await
                        .send(ServerToClientMsg::Error(
                            "Cannot send a DM to yourself".to_owned(),
                        ))
                        .await?;
                }
                (Some(writer), None) => {
                    writer
                        .lock()
                        .await
                        .send(ServerToClientMsg::Error(format!(
                            "User {to} does not exist"
                        )))
                        .await?;
                }
                (_, Some(target_writer)) => {
                    target_writer
                        .lock()
                        .await
                        .send(ServerToClientMsg::Message {
                            from: name.into(),
                            message,
                        })
                        .await?;
                }
                _ => {}
            }
        }
        _ => {
            let clients_guard = clients.read().await;
            if let Some(writer) = clients_guard.get(name) {
                writer
                    .lock()
                    .await
                    .send(ServerToClientMsg::Error(
                        "Unexpected message received".to_owned(),
                    ))
                    .await?;
                anyhow::bail!("Unexpected message received");
            }
        }
    }
    Ok(())
}

impl RunningServer {
    pub async fn new(max_clients: usize) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let server_future = run_server(rx, listener, max_clients);
        Ok(RunningServer {
            max_clients,
            port,
            future: Box::pin(server_future),
            tx,
        })
    }
}
