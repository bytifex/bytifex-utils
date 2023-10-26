use crate::containers::object_pool::{ObjectPool, ObjectPoolIndex};

use super::types::{arc_mutex_new, ArcMutex};

type BoxedCallback<T> = Box<dyn FnMut(&T) + Send>;

pub struct Subscription<T> {
    callback_index: ObjectPoolIndex,
    callbacks: ArcMutex<ObjectPool<BoxedCallback<T>>>,
}

#[derive(Clone)]
pub struct Sender<T> {
    callbacks: ArcMutex<ObjectPool<BoxedCallback<T>>>,
}

pub struct Subscriber<T> {
    callbacks: ArcMutex<ObjectPool<BoxedCallback<T>>>,
}

impl<T> Default for Sender<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Sender<T> {
    pub fn new() -> Self {
        Self {
            callbacks: arc_mutex_new(ObjectPool::new()),
        }
    }

    pub fn create_subscriber(&self) -> Subscriber<T> {
        Subscriber {
            callbacks: self.callbacks.clone(),
        }
    }

    pub fn trigger(&self, obj: &T) {
        for f in self.callbacks.lock().iter_mut() {
            (f)(obj);
        }
    }
}

impl<T> Subscriber<T> {
    pub fn subscribe(&self, f: impl FnMut(&T) + Send + 'static) -> Subscription<T> {
        let index = self.callbacks.lock().create_object(Box::new(f));
        Subscription {
            callback_index: index,
            callbacks: self.callbacks.clone(),
        }
    }
}

impl<T> Drop for Subscription<T> {
    fn drop(&mut self) {
        self.callbacks
            .lock()
            .release_object(self.callback_index.invalidate());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn subscribe_and_trigger() {
        let sender = Sender::<usize>::new();
        let subscriber = sender.create_subscriber();

        let counter0 = Arc::new(AtomicUsize::new(0));
        let counter1 = Arc::new(AtomicUsize::new(0));

        let counter0_clone = counter0.clone();
        let _subscription0 = subscriber.subscribe(move |obj_ref| {
            assert_eq!(counter0_clone.fetch_add(1, Ordering::Relaxed), *obj_ref)
        });

        let counter1_clone = counter1.clone();
        let _subscription1 = subscriber.subscribe(move |obj_ref| {
            assert_eq!(counter1_clone.fetch_add(1, Ordering::Relaxed), *obj_ref)
        });

        counter0.store(0, Ordering::Relaxed);
        counter1.store(0, Ordering::Relaxed);
        {
            sender.trigger(&0);
            sender.trigger(&1);
            sender.trigger(&2);
            sender.trigger(&3);
            sender.trigger(&4);
        }
        assert_eq!(counter0.load(Ordering::Relaxed), 5);
        assert_eq!(counter1.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn drop_subscription() {
        let sender = Sender::<usize>::new();
        let subscriber = sender.create_subscriber();

        let counter = Arc::new(AtomicUsize::new(0));

        let counter_clone = counter.clone();
        let subscription = subscriber.subscribe(move |obj_ref| {
            assert_eq!(counter_clone.fetch_add(1, Ordering::Relaxed), *obj_ref)
        });

        // all events have to reach the subscribed function
        counter.store(0, Ordering::Relaxed);
        {
            sender.trigger(&0);
            sender.trigger(&1);
            sender.trigger(&2);
            sender.trigger(&3);
            sender.trigger(&4);
        }
        assert_eq!(counter.load(Ordering::Relaxed), 5);

        // drop subscription
        std::mem::drop(subscription);

        // none of the events should reach the subscribed function
        counter.store(0, Ordering::Relaxed);
        {
            sender.trigger(&0);
            sender.trigger(&1);
            sender.trigger(&2);
            sender.trigger(&3);
            sender.trigger(&4);
        }
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
