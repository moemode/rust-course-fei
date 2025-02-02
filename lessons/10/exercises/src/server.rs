use std::{future::Future, pin::Pin};

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
                if tasks.len() >= max_clients {
                    println!("Server is full");
                    writer.send(ServerToClientMsg::Error("Server is full".to_owned())).await?;
                    continue;
                }
                tasks.spawn_local(handle_client(reader, writer));
            }
            /*task_res = tasks.join_next() => {
                println!("Task completed");
                if let Some(Err(e)) = task_res {
                    println!("Error in client task: {e}");
                }
            }*/
        }
    }
    Ok(())
}

async fn handle_client(
    mut reader: MessageReader<ClientToServerMsg, OwnedReadHalf>,
    mut writer: MessageWriter<ServerToClientMsg, OwnedWriteHalf>,
) -> anyhow::Result<()> {
    while let Some(Ok(msg)) = reader.recv().await {
        println!("Received message: {:?}", msg);
    }
    Ok(())
}

impl RunningServer {
    pub async fn new(max_clients: usize) -> anyhow::Result<Self> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:5000").await?;
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
