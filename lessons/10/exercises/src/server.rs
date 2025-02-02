use std::{future::Future, pin::Pin};

use futures_util::future::Join;
use tokio::task::JoinSet;

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

async fn run_server_loop(
    mut rx: tokio::sync::oneshot::Receiver<()>,
    listener: tokio::net::TcpListener,
) -> anyhow::Result<()> {
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut rx => {
                println!("Server is shutting down.");
                break;
            }
            Ok((mut socket, _)) = listener.accept() => {
                tasks.spawn_local(async move {
                    let (mut reader, mut writer) = socket.split();
                    tokio::io::copy(&mut reader, &mut writer).await.expect("Failed to copy data");
                });
            }
            task_res = tasks.join_next() => {
                if let Some(Err(e)) = task_res {
                    eprintln!("Error in client task: {e}");
                }
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
        let server_future = run_server_loop(rx, listener);
        Ok(RunningServer {
            max_clients,
            port,
            future: Box::pin(server_future),
            tx,
        })
    }
}
