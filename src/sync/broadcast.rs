#![allow(clippy::type_complexity)]

use std::collections::VecDeque;

use crate::containers::object_pool::{ObjectPool, ObjectPoolIndex};

use super::types::{arc_mutex_new, ArcMutex};

struct ReceiverQueue<T> {
    queue: VecDeque<T>,
    is_stopped: bool,
}

#[derive(Clone)]
struct ReceiverQueueList<T>
where
    T: Clone,
{
    receiver_queues: ArcMutex<ObjectPool<ArcMutex<ReceiverQueue<T>>>>,
    to_be_removed: ArcMutex<Vec<ObjectPoolIndex>>,
}

#[derive(Clone)]
pub struct Sender<T>
where
    T: Clone,
{
    receiver_queues: ReceiverQueueList<T>,
}

pub struct Receiver<T>
where
    T: Clone,
{
    receiver_queues: ReceiverQueueList<T>,
    queue_id: ObjectPoolIndex,
    queue: ArcMutex<ReceiverQueue<T>>,
}

impl<T> ReceiverQueue<T>
where
    T: Clone,
{
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            is_stopped: false,
        }
    }
}

impl<T> ReceiverQueueList<T>
where
    T: Clone,
{
    pub fn new() -> Self {
        Self {
            receiver_queues: arc_mutex_new(ObjectPool::new()),
            to_be_removed: arc_mutex_new(Vec::new()),
        }
    }

    fn handle_to_be_removed(&self) {
        let mut receiver_queues_guard = self.receiver_queues.lock();

        let mut to_be_removed_guard = self.to_be_removed.lock();
        while let Some(id) = to_be_removed_guard.pop() {
            receiver_queues_guard.release_object(id);
        }
    }
}

impl<T> Default for Sender<T>
where
    T: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Sender<T>
where
    T: Clone,
{
    pub fn new() -> Self {
        Self {
            receiver_queues: ReceiverQueueList::new(),
        }
    }

    pub fn send(&self, object: T) {
        self.receiver_queues.handle_to_be_removed();
        for queue in self.receiver_queues.receiver_queues.lock().iter() {
            let mut queue_guard = queue.lock();
            if !queue_guard.is_stopped {
                queue_guard.queue.push_back(object.clone());
            }
        }
    }

    pub fn send_directly(&self, object: T, receiver: &Receiver<T>) {
        let mut queue = receiver.queue.lock();
        if !queue.is_stopped {
            queue.queue.push_back(object);
        }
    }

    pub fn create_receiver(&self) -> Receiver<T> {
        let queue = arc_mutex_new(ReceiverQueue::<T>::new());
        let queue_id = self
            .receiver_queues
            .receiver_queues
            .lock()
            .create_object(queue.clone());
        Receiver {
            receiver_queues: self.receiver_queues.clone(),
            queue_id,
            queue,
        }
    }
}

impl<T> Receiver<T>
where
    T: Clone,
{
    pub fn stop(&mut self) {
        self.queue.lock().is_stopped = true;
    }

    pub fn resume(&mut self) {
        self.queue.lock().is_stopped = false;
    }

    pub fn pop(&self) -> Option<T> {
        self.queue.lock().queue.pop_front()
    }

    pub fn create_receiver(&self) -> Receiver<T> {
        let queue = arc_mutex_new(ReceiverQueue::<T>::new());
        let queue_id = self
            .receiver_queues
            .receiver_queues
            .lock()
            .create_object(queue.clone());
        Receiver {
            receiver_queues: self.receiver_queues.clone(),
            queue_id,
            queue,
        }
    }
}

impl<T> Clone for Receiver<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        self.create_receiver()
    }
}

impl<T> Drop for Receiver<T>
where
    T: Clone,
{
    fn drop(&mut self) {
        self.receiver_queues
            .to_be_removed
            .lock()
            .push(self.queue_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send() {
        let sender = Sender::<String>::new();

        let receiver0 = sender.create_receiver();
        let receiver1 = sender.create_receiver();

        sender.send("0".to_string());
        sender.send("1".to_string());
        let sender_clone = sender.clone();
        sender_clone.send("2".to_string());
        sender_clone.send("3".to_string());
        sender_clone.send("4".to_string());

        assert_eq!(receiver0.pop(), Some("0".to_string()));
        assert_eq!(receiver0.pop(), Some("1".to_string()));
        assert_eq!(receiver0.pop(), Some("2".to_string()));
        assert_eq!(receiver0.pop(), Some("3".to_string()));
        assert_eq!(receiver0.pop(), Some("4".to_string()));

        assert_eq!(receiver1.pop(), Some("0".to_string()));
        assert_eq!(receiver1.pop(), Some("1".to_string()));
        assert_eq!(receiver1.pop(), Some("2".to_string()));
        assert_eq!(receiver1.pop(), Some("3".to_string()));
        assert_eq!(receiver1.pop(), Some("4".to_string()));
    }

    #[test]
    fn send_directly() {
        let sender = Sender::<String>::new();

        let receiver0 = sender.create_receiver();
        let receiver1 = sender.create_receiver();

        sender.send_directly("0".to_string(), &receiver0);
        sender.send_directly("1".to_string(), &receiver0);
        sender.send_directly("2".to_string(), &receiver0);
        sender.send_directly("3".to_string(), &receiver0);
        sender.send_directly("4".to_string(), &receiver0);

        assert_eq!(receiver0.pop(), Some("0".to_string()));
        assert_eq!(receiver0.pop(), Some("1".to_string()));
        assert_eq!(receiver0.pop(), Some("2".to_string()));
        assert_eq!(receiver0.pop(), Some("3".to_string()));
        assert_eq!(receiver0.pop(), Some("4".to_string()));

        assert_eq!(receiver1.pop(), None);
    }

    #[test]
    fn send_stop_send_resume_send() {
        let sender = Sender::<String>::new();

        let mut receiver = sender.create_receiver();

        sender.send("0".to_string());
        sender.send("1".to_string());

        receiver.stop();

        sender.send("2".to_string());
        sender.send("3".to_string());

        receiver.resume();

        sender.send("4".to_string());

        assert_eq!(receiver.pop(), Some("0".to_string()));
        assert_eq!(receiver.pop(), Some("1".to_string()));
        assert_eq!(receiver.pop(), Some("4".to_string()));
    }

    #[test]
    fn drop_receiver() {
        let sender = Sender::<String>::new();

        {
            let _receiver = sender.create_receiver();

            sender.send("0".to_string());
            sender.send("1".to_string());
        }

        sender.send("0".to_string());

        assert_eq!(sender.receiver_queues.receiver_queues.lock().len(), 0);
    }
}
