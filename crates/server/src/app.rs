//! Process-wide application state shared by every HTTP and WebSocket handler.

use crate::agents::AgentManager;
use crate::config::Config;
use crate::drafts::DraftHub;
use crate::events::EventStore;
use crate::presence::PresenceHub;
use crate::projection::Projection;
use sqlx::SqlitePool;
use std::sync::Arc;
use threadbase_shared::now_ms;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: SqlitePool,
    pub events: EventStore,
    pub projection: Projection,
    pub presence: PresenceHub,
    pub drafts: DraftHub,
    pub agents: AgentManager,
    pub started_at: i64,
}

impl AppState {
    #[must_use]
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        let events = EventStore::new(pool.clone());
        let (state_tx, _) = broadcast::channel(256);
        let projection = Projection::new(pool.clone(), state_tx);
        Self {
            agents: AgentManager::new(pool.clone(), events.clone(), projection.clone()),
            drafts: DraftHub::new(pool.clone()),
            events,
            projection,
            presence: PresenceHub::new(),
            started_at: now_ms(),
            config: Arc::new(config),
            pool,
        }
    }
}
