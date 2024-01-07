#![allow(clippy::type_complexity)]

use std::{collections::VecDeque, sync::Arc};

use tokio::sync::Notify;

use crate::containers::object_pool::{ObjectPool, ObjectPoolIndex};

use super::{
    types::{arc_mutex_new, ArcMutex},
    usage_counter::{UsageCounter, UsageCounterWatcher},
};

#[derive(Clone)]
struct ReceiverQueue<T> {
    queue: ArcMutex<VecDeque<T>>,
    is_stopped: ArcMutex<bool>,
    notify: Arc<Notify>,
}

#[derive(Clone)]
struct ReceiverQueueList<T>
where
    T: Clone,
{
    receiver_queues: ArcMutex<ObjectPool<ReceiverQueue<T>>>,
    to_be_removed: ArcMutex<Vec<ObjectPoolIndex>>,
}

#[derive(Clone)]
pub struct Sender<T>
where
    T: Clone,
{
    receiver_queues: ReceiverQueueList<T>,
    usage_counter: UsageCounter,
}

pub struct Receiver<T>
where
    T: Clone,
{
    receiver_queues: ReceiverQueueList<T>,
    queue_id: ObjectPoolIndex,
    queue: ReceiverQueue<T>,
    usage_counter_watcher: UsageCounterWatcher,
}

impl<T> ReceiverQueue<T>
where
    T: Clone,
{
    pub fn new() -> Self {
        Self {
            queue: arc_mutex_new(VecDeque::new()),
            is_stopped: arc_mutex_new(false),
            notify: Arc::new(Notify::new()),
        }
    }

    fn add_object_if_not_stopped(&self, object: T) {
        let mut queue_guard = self.queue.lock();
        if !*self.is_stopped.lock() {
            queue_guard.push_back(object);
            self.notify.notify_waiters();
        }
        drop(queue_guard);
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
            usage_counter: UsageCounter::new(),
        }
    }

    pub fn send(&self, object: T) {
        self.receiver_queues.handle_to_be_removed();
        for queue in self.receiver_queues.receiver_queues.lock().iter() {
            queue.add_object_if_not_stopped(object.clone());
        }
    }

    pub fn send_directly(&self, object: T, receiver: &Receiver<T>) {
        receiver.queue.add_object_if_not_stopped(object.clone());
    }

    pub fn create_receiver(&self) -> Receiver<T> {
        let queue = ReceiverQueue::<T>::new();
        let queue_id = self
            .receiver_queues
            .receiver_queues
            .lock()
            .create_object(queue.clone());
        Receiver {
            receiver_queues: self.receiver_queues.clone(),
            queue_id,
            queue,
            usage_counter_watcher: self.usage_counter.watcher(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SenderDropped;

impl<T> Receiver<T>
where
    T: Clone,
{
    pub fn stop(&mut self) {
        *self.queue.is_stopped.lock() = true;
    }

    pub fn resume(&mut self) {
        *self.queue.is_stopped.lock() = false;
    }

    pub fn try_pop(&self) -> Result<Option<T>, SenderDropped> {
        if let Some(object) = self.queue.queue.lock().pop_front() {
            Ok(Some(object))
        } else if self.usage_counter_watcher.is_observed_dropped() {
            Err(SenderDropped)
        } else {
            Ok(None)
        }
    }

    pub async fn pop(&self) -> Result<T, SenderDropped> {
        loop {
            if let Some(object) = self.try_pop()? {
                break Ok(object);
            } else {
                self.queue.notify.notified().await;
            }
        }
    }

    pub fn create_receiver(&self) -> Receiver<T> {
        let queue = ReceiverQueue::<T>::new();
        let queue_id = self
            .receiver_queues
            .receiver_queues
            .lock()
            .create_object(queue.clone());
        Receiver {
            receiver_queues: self.receiver_queues.clone(),
            queue_id,
            queue,
            usage_counter_watcher: self.usage_counter_watcher.clone(),
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

    #[tokio::test]
    async fn send() {
        let sender = Sender::<String>::new();

        let receiver0 = sender.create_receiver();
        let receiver1 = sender.create_receiver();

        sender.send("0".to_string());
        sender.send("1".to_string());
        {
            let sender = sender.clone();
            sender.send("2".to_string());
            sender.send("3".to_string());
            sender.send("4".to_string());
        }

        assert_eq!(receiver0.try_pop().unwrap(), Some("0".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("1".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("2".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("3".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("4".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), None);

        assert_eq!(receiver1.pop().await.unwrap(), "0".to_string());
        assert_eq!(receiver1.pop().await.unwrap(), "1".to_string());
        assert_eq!(receiver1.pop().await.unwrap(), "2".to_string());
        assert_eq!(receiver1.pop().await.unwrap(), "3".to_string());
        assert_eq!(receiver1.pop().await.unwrap(), "4".to_string());
        assert_eq!(receiver1.try_pop().unwrap(), None);
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

        assert_eq!(receiver0.try_pop().unwrap(), Some("0".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("1".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("2".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("3".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), Some("4".to_string()));

        assert_eq!(receiver1.try_pop().unwrap(), None);
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

        assert_eq!(receiver.try_pop().unwrap(), Some("0".to_string()));
        assert_eq!(receiver.try_pop().unwrap(), Some("1".to_string()));
        assert_eq!(receiver.try_pop().unwrap(), Some("4".to_string()));
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

    #[tokio::test]
    async fn drop_sender() {
        let (receiver0, receiver1) = {
            let sender = Sender::<usize>::new();
            let ret = (sender.create_receiver(), sender.create_receiver());

            sender.send(7);

            ret
        };

        let receiver2 = receiver0.create_receiver();

        assert_eq!(receiver0.try_pop().unwrap(), Some(7));
        assert_eq!(receiver1.try_pop().unwrap(), Some(7));

        assert_eq!(receiver0.pop().await, Err(SenderDropped));
        assert_eq!(receiver1.try_pop(), Err(SenderDropped));
        assert_eq!(receiver2.try_pop(), Err(SenderDropped));
    }
}
