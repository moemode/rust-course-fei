//! TODO: Implement a Factorio-like assembly pipeline
//! Welcome to the Space Age!
//!
//! Implement a struct called `FactorioBuilder`, which will allow configuring a *pipeline* of
//! *assembly lines*.
//! A pipeline has an input (a queue) that receives *items* and an output (a queue) that produces
//! items. Between these two, there is a set of assembly lines connected in a series.
//! Each assembly line represents a computation that receives an item, does something and
//! produces an item, which it then forwards to the following assembly line (or
//! to the output, if it is the last assembly line in the pipeline).
//!
//! ## Creation of the builder
//! The builder should offer a `new` method, which will create a builder representing an empty
//! pipeline, which simply forwards all input items directly to the output.
//! The `new` method receives a number representing queue size, which will be applied to all
//! assembly lines and also the input/output queue.
//!
//! Note that the type of items that are sent as inputs to the pipeline might be different than the
//! type of outputs produced by the pipeline. However, the initial empty pipeline **must** have the
//! same input and output type (because the items in an empty pipeline are directly forwarded to
//! the output), so you should enable the `new` method only if that is the case.
//!
//! ## Execution rules of assembly lines
//! Below, you will find four kinds of assembly lines that you should implement.
//! Here is a list of shared properties for all of them:
//! - Each assembly line has an input queue of a certain size.
//!   The queue is filled with items coming from a previous assembly line (or from the input).
//! - When an assembly line receives an item, it performs some computation on it, and then forwards
//!   the result to the next assembly line in the pipeline, or to the output queue, if the assembly
//!   line is at the very end of the pipeline.
//! - Each assembly line should run in parallel w.r.t. the other assembly lines.
//! - Items cannot "skip ahead" in the pipeline. Each item has to be either sent to the output
//!   in the same order as it was sent on the input, or discarded (using `filter/filter_map`).
//! - When you send an input into the pipeline, it should apply backpressure. In other words, if the
//!   input queue is full, the send should block. The same behavior should be applied for all
//!   intermediate queues between individual assembly lines.
//!
//! ## What kinds of assembly lines should be implemented
//! Implement the following four kinds of assembly lines.
//! `map/filter/filter_map/fork_join` should be methods on `FactorioBuilder`, which consume the
//! builder and return a new builder, possibly with different input/output types.
//! The returned builder should contain the corresponding assembly line **at its end**.
//! In other words, the previous output becomes the input of the newly added assembly line, and the
//! newly added assembly line becomes the output of the pipeline.
//!
//! In the following ASCII diagrams, `<item X>` means "an item with type X".
//!
//! ### map
//! Adds a `map` assembly line to the end of the pipeline.
//! `map` receives a function that should be applied to each item that goes through it.
//! The function will produce a new item that will be forwarded further through the pipeline.
//!
//! ```
//! <item A> --> map(A) --> <item B>
//! ```
//!
//! Note that `map` might change the output type of the pipeline.
//!
//! ### filter
//! Adds a `filter` assembly line to the end of the pipeline.
//! `filter` receives a function that should be applied to each item that goes through it.
//! The function will receive a shared reference to the input item and return a boolean.
//! If it returns `true`, then the input item is forwarded further through the pipeline.
//! If it returns `false`, then the item will be discarded.
//!
//! ```
//!                          [true]
//! <item A> --> filter(&A) -------> <item A>
//!                 |
//!                 | [false]
//!                 |
//!                 x (discard)
//! ```
//!
//! ### filter_map
//! Adds a `filter_map` assembly line to the end of the pipeline.
//! `filter_map` receives a function that should be applied to each item that goes through it.
//! The function will receive an item and return an `Option` of a possibly different type.
//! If it returns `Some`, then the item wrapped within the Option is forwarded further through the
//! pipeline.
//! If it returns `None`, then the item will be discarded.
//!
//! ```
//!                            [Some(B)]
//! <item A> --> filter_map(A) -------> <item B>
//!                  |
//!                  | [None]
//!                  |
//!                  x (discard)
//! ```
//!
//! Note that `filter_map` might change the output type of the pipeline.
//!
//! ### fork_join
//! Adds a `fork_join` assembly line to the end of the pipeline.
//! `fork_join` creates `N` parallel internal assembly lines that will process each input item
//! separately, **in parallel**. Each input item will be processed by all `N` lines.
//! Each internal assembly line has an input queue with size that is stored in the builder.
//! `fork_join` receives a fork function that will run on each internal assembly line, the number
//! of internal assembly lines (`N`), and a join function.
//! The fork function will receive a reference to the input item and a zero-based index of the
//! internal assembly line that executes the fork function.
//!
//! The results of the internal assembly lines will be joined together using the provided join
//! function, which will receive a `Vec` of the intermediate results.
//! This combined result will be then passed forward through the pipeline.
//! The internal assembly lines produce an item of some type, which is then
//! sent to the join function. The output of the join function then determines what type will be
//! passed to the rest of the pipeline.
//!
//! TODO(bonus): The internal lines need to be synchronized. They can handle
//! additional items even if some other line is still processing the previous item, but the join
//! function needs to receive the intermediate results **in the original order**.
//!
//! ```
//!                         --> fork(&A, 0)   --> <item R> --v
//!                         |                                |
//! <item A> --> fork_join                                   --> join(Vec<R>) --> <item B>
//!                         |                                |
//!                         --> fork(&A, 1)   --> <item R> --^
//!                         |                                |
//!                         ...                              |
//!                         |                                |
//!                         --> fork(&A, N-1) --> <item R> --^
//! ```
//!
//! Note that `fork_join` might change the output type of the pipeline.
//!
//! Note: this assembly line will require you to create a bunch of channels.
//! Don't be afraid of it :)
//!
//! ## Creation of the pipeline
//! Once all assembly lines of the pipeline have been configured, the builder should allow creating
//! the actual pipeline using a `build` method. This method returns a struct representing the
//! pipeline, a channel that can be used to send inputs to the pipeline, and a channel that can be
//! used to read the outputs from the end of the pipeline.
//!
//! **All threads should be created when configuring the pipeline. After the `build` function
//! returns a completed pipeline, no new threads should be spawned when items go through the
//! pipeline!**
//!
//! ## Closing of the pipeline
//! The created pipeline should have a `close` method, which consumes it and waits until all
//! assembly lines have terminated.
//! Think carefully about the receive and send channels and what happens when they are closed.
//!
//! See tests for more details. Some tests also contain simple ASCII diagrams that render the
//! pipelines created in the test.
//!
//! **DO NOT** use Rayon or any other crate, implement the pipeline manually using only libstd.
//!
//! Hint: When writing parallel code, you might run into deadlocks (which will be presented as a
//! "blank screen" with no output and maybe a spinning wheel :) ).
//! If you want to see interactive output during the execution of a test, you can add stderr print
//! statements (e.g. using `eprintln!`) and run tests with `cargo test -- --nocapture`, so that you
//! see the output interactively. Alternatively, you can try to use a debugger (e.g. GDB, LLDB or
//! GDB/LLDB integrated within an IDE).

use std::{
    sync::{
        self,
        mpsc::{sync_channel, Receiver, Sender, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    vec,
};

pub struct FactorioBuilder<I, O> {
    queue_size: usize,
    pipeline: Pipeline,
    input_tx: SyncSender<I>,
    output_rx: Receiver<O>,
}

pub struct Pipeline {
    handles: Vec<JoinHandle<()>>,
}

impl<T> FactorioBuilder<T, T>
where
    T: Send + 'static,
{
    pub fn new(queue_size: usize) -> Self {
        let (input_tx, input_rx) = sync_channel(queue_size);
        let (output_tx, output_rx) = sync_channel(queue_size);
        let h = thread::spawn(move || {
            while let Ok(t) = input_rx.recv() {
                if output_tx.send(t).is_err() {
                    return;
                }
            }
        });
        FactorioBuilder {
            queue_size,
            pipeline: Pipeline { handles: vec![h] },
            input_tx,
            output_rx,
        }
    }
}

impl<I, O> FactorioBuilder<I, O>
where
    I: Send + 'static,
    O: Send + Sync + 'static,
{
    pub fn build(self) -> (Pipeline, SyncSender<I>, Receiver<O>) {
        (self.pipeline, self.input_tx, self.output_rx)
    }

    pub fn map<F, U>(self, f: F) -> FactorioBuilder<I, U>
    where
        F: Fn(O) -> U + Send + 'static,
        U: Send + 'static,
    {
        let f = move |o| Some(f(o));
        self.filter_map(f)
    }

    pub fn filter<F>(self, f: F) -> FactorioBuilder<I, O>
    where
        F: Fn(&O) -> bool + Send + 'static,
    {
        let f = move |o| f(&o).then_some(o);
        self.filter_map(f)
    }

    pub fn filter_map<F, U>(self, f: F) -> FactorioBuilder<I, U>
    where
        F: Fn(O) -> Option<U> + Send + 'static,
        U: Send + 'static,
    {
        let (output_tx, output_rx) = sync_channel::<U>(self.queue_size);
        let old_output_rx = self.output_rx;
        let h = thread::spawn(move || {
            while let Ok(o) = old_output_rx.recv() {
                if let Some(u) = f(o) {
                    if output_tx.send(u).is_err() {
                        return;
                    }
                }
            }
        });
        let mut handles = self.pipeline.handles;
        handles.push(h);
        FactorioBuilder {
            queue_size: self.queue_size,
            pipeline: Pipeline { handles },
            input_tx: self.input_tx,
            output_rx,
        }
    }

    pub fn fork_join<F, L, J, U>(
        self,
        worker_function: F,
        n_lines: usize,
        join: J,
    ) -> FactorioBuilder<I, U>
    where
        F: Fn(&O, usize) -> L + Sync + Send + 'static,
        L: 'static + Send,
        J: Fn(Vec<L>) -> U + Send + 'static,
        U: 'static + Send,
    {
        let worker_function = Arc::new(worker_function);
        let mut worker_input_tx = Vec::with_capacity(n_lines);
        let mut worker_output_rx = Vec::with_capacity(n_lines);
        let mut worker_handles = vec![];
        for _ in 0..n_lines {
            let (input_tx, input_rx) = sync_channel::<(Arc<O>, usize)>(self.queue_size);
            let (output_tx, output_rx) = sync_channel::<L>(self.queue_size);
            worker_input_tx.push(input_tx);
            worker_output_rx.push(output_rx);
            let worker_fn = worker_function.clone();
            let h = thread::spawn(move || {
                while let Ok((o, line)) = input_rx.recv() {
                    if output_tx.send(worker_fn(o.as_ref(), line)).is_err() {
                        return;
                    }
                }
            });
            worker_handles.push(h);
        }

        let distributer = thread::spawn(move || {
            while let Ok(o) = self.output_rx.recv() {
                let o = Arc::new(o);
                for (l, in_tx) in worker_input_tx.iter().enumerate() {
                    if in_tx.send((o.clone(), l)).is_err() {
                        return;
                    }
                }
            }
        });

        let (new_output_tx, new_output_rx) = sync_channel::<U>(self.queue_size);
        let gatherer = thread::spawn(move || loop {
            let mut worker_outputs = vec![];
            for out_rx in &worker_output_rx {
                if let Ok(worker_output) = out_rx.recv() {
                    worker_outputs.push(worker_output);
                } else {
                    return;
                }
            }
            let output = join(worker_outputs);
            if new_output_tx.send(output).is_err() {
                return;
            };
        });

        let mut handles = self.pipeline.handles;
        handles.extend(worker_handles);
        handles.push(distributer);
        handles.push(gatherer);
        FactorioBuilder {
            queue_size: self.queue_size,
            pipeline: Pipeline { handles },
            input_tx: self.input_tx,
            output_rx: new_output_rx,
        }
    }
}

impl Pipeline {
    fn new() -> Self {
        Self { handles: vec![] }
    }

    pub fn close(self) {
        for h in self.handles {
            h.join().unwrap();
        }
    }
}
