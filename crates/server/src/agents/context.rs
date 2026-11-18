//! The transcript a forked thread hands to its first agent session.
//!
//! ACP sessions cannot be branched, so the copied turns ride once with the
//! first prompt; later prompts use the adapter's own context.

use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_shared::{MessageRole, ThreadEvent};

/// Context budget for one fork hand-off. A prompt is billed like any other, so
/// the newest turns are kept and older ones dropped instead of uploading an
/// arbitrarily long history.
const LIMIT_CHARS: usize = 48_000;
/// One turn should not crowd out the rest of the history on its own.
const BLOCK_CHARS: usize = 8_000;

const HEADER: &str = "\
This thread is a fork. The request above comes first; the messages below are the
conversation it forked from, earlier, and are prior context for that request.

--- conversation so far ---
";
const FOOTER: &str = "--- end of conversation ---";

/// The inherited conversation, or `None` when this thread has nothing to hand
/// over. A read failure is the caller's to report; a malformed row is skipped
/// rather than losing the whole context.
pub async fn fork_history(
    pool: &SqlitePool,
    thread_id: &str,
) -> Result<Option<String>, ServerError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT payload_json FROM events
          WHERE thread_id = ? AND subagent_id IS NULL
            AND kind IN ('turn_started', 'message_completed', 'system_notice')
          ORDER BY seq ASC",
    )
    .bind(thread_id)
    .fetch_all(pool)
    .await?;
    let blocks: Vec<String> = rows
        .into_iter()
        .filter_map(|payload| serde_json::from_str::<ThreadEvent>(&payload).ok())
        .filter_map(block)
        .collect();
    Ok(render(blocks))
}

fn block(event: ThreadEvent) -> Option<String> {
    let (who, text) = match event {
        ThreadEvent::TurnStarted { author, prompt, .. } => (author.display_name, prompt),
        ThreadEvent::MessageCompleted { message } if !message.text.trim().is_empty() => {
            let who = match message.role {
                MessageRole::User => message
                    .author
                    .map_or_else(|| "User".to_owned(), |author| author.display_name),
                MessageRole::Agent => "Agent".to_owned(),
            };
            (who, message.text)
        }
        ThreadEvent::SystemNotice { message, .. } => ("System".to_owned(), message),
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let text: String = text.chars().take(BLOCK_CHARS).collect();
    Some(format!("{who}: {text}"))
}

/// Newest whole blocks that fit the budget, plus whether older ones were cut.
fn fit(blocks: &[String]) -> (Vec<&String>, bool) {
    let mut kept = Vec::new();
    let mut used = HEADER.len() + FOOTER.len();
    for block in blocks.iter().rev() {
        if used + block.len() + 2 > LIMIT_CHARS {
            break;
        }
        used += block.len() + 2;
        kept.push(block);
    }
    let truncated = kept.len() < blocks.len();
    (kept, truncated)
}

fn render(blocks: Vec<String>) -> Option<String> {
    if blocks.is_empty() {
        return None;
    }
    let (kept, truncated) = fit(&blocks);
    let mut out = String::from(HEADER);
    if truncated {
        out.push_str("\n(Earlier messages were left out to keep this context short.)\n");
    }
    for block in kept.into_iter().rev() {
        out.push('\n');
        out.push_str(block);
        out.push('\n');
    }
    out.push('\n');
    out.push_str(FOOTER);
    out.push('\n');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{fit, render, LIMIT_CHARS};

    #[test]
    fn keeps_the_newest_blocks_that_fit() {
        let blocks = vec![
            "x".repeat(LIMIT_CHARS),
            "Agent: the newest answer".to_owned(),
        ];
        let (kept, truncated) = fit(&blocks);
        assert_eq!(kept, vec![&blocks[1]]);
        assert!(truncated);
    }

    #[test]
    fn no_blocks_means_no_context() {
        assert!(render(Vec::new()).is_none());
    }
}
