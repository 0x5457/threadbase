//! Per-thread collaborative draft rooms over the Yjs sync protocol.

use crate::auth::CurrentUser;
use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use threadbase_draft::DraftDoc;
use tokio::sync::{broadcast, Mutex, RwLock};
use y_sync::sync::{DefaultProtocol, Message as SyncMessageEnvelope, Protocol, SyncMessage};
use yrs::encoding::read::Cursor;
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::{Encode, Encoder};
use yrs::Update;

#[derive(Clone)]
pub struct DraftHub {
    pool: SqlitePool,
    rooms: Arc<RwLock<HashMap<String, Arc<Mutex<DraftRoom>>>>>,
}

pub(crate) struct DraftRoom {
    draft: DraftDoc,
    tx: broadcast::Sender<Vec<u8>>,
    _doc_sub: yrs::UpdateSubscription,
    _awareness_sub: y_sync::awareness::UpdateSubscription,
}

impl DraftHub {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            rooms: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(crate) async fn room(
        &self,
        thread_id: &str,
    ) -> Result<Arc<Mutex<DraftRoom>>, anyhow::Error> {
        if let Some(room) = self.rooms.read().await.get(thread_id).cloned() {
            return Ok(room);
        }
        let mut rooms = self.rooms.write().await;
        if let Some(room) = rooms.get(thread_id).cloned() {
            return Ok(room);
        }
        let snapshot: Option<(i64, Vec<u8>)> =
            sqlx::query_as("SELECT upto_seq, state_blob FROM draft_snapshots WHERE thread_id = ?")
                .bind(thread_id)
                .fetch_optional(&self.pool)
                .await?;
        let (upto_seq, blob) = snapshot.unwrap_or((0, Vec::new()));
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
        let room = Arc::new(Mutex::new(DraftRoom::new(draft)));
        rooms.insert(thread_id.to_owned(), room.clone());
        Ok(room)
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
            room.draft.encode_full()
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

    pub async fn clear(&self, thread_id: &str) -> Result<(), anyhow::Error> {
        let room = self.room(thread_id).await?;
        {
            let mut room = room.lock().await;
            room.draft.reset();
        }
        self.snapshot(thread_id, 0).await?;
        Ok(())
    }
}

impl DraftRoom {
    fn new(mut draft: DraftDoc) -> Self {
        let (tx, _) = broadcast::channel(512);
        let sink = tx.clone();
        let doc_sub = draft
            .doc()
            .observe_update_v1(move |_, event| {
                let message = SyncMessageEnvelope::Sync(SyncMessage::Update(event.update.clone()));
                let _ = sink.send(message.encode_v1());
            })
            .expect("document observer");
        let sink = tx.clone();
        let awareness_sub = draft.awareness_mut().on_update(move |awareness, event| {
            let mut changed = Vec::new();
            changed.extend_from_slice(event.added());
            changed.extend_from_slice(event.updated());
            changed.extend_from_slice(event.removed());
            if let Ok(update) = awareness.update_with_clients(changed) {
                let _ = sink.send(SyncMessageEnvelope::Awareness(update).encode_v1());
            }
        });
        Self {
            draft,
            tx,
            _doc_sub: doc_sub,
            _awareness_sub: awareness_sub,
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }

    fn initial(&mut self) -> Vec<u8> {
        let mut encoder = yrs::updates::encoder::EncoderV1::new();
        let protocol = DefaultProtocol;
        let awareness = self.draft.awareness();
        protocol
            .start(awareness, &mut encoder)
            .expect("encode initial sync");
        encoder.to_vec()
    }

    fn apply(&mut self, bytes: &[u8]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let mut direct = Vec::new();
        let mut persisted = Vec::new();
        let mut decoder = DecoderV1::new(Cursor::new(bytes));
        let protocol = DefaultProtocol;
        while let Ok(message) = SyncMessageEnvelope::decode(&mut decoder) {
            match message {
                SyncMessageEnvelope::Sync(SyncMessage::SyncStep1(state_vector)) => {
                    if let Ok(Some(reply)) =
                        protocol.handle_sync_step1(self.draft.awareness(), state_vector)
                    {
                        direct.push(reply.encode_v1());
                    }
                }
                SyncMessageEnvelope::Sync(SyncMessage::SyncStep2(update))
                | SyncMessageEnvelope::Sync(SyncMessage::Update(update)) => {
                    let Ok(decoded) = Update::decode_v1(&update) else {
                        continue;
                    };
                    if protocol
                        .handle_update(self.draft.awareness_mut(), decoded)
                        .is_ok()
                    {
                        persisted.push(update);
                    }
                }
                SyncMessageEnvelope::Awareness(update) => {
                    let _ = protocol.handle_awareness_update(self.draft.awareness_mut(), update);
                }
                SyncMessageEnvelope::AwarenessQuery => {
                    if let Ok(Some(reply)) = protocol.handle_awareness_query(self.draft.awareness())
                    {
                        direct.push(reply.encode_v1());
                    }
                }
                SyncMessageEnvelope::Auth(_) | SyncMessageEnvelope::Custom(_, _) => {}
            }
        }
        (direct, persisted)
    }
}

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Response, crate::error::ServerError> {
    let role = crate::authz::require_read(&state.pool, &thread_id, &user).await?;
    let can_write = role.can_write();
    let room = state
        .drafts
        .room(&thread_id)
        .await
        .map_err(|error| crate::error::ServerError::internal(error.to_string()))?;
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, room, thread_id, can_write)))
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    room: Arc<Mutex<DraftRoom>>,
    thread_id: String,
    can_write: bool,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut updates = {
        let room = room.lock().await;
        room.subscribe()
    };
    let initial = {
        let mut room = room.lock().await;
        room.initial()
    };
    if sender.send(Message::Binary(initial.into())).await.is_err() {
        return;
    }
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                let Some(Ok(message)) = incoming else { break; };
                match message {
                    Message::Binary(bytes) => {
                        if !can_write {
                            continue;
                        }
                        let (direct, persisted) = {
                            let mut room = room.lock().await;
                            room.apply(&bytes)
                        };
                        for reply in direct {
                            if sender.send(Message::Binary(reply.into())).await.is_err() {
                                return;
                            }
                        }
                        for update in persisted {
                            if let Err(error) = state.drafts.persist_update(&thread_id, &update).await {
                                tracing::warn!(%error, thread_id, "failed to persist draft update");
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Text(_) | Message::Pong(_) => {}
                }
            }
            update = updates.recv() => {
                match update {
                    Ok(update) => {
                        if sender.send(Message::Binary(update.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = heartbeat.tick() => {
                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::events::test_support::seed_thread;
    use y_sync::sync::{Message as Envelope, SyncMessage};

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
            room.draft.append_block("b1", "u1", "hello", 1).unwrap();
            (rx, room.initial())
        };
        assert!(!initial.is_empty());
        let broadcast = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(!broadcast.is_empty());

        let mut other = DraftDoc::new();
        other.append_block("b2", "u2", "world", 2).unwrap();
        let update = other.encode_full();
        let (direct, persisted) = {
            let mut room = room.lock().await;
            room.apply(&update_message(update.clone()))
        };
        assert!(direct.is_empty());
        assert_eq!(persisted.len(), 1);
        for update in persisted {
            hub.persist_update(thread.as_str(), &update).await.unwrap();
        }
        hub.snapshot(thread.as_str(), 1).await.unwrap();

        let restarted = DraftHub::new(pool);
        let room = restarted.room(thread.as_str()).await.unwrap();
        let blocks = room.lock().await.draft.blocks();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].text, "world");
    }

    #[tokio::test]
    async fn reset_clears_all_blocks() {
        let pool = db::connect_memory().await.unwrap();
        let (_, _, thread) = seed_thread(&pool).await;
        let hub = DraftHub::new(pool);
        let room = hub.room(thread.as_str()).await.unwrap();
        {
            let mut room = room.lock().await;
            room.draft.append_block("b1", "u1", "hello", 1).unwrap();
            room.draft.reset();
            assert!(room.draft.blocks().is_empty());
        }
    }
}
