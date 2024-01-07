use std::sync::{Arc, Weak};

#[derive(Clone)]
pub struct UsageCounter(Arc<()>);

#[derive(Clone)]
pub struct UsageCounterWatcher(Weak<()>);

impl Default for UsageCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageCounter {
    pub fn new() -> Self {
        Self(Arc::new(()))
    }

    pub fn is_this_the_last(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }

    pub fn number_of_usages(&self) -> usize {
        Arc::strong_count(&self.0)
    }

    pub fn watcher(&self) -> UsageCounterWatcher {
        UsageCounterWatcher(Arc::downgrade(&self.0))
    }
}

impl UsageCounterWatcher {
    pub fn is_observed_the_last(&self) -> bool {
        Weak::strong_count(&self.0) == 1
    }

    pub fn is_observed_dropped(&self) -> bool {
        Weak::strong_count(&self.0) == 0
    }

    pub fn number_of_usages(&self) -> usize {
        Weak::strong_count(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::UsageCounter;

    #[test]
    fn usage_counter() {
        let usage_counter = UsageCounter::new();

        let watcher0 = usage_counter.watcher();
        let watcher1 = usage_counter.watcher();

        assert_eq!(usage_counter.number_of_usages(), 1);
        assert!(usage_counter.is_this_the_last());

        assert_eq!(watcher0.number_of_usages(), 1);
        assert!(watcher0.is_observed_the_last());
        assert!(!watcher0.is_observed_dropped());

        assert_eq!(watcher1.number_of_usages(), 1);
        assert!(watcher1.is_observed_the_last());
        assert!(!watcher1.is_observed_dropped());

        {
            let usage_counter = usage_counter.clone();

            assert_eq!(usage_counter.number_of_usages(), 2);
            assert!(!usage_counter.is_this_the_last());

            assert_eq!(watcher0.number_of_usages(), 2);
            assert!(!watcher0.is_observed_the_last());
            assert!(!watcher0.is_observed_dropped());

            assert_eq!(watcher1.number_of_usages(), 2);
            assert!(!watcher1.is_observed_the_last());
            assert!(!watcher1.is_observed_dropped());
        }

        assert_eq!(usage_counter.number_of_usages(), 1);
        assert!(usage_counter.is_this_the_last());

        assert_eq!(watcher0.number_of_usages(), 1);
        assert!(watcher0.is_observed_the_last());
        assert!(!watcher0.is_observed_dropped());

        assert_eq!(watcher1.number_of_usages(), 1);
        assert!(watcher1.is_observed_the_last());
        assert!(!watcher1.is_observed_dropped());

        drop(usage_counter);

        assert_eq!(watcher0.number_of_usages(), 0);
        assert!(!watcher0.is_observed_the_last());
        assert!(watcher0.is_observed_dropped());

        assert_eq!(watcher1.number_of_usages(), 0);
        assert!(!watcher1.is_observed_the_last());
        assert!(watcher1.is_observed_dropped());
    }
}
