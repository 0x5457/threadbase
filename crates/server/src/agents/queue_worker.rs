//! Drains the persisted input queue for a thread.

use super::{begin_turn, AgentManager, Inner, TurnStart};
use crate::error::ServerError;
use crate::{queue, threads};
use threadbase_shared::{Delivery, ThreadStatus, UserBrief};
use tokio::sync::mpsc;

pub(crate) fn spawn(manager: AgentManager, mut rx: mpsc::UnboundedReceiver<String>) {
    tokio::spawn(async move {
        while let Some(thread_id) = rx.recv().await {
            if let Err(error) = process(&manager, &thread_id).await {
                tracing::warn!(%error, thread_id, "queue processing failed");
            }
        }
    });
}

async fn process(manager: &AgentManager, thread_id: &str) -> Result<(), ServerError> {
    let inner = &manager.inner;
    // Observe an already-running turn before touching the queue. The adapter
    // may not be started yet, which is fine: no session means no turn.
    if let Some(session) = inner.session(thread_id).await {
        let status = threads::status(&inner.pool, thread_id).await?;
        if session
            .observe_running(status == ThreadStatus::Running)
            .await
        {
            return Ok(());
        }
    }
    if queue::head(&inner.pool, thread_id).await?.is_none() {
        return Ok(());
    }
    // Setup owns the queue until it finishes; it kicks the worker on success.
    if crate::setup::busy(&inner.pool, thread_id).await {
        return Ok(());
    }
    // Wait for a global turn slot before starting an adapter or preparing the
    // prompt. Direct sends queue instead of blocking; this worker is the one
    // place that should wait.
    let permit = inner.acquire_turn_permit().await;
    // Input may have been superseded while this worker was waiting.
    let Some((id, author_id, text, authors_json, attachments_json)) =
        queue::head(&inner.pool, thread_id).await?
    else {
        return Ok(());
    };
    // The adapter may be gone: after a restart, after idle eviction, or
    // because nothing has started it yet.
    manager.ensure_started(thread_id).await?;
    let Some(session) = inner.session(thread_id).await else {
        return Err(ServerError::internal("agent is not running"));
    };
    let status = threads::status(&inner.pool, thread_id).await?;
    if session
        .observe_running(status == ThreadStatus::Running)
        .await
    {
        return Ok(());
    }
    let parsed_authors = authors_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<UserBrief>>(json).ok())
        .filter(|authors| !authors.is_empty());
    // The restart continuation is authored by Threadbase, not by the thread
    // creator whose id had to satisfy the author foreign key.
    let (author, authors) = match queue::restart_author(authors_json.as_deref()) {
        Some(system) => (system.clone(), vec![system]),
        None => {
            let author = author_brief(inner, &author_id).await;
            let authors = parsed_authors.unwrap_or_else(|| vec![author.clone()]);
            (author, authors)
        }
    };
    let attachments = attachments_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<threadbase_shared::AttachmentDto>>(json).ok())
        .filter(|attachments| !attachments.is_empty())
        .unwrap_or_default();
    let Some(claim) = session.claim_start().await else {
        return Ok(());
    };
    if let Err(error) = queue::dispatch(&inner.pool, &inner.projection, thread_id, &id).await {
        claim.release().await;
        return Err(error);
    }
    // Drop the row from every queue bar before the prompt reaches the
    // transcript. Publishing after the turn starts leaves the message visible
    // in the queue and in the thread for as long as the adapter takes to
    // accept it.
    let _ = queue::publish(&inner.pool, &inner.events, thread_id).await;
    let start = TurnStart {
        thread_id,
        author: &author,
        authors: &authors,
        prompt: &text,
        delivery: Delivery::Queued,
        attachments: &attachments,
        permit: Some(permit),
    };
    match begin_turn(inner, claim, start).await {
        Ok((handle, _)) => {
            queue::complete(
                &inner.pool,
                &inner.projection,
                thread_id,
                &id,
                Some(handle.turn_id.as_str()),
            )
            .await?;
            Ok(())
        }
        Err(error) => {
            queue::fail(&inner.pool, &inner.projection, thread_id, &id).await?;
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
