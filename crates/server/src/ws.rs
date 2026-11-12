//! WebSocket transport for event replay, live events and presence.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use threadbase_shared::{now_ms, ClientWsMessage, Focus, ServerWsMessage, ThreadId};
use tokio::sync::broadcast::error::RecvError;

pub async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    user: CurrentUser,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, user))
}

async fn handle_socket(socket: WebSocket, state: AppState, user: CurrentUser) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = state.events.subscribe();
    let mut presence = state.presence.subscribe();
    let mut state_rx = state.projection.subscribe();
    let mut subscriptions: HashMap<String, u64> = HashMap::new();
    let mut projects: HashSet<String> = HashSet::new();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let unread = crate::inbox::unread_count(&state.pool, user.user.id.as_str())
        .await
        .unwrap_or(0);
    if send_json(&mut sender, &ServerWsMessage::InboxCount { unread })
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                let Some(Ok(message)) = incoming else { break; };
                match message {
                    Message::Text(text) => {
                        let Ok(client) = serde_json::from_str::<ClientWsMessage>(text.as_str()) else {
                            let _ = send_json(&mut sender, &ServerWsMessage::Notice {
                                level: "error".into(),
                                message: "invalid WebSocket message".into(),
                            }).await;
                            continue;
                        };
                        match client {
                            ClientWsMessage::Subscribe { thread_id, after_seq } => {
                                let thread = thread_id.as_str().to_owned();
                                if let Err(error) = authz::require_read(&state.pool, &thread, &user).await {
                                    let _ = send_json(&mut sender, &ServerWsMessage::Notice {
                                        level: "error".into(),
                                        message: error.0.message,
                                    }).await;
                                    continue;
                                }
                                match state.events.page(&thread_id, after_seq, 500).await {
                                    Ok(page) => {
                                        let last = page.last_seq.max(after_seq);
                                        if send_json(&mut sender, &ServerWsMessage::EventsPage { page }).await.is_err() {
                                            break;
                                        }
                                        subscriptions.insert(thread.clone(), last);
                                        if let Some(state_message) = thread_state(&state, &thread_id).await {
                                            if send_json(&mut sender, &state_message).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        let _ = send_json(&mut sender, &ServerWsMessage::Notice {
                                            level: "error".into(),
                                            message: error.to_string(),
                                        }).await;
                                    }
                                }
                            }
                            ClientWsMessage::Unsubscribe { thread_id } => {
                                subscriptions.remove(thread_id.as_str());
                            }
                            ClientWsMessage::Presence {
                                project_id,
                                target,
                                thread_id,
                                panel,
                                typing,
                                hidden,
                            } => {
                                projects.insert(project_id.0.clone());
                                let focus = Focus {
                                    user_id: user.user.id.clone(),
                                    project_id: project_id.clone(),
                                    thread_id,
                                    panel,
                                    target,
                                    typing,
                                    hidden,
                                    updated_at: now_ms(),
                                };
                                state.presence.update(focus).await;
                            }
                            ClientWsMessage::Ping => {
                                if send_json(&mut sender, &ServerWsMessage::Pong).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        if sender.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Binary(_) | Message::Pong(_) => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if subscriptions.contains_key(event.thread_id.as_str()) {
                            if send_json(&mut sender, &ServerWsMessage::Event { event: event.clone() }).await.is_err() {
                                break;
                            }
                            subscriptions.insert(event.thread_id.0.clone(), event.seq);
                        }
                    }
                    Err(RecvError::Lagged(_)) => {
                        for (thread, after_seq) in &subscriptions {
                            if let Ok(mut page) = state.events.page(&ThreadId::from(thread.clone()), *after_seq, 500).await {
                                page.resync = true;
                                if send_json(&mut sender, &ServerWsMessage::EventsPage { page }).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
            message = presence.recv() => {
                match message {
                    Ok(message) => {
                        if send_json(&mut sender, &message).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
            state_message = state_rx.recv() => {
                match state_message {
                    Ok(message) => {
                        if send_json(&mut sender, &message).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
            _ = heartbeat.tick() => {
                if sender.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }

    if !projects.is_empty() {
        state.presence.remove_user(&user.user.id.0).await;
    }
    tracing::debug!(user = %user.user.id, "websocket disconnected");
}

async fn thread_state(state: &AppState, thread_id: &ThreadId) -> Option<ServerWsMessage> {
    let snapshot = state.projection.snapshot(thread_id.as_str()).await.ok()?;
    Some(ServerWsMessage::ThreadState { state: snapshot })
}

async fn send_json(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &ServerWsMessage,
) -> Result<(), ()> {
    let text = serde_json::to_string(message).map_err(|_| ())?;
    tokio::time::timeout(
        Duration::from_secs(5),
        sender.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}
