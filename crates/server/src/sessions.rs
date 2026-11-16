//! The Sessions read model: threads and host tmux sessions composed into one
//! list for the workspace's "what is running right now" surface.
//!
//! This is intentionally a projection, not an aggregate. Threads keep their
//! SQLite lifecycle and tmux sessions stay runtime processes discovered from
//! tmux itself; the only shared interface is the ordered list the UI renders.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::terminal::{self, Access, SessionRef};
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use sqlx::FromRow;
use std::collections::HashMap;
use threadbase_shared::{
    SessionSummaryDto, SessionsResponse, ThreadStatus, UserAccess, WorkbenchShellSummaryDto,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/sessions", get(list))
}

#[derive(Debug, FromRow)]
struct ThreadSummaryRow {
    id: String,
    title: String,
    project_id: String,
    project_name: String,
    agent: String,
    status: String,
    last_activity_at: i64,
    pinned: i64,
    pending_approvals: i64,
}

async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<SessionsResponse>, ServerError> {
    let mut sessions = thread_sessions(&state).await?;
    let mut warnings = Vec::new();
    let may_use_terminal = may_use_terminal(&user);
    if state.config.terminal && may_use_terminal {
        attach_workbench_shells(&state, &user, &mut sessions).await;
        if may_see_host(&state, &user) {
            attach_host_sessions(&state, &mut sessions, &mut warnings).await;
        }
    }
    sort(&mut sessions);
    Ok(Json(SessionsResponse { sessions, warnings }))
}

fn may_use_terminal(user: &CurrentUser) -> bool {
    user.is_admin() || matches!(user.user.access, UserAccess::Member)
}

fn may_see_host(state: &AppState, user: &CurrentUser) -> bool {
    state.config.terminal_host && may_use_terminal(user)
}

async fn thread_sessions(state: &AppState) -> Result<Vec<SessionSummaryDto>, ServerError> {
    let rows: Vec<ThreadSummaryRow> = sqlx::query_as(
        "SELECT t.id, t.title, t.project_id, p.name AS project_name, t.agent, t.status,
                COALESCE(t.last_activity_at, t.updated_at) AS last_activity_at,
                t.pinned,
                (SELECT COUNT(*) FROM approvals a
                  WHERE a.thread_id = t.id AND a.status = 'pending') AS pending_approvals
           FROM threads t
           JOIN projects p ON p.id = t.project_id
          WHERE t.archived_at IS NULL
          ORDER BY t.pinned DESC, last_activity_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| SessionSummaryDto::Thread {
            id: row.id.into(),
            title: row.title,
            project_id: row.project_id.into(),
            project_name: row.project_name,
            agent: row.agent,
            status: threadbase_core::state::parse_thread_status(&row.status)
                .unwrap_or(ThreadStatus::Idle),
            last_activity_at: row.last_activity_at,
            pinned: row.pinned != 0,
            pending_approvals: u32::try_from(row.pending_approvals).unwrap_or(0),
            terminal: None,
        })
        .collect())
}

/// Match live `t-<thread>` shells to thread rows. The tmux list is one process;
/// the per-shell path/controller reads are cheap and bounded by the number of
/// live workbench shells.
async fn attach_workbench_shells(
    state: &AppState,
    user: &CurrentUser,
    sessions: &mut [SessionSummaryDto],
) {
    let live = state.terminal.sessions().await;
    if live.is_empty() {
        return;
    }
    let may_drive = terminal::is_driver(&state.config.terminal_drivers, &user.user.username);
    let can_drive =
        terminal::access_for(user.user.access, user.is_admin() || may_drive) == Access::Drive;
    let mut shells: HashMap<String, WorkbenchShellSummaryDto> = HashMap::new();
    for id in live {
        let Some(thread_id) = id.strip_prefix("t-") else {
            continue;
        };
        let room = state.terminal.room(&id);
        shells.insert(
            thread_id.to_owned(),
            WorkbenchShellSummaryDto {
                viewers: room.as_ref().map_or(0, |room| room.viewers()),
                controller: room
                    .as_ref()
                    .and_then(|room| room.controller())
                    .map(Into::into),
                can_drive,
            },
        );
    }
    for session in sessions {
        if let SessionSummaryDto::Thread { id, terminal, .. } = session {
            *terminal = shells.remove(id.as_str());
        }
    }
}

async fn attach_host_sessions(
    state: &AppState,
    sessions: &mut Vec<SessionSummaryDto>,
    warnings: &mut Vec<String>,
) {
    let servers = state
        .terminal
        .host_sessions(&state.config.terminal_host_sockets)
        .await;
    for server in servers {
        if let Some(error) = server.error {
            warnings.push(format!("{}: {error}", server.socket));
        }
        for session in server.sessions {
            let viewers = state
                .terminal
                .room(&SessionRef::host(&server.socket, &session.id).key())
                .map_or(0, |room| room.viewers());
            sessions.push(SessionSummaryDto::Tmux {
                id: session.id,
                socket: server.socket.clone(),
                name: session.name,
                cwd: session.cwd,
                command: session.command,
                windows: session.windows,
                attached: session.attached,
                viewers,
                created_at: session.created_at,
                activity_at: session.activity_at,
            });
        }
    }
}

fn rank(session: &SessionSummaryDto) -> (bool, bool, i64) {
    match session {
        SessionSummaryDto::Thread {
            pinned,
            status,
            last_activity_at,
            ..
        } => (
            *pinned,
            matches!(
                status,
                ThreadStatus::Preparing | ThreadStatus::Running | ThreadStatus::RequiresAction
            ),
            *last_activity_at,
        ),
        SessionSummaryDto::Tmux {
            attached,
            viewers,
            activity_at,
            ..
        } => (false, *attached > 0 || *viewers > 0, *activity_at),
    }
}

fn sort(sessions: &mut [SessionSummaryDto]) {
    sessions.sort_by(|left, right| {
        rank(right)
            .cmp(&rank(left))
            .then_with(|| title(left).cmp(title(right)))
    });
}

fn title(session: &SessionSummaryDto) -> &str {
    match session {
        SessionSummaryDto::Thread { title, .. } => title,
        SessionSummaryDto::Tmux { name, .. } => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use threadbase_shared::ProjectId;

    fn thread(title: &str, pinned: bool, status: ThreadStatus, activity: i64) -> SessionSummaryDto {
        SessionSummaryDto::Thread {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            title: title.to_owned(),
            project_id: ProjectId::from("01ARZ3NDEKTSV4RRFFQ69G5FAW"),
            project_name: "repo".to_owned(),
            agent: "codex".to_owned(),
            status,
            last_activity_at: activity,
            pinned,
            pending_approvals: 0,
            terminal: None,
        }
    }

    fn tmux(name: &str, attached: u32, activity: i64) -> SessionSummaryDto {
        SessionSummaryDto::Tmux {
            id: "$0".to_owned(),
            socket: "default".to_owned(),
            name: name.to_owned(),
            cwd: None,
            command: Some("bash".to_owned()),
            windows: 1,
            attached,
            viewers: 0,
            created_at: activity,
            activity_at: activity,
        }
    }

    #[test]
    fn pinned_then_active_then_recent() {
        let mut sessions = vec![
            thread("idle", false, ThreadStatus::Idle, 100),
            tmux("attached", 1, 50),
            thread("running", false, ThreadStatus::Running, 40),
            thread("pinned", true, ThreadStatus::Idle, 10),
        ];
        sort(&mut sessions);
        let titles: Vec<_> = sessions.iter().map(title).collect();
        assert_eq!(titles, ["pinned", "attached", "running", "idle"]);
    }
}
