use tokio::sync::Notify as TokioNotify;

pub struct Notify {
    inner: TokioNotify,
}

impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}

impl Notify {
    pub fn new() -> Self {
        Self {
            inner: TokioNotify::new(),
        }
    }
    pub fn notify_waiters(&self) {
        self.inner.notify_waiters();
    }

    pub async fn notified(&self) {
        self.inner.notified().await
    }
}
