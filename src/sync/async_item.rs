//! # Example
//! ```
//! use bytifex_utils::sync::async_item::AsyncItem;
//!
//! # #[tokio::main]
//! # async fn main() {
//! let item = AsyncItem::new();
//! item.set(7).await;
//! assert_eq!(7, *item.read().await);
//! # }
//! ```

use std::{ops::Deref, sync::Arc};

use tokio::sync::Notify;

use super::types::{arc_rw_lock_new, ArcRwLock, RwLockReadGuard};

pub struct AsyncItem<T: Send> {
    value: ArcRwLock<Option<T>>,
    // todo!("use an async condvar")
    notify: Arc<Notify>,
}

impl<T: Send> Clone for AsyncItem<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            notify: self.notify.clone(),
        }
    }
}

pub struct AsyncItemReadGuard<'a, T: Send> {
    inner: RwLockReadGuard<'a, Option<T>>,
}

impl<T: Send> Deref for AsyncItemReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        if let Some(value) = self.inner.as_ref() {
            value
        } else {
            unreachable!()
        }
    }
}

impl<T: Send> Default for AsyncItem<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send> AsyncItem<T> {
    pub fn new() -> Self {
        Self {
            value: arc_rw_lock_new(None),
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn unset(&self) {
        let mut value_guard = self.value.write();
        *value_guard = None;
        self.notify.notify_waiters();
        drop(value_guard);
    }

    pub async fn set(&self, value: T) {
        let mut value_guard = self.value.write();
        *value_guard = Some(value);
        self.notify.notify_waiters();
        drop(value_guard);
    }

    pub async fn read(&self) -> AsyncItemReadGuard<T> {
        loop {
            if let Some(guard) = self.try_read() {
                break guard;
            }

            self.notify.notified().await;
        }
    }

    pub fn try_read(&self) -> Option<AsyncItemReadGuard<T>> {
        let value_guard = self.value.read();
        if value_guard.is_some() {
            Some(AsyncItemReadGuard { inner: value_guard })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use tokio::time::sleep;

    use super::AsyncItem;

    #[tokio::test(flavor = "multi_thread")]
    async fn set_then_multiple_get() {
        let item = AsyncItem::new();

        item.set(7).await;

        assert_eq!(7, *item.read().await);
        assert_eq!(7, *item.read().await);
        assert_eq!(7, *item.read().await);
        assert_eq!(7, *item.read().await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn multiple_gets_then_set() {
        let item = Arc::new(AsyncItem::new());

        let mut tasks = Vec::new();
        for _ in 0..100 {
            let item = item.clone();
            tasks.push(tokio::spawn(async move {
                assert_eq!(7, *item.read().await);
            }));
        }

        sleep(Duration::from_millis(200)).await;

        item.set(7).await;

        let join_task = async move {
            for task in tasks {
                task.await.unwrap();
            }
        };

        tokio::select! {
            _ = sleep(Duration::from_secs(2)) => {
                assert!(false);
            }
            _ = join_task => {
            }
        }
    }
}
