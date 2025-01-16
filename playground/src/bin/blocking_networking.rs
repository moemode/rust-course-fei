use std::io::Read;
use std::net::TcpListener;

fn main() -> anyhow::Result<()> {
    let s = TcpListener::bind(("127.0.0.1", 5555))?;
    let (mut client, addr) = s.accept()?;
    let mut buffer = [0u8; 1024];
    let mut loaded = 0;
    loop {
        let read_bytes = client.read(&mut buffer)?;
        loaded += read_bytes;
        if let Some(position) = buffer.iter().position(|c| *c == b'\n') {
            let msg = &buffer[..position];
            let msg = String::from_utf8_lossy(&msg).to_string();
            buffer.copy_within(position + 1.., 0);
            loaded -= position + 1;
            println!("Client sent {msg}");
        }
    }
    Ok(())
}
