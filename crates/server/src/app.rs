//! Process-wide application state shared by every HTTP and WebSocket handler.

use crate::agents::AgentManager;
use crate::config::Config;
use crate::drafts::DraftHub;
use crate::events::EventStore;
use crate::presence::PresenceHub;
use crate::projection::Projection;
use crate::push::PushHub;
use crate::terminal::TerminalHub;
use sqlx::SqlitePool;
use std::sync::Arc;
use threadbase_core::permissions::PermissionPolicy;
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
    pub push: Arc<PushHub>,
    pub terminal: Arc<TerminalHub>,
    pub started_at: i64,
    /// Permission defaults and ceiling, resolved into every thread's DTO and
    /// enforced on every agent permission request.
    pub policy: PermissionPolicy,
}

impl AppState {
    #[must_use]
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        if config.terminal_host {
            tracing::warn!(
                "THREADBASE_TERMINAL_HOST is on: every member can attach to the host's tmux sessions"
            );
        }
        let policy = PermissionPolicy::new(config.permission_mode, config.max_permission_mode);
        let events = EventStore::new(pool.clone());
        let (state_tx, _) = broadcast::channel(256);
        let projection = Projection::new(pool.clone(), state_tx);
        let terminal = Arc::new(TerminalHub::new(
            &config.data_dir,
            &config.terminal_shell,
            &config.terminal_socket,
        ));
        let presence = PresenceHub::new();
        let push = Arc::new(PushHub::new(pool.clone()));
        let agents = AgentManager::new(
            pool.clone(),
            events.clone(),
            projection.clone(),
            presence.clone(),
            config.idle_session_seconds,
            config.max_active_turns,
            policy,
        );
        Self {
            agents,
            push,
            terminal,
            drafts: DraftHub::new(pool.clone()),
            events,
            projection,
            presence,
            started_at: now_ms(),
            config: Arc::new(config),
            policy,
            pool,
        }
    }
}
