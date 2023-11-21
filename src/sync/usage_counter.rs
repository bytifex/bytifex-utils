use std::sync::Arc;

#[derive(Clone)]
pub struct UsageCounter(Arc<()>);

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
}

#[cfg(test)]
mod tests {
    use super::UsageCounter;

    #[test]
    fn usage_counter() {
        let usage_counter = UsageCounter::new();

        assert_eq!(usage_counter.number_of_usages(), 1);
        assert!(usage_counter.is_this_the_last());

        {
            let usage_counter = usage_counter.clone();

            assert_eq!(usage_counter.number_of_usages(), 2);
            assert!(!usage_counter.is_this_the_last());
        }

        assert_eq!(usage_counter.number_of_usages(), 1);
        assert!(usage_counter.is_this_the_last());
    }
}
