//! Shared server state.

use crate::config::Config;
use crate::drafts::DraftHub;
use crate::events::EventStore;
use crate::presence::PresenceHub;
use crate::rate_limit::RateLimiter;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: SqlitePool,
    pub events: EventStore,
    pub started_at: i64,
    pub login_limiter: RateLimiter,
    pub presence: PresenceHub,
    pub drafts: DraftHub,
}

impl AppState {
    #[must_use]
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        Self {
            config: Arc::new(config),
            events: EventStore::new(pool.clone()),
            drafts: DraftHub::new(pool.clone()),
            pool,
            started_at: threadbase_shared::now_ms(),
            login_limiter: RateLimiter::new(),
            presence: PresenceHub::new(),
        }
    }
}
