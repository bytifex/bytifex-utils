//! # Multi Producer, Collective Consumer
//!
//! Every message sent throught the channel is received only once by any Consumer

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
};

use tokio::sync::watch;

use super::types::{ArcMutex, arc_mutex_new};

#[derive(Debug)]
pub enum SendError {
    Disconnected,
}

#[derive(Debug)]
pub enum RecvError {
    Disconnected,
}

#[derive(Debug)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

struct Shared<T: Send> {
    queue: ArcMutex<VecDeque<T>>,
    sender_count: Arc<AtomicUsize>,
    // todo!("use an async condvar")
    queue_watcher_sender: Arc<watch::Sender<()>>,
}

pub struct Sender<T: Send> {
    shared: Shared<T>,
}

pub struct Receiver<T: Send> {
    shared: Shared<T>,
    queue_watcher_receiver: watch::Receiver<()>,
}

pub fn channel<T: Send>() -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = watch::channel(());

    let shared = Shared {
        queue: arc_mutex_new(VecDeque::new()),
        sender_count: Arc::new(AtomicUsize::new(1)),
        queue_watcher_sender: Arc::new(sender),
    };

    (
        Sender {
            shared: shared.clone(),
        },
        Receiver {
            shared,
            queue_watcher_receiver: receiver,
        },
    )
}

impl<T: Send> Sender<T> {
    pub fn send(&self, msg: T) -> Result<(), SendError> {
        if self.shared.queue_watcher_sender.receiver_count() != 0 {
            self.shared.queue.lock().push_back(msg);
            let _ = self.shared.queue_watcher_sender.send(());

            Ok(())
        } else {
            Err(SendError::Disconnected)
        }
    }
}

impl<T: Send> Receiver<T> {
    pub async fn recv_async(&mut self) -> Result<T, RecvError> {
        loop {
            if self.queue_watcher_receiver.changed().await.is_ok() {
                match self.try_pop() {
                    Ok(msg) => break Ok(msg),
                    Err(TryRecvError::Disconnected) => break Err(RecvError::Disconnected),
                    Err(TryRecvError::Empty) => (),
                }
            } else {
                // unreachable!("This could not happen, since Self also holds a clone of the sender part");
                break Err(RecvError::Disconnected);
            }
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        match self.queue_watcher_receiver.has_changed() {
            Ok(true) => self.try_pop(),
            Ok(false) => {
                if self.shared.sender_count.load(atomic::Ordering::SeqCst) == 0 {
                    Err(TryRecvError::Disconnected)
                } else {
                    Err(TryRecvError::Empty)
                }
            }
            Err(_) => {
                // unreachable!("This could not happen, since Self also holds a clone of the sender part");
                Err(TryRecvError::Disconnected)
            }
        }
    }

    pub fn try_pop(&self) -> Result<T, TryRecvError> {
        let mut queue_guard = self.shared.queue.lock();
        if let Some(msg) = queue_guard.pop_front() {
            let _ = self.shared.queue_watcher_sender.send(());
            Ok(msg)
        } else if self.shared.sender_count.load(atomic::Ordering::SeqCst) == 0 {
            Err(TryRecvError::Disconnected)
        } else {
            Err(TryRecvError::Empty)
        }
    }
}

impl<T: Send> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            sender_count: self.sender_count.clone(),
            queue_watcher_sender: self.queue_watcher_sender.clone(),
        }
    }
}

impl<T: Send> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared
            .sender_count
            .fetch_add(1, atomic::Ordering::SeqCst);
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T: Send> Drop for Sender<T> {
    fn drop(&mut self) {
        self.shared
            .sender_count
            .fetch_sub(1, atomic::Ordering::SeqCst);

        let _ = self.shared.queue_watcher_sender.send(());
    }
}

impl<T: Send> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            queue_watcher_receiver: self.queue_watcher_receiver.clone(),
        }
    }
}

impl<T: Send> Drop for Receiver<T> {
    fn drop(&mut self) {
        // if this is the last receiver, then empty the queue
        if self.shared.queue_watcher_sender.receiver_count() == 1 {
            let mut queue_guard = self.shared.queue.lock();
            if !queue_guard.is_empty() {
                queue_guard.clear();
                let _ = self.shared.queue_watcher_sender.send(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use crate::sync::types::ArcMutex;

    use super::{Receiver, channel};

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    struct Msg(usize);

    async fn run_worker(received_values: ArcMutex<Vec<Msg>>, mut receiver: Receiver<Msg>) {
        while let Ok(msg) = receiver.recv_async().await {
            received_values.lock().push(msg);
        }
    }

    async fn run_test(number_of_workers: usize) {
        let received_values = Arc::new(Mutex::new(Vec::<Msg>::new()));

        let (sender, receiver) = channel();

        let mut workers = Vec::new();
        for _ in 0..number_of_workers {
            let received_values = received_values.clone();
            let receiver = receiver.clone();
            workers.push(tokio::spawn(run_worker(received_values, receiver)));
        }

        sender.send(Msg(7)).unwrap();

        drop(sender);

        for worker in workers {
            worker.await.unwrap();
        }

        let received_values = received_values.lock();
        assert_eq!(received_values.len(), 1);
        assert_eq!(*received_values.first().unwrap(), Msg(7));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn single_worker_current_thread() {
        // running the same test multiple times to ensure no race condition happened
        for _i in 0..1000 {
            run_test(1).await;
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn multiple_workers_current_thread() {
        // running the same test multiple times to ensure no race condition happened
        for _i in 0..1000 {
            run_test(10).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn single_worker_multi_thread() {
        // running the same test multiple times to ensure no race condition happened
        for _i in 0..1000 {
            run_test(1).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn multiple_workers_multi_thread() {
        // running the same test multiple times to ensure no race condition happened
        for _i in 0..1000 {
            run_test(10).await;
        }
    }
}
