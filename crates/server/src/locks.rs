//! Per-key asynchronous locks.
//!
//! Both the event log and the thread-state projection need to serialize work
//! for one thread without blocking unrelated threads. Keys are thread ids, so
//! the map stays proportional to the number of live conversations.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Default)]
pub struct KeyedLocks {
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl KeyedLocks {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire the lock for `key`, creating it on first use.
    pub async fn lock(&self, key: &str) -> OwnedMutexGuard<()> {
        let lock = self
            .locks
            .lock()
            .await
            .entry(key.to_owned())
            .or_default()
            .clone();
        lock.lock_owned().await
    }
}
