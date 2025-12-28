use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::containers::object_pool::{ObjectPool, ObjectPoolIndex};

use super::types::{ArcMutex, arc_mutex_new};

type ObserverFunction<T> = Box<dyn Fn(&T)>;

pub struct Observable<T> {
    value: T,
    observers: ObjectPool<ObserverFunction<T>>,
    to_be_deleted_observers: ArcMutex<Vec<ObjectPoolIndex>>,
}

pub struct ObservableBorrower<'a, T> {
    observable: &'a mut Observable<T>,
}

pub struct Observer<T> {
    observer_index: ObjectPoolIndex,
    to_be_deleted_observers: ArcMutex<Vec<ObjectPoolIndex>>,
    _phantom: PhantomData<T>,
}

impl<T> Observable<T> {
    pub fn new(initial_value: T) -> Self {
        Self {
            value: initial_value,
            observers: ObjectPool::new(),
            to_be_deleted_observers: arc_mutex_new(Vec::new()),
        }
    }

    pub fn set(&mut self, value: T) {
        self.value = value;
        self.trigger_observers();
    }

    pub fn get_ref(&self) -> &T {
        &self.value
    }

    pub fn borrow_mut(&mut self) -> ObservableBorrower<'_, T> {
        ObservableBorrower { observable: self }
    }

    pub fn observe(&mut self, function: impl Fn(&T) + 'static) -> Observer<T> {
        let observer_index = self.observers.create_object(Box::new(function));

        Observer {
            observer_index,
            to_be_deleted_observers: self.to_be_deleted_observers.clone(),
            _phantom: PhantomData,
        }
    }

    fn trigger_observers(&mut self) {
        {
            let mut to_be_deleted_observers = self.to_be_deleted_observers.lock();

            for index in to_be_deleted_observers.iter() {
                self.observers.release_object(*index);
            }
            to_be_deleted_observers.clear();
        }

        for observer in self.observers.iter() {
            observer(&self.value);
        }
    }
}

impl<T> Deref for Observable<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> Drop for Observer<T> {
    fn drop(&mut self) {
        self.to_be_deleted_observers
            .lock()
            .push(self.observer_index);
    }
}

impl<T> Drop for ObservableBorrower<'_, T> {
    fn drop(&mut self) {
        self.observable.trigger_observers();
    }
}

impl<T> Deref for ObservableBorrower<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.observable.value
    }
}

impl<T> DerefMut for ObservableBorrower<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.observable.value
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use closure::closure;
    use parking_lot::RwLock;

    use crate::sync::observable_fn::Observable;

    #[test]
    fn observable_borrow() {
        let observer_value_received = Arc::new(RwLock::new(None));

        let mut observable = Observable::new(0);

        let _observer = observable.observe(closure!(
            clone observer_value_received,
            |new_value| {
                *observer_value_received.write() = Some(*new_value);
            }
        ));

        {
            let mut borrower = observable.borrow_mut();
            assert_eq!(*borrower, 0);
            *borrower = 1;
        }
        assert_eq!(*observable.get_ref(), 1);
        assert_eq!(*observer_value_received.read(), Some(1));
    }

    #[test]
    fn observable_set() {
        let observer0_value_received = Arc::new(RwLock::new(None));
        let observer1_value_received = Arc::new(RwLock::new(None));

        let mut observable = Observable::new(0);

        let _observer0 = observable.observe(closure!(
            clone observer0_value_received,
            |new_value| {
                *observer0_value_received.write() = Some(*new_value);
            }
        ));

        observable.set(1);
        assert_eq!(*observable.get_ref(), 1);
        assert_eq!(*observer0_value_received.read(), Some(1));
        assert_eq!(*observer1_value_received.read(), None);

        {
            let _observer1 = observable.observe(closure!(
                clone observer1_value_received,
                |new_value| {
                    *observer1_value_received.write() = Some(*new_value);
                }
            ));

            observable.set(2);
            assert_eq!(*observable.get_ref(), 2);
            assert_eq!(*observer0_value_received.read(), Some(2));
            assert_eq!(*observer1_value_received.read(), Some(2));
        }

        observable.set(3);
        assert_eq!(*observable.get_ref(), 3);
        assert_eq!(*observer0_value_received.read(), Some(3));
        assert_eq!(*observer1_value_received.read(), Some(2));
    }
}
