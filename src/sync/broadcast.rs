#![allow(clippy::type_complexity)]

use crate::sync::notify::Notify;
use std::{collections::VecDeque, sync::Arc};

use crate::containers::object_pool::ObjectPool;

use super::{
    types::{ArcMutex, arc_mutex_new},
    usage_counter::{UsageCounter, UsageCounterWatcher},
};

struct ReceiverQueue<T> {
    queue: ArcMutex<VecDeque<T>>,
    is_stopped: ArcMutex<bool>,
    notify: Arc<Notify>,
}

impl<T> Clone for ReceiverQueue<T> {
    fn clone(&self) -> Self {
        Self {
            queue: self.queue.clone(),
            is_stopped: self.is_stopped.clone(),
            notify: self.notify.clone(),
        }
    }
}

struct ReceiverSubscription<T> {
    deliver: Box<dyn Fn(&T)>,
}

crate::object_pool_index!(struct ReceiverSubscriptionIndex);

struct ReceiverSubscriptionList<T> {
    subscriptions: ArcMutex<ObjectPool<ReceiverSubscription<T>, ReceiverSubscriptionIndex>>,
    to_be_removed: ArcMutex<Vec<ReceiverSubscriptionIndex>>,
}

impl<T> Clone for ReceiverSubscriptionList<T> {
    fn clone(&self) -> Self {
        Self {
            subscriptions: self.subscriptions.clone(),
            to_be_removed: self.to_be_removed.clone(),
        }
    }
}

pub struct Sender<T> {
    receiver_subscriptions: ReceiverSubscriptionList<T>,
    usage_counter: UsageCounter,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            receiver_subscriptions: self.receiver_subscriptions.clone(),
            usage_counter: self.usage_counter.clone(),
        }
    }
}

pub struct Source<T: 'static> {
    receiver: Receiver<T, ()>,
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self {
            receiver: self.receiver.clone(),
        }
    }
}

pub struct Receiver<T, TargetType>
where
    T: 'static,
    TargetType: 'static,
{
    receiver_subscriptions: ReceiverSubscriptionList<T>,
    queue_id: ReceiverSubscriptionIndex,
    queue: ReceiverQueue<TargetType>,
    filter_map_fn: Arc<dyn Fn(&T) -> Option<TargetType>>,
    usage_counter_watcher: UsageCounterWatcher,
}

impl<T> ReceiverQueue<T> {
    fn new() -> Self {
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

impl<T> ReceiverSubscriptionList<T> {
    fn new() -> Self {
        Self {
            subscriptions: arc_mutex_new(ObjectPool::new()),
            to_be_removed: arc_mutex_new(Vec::new()),
        }
    }

    fn create_mapped_receiver<TargetType>(
        &self,
        usage_counter_watcher: UsageCounterWatcher,
        filter_map_fn: Arc<dyn Fn(&T) -> Option<TargetType>>,
    ) -> Receiver<T, TargetType>
    where
        T: 'static,
        TargetType: 'static,
    {
        let target_queue = ReceiverQueue::<TargetType>::new();
        let delivery_queue = target_queue.clone();
        let delivery_filter_map_fn = filter_map_fn.clone();
        let queue_id = self
            .subscriptions
            .lock()
            .create_object(ReceiverSubscription {
                deliver: Box::new(move |object| {
                    if let Some(target) = delivery_filter_map_fn(object) {
                        delivery_queue.add_object_if_not_stopped(target);
                    }
                }),
            });
        Receiver {
            receiver_subscriptions: self.clone(),
            queue_id,
            queue: target_queue,
            filter_map_fn,
            usage_counter_watcher,
        }
    }

    fn handle_to_be_removed(&self) {
        let mut receiver_subscriptions_guard = self.subscriptions.lock();

        let mut to_be_removed_guard = self.to_be_removed.lock();
        while let Some(id) = to_be_removed_guard.pop() {
            receiver_subscriptions_guard.release_object(id);
        }
    }
}

impl<T> Default for Sender<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Sender<T> {
    pub fn new() -> Self {
        Self {
            receiver_subscriptions: ReceiverSubscriptionList::new(),
            usage_counter: UsageCounter::new(),
        }
    }

    pub fn send(&self, object: T) {
        self.receiver_subscriptions.handle_to_be_removed();
        for receiver in self.receiver_subscriptions.subscriptions.lock().iter() {
            (receiver.deliver)(&object);
        }
    }

    pub fn send_directly<TargetType>(&self, object: T, receiver: &Receiver<T, TargetType>)
    where
        TargetType: 'static,
    {
        if let Some(target) = (receiver.filter_map_fn)(&object) {
            receiver.queue.add_object_if_not_stopped(target);
        }
    }

    pub fn create_mapped_receiver<TargetType>(
        &self,
        filter_map_fn: impl Fn(&T) -> Option<TargetType> + 'static,
    ) -> Receiver<T, TargetType>
    where
        T: 'static,
        TargetType: 'static,
    {
        self.receiver_subscriptions
            .create_mapped_receiver(self.usage_counter.watcher(), Arc::new(filter_map_fn))
    }

    pub fn create_source(&self) -> Source<T> {
        let mut receiver = self.create_mapped_receiver(|_| None);
        receiver.stop();
        Source { receiver }
    }
}

impl<T> Sender<T>
where
    T: Clone + 'static,
{
    pub fn create_receiver(&self) -> Receiver<T, T> {
        self.create_mapped_receiver(|object| Some(object.clone()))
    }
}

impl<T> Source<T> {
    pub fn create_mapped_receiver<TargetType>(
        &self,
        filter_map_fn: impl Fn(&T) -> Option<TargetType> + 'static,
    ) -> Receiver<T, TargetType> {
        self.receiver.receiver_subscriptions.create_mapped_receiver(
            self.receiver.usage_counter_watcher.clone(),
            Arc::new(filter_map_fn),
        )
    }
}

impl<T> Source<T>
where
    T: Clone + 'static,
{
    pub fn create_receiver(&self) -> Receiver<T, T> {
        self.create_mapped_receiver(|object| Some(object.clone()))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SenderDropped;

impl<T, TargetType> Receiver<T, TargetType>
where
    T: 'static,
    TargetType: 'static,
{
    pub fn stop(&mut self) {
        *self.queue.is_stopped.lock() = true;
    }

    pub fn resume(&mut self) {
        *self.queue.is_stopped.lock() = false;
    }

    pub fn try_pop(&self) -> Result<Option<TargetType>, SenderDropped> {
        if let Some(object) = self.queue.queue.lock().pop_front() {
            Ok(Some(object))
        } else if self.usage_counter_watcher.is_observed_dropped() {
            Err(SenderDropped)
        } else {
            Ok(None)
        }
    }

    pub async fn pop(&self) -> Result<TargetType, SenderDropped> {
        loop {
            if let Some(object) = self.try_pop()? {
                break Ok(object);
            } else {
                self.queue.notify.notified().await;
            }
        }
    }
}

impl<T, TargetType> Clone for Receiver<T, TargetType>
where
    T: 'static,
    TargetType: 'static,
{
    fn clone(&self) -> Self {
        self.receiver_subscriptions.create_mapped_receiver(
            self.usage_counter_watcher.clone(),
            self.filter_map_fn.clone(),
        )
    }
}

impl<T, TargetType> Drop for Receiver<T, TargetType>
where
    T: 'static,
    TargetType: 'static,
{
    fn drop(&mut self) {
        self.receiver_subscriptions
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
    fn send_to_mapped_receiver() {
        let sender = Sender::<String>::new();
        let receiver = sender.create_mapped_receiver(|object| {
            object.parse::<usize>().ok().filter(|value| value % 2 == 0)
        });

        sender.send("1".to_string());
        sender.send("2".to_string());
        sender.send_directly("3".to_string(), &receiver);
        sender.send_directly("4".to_string(), &receiver);

        assert_eq!(receiver.try_pop().unwrap(), Some(2));
        assert_eq!(receiver.try_pop().unwrap(), Some(4));
        assert_eq!(receiver.try_pop().unwrap(), None);
    }

    #[test]
    fn cloned_mapped_receiver_preserves_filter_map() {
        let sender = Sender::<String>::new();
        let receiver0 = sender
            .create_mapped_receiver(|object| object.strip_prefix("id:").map(str::to_uppercase));
        let receiver1 = receiver0.clone();

        sender.send("id:one".to_string());
        sender.send("ignored".to_string());

        assert_eq!(receiver0.try_pop().unwrap(), Some("ONE".to_string()));
        assert_eq!(receiver1.try_pop().unwrap(), Some("ONE".to_string()));
        assert_eq!(receiver0.try_pop().unwrap(), None);
        assert_eq!(receiver1.try_pop().unwrap(), None);
    }

    #[test]
    fn mapped_receiver_stop_and_resume() {
        let sender = Sender::<usize>::new();
        let mut receiver = sender.create_mapped_receiver(|value| value.checked_mul(2));

        sender.send(1);
        receiver.stop();
        sender.send(2);
        receiver.resume();
        sender.send(3);

        assert_eq!(receiver.try_pop().unwrap(), Some(2));
        assert_eq!(receiver.try_pop().unwrap(), Some(6));
        assert_eq!(receiver.try_pop().unwrap(), None);
    }

    #[test]
    fn mapped_receiver_supports_non_clone_target() {
        struct NonCloneTarget(usize);

        let sender = Sender::<usize>::new();
        let receiver = sender.create_mapped_receiver(|value| Some(NonCloneTarget(*value)));

        sender.send(7);

        assert_eq!(receiver.try_pop().unwrap().unwrap().0, 7);
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

        assert_eq!(sender.receiver_subscriptions.subscriptions.lock().len(), 0);
    }

    #[tokio::test]
    async fn drop_sender() {
        let (receiver0, receiver1, source) = {
            let sender = Sender::<usize>::new();
            let source = sender.create_source();
            let ret = (sender.create_receiver(), source.create_receiver(), source);

            sender.send(7);

            ret
        };

        let receiver2 = source.create_receiver();

        assert_eq!(receiver0.try_pop().unwrap(), Some(7));
        assert_eq!(receiver1.try_pop().unwrap(), Some(7));

        assert_eq!(receiver0.pop().await, Err(SenderDropped));
        assert_eq!(receiver1.try_pop(), Err(SenderDropped));
        assert_eq!(receiver2.try_pop(), Err(SenderDropped));
    }
}
