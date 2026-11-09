//! Append-only event log with per-thread sequencing, replay and snapshots.

use sqlx::{FromRow, SqlitePool};
use std::sync::Arc;
use threadbase_core::TimelineState;
use threadbase_shared::{
    now_ms, Actor, ActorKind, ApiError, Delivery, EventEnvelope, EventsPageDto, Origin,
    ThreadEvent, ThreadId,
};
use tokio::sync::{broadcast, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("corrupt event row: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[derive(Debug, Clone, Default)]
pub struct EventMeta {
    pub origin: Option<Origin>,
    pub subagent_id: Option<String>,
    pub parent_subagent_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub delivery: Option<Delivery>,
}

#[derive(Clone)]
pub struct EventStore {
    pool: SqlitePool,
    append_lock: Arc<Mutex<()>>,
    event_tx: broadcast::Sender<EventEnvelope>,
}

impl EventStore {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        let (event_tx, _) = broadcast::channel(4096);
        Self {
            pool,
            append_lock: Arc::new(Mutex::new(())),
            event_tx,
        }
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.event_tx.subscribe()
    }

    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn append(
        &self,
        thread_id: &ThreadId,
        actor: Actor,
        payload: ThreadEvent,
        meta: EventMeta,
    ) -> Result<EventEnvelope, EventStoreError> {
        let _guard = self.append_lock.lock().await;
        let mut tx = self.pool.begin().await?;
        let seq: i64 = sqlx::query_scalar(
            "INSERT INTO thread_counters(thread_id, last_seq) VALUES(?, 1)
             ON CONFLICT(thread_id) DO UPDATE SET last_seq = last_seq + 1
             RETURNING last_seq",
        )
        .bind(thread_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let seq =
            u64::try_from(seq).map_err(|_| EventStoreError::Corrupt("negative seq".into()))?;
        let ts = now_ms();
        let envelope = EventEnvelope {
            seq,
            thread_id: thread_id.clone(),
            ts,
            actor: actor.clone(),
            origin: meta.origin.unwrap_or(Origin::Local),
            subagent_id: meta.subagent_id.as_deref().map(Into::into),
            parent_subagent_id: meta.parent_subagent_id.as_deref().map(Into::into),
            tool_call_id: meta.tool_call_id.as_deref().map(Into::into),
            delivery: meta.delivery,
            payload: payload.clone(),
        };
        let payload_json = serde_json::to_string(&payload)?;
        sqlx::query(
            "INSERT INTO events(
                thread_id, seq, ts, actor_kind, actor_id, actor_name, origin,
                subagent_id, parent_subagent_id, tool_call_id, delivery, kind, payload_json
             ) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(thread_id.as_str())
        .bind(i64::try_from(seq).unwrap_or(i64::MAX))
        .bind(ts)
        .bind(actor_kind_str(&actor.kind))
        .bind(actor.id.as_deref())
        .bind(actor.name.as_deref())
        .bind(origin_str(envelope.origin))
        .bind(envelope.subagent_id.as_ref().map(|id| id.as_str()))
        .bind(envelope.parent_subagent_id.as_ref().map(|id| id.as_str()))
        .bind(envelope.tool_call_id.as_ref().map(|id| id.as_str()))
        .bind(envelope.delivery.map(delivery_str))
        .bind(payload.kind())
        .bind(payload_json)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE threads SET last_activity_at = ?, updated_at = ? WHERE id = ?")
            .bind(ts)
            .bind(ts)
            .bind(thread_id.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        let _ = self.event_tx.send(envelope.clone());
        Ok(envelope)
    }

    pub async fn page(
        &self,
        thread_id: &ThreadId,
        after_seq: u64,
        limit: u32,
    ) -> Result<EventsPageDto, EventStoreError> {
        let limit = limit.clamp(1, 1000);
        let (oldest, last): (i64, i64) = sqlx::query_as(
            "SELECT COALESCE(MIN(seq), 0), COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?",
        )
        .bind(thread_id.as_str())
        .fetch_one(&self.pool)
        .await?;
        let oldest = u64::try_from(oldest).unwrap_or(0);
        let last = u64::try_from(last).unwrap_or(0);
        let resync = after_seq > 0 && oldest > 0 && after_seq + 1 < oldest;
        let rows = if after_seq == 0 || resync {
            sqlx::query_as::<_, EventRow>(
                "SELECT * FROM (
                    SELECT * FROM events WHERE thread_id = ? ORDER BY seq DESC LIMIT ?
                 ) ORDER BY seq ASC",
            )
            .bind(thread_id.as_str())
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, EventRow>(
                "SELECT * FROM events WHERE thread_id = ? AND seq > ? ORDER BY seq ASC LIMIT ?",
            )
            .bind(thread_id.as_str())
            .bind(i64::try_from(after_seq).unwrap_or(i64::MAX))
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await?
        };
        let events = rows
            .into_iter()
            .map(EventRow::into_envelope)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EventsPageDto {
            thread_id: thread_id.clone(),
            events,
            last_seq: last,
            oldest_seq: oldest,
            resync,
        })
    }

    pub async fn all_events(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Vec<EventEnvelope>, EventStoreError> {
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT * FROM events WHERE thread_id = ? ORDER BY seq ASC",
        )
        .bind(thread_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(EventRow::into_envelope)
            .collect::<Result<Vec<_>, _>>()
    }

    /// Load the latest snapshot plus the events after it.
    pub async fn load_state(&self, thread_id: &ThreadId) -> Result<TimelineState, EventStoreError> {
        let snapshot: Option<SnapshotRow> =
            sqlx::query_as("SELECT upto_seq, state_json FROM snapshots WHERE thread_id = ?")
                .bind(thread_id.as_str())
                .fetch_optional(&self.pool)
                .await?;
        let (mut state, upto_seq) = if let Some(snapshot) = snapshot {
            let state: TimelineState = serde_json::from_str(&snapshot.state_json)
                .map_err(|error| EventStoreError::Corrupt(error.to_string()))?;
            (state, u64::try_from(snapshot.upto_seq).unwrap_or(0))
        } else {
            (TimelineState::default(), 0)
        };
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT * FROM events WHERE thread_id = ? AND seq > ? ORDER BY seq ASC",
        )
        .bind(thread_id.as_str())
        .bind(i64::try_from(upto_seq).unwrap_or(0))
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            state.apply(&row.into_envelope()?);
        }
        Ok(state)
    }

    pub async fn save_snapshot(
        &self,
        thread_id: &ThreadId,
    ) -> Result<TimelineState, EventStoreError> {
        let state = self.load_state(thread_id).await?;
        let state_json = serde_json::to_string(&state)?;
        sqlx::query(
            "INSERT INTO snapshots(thread_id, upto_seq, state_json, created_at)
             VALUES(?, ?, ?, ?)
             ON CONFLICT(thread_id) DO UPDATE SET
                upto_seq = excluded.upto_seq,
                state_json = excluded.state_json,
                created_at = excluded.created_at",
        )
        .bind(thread_id.as_str())
        .bind(i64::try_from(state.last_seq).unwrap_or(i64::MAX))
        .bind(state_json)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(state)
    }

    pub async fn maybe_snapshot(
        &self,
        thread_id: &ThreadId,
        last_seq: u64,
        turn_ended: bool,
    ) -> Result<(), EventStoreError> {
        if turn_ended || (last_seq > 0 && last_seq.is_multiple_of(500)) {
            self.save_snapshot(thread_id).await?;
        }
        Ok(())
    }

    pub async fn last_seq(&self, thread_id: &ThreadId) -> Result<u64, EventStoreError> {
        let last: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?")
                .bind(thread_id.as_str())
                .fetch_one(&self.pool)
                .await?;
        Ok(u64::try_from(last).unwrap_or(0))
    }
}

#[derive(Debug, FromRow)]
struct EventRow {
    #[allow(dead_code)]
    thread_id: String,
    seq: i64,
    ts: i64,
    actor_kind: String,
    actor_id: Option<String>,
    actor_name: Option<String>,
    origin: String,
    subagent_id: Option<String>,
    parent_subagent_id: Option<String>,
    tool_call_id: Option<String>,
    delivery: Option<String>,
    #[allow(dead_code)]
    kind: String,
    payload_json: String,
}

impl EventRow {
    fn into_envelope(self) -> Result<EventEnvelope, EventStoreError> {
        let payload: ThreadEvent = serde_json::from_str(&self.payload_json)?;
        Ok(EventEnvelope {
            seq: u64::try_from(self.seq).unwrap_or(0),
            thread_id: ThreadId::from(self.thread_id),
            ts: self.ts,
            actor: Actor {
                kind: parse_actor_kind(&self.actor_kind),
                id: self.actor_id,
                name: self.actor_name,
            },
            origin: parse_origin(&self.origin),
            subagent_id: self.subagent_id.map(Into::into),
            parent_subagent_id: self.parent_subagent_id.map(Into::into),
            tool_call_id: self.tool_call_id.map(Into::into),
            delivery: self.delivery.as_deref().and_then(parse_delivery),
            payload,
        })
    }
}

#[derive(Debug, FromRow)]
struct SnapshotRow {
    upto_seq: i64,
    state_json: String,
}

fn actor_kind_str(kind: &ActorKind) -> &'static str {
    match kind {
        ActorKind::User => "user",
        ActorKind::Agent => "agent",
        ActorKind::System => "system",
    }
}

fn parse_actor_kind(value: &str) -> ActorKind {
    match value {
        "user" => ActorKind::User,
        "agent" => ActorKind::Agent,
        _ => ActorKind::System,
    }
}

fn origin_str(origin: Origin) -> &'static str {
    match origin {
        Origin::Local => "local",
        Origin::Relay => "relay",
    }
}

fn parse_origin(value: &str) -> Origin {
    match value {
        "relay" => Origin::Relay,
        _ => Origin::Local,
    }
}

fn delivery_str(delivery: Delivery) -> &'static str {
    match delivery {
        Delivery::Injected => "injected",
        Delivery::StoppedAndSent => "stopped_and_sent",
        Delivery::Queued => "queued",
        Delivery::Dropped => "dropped",
    }
}

fn parse_delivery(value: &str) -> Option<Delivery> {
    match value {
        "injected" => Some(Delivery::Injected),
        "stopped_and_sent" => Some(Delivery::StoppedAndSent),
        "queued" => Some(Delivery::Queued),
        "dropped" => Some(Delivery::Dropped),
        _ => None,
    }
}

/// Seed helpers shared by server integration tests.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use threadbase_shared::{ProjectId, UserId};

    pub async fn seed_thread(pool: &SqlitePool) -> (UserId, ProjectId, ThreadId) {
        let user_id = UserId::new();
        let project_id = ProjectId::new();
        let thread_id = ThreadId::new();
        let now = now_ms();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, is_admin, created_at) VALUES(?,?,?,?,?)",
        )
        .bind(user_id.as_str())
        .bind("tester")
        .bind("Tester")
        .bind(0)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO projects(id, name, path, default_branch, created_by, created_at, updated_at)
             VALUES(?,?,?,?,?,?,?)",
        )
        .bind(project_id.as_str())
        .bind("repo")
        .bind("/tmp/repo")
        .bind("main")
        .bind(user_id.as_str())
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch, created_by, created_at, updated_at)
             VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(thread_id.as_str())
        .bind(project_id.as_str())
        .bind("Test thread")
        .bind("idle")
        .bind("codex")
        .bind("thread/test")
        .bind("main")
        .bind(user_id.as_str())
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
        (user_id, project_id, thread_id)
    }

    pub fn notice(message: &str) -> ThreadEvent {
        ThreadEvent::SystemNotice {
            level: "info".into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::Row;
    use test_support::{notice, seed_thread};

    fn system_actor() -> Actor {
        Actor::system()
    }

    #[tokio::test]
    async fn assigns_monotonic_sequences() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let store = EventStore::new(pool);
        let mut seqs = Vec::new();
        for index in 0..5 {
            let event = store
                .append(
                    &thread,
                    system_actor(),
                    notice(&format!("n{index}")),
                    EventMeta::default(),
                )
                .await
                .unwrap();
            seqs.push(event.seq);
        }
        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn concurrent_appends_do_not_duplicate_sequences() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let store = EventStore::new(pool);
        let mut handles = Vec::new();
        for worker in 0..8 {
            let store = store.clone();
            let thread = thread.clone();
            handles.push(tokio::spawn(async move {
                let mut seqs = Vec::new();
                for index in 0..25 {
                    let event = store
                        .append(
                            &thread,
                            system_actor(),
                            notice(&format!("w{worker}-{index}")),
                            EventMeta::default(),
                        )
                        .await
                        .unwrap();
                    seqs.push(event.seq);
                }
                seqs
            }));
        }
        let mut all = Vec::new();
        for handle in handles {
            all.extend(handle.await.unwrap());
        }
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 200);
        assert_eq!(all.first(), Some(&1));
        assert_eq!(all.last(), Some(&200));
    }

    #[tokio::test]
    async fn after_seq_replay_and_resync() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let store = EventStore::new(pool.clone());
        for index in 0..5 {
            store
                .append(
                    &thread,
                    system_actor(),
                    notice(&format!("n{index}")),
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        let page = store.page(&thread, 2, 10).await.unwrap();
        assert_eq!(page.events.len(), 3);
        assert_eq!(page.events[0].seq, 3);
        assert!(!page.resync);

        sqlx::query("DELETE FROM events WHERE thread_id = ? AND seq IN (1,2)")
            .bind(thread.as_str())
            .execute(&pool)
            .await
            .unwrap();
        let page = store.page(&thread, 1, 10).await.unwrap();
        assert!(page.resync);
        assert_eq!(page.events.first().map(|event| event.seq), Some(3));
        assert_eq!(page.oldest_seq, 3);
    }

    #[tokio::test]
    async fn snapshot_restores_and_then_applies_new_events() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let store = EventStore::new(pool.clone());
        for index in 0..5 {
            store
                .append(
                    &thread,
                    system_actor(),
                    notice(&format!("n{index}")),
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        let state = store.save_snapshot(&thread).await.unwrap();
        assert_eq!(state.last_seq, 5);
        for index in 5..8 {
            store
                .append(
                    &thread,
                    system_actor(),
                    notice(&format!("n{index}")),
                    EventMeta::default(),
                )
                .await
                .unwrap();
        }
        let restored = store.load_state(&thread).await.unwrap();
        assert_eq!(restored.last_seq, 8);
        assert_eq!(restored.notices.len(), 8);
        let upto: i64 = sqlx::query("SELECT upto_seq FROM snapshots WHERE thread_id = ?")
            .bind(thread.as_str())
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("upto_seq");
        assert_eq!(upto, 5);
    }

    #[tokio::test]
    async fn latest_window_loads_for_large_thread() {
        let pool = db::connect_memory().await.unwrap();
        let (user_id, _, thread) = seed_thread(&pool).await;
        let mut tx = pool.begin().await.unwrap();
        for seq in 1..=100_000i64 {
            sqlx::query(
                "INSERT INTO events(thread_id, seq, ts, actor_kind, actor_name, origin, kind, payload_json)
                 VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(thread.as_str())
            .bind(seq)
            .bind(now_ms())
            .bind("system")
            .bind("Threadbase")
            .bind("local")
            .bind("system_notice")
            .bind(r#"{"type":"system_notice","level":"info","message":"n"}"#)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO thread_counters(thread_id, last_seq) VALUES(?, ?)")
            .bind(thread.as_str())
            .bind(100_000i64)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let _ = user_id;
        let store = EventStore::new(pool);
        let page = store.page(&thread, 0, 200).await.unwrap();
        assert_eq!(page.events.len(), 200);
        assert_eq!(page.events[0].seq, 99_801);
        assert_eq!(page.last_seq, 100_000);
        assert!(!page.resync);
        let page = store.page(&thread, 99_900, 200).await.unwrap();
        assert_eq!(page.events.len(), 100);
        assert_eq!(page.events[0].seq, 99_901);
    }
}
