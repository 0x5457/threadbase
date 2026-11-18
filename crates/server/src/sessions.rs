//! Sessions read model: threads and host tmux sessions in one list.
//!
//! Nothing is stored here. Threads come from SQLite and tmux sessions are read
//! from tmux, so this module only orders them.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::error::ServerError;
use crate::terminal::{self, Access, SessionRef};
use axum::extract::{Path as HttpPath, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use sqlx::FromRow;
use std::collections::HashMap;
use std::path::Path as FsPath;
use threadbase_shared::{
    now_ms, ApiError, CreateThreadFromTmuxRequest, CreateThreadRequest, SessionSummaryDto,
    SessionsResponse, ThreadResponse, ThreadStatus, TmuxLinkDto, UserAccess, UserBrief,
    WorkbenchShellSummaryDto,
};
use ulid::Ulid;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/sessions", get(list)).route(
        "/api/tmux/sessions/{socket}/{session_id}/thread",
        post(create_thread_from_tmux),
    )
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
    has_draft: i64,
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
            let projects = project_paths(&state.pool).await.unwrap_or_default();
            let links = tmux_links(&state.pool).await.unwrap_or_default();
            attach_host_sessions(&state, &mut sessions, &mut warnings, &projects, &links).await;
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
                  WHERE a.thread_id = t.id AND a.status = 'pending') AS pending_approvals,
                t.has_draft
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
            has_draft: row.has_draft != 0,
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
    struct ShellInfo {
        viewer_ids: Vec<String>,
        controller: Option<String>,
    }
    let mut shells: HashMap<String, ShellInfo> = HashMap::new();
    let mut profile_ids = Vec::new();
    for id in live {
        let Some(thread_id) = id.strip_prefix("t-") else {
            continue;
        };
        let room = state.terminal.room(&id);
        let viewer_ids = room
            .as_ref()
            .map(|room| room.viewer_ids())
            .unwrap_or_default();
        let controller = room.as_ref().and_then(|room| room.controller());
        profile_ids.extend(viewer_ids.iter().cloned());
        if let Some(controller) = &controller {
            profile_ids.push(controller.clone());
        }
        shells.insert(
            thread_id.to_owned(),
            ShellInfo {
                viewer_ids,
                controller,
            },
        );
    }
    let profiles = brief_map(&state.pool, &profile_ids).await;
    for session in sessions {
        if let SessionSummaryDto::Thread { id, terminal, .. } = session {
            let Some(info) = shells.remove(id.as_str()) else {
                continue;
            };
            *terminal = Some(WorkbenchShellSummaryDto {
                viewers: visible_users(&info.viewer_ids, &profiles),
                controller: info
                    .controller
                    .as_deref()
                    .and_then(|id| profiles.get(id).cloned()),
                can_drive,
            });
        }
    }
}

async fn brief_map(pool: &sqlx::SqlitePool, ids: &[String]) -> HashMap<String, UserBrief> {
    crate::auth::load_user_briefs(pool, ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|user| (user.id.to_string(), user))
        .collect()
}

fn visible_users(ids: &[String], profiles: &HashMap<String, UserBrief>) -> Vec<UserBrief> {
    ids.iter()
        .filter_map(|id| profiles.get(id).cloned())
        .collect()
}

async fn attach_host_sessions(
    state: &AppState,
    sessions: &mut Vec<SessionSummaryDto>,
    warnings: &mut Vec<String>,
    projects: &[ProjectPath],
    links: &HashMap<(String, String, i64), TmuxLinkDto>,
) {
    let servers = state
        .terminal
        .host_sessions(&state.config.terminal_host_sockets)
        .await;
    for server in servers {
        if let Some(error) = server.error {
            warnings.push(format!("{}: {error}", server.socket));
        }
        let mut profile_ids = Vec::new();
        let mut pending = Vec::new();
        for session in server.sessions {
            let room = state
                .terminal
                .room(&SessionRef::host(&server.socket, &session.id).key());
            let viewer_ids = room
                .as_ref()
                .map(|room| room.viewer_ids())
                .unwrap_or_default();
            let controller = room.as_ref().and_then(|room| room.controller());
            profile_ids.extend(viewer_ids.iter().cloned());
            if let Some(controller) = &controller {
                profile_ids.push(controller.clone());
            }
            pending.push((session, viewer_ids, controller));
        }
        let profiles = brief_map(&state.pool, &profile_ids).await;
        for (session, viewer_ids, controller) in pending {
            let link = links
                .get(&(
                    server.socket.clone(),
                    session.id.clone(),
                    session.created_at,
                ))
                .cloned();
            let inferred = session
                .cwd
                .as_deref()
                .and_then(|cwd| project_for(cwd, projects));
            let (project_id, project_name) = if let Some(link) = &link {
                (
                    Some(link.project_id.clone()),
                    Some(link.project_name.clone()),
                )
            } else if let Some(project) = inferred {
                (Some(project.id.clone().into()), Some(project.name.clone()))
            } else {
                (None, None)
            };
            sessions.push(SessionSummaryDto::Tmux {
                id: session.id,
                socket: server.socket.clone(),
                name: session.name,
                cwd: session.cwd,
                command: session.command,
                project_id,
                project_name,
                link,
                windows: session.windows,
                attached: session.attached,
                viewers: visible_users(&viewer_ids, &profiles),
                controller: controller
                    .as_deref()
                    .and_then(|id| profiles.get(id).cloned()),
                created_at: session.created_at,
                activity_at: session.activity_at,
            });
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct ProjectPath {
    id: String,
    name: String,
    path: String,
}

async fn project_paths(pool: &sqlx::SqlitePool) -> Result<Vec<ProjectPath>, ServerError> {
    Ok(
        sqlx::query_as::<_, ProjectPath>("SELECT id, name, path FROM projects")
            .fetch_all(pool)
            .await?,
    )
}

/// Longest registered project whose directory contains the live cwd. Longest
/// wins so nested projects resolve to the more specific checkout.
fn project_for<'a>(cwd: &str, projects: &'a [ProjectPath]) -> Option<&'a ProjectPath> {
    let cwd = FsPath::new(cwd);
    projects
        .iter()
        .filter(|project| cwd.starts_with(FsPath::new(&project.path)))
        .max_by_key(|project| project.path.len())
}

#[derive(Debug, FromRow)]
struct LinkRow {
    socket: String,
    session_id: String,
    session_created_at: i64,
    project_id: String,
    project_name: String,
    thread_id: Option<String>,
    thread_title: Option<String>,
    created_at: i64,
}

async fn tmux_links(
    pool: &sqlx::SqlitePool,
) -> Result<HashMap<(String, String, i64), TmuxLinkDto>, ServerError> {
    let rows: Vec<LinkRow> = sqlx::query_as(
        "SELECT l.socket, l.session_id, l.session_created_at, l.project_id,
                p.name AS project_name, l.thread_id, t.title AS thread_title,
                l.created_at
           FROM tmux_links l
           JOIN projects p ON p.id = l.project_id
           LEFT JOIN threads t ON t.id = l.thread_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let link = TmuxLinkDto {
                project_id: row.project_id.into(),
                project_name: row.project_name,
                thread_id: row.thread_id.map(Into::into),
                thread_title: row.thread_title,
                created_at: row.created_at,
            };
            ((row.socket, row.session_id, row.session_created_at), link)
        })
        .collect())
}

async fn create_thread_from_tmux(
    State(state): State<AppState>,
    user: CurrentUser,
    HttpPath((socket, session_id)): HttpPath<(String, String)>,
    Json(request): Json<CreateThreadFromTmuxRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    crate::routes_terminal::host_gate(&state, &user)?;
    if !crate::routes_terminal::host_socket_allowed(&state, &socket)
        || !terminal::is_host_session_id(&session_id)
    {
        return Err(ServerError(ApiError::not_found("tmux session not found")));
    }
    let Some(session) = state.terminal.host_session(&socket, &session_id).await else {
        return Err(ServerError(ApiError::not_found("tmux session not found")));
    };
    let Some(cwd) = session.cwd.as_deref().filter(|cwd| !cwd.is_empty()) else {
        return Err(ServerError(ApiError::conflict(
            "this tmux session has no working directory",
        )));
    };
    let projects = project_paths(&state.pool).await?;
    let Some(project) = project_for(cwd, &projects) else {
        return Err(ServerError(ApiError::conflict(
            "this tmux session is not inside a registered project",
        )));
    };
    let existing: Option<Option<String>> = sqlx::query_scalar(
        "SELECT thread_id FROM tmux_links
          WHERE socket = ? AND session_id = ? AND session_created_at = ?",
    )
    .bind(&socket)
    .bind(&session_id)
    .bind(session.created_at)
    .fetch_optional(&state.pool)
    .await?;
    if existing.flatten().is_some() {
        return Err(ServerError(ApiError::conflict(
            "this tmux session already has a thread",
        )));
    }
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            if session.name.trim().is_empty() {
                "Thread from tmux".to_owned()
            } else {
                session.name.clone()
            }
        });
    let create = CreateThreadRequest {
        title: Some(title),
        agent: request.agent.clone(),
        model: request.model.clone(),
        effort: request.effort.clone(),
        base_branch: None,
        initial_prompt: request.initial_prompt.clone(),
        worktree: request.worktree,
    };
    let thread =
        crate::routes_workspace::create_thread_and_start(&state, &user, &project.id, &create)
            .await?;
    let now = now_ms();
    if let Err(error) = sqlx::query(
        "INSERT INTO tmux_links(id, project_id, thread_id, socket, session_id,
              session_name, session_created_at, cwd, created_by, created_at)
         VALUES(?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(socket, session_id, session_created_at)
         DO UPDATE SET thread_id = COALESCE(tmux_links.thread_id, excluded.thread_id),
                       project_id = excluded.project_id,
                       session_name = excluded.session_name,
                       cwd = excluded.cwd",
    )
    .bind(Ulid::new().to_string())
    .bind(&project.id)
    .bind(thread.id.0.as_str())
    .bind(&socket)
    .bind(&session_id)
    .bind(&session.name)
    .bind(session.created_at)
    .bind(cwd)
    .bind(user.user.id.as_str())
    .bind(now)
    .execute(&state.pool)
    .await
    {
        tracing::warn!(%error, "could not link the tmux session to its new thread");
    }
    Ok(Json(ThreadResponse { thread }))
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
        } => (false, *attached > 0 || !viewers.is_empty(), *activity_at),
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
            has_draft: false,
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
            project_id: None,
            project_name: None,
            link: None,
            windows: 1,
            attached,
            viewers: Vec::new(),
            controller: None,
            created_at: activity,
            activity_at: activity,
        }
    }

    #[test]
    fn project_inference_prefers_the_longest_prefix() {
        let projects = vec![
            ProjectPath {
                id: "p1".to_owned(),
                name: "repo".to_owned(),
                path: "/work/repo".to_owned(),
            },
            ProjectPath {
                id: "p2".to_owned(),
                name: "nested".to_owned(),
                path: "/work/repo/nested".to_owned(),
            },
        ];
        assert_eq!(
            project_for("/work/repo/nested/src", &projects)
                .unwrap()
                .name,
            "nested"
        );
        assert_eq!(
            project_for("/work/repo/src", &projects).unwrap().name,
            "repo"
        );
        assert!(project_for("/work/other", &projects).is_none());
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
