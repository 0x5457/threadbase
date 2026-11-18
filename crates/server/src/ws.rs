//! WebSocket transport for event replay, live events and presence.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::header::ORIGIN;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use threadbase_shared::{now_ms, ClientWsMessage, Focus, ServerWsMessage, ThreadId};
use tokio::sync::broadcast::error::RecvError;

pub async fn handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<AppState>,
    user: CurrentUser,
) -> Response {
    if let Some(origin) = headers.get(ORIGIN) {
        let allowed = origin
            .to_str()
            .ok()
            .is_some_and(|origin| crate::middleware::origin_allowed(&state.config, origin));
        if !allowed {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
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
    if send_json(
        &mut sender,
        &ServerWsMessage::InboxCount {
            user_id: user.user.id.clone(),
            unread,
        },
    )
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
                        let message = visible_presence(message, &subscriptions, user.user.id.as_str());
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
                    Ok(ServerWsMessage::ThreadState { state }) => {
                        if !subscriptions.contains_key(state.thread_id.as_str()) {
                            continue;
                        }
                        if send_json(&mut sender, &ServerWsMessage::ThreadState { state }).await.is_err() {
                            break;
                        }
                    }
                    Ok(ServerWsMessage::ThreadRewound { thread_id }) => {
                        if !subscriptions.contains_key(thread_id.as_str()) {
                            continue;
                        }
                        // The client re-subscribes from zero after this frame;
                        // reset the cursor now so a lagged resync in between
                        // cannot serve only the new branch on top of old rows.
                        subscriptions.insert(thread_id.0.clone(), 0);
                        if send_json(&mut sender, &ServerWsMessage::ThreadRewound { thread_id }).await.is_err() {
                            break;
                        }
                    }
                    Ok(ServerWsMessage::InboxCount { user_id, unread }) => {
                        // The projection channel is global; only the owner's
                        // sockets see this count.
                        if user_id != user.user.id {
                            continue;
                        }
                        if send_json(&mut sender, &ServerWsMessage::InboxCount { user_id, unread }).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
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

/// Presence is broadcast per project, but a socket only reads the threads it
/// subscribed to. Teammates' focus on other threads is dropped, so a viewer may
/// miss an activity hint for a thread they have not opened.
fn visible_presence(
    message: ServerWsMessage,
    subscriptions: &HashMap<String, u64>,
    viewer_id: &str,
) -> ServerWsMessage {
    match message {
        ServerWsMessage::Presence {
            project_id,
            focuses,
            online_user_ids,
        } => ServerWsMessage::Presence {
            project_id,
            focuses: focuses
                .into_iter()
                .filter(|focus| {
                    focus.user_id.as_str() == viewer_id
                        || match &focus.thread_id {
                            None => true,
                            Some(thread_id) => subscriptions.contains_key(thread_id.as_str()),
                        }
                })
                .collect(),
            online_user_ids,
        },
        other => other,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use threadbase_shared::{now_ms, Panel, ProjectId, UserId};

    fn focus(user: &str, thread_id: Option<&str>) -> Focus {
        Focus {
            user_id: UserId::from(user),
            project_id: ProjectId::from("p1"),
            thread_id: thread_id.map(ThreadId::from),
            panel: Panel::Transcript,
            target: None,
            typing: false,
            hidden: false,
            updated_at: now_ms(),
        }
    }

    #[test]
    fn presence_keeps_project_self_and_subscribed_thread_focus() {
        let mut subscriptions = HashMap::new();
        subscriptions.insert("t1".to_owned(), 0);
        let message = ServerWsMessage::Presence {
            project_id: ProjectId::from("p1"),
            focuses: vec![
                focus("u1", None),
                focus("u1", Some("t1")),
                focus("u1", Some("t2")),
                focus("u2", Some("t1")),
                focus("u2", Some("t2")),
            ],
            online_user_ids: vec![UserId::from("u1"), UserId::from("u2")],
        };
        let ServerWsMessage::Presence {
            focuses,
            online_user_ids,
            ..
        } = visible_presence(message, &subscriptions, "u1")
        else {
            panic!("unexpected message");
        };
        let threads: Vec<(String, Option<String>)> = focuses
            .iter()
            .map(|focus| {
                (
                    focus.user_id.0.clone(),
                    focus
                        .thread_id
                        .as_ref()
                        .map(|thread_id| thread_id.as_str().to_owned()),
                )
            })
            .collect();
        assert_eq!(
            threads,
            vec![
                ("u1".to_owned(), None),
                ("u1".to_owned(), Some("t1".to_owned())),
                ("u1".to_owned(), Some("t2".to_owned())),
                ("u2".to_owned(), Some("t1".to_owned())),
            ]
        );
        assert_eq!(
            online_user_ids,
            vec![UserId::from("u1"), UserId::from("u2")]
        );
    }
}
