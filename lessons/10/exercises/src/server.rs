use std::{future::Future, pin::Pin};

use futures_util::future;
use tokio::{
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    task::JoinSet,
};

use crate::{
    messages::{ClientToServerMsg, ServerToClientMsg},
    reader::MessageReader,
    writer::MessageWriter,
};

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
    let mut tasks = JoinSet::new();
    tasks.spawn_local(future::pending());
    println!("Server is running");
    loop {
        println!("Entering select loop");
        tokio::select! {
            _ = &mut rx => {
                println!("Server is shutting down.");
                break;
            }
            Ok((client, _)) = listener.accept() => {
                println!("New client accepted.");
                let (rx, tx) = client.into_split();
                let reader = MessageReader::<ClientToServerMsg, _>::new(rx);
                let mut writer = MessageWriter::<ServerToClientMsg, _>::new(tx);
                println!("tasks.len(): {}", tasks.len());
                if tasks.len() > max_clients {
                    println!("Server is full");
                    writer.send(ServerToClientMsg::Error("Server is full".to_owned())).await?;
                    continue;
                }
                tasks.spawn_local(handle_client(reader, writer));
            }
            task_res = tasks.join_next() => {
                println!("Task completed");
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
    mut reader: MessageReader<ClientToServerMsg, OwnedReadHalf>,
    mut writer: MessageWriter<ServerToClientMsg, OwnedWriteHalf>,
) -> anyhow::Result<String> {
    tokio::select! {
        msg = reader.recv() => match msg {
            Some(Ok(ClientToServerMsg::Join { name })) => {
                writer.send(ServerToClientMsg::Welcome).await?;
                Ok(name)
            }
            Some(Ok(_)) => {
                writer.send(ServerToClientMsg::Error("Unexpected message received".to_owned())).await?;
                Err(anyhow::anyhow!("Unexpected message received"))
            }
            Some(Err(e)) => Err(e.into()),
            None => {
                writer.send(ServerToClientMsg::Error("Connection closed".to_owned())).await?;
                Err(anyhow::anyhow!("Connection closed"))
            }
        },
        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
            writer.send(ServerToClientMsg::Error("Timed out waiting for Join".to_owned())).await?;
            Err(anyhow::anyhow!("Timed out waiting for Join"))
        }
    }
}

async fn handle_client(
    mut reader: MessageReader<ClientToServerMsg, OwnedReadHalf>,
    mut writer: MessageWriter<ServerToClientMsg, OwnedWriteHalf>,
) -> anyhow::Result<()> {
    join_client(reader, writer).await?;
    Ok(())
}

impl RunningServer {
    pub async fn new(max_clients: usize) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let server_future = run_server(rx, listener, max_clients);
        println!("Server is listening on port {port}");
        Ok(RunningServer {
            max_clients,
            port,
            future: Box::pin(server_future),
            tx,
        })
    }
}
