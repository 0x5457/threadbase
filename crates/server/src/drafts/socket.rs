//! WebSocket transport for draft rooms.

use super::room::DraftRoom;
use crate::app::AppState;
use crate::auth::CurrentUser;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use threadbase_draft::{MAX_UPDATE_BYTES, MAX_WS_MESSAGE_BYTES};
use tokio::sync::{broadcast, Mutex};

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
    Ok(ws
        .max_message_size(MAX_WS_MESSAGE_BYTES)
        .max_frame_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_socket(socket, state, room, thread_id, can_write)))
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
        let room = room.lock().await;
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
                        if bytes.len() > MAX_WS_MESSAGE_BYTES {
                            tracing::warn!(
                                thread_id,
                                len = bytes.len(),
                                "draft frame exceeds limit; closing"
                            );
                            return;
                        }
                        let applied = {
                            let mut room = room.lock().await;
                            room.apply(&bytes)
                        };
                        if !applied.limits_ok {
                            tracing::warn!(thread_id, "draft limits exceeded; dropping room");
                            state.drafts.drop_room(&thread_id).await;
                            return;
                        }
                        for reply in applied.direct {
                            if sender.send(Message::Binary(reply.into())).await.is_err() {
                                return;
                            }
                        }
                        if !state.drafts.is_canonical(&thread_id, &room).await {
                            tracing::warn!(thread_id, "draft room no longer canonical; closing");
                            return;
                        }
                        for update in applied.persisted {
                            if update.len() > MAX_UPDATE_BYTES {
                                tracing::warn!(
                                    thread_id,
                                    len = update.len(),
                                    "draft update exceeds limit; skipping"
                                );
                                continue;
                            }
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
