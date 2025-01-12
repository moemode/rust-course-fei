//! You can use this file for experiments.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

fn main() {
    /*    std::thread::scope(|s| {
        s.spawn(|| {
            println!("Hello from thread");
        });
    }); */

    let v = Arc::new(Mutex::new(5));
    let v2 = v.clone();
    let h = std::thread::spawn(move || {
        let mut guard = v.lock().unwrap();
        *guard += 1;
        println!("Hello from thread: {:?}", *guard);
    });
    println!("v2: {:?}", v2);
    h.join().unwrap();
}
