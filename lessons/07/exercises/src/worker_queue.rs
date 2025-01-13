//! TODO: implement a simple parallel job queue
//!
//! Implement a struct `WorkerQueue`, which will manage N worker threads.
//! It will allow its users to execute a job on a single worker, and then read the result of that
//! job.
//!
//! ## Creation of the queue
//! The queue should offer a `new` associated function, which will receive the number of workers
//! in the queue, along with the size of a queue for each individual workers.
//! For example, if you execute `WorkerQueue::new(4, 2)`, then four worker threads should be
//! spawned, and each worker thread should have its own queue of size (bound) of `2`.
//!
//! ## Jobs
//! It will be possible to execute a job using the `enqueue` method, which receives something
//! callable that can be executed within a worker.
//! `enqueue` should be callable on a shared reference to the queue.
//!
//! You will need to make sure that the passed function can be safely passed to a worker thread.
//! The queue should be generic over the return type of jobs, all jobs will return the same type.
//!
//! ## Job scheduling
//! Jobs should be scheduled in a trivial round-robin matter.
//! In other words, the first job goes to worker 0, the second to worker 1, the third to worker 2,
//! etc., until you run out of workers and you start from the beginning again.
//! Note that the goal is for the workers to run in parallel, so they should not block each other
//! from executing jobs.
//!
//! ## Reading results
//! The queue should offer a `next_result` method, which will block until the next result is
//! ready. Note that results can "skip ahead" one another, e.g. if you enqueue a job A, and then job
//! B, and job B finishes sooner than job A, then `next_result` should return the result of job B.
//! `next_result` should be callable on a shared reference to the queue.
//!
//! ## Closing of the queue
//! The queue should have a `close` method, which consumes it, drops all resources and waits
//! until all worker threads have terminated.
//!
//! See tests for more details.
//!
//! **DO NOT** use Rayon or any other crate, implement the queue manually using only libstd.
//!
//! TODO(question): is it possible to enqueue work to WorkerQueue from multiple threads?
//! Try it and see what happens. If it's not possible, how could you make it work?
//!
//! Hint: When writing parallel code, you might run into deadlocks (which will be presented as a
//! "blank screen" with no output and maybe a spinning wheel :) ).
//! If you want to see interactive output during the execution of a test, you can add stderr print
//! statements (e.g. using `eprintln!`) and run tests with `cargo test -- --nocapture`, so that you
//! see the output interactively. Alternatively, you can try to use a debugger (e.g. GDB, LLDB or
//! GDB/LLDB integrated within an IDE).

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{channel, sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    vec,
};

pub struct WorkerQueue<T> {
    results_rx: Receiver<T>,
    jobs_txs: Vec<SyncSender<Box<dyn FnOnce() -> T + Send>>>,
    handles: Vec<JoinHandle<()>>,
    next_worker: Arc<AtomicUsize>,
}

impl<T: Send + 'static> WorkerQueue<T> {
    pub fn new(n_threads: usize, queue_size: usize) -> Self {
        let (results_tx, results_rx) = channel();
        let mut handles = vec![];
        let mut jobs_txs = vec![];
        for _ in 0..n_threads {
            let results_tx = results_tx.clone();
            let (jobs_tx, jobs_rx) = sync_channel::<Box<dyn FnOnce() -> T + Send>>(queue_size);
            let h = thread::spawn(move || {
                while let Ok(job) = jobs_rx.recv() {
                    let result = job();
                    results_tx.send(result).unwrap();
                }
            });
            jobs_txs.push(jobs_tx);
            handles.push(h);
        }
        WorkerQueue {
            results_rx,
            jobs_txs,
            handles,
            next_worker: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn enqueue<F>(&self, job: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let worker = self
            .next_worker
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
                Some((x + 1) % self.handles.len())
            })
            .unwrap();
        self.jobs_txs[worker].send(Box::new(job)).unwrap();
    }

    pub fn next_result(&self) -> T {
        self.results_rx.recv().unwrap()
    }

    pub fn close(self) {
        drop(self.jobs_txs);
        for handle in self.handles {
            handle.join().unwrap();
        }
    }
}
