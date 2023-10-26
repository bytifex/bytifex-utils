use std::sync::Arc;

use tokio::sync::watch::{self, Receiver, Sender};

#[derive(Clone)]
pub struct AppLoopState(Arc<Sender<bool>>);

#[derive(Clone)]
pub struct AppLoopStateWatcher(Receiver<bool>);

impl Default for AppLoopState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppLoopState {
    pub fn new() -> Self {
        let (sender, _receiver) = watch::channel(true);

        Self(Arc::new(sender))
    }

    pub fn watcher(&self) -> AppLoopStateWatcher {
        AppLoopStateWatcher(self.0.subscribe())
    }

    pub fn stop_loop(&self) {
        let _ = self.0.send(false);
    }

    pub fn should_run(&self) -> bool {
        *self.0.borrow()
    }
}

impl AppLoopStateWatcher {
    pub fn should_run(&self) -> bool {
        *self.0.borrow()
    }

    pub async fn wait_for_quit(&self) {
        let mut run_loop = self.0.clone();
        while let Ok(()) = run_loop.changed().await {
            if !*run_loop.borrow() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::{sleep_until, Instant};

    use super::AppLoopState;

    #[tokio::test(flavor = "multi_thread")]
    async fn quit() {
        let state = AppLoopState::new();
        let state_watcher = state.watcher();

        tokio::spawn(async move {
            state.stop_loop();
        });

        let timeout = Duration::from_secs(1);
        let timestamp = Instant::now() + timeout;
        loop {
            tokio::select! {
                _ = sleep_until(timestamp) => {
                    break;
                }
                _ = state_watcher.wait_for_quit() => {
                    break;
                }
            }
        }
    }
}
