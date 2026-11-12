//! Single-thread read model and membership.

use super::query::{dto, invites, load_row, members, ThreadRow};
use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::projects;
use crate::{queue, subagents};
use sqlx::SqlitePool;
use threadbase_shared::{ChangeStats, ChangesDto, ShareStateDto, ThreadBundleDto, ThreadId};

pub async fn bundle(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<ThreadBundleDto, ServerError> {
    let role = authz::require_read(pool, thread_id, user).await?;
    let row = load_row(pool, thread_id).await?;
    let thread = dto(pool, &row, user).await?;
    let project = projects::get(pool, &row.project_id).await?;
    let subagents = subagents::list(pool, thread_id).await?;
    let approvals = crate::approvals::list_pending_for_thread(pool, thread_id).await?;
    let elicitations = crate::elicitations::list_for_thread(pool, thread_id).await?;
    let queue = queue::list_for_thread(pool, thread_id).await?;
    let comments = crate::comments::list_for_thread(pool, thread_id).await?;
    let settings = crate::settings::load(pool, user).await?;
    let changes = crate::diff::list(
        pool,
        &row,
        user,
        &threadbase_shared::BaselineDto {
            kind: settings.default_baseline,
            commit: None,
        },
    )
    .await
    .unwrap_or_else(|_| empty_changes(&row));
    let members = members(pool, &row).await?;
    let invites = invites(pool, thread_id).await?;
    let share = ShareStateDto {
        thread_id: ThreadId::from(thread_id.to_owned()),
        share_level: thread.share_level,
        members: members.clone(),
        pending_invites: invites,
        share_token: None,
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
        members,
        share,
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
    }
}
