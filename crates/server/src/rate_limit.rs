//! Small in-process rate limiter for authentication and write endpoints.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

impl RateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `false` when the key has exhausted its allowance.
    pub fn check(&self, key: &str, max: usize, window: Duration) -> bool {
        let now = Instant::now();
        let mut buckets = self
            .buckets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let bucket = buckets.entry(key.to_owned()).or_default();
        while bucket
            .front()
            .is_some_and(|instant| now.duration_since(*instant) > window)
        {
            bucket.pop_front();
        }
        if bucket.len() >= max {
            return false;
        }
        bucket.push_back(now);
        if buckets.len() > 10_000 {
            buckets.retain(|_, bucket| {
                bucket
                    .back()
                    .is_some_and(|instant| now.duration_since(*instant) <= window)
            });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_within_window() {
        let limiter = RateLimiter::new();
        assert!(limiter.check("k", 2, Duration::from_secs(60)));
        assert!(limiter.check("k", 2, Duration::from_secs(60)));
        assert!(!limiter.check("k", 2, Duration::from_secs(60)));
        assert!(limiter.check("other", 2, Duration::from_secs(60)));
    }
}
