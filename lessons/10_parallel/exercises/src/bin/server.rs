//! You can use this file for experiments.

use tokio::task::JoinSet;
use week10_parallel::messages::ClientToServerMsg;
use week10_parallel::reader::MessageReader;

async fn handle_client(mut reader: MessageReader<ClientToServerMsg, tokio::net::TcpStream>) {
    while let Some(Ok(msg)) = reader.recv().await {
        println!("{msg:?}");
    }
}

async fn run_server() -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 5555)).await?;
    let mut tasks = JoinSet::new();

    loop {
        tokio::select! {
            accept_res = listener.accept() => {
                let (client, addr) = accept_res?;
                println!("Connected from: {addr}");
                let reader = MessageReader::<ClientToServerMsg, _>::new(client);
                tasks.spawn(handle_client(reader));
            }
            task_res = tasks.join_next() => {
                if let Some(Err(e)) = task_res {
                    eprintln!("Error in client task: {e}");
                }
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_server())?;
    Ok(())
}
