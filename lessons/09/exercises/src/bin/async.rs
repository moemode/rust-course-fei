use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    os::{
        fd::{AsRawFd, RawFd},
        unix::process,
    },
    sync::Arc,
    time::Instant,
};

use epoll::{Event, Events};
use week09::{
    messages::ClientToServerMsg, messages::ServerToClientMsg, reader::MessageReader,
    writer::MessageWriter,
};

struct SocketWrapper(Arc<TcpStream>);

impl Read for SocketWrapper {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.as_ref().read(buf)
    }
}

impl Write for SocketWrapper {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.as_ref().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.as_ref().flush()
    }
}

impl SocketWrapper {
    fn inner(&self) -> &TcpStream {
        self.0.as_ref()
    }
}
struct ServerProcess {
    listener: TcpListener,
    state: ProcessState,
}

struct Task {
    process: Box<dyn ProcessThatMightBlock>,
    fds: Vec<RawFd>,
}

impl ServerProcess {
    fn new() -> Self {
        let server = TcpListener::bind(("127.0.0.1", 5555)).unwrap();
        server.set_nonblocking(true).unwrap();
        Self {
            listener: server,
            state: ProcessState::Start,
        }
    }
}

enum ProcessState {
    Start,
    WaitingForAccept,
}

trait ProcessThatMightBlock {
    fn progress(
        &mut self,
        observe_fds: &mut Vec<RawFd>,
        new_processes: &mut Vec<Box<dyn ProcessThatMightBlock>>,
    );
}

impl ProcessThatMightBlock for ServerProcess {
    fn progress(
        &mut self,
        observe_fds: &mut Vec<RawFd>,
        new_processes: &mut Vec<Box<dyn ProcessThatMightBlock>>,
    ) {
        match self.state {
            ProcessState::Start => {
                observe_fds.push(self.listener.as_raw_fd());
                self.state = ProcessState::WaitingForAccept;
            }
            ProcessState::WaitingForAccept => {
                let (client, address) = self.listener.accept().unwrap();
                eprintln!("Client connected from {}", address);
                let client = Arc::new(client);
                let client = ClientProcess {
                    reader: MessageReader::<ClientToServerMsg, SocketWrapper>::new(SocketWrapper(
                        client.clone(),
                    )),
                    writer: MessageWriter::<ServerToClientMsg, SocketWrapper>::new(SocketWrapper(
                        client,
                    )),
                    address,
                    connected: true,
                    last_activity: Instant::now(),
                    state: ClientState::WaitingForAuthenticate,
                };
                new_processes.push(Box::new(client));
            }
        }
    }
}

enum ClientState {
    Start,
    WaitingForAuthenticate,
    Authenticated,
}

struct ClientProcess {
    reader: MessageReader<ClientToServerMsg, SocketWrapper>,
    writer: MessageWriter<ServerToClientMsg, SocketWrapper>,
    address: SocketAddr,
    connected: bool,
    last_activity: Instant,
    state: ClientState,
}

impl ProcessThatMightBlock for ClientProcess {
    fn progress(
        &mut self,
        observe_fds: &mut Vec<RawFd>,
        new_processes: &mut Vec<Box<dyn ProcessThatMightBlock>>,
    ) {
        match self.state {
            ClientState::Start => {
                observe_fds.push(self.reader.inner().inner().as_raw_fd());
                self.state = ClientState::WaitingForAuthenticate;
            }
            ClientState::WaitingForAuthenticate => {
                let msg = self.reader.recv();
            }
            ClientState::Authenticated => todo!(),
        }
    }
}

#[derive(Default)]
struct EventLoop {
    tasks: Vec<Task>,
}

impl EventLoop {
    fn add_process<P: ProcessThatMightBlock + 'static>(&mut self, process: P) {
        self.tasks.push(Task {
            process: Box::new(process),
            fds: vec![],
        });
    }

    fn run(mut self) -> anyhow::Result<()> {
        let epoll = epoll::create(false)?;
        for task in &mut self.tasks {
            task.process.progress(&mut task.fds, &mut vec![]);
            for fd in &task.fds {
                epoll::ctl(
                    epoll,
                    epoll::ControlOptions::EPOLL_CTL_ADD,
                    *fd,
                    Event::new(Events::EPOLLIN, *fd as u64),
                )?;
            }
        }

        loop {
            let mut events = [Event::new(Events::empty(), 0); 1024];
            let event_count = epoll::wait(epoll, -1, &mut events)?;
            let mut new_processes = vec![];
            for event in &events[..event_count] {
                let fd = event.data as RawFd;
                for task in &mut self.tasks {
                    if task.fds.contains(&fd) {
                        task.process.progress(&mut vec![], &mut new_processes);
                    }
                }
            }
            for process in new_processes {
                let mut task = Task {
                    process,
                    fds: vec![],
                };
                task.process.progress(&mut task.fds, &mut vec![]);
                for fd in &task.fds {
                    epoll::ctl(
                        epoll,
                        epoll::ControlOptions::EPOLL_CTL_ADD,
                        *fd,
                        Event::new(Events::EPOLLIN, *fd as u64),
                    )?;
                }
                self.tasks.push(task);
            }
        }
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let mut eventloop = EventLoop::default();
    eventloop.add_process(ServerProcess::new());
    eventloop.run()?;
    Ok(())
}
