//! Single-thread read model.

use super::query::{dto, load_row, ThreadRow};
use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::projects;
use crate::{queue, subagents};
use sqlx::SqlitePool;
use threadbase_core::permissions::PermissionPolicy;
use threadbase_shared::{ChangeStats, ChangesDto, ThreadBundleDto};

pub async fn bundle(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
    include_changes: bool,
    policy: &PermissionPolicy,
) -> Result<ThreadBundleDto, ServerError> {
    let role = authz::require_read(pool, thread_id, user).await?;
    let row = load_row(pool, thread_id).await?;
    let thread = dto(pool, &row, policy).await?;
    let project = projects::get(pool, &row.project_id).await?;
    let subagents = subagents::list(pool, thread_id).await?;
    let approvals = crate::approvals::list_pending_for_thread(pool, thread_id).await?;
    let elicitations = crate::elicitations::list_for_thread(pool, thread_id).await?;
    let queue = queue::list_for_thread(pool, thread_id).await?;
    let comments = crate::comments::list_for_thread(pool, thread_id).await?;
    let settings = crate::settings::load(pool, user).await?;
    // The thread view polls this endpoint for comments/approvals/queue and
    // never reads `changes`; skipping the diff keeps four git subprocesses off
    // every poll. The Changes panel asks for the full bundle.
    let changes = if include_changes {
        crate::diff::list(
            pool,
            &row,
            user,
            &threadbase_shared::BaselineDto {
                kind: settings.default_baseline,
                commit: None,
            },
        )
        .await
        .unwrap_or_else(|_| empty_changes(&row))
    } else {
        empty_changes(&row)
    };
    Ok(ThreadBundleDto {
        thread,
        project,
        role,
        subagents,
        approvals,
        elicitations,
        queue,
        comments,
        changes,
    })
}

fn empty_changes(row: &ThreadRow) -> ChangesDto {
    ChangesDto {
        baseline: threadbase_shared::BaselineDto {
            kind: threadbase_shared::BaselineKind::BaseBranch,
            commit: None,
        },
        base_label: row.base_branch.clone(),
        files: Vec::new(),
        stats: ChangeStats {
            files: 0,
            additions: 0,
            deletions: 0,
            seen: 0,
            total: 0,
        },
        commit: row.base_sha.clone(),
        truncated: false,
    }
}
