//! Per-thread collaborative draft rooms over the Yjs sync protocol.
//!
//! [`DraftHub`] owns room lifecycle and persistence, [`room`] owns the CRDT
//! state and y-sync protocol handling, and [`socket`] carries the WebSocket
//! transport.

mod room;
mod socket;

pub use socket::handler;

use crate::locks::KeyedLocks;
use room::DraftRoom;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use threadbase_draft::DraftDoc;
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock};

#[derive(Clone)]
pub struct DraftHub {
    pool: SqlitePool,
    rooms: Arc<RwLock<HashMap<String, Arc<Mutex<DraftRoom>>>>>,
    /// Serializes first loads per thread so one room is created per thread.
    load_locks: KeyedLocks,
    /// Serializes the peek → send → take sequence per thread.
    send_locks: KeyedLocks,
}

impl DraftHub {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            rooms: Arc::new(RwLock::new(HashMap::new())),
            load_locks: KeyedLocks::new(),
            send_locks: KeyedLocks::new(),
        }
    }

    pub(crate) async fn room(
        &self,
        thread_id: &str,
    ) -> Result<Arc<Mutex<DraftRoom>>, anyhow::Error> {
        if let Some(room) = self.rooms.read().await.get(thread_id).cloned() {
            return Ok(room);
        }
        // The global map lock is only held for the lookup and insert; database
        // IO happens under the per-thread load lock.
        let _load = self.load_locks.lock(thread_id).await;
        if let Some(room) = self.rooms.read().await.get(thread_id).cloned() {
            return Ok(room);
        }
        let draft = self.load_draft(thread_id).await?;
        let room = Arc::new(Mutex::new(DraftRoom::new(draft)));
        let mut rooms = self.rooms.write().await;
        Ok(rooms.entry(thread_id.to_owned()).or_insert(room).clone())
    }

    async fn load_draft(&self, thread_id: &str) -> Result<DraftDoc, anyhow::Error> {
        let snapshot: Option<(i64, Vec<u8>)> =
            sqlx::query_as("SELECT upto_seq, state_blob FROM draft_snapshots WHERE thread_id = ?")
                .bind(thread_id)
                .fetch_optional(&self.pool)
                .await?;
        let (upto_seq, blob) = snapshot.unwrap_or_default();
        let mut draft = DraftDoc::from_snapshot(&blob)?;
        let updates: Vec<(Vec<u8>,)> = sqlx::query_as(
            "SELECT update_blob FROM draft_updates WHERE thread_id = ? AND seq > ? ORDER BY seq ASC",
        )
        .bind(thread_id)
        .bind(upto_seq)
        .fetch_all(&self.pool)
        .await?;
        for (update,) in updates {
            draft.apply_update(&update)?;
        }
        Ok(draft)
    }

    pub async fn persist_update(
        &self,
        thread_id: &str,
        update: &[u8],
    ) -> Result<(), anyhow::Error> {
        let seq: i64 = sqlx::query_scalar(
            "INSERT INTO draft_updates(thread_id, seq, update_blob, created_at)
             VALUES(?, (SELECT COALESCE(MAX(seq), 0) + 1 FROM draft_updates WHERE thread_id = ?), ?, ?)
             RETURNING seq",
        )
        .bind(thread_id)
        .bind(thread_id)
        .bind(update)
        .bind(threadbase_shared::now_ms())
        .fetch_one(&self.pool)
        .await?;
        if seq % 200 == 0 {
            self.snapshot(thread_id, u64::try_from(seq).unwrap_or(0))
                .await?;
        }
        Ok(())
    }

    pub async fn snapshot(&self, thread_id: &str, upto_seq: u64) -> Result<(), anyhow::Error> {
        let room = self.room(thread_id).await?;
        let full = {
            let room = room.lock().await;
            room.encode_full()
        };
        sqlx::query(
            "INSERT INTO draft_snapshots(thread_id, upto_seq, state_blob, updated_at)
             VALUES(?, ?, ?, ?)
             ON CONFLICT(thread_id) DO UPDATE SET
                upto_seq = excluded.upto_seq,
                state_blob = excluded.state_blob,
                updated_at = excluded.updated_at",
        )
        .bind(thread_id)
        .bind(i64::try_from(upto_seq).unwrap_or(i64::MAX))
        .bind(full)
        .bind(threadbase_shared::now_ms())
        .execute(&self.pool)
        .await?;
        sqlx::query("DELETE FROM draft_updates WHERE thread_id = ? AND seq <= ?")
            .bind(thread_id)
            .bind(i64::try_from(upto_seq).unwrap_or(i64::MAX))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Serialize the peek → send → take sequence for one thread so two
    /// senders cannot consume the same draft blocks.
    pub async fn send_guard(&self, thread_id: &str) -> OwnedMutexGuard<()> {
        self.send_locks.lock(thread_id).await
    }

    /// Non-destructive read of the shared draft: text plus editor ids.
    pub async fn peek_draft(
        &self,
        thread_id: &str,
    ) -> Result<(String, Vec<String>), anyhow::Error> {
        let room = self.room(thread_id).await?;
        let draft = room.lock().await.peek_draft();
        Ok(draft)
    }

    /// Drop a room so the next access reloads from persisted state.
    pub async fn drop_room(&self, thread_id: &str) {
        self.rooms.write().await.remove(thread_id);
    }

    /// Whether `room` is still the canonical room for the thread. Orphaned
    /// rooms (dropped after a limit violation) must not persist updates.
    pub(crate) async fn is_canonical(&self, thread_id: &str, room: &Arc<Mutex<DraftRoom>>) -> bool {
        let rooms = self.rooms.read().await;
        rooms
            .get(thread_id)
            .is_some_and(|current| Arc::ptr_eq(current, room))
    }

    pub async fn take_draft(
        &self,
        thread_id: &str,
    ) -> Result<(String, Vec<String>), anyhow::Error> {
        let room = self.room(thread_id).await?;
        let taken = {
            let mut room = room.lock().await;
            room.take_draft()
        };
        if taken.0.is_empty() && taken.1.is_empty() {
            return Ok(taken);
        }
        let max_seq: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), 0) FROM draft_updates WHERE thread_id = ?",
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await?;
        self.snapshot(thread_id, u64::try_from(max_seq).unwrap_or(0))
            .await?;
        Ok(taken)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::events::test_support::seed_thread;
    use std::time::Duration;
    use y_sync::sync::{Message as Envelope, SyncMessage};
    use yrs::updates::encoder::Encode;

    fn update_message(bytes: Vec<u8>) -> Vec<u8> {
        Envelope::Sync(SyncMessage::Update(bytes)).encode_v1()
    }

    #[tokio::test]
    async fn draft_updates_sync_and_survive_restart() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let hub = DraftHub::new(pool.clone());
        let room = hub.room(thread.as_str()).await.unwrap();
        let (mut rx, initial) = {
            let mut room = room.lock().await;
            let rx = room.subscribe();
            threadbase_draft::seed_text(room.draft_mut(), "u1", &["hello"]).unwrap();
            (rx, room.initial())
        };
        assert!(!initial.is_empty());
        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(!broadcast.is_empty());

        let mut other = DraftDoc::new();
        threadbase_draft::seed_text(&mut other, "u2", &["world"]).unwrap();
        let update = other.encode_full();
        let applied = {
            let mut room = room.lock().await;
            room.apply(&update_message(update.clone()))
        };
        assert!(applied.limits_ok);
        assert!(applied.direct.is_empty());
        assert_eq!(applied.persisted.len(), 1);
        for update in applied.persisted {
            hub.persist_update(thread.as_str(), &update).await.unwrap();
        }
        hub.snapshot(thread.as_str(), 1).await.unwrap();

        let restarted = DraftHub::new(pool);
        let room = restarted.room(thread.as_str()).await.unwrap();
        let (text, authors) = {
            let room = room.lock().await;
            room.peek_draft()
        };
        assert_eq!(text, "hello\nworld");
        assert_eq!(authors, vec!["u1".to_owned(), "u2".to_owned()]);
    }

    fn oversized_update(text_len: usize) -> Vec<u8> {
        let mut doc = DraftDoc::new();
        let line = "x".repeat(text_len);
        threadbase_draft::seed_text(&mut doc, "u1", &[&line]).unwrap();
        doc.encode_full()
    }

    #[tokio::test]
    async fn oversized_update_is_not_broadcast_or_persisted() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let hub = DraftHub::new(pool.clone());
        let room = hub.room(thread.as_str()).await.unwrap();
        let mut rx = {
            let room = room.lock().await;
            room.subscribe()
        };

        let update = oversized_update(threadbase_draft::MAX_THREAD_DRAFT_BYTES + 1);
        let applied = {
            let mut room = room.lock().await;
            room.apply(&update_message(update))
        };
        assert!(!applied.limits_ok, "oversized update must be rejected");
        assert!(applied.direct.is_empty());
        assert!(
            applied.persisted.is_empty(),
            "rejected update must not be persisted"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(200), rx.recv())
                .await
                .is_err(),
            "rejected update must not be broadcast"
        );
    }

    #[tokio::test]
    async fn take_clears_the_shared_draft() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let hub = DraftHub::new(pool);
        let room = hub.room(thread.as_str()).await.unwrap();
        {
            let mut room = room.lock().await;
            threadbase_draft::seed_text(room.draft_mut(), "u1", &["hello"]).unwrap();
            let (text, authors) = room.take_draft();
            assert_eq!(text, "hello");
            assert_eq!(authors, vec!["u1".to_owned()]);
            assert!(room.peek_draft().0.is_empty());
        }
    }

    #[tokio::test]
    async fn concurrent_room_loads_share_one_room() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let hub = DraftHub::new(pool);
        let (left, right) = tokio::join!(hub.room(thread.as_str()), hub.room(thread.as_str()));
        let left = left.unwrap();
        let right = right.unwrap();
        assert!(Arc::ptr_eq(&left, &right), "one room per thread");
    }

    #[tokio::test]
    async fn send_guard_serializes_one_thread() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let hub = DraftHub::new(pool);
        let first = hub.send_guard(thread.as_str()).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), hub.send_guard(thread.as_str()))
                .await
                .is_err(),
            "a second sender must wait for the first"
        );
        drop(first);
        let _second = hub.send_guard(thread.as_str()).await;
    }
}
