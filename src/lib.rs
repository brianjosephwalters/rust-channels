use std::sync::{Arc, Condvar, Mutex};
use std::collections::{VecDeque};

pub struct Sender<T> {
    inner: Arc<Inner<T>>
}

impl<T> Sender<T> {
    pub fn sender(&mut self, t: T) {
        // Take the lock
        let queue = self.inner.queue.lock().unwrap();
        queue.push_back(t);
        drop(queue);
        self.inner.available.notify_one();
    }
}

pub struct Receiver<T> {
    inner: Arc<Inner<T>>
}

impl<T> Receiver<T> {
    /// This is a blocking version of recv - if nothing is in the channel yet, we wait for
    /// something to be placed there.  
    pub fn recv(&mut self) -> T {
        // Take the lock
        let queue = self.inner.queue.lock().unwrap();
        // 
        loop {
            match queue.pop_front() {
                Some(t) => return t,
                None => {
                    self.inner.available.wait(queue).unwrap();
                }
            }    
        }
    }
}

struct Inner<T> {
    queue: Mutex<VecDeque<T>>,
    available: Condvar,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Inner {
        queue: Mutex::new(VecDeque::new()),
        available: Condvar::new()
    };
    let inner = Arc::new(inner);
    (
        Sender {
            inner: inner.clone(),
        },
        Receiver {
            inner: inner.clone(),
        }
    )
}


