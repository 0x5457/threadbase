//! Drains the persisted input queue for a thread.

use super::{begin_turn, Inner, TurnStart};
use crate::error::ServerError;
use crate::{queue, threads};
use std::sync::Arc;
use threadbase_shared::{Delivery, ThreadStatus, UserBrief};
use tokio::sync::mpsc;

pub(crate) fn spawn(inner: Arc<Inner>, mut rx: mpsc::UnboundedReceiver<String>) {
    tokio::spawn(async move {
        while let Some(thread_id) = rx.recv().await {
            if let Err(error) = process(&inner, &thread_id).await {
                tracing::warn!(%error, thread_id, "queue processing failed");
            }
        }
    });
}

async fn process(inner: &Arc<Inner>, thread_id: &str) -> Result<(), ServerError> {
    let Some(session) = inner.session(thread_id).await else {
        return Err(ServerError::internal("agent is not running"));
    };
    let mut session = session.lock().await;
    let status = threads::status(&inner.pool, thread_id).await?;
    if session.turn.is_some() && status == ThreadStatus::Running {
        return Ok(());
    }
    if status != ThreadStatus::Running {
        session.turn = None;
    }
    let Some((id, author_id, text, authors_json)) = queue::head(&inner.pool, thread_id).await?
    else {
        return Ok(());
    };
    queue::dispatch(&inner.pool, &inner.projection, thread_id, &id).await?;
    let author = author_brief(inner, &author_id).await;
    let authors = authors_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<UserBrief>>(json).ok())
        .filter(|authors| !authors.is_empty())
        .unwrap_or_else(|| vec![author.clone()]);
    let start = TurnStart {
        thread_id,
        author: &author,
        authors: &authors,
        prompt: &text,
        delivery: Delivery::Queued,
    };
    match begin_turn(inner, &mut session, start).await {
        Ok((handle, _)) => {
            queue::complete(
                &inner.pool,
                &inner.projection,
                thread_id,
                &id,
                Some(handle.turn_id.as_str()),
            )
            .await?;
            let _ = queue::publish(&inner.pool, &inner.events, thread_id).await;
            Ok(())
        }
        Err(error) => {
            queue::fail(&inner.pool, &inner.projection, thread_id, &id).await?;
            let _ = queue::publish(&inner.pool, &inner.events, thread_id).await;
            Err(error)
        }
    }
}

async fn author_brief(inner: &Inner, author_id: &str) -> UserBrief {
    match crate::auth::load_user_by_id(&inner.pool, author_id).await {
        Ok(Some(user)) => UserBrief::from_user(&user),
        _ => UserBrief {
            id: author_id.to_owned().into(),
            username: "member".into(),
            display_name: "Member".into(),
            avatar_url: None,
            color: threadbase_shared::user_color(author_id).to_owned(),
        },
    }
}
