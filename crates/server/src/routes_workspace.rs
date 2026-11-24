//! Project, thread and change-review routes.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use crate::diff;
use crate::error::ServerError;
use crate::events::EventMeta;
use crate::projects;
use crate::threads;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use serde::Deserialize;
use std::path::Path as FsPath;
use threadbase_agent::SessionId;
use threadbase_core::state::PlacementMachine;
use threadbase_shared::api::{
    AgentSessionDto, AgentSessionsResponse, AttachThreadRequest, CreateProjectRequest,
    CreateThreadRequest, ForkThreadRequest, MergeBackRequest, PlacementRequest, RewindRequest,
    SeenRequest, ThreadState, UpdateProjectRequest, UpdateThreadRequest,
};
use threadbase_shared::{
    Actor, ApiError, BaselineDto, BaselineKind, ChangesDto, FileDiffDto, MergeBackResponse,
    OkResponse, OlderEventsDto, ProjectResponse, ProjectsResponse, ThreadDto, ThreadEvent,
    ThreadHistoryResponse, ThreadId, ThreadResponse, ThreadsResponse,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/threads", get(list_all_threads))
        .route("/api/threads/{thread_id}/bundle", get(thread_bundle))
        .route("/api/threads/{thread_id}/commits", get(thread_commits))
        .route(
            "/api/threads/{thread_id}/commits/{sha}",
            get(thread_commit_changes),
        )
        .route(
            "/api/threads/{thread_id}/commits/{sha}/file",
            get(thread_commit_file_diff),
        )
        .route("/api/threads/{thread_id}/state", get(thread_state))
        .route(
            "/api/threads/{thread_id}/events/older",
            get(thread_events_older),
        )
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{project_id}",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route(
            "/api/projects/{project_id}/threads",
            get(list_threads).post(create_thread),
        )
        .route(
            "/api/projects/{project_id}/agent-sessions",
            get(list_agent_sessions),
        )
        .route(
            "/api/projects/{project_id}/threads/attach",
            post(attach_thread),
        )
        .route(
            "/api/threads/{thread_id}",
            get(get_thread).patch(update_thread).delete(delete_thread),
        )
        .route("/api/threads/{thread_id}/fork", post(fork_thread))
        .route(
            "/api/threads/{thread_id}/attach",
            post(attach_existing_thread),
        )
        .route("/api/threads/{thread_id}/placement", post(set_placement))
        .route("/api/threads/{thread_id}/rewind", post(rewind_thread))
        .route(
            "/api/threads/{thread_id}/turns/{turn_id}/changes",
            get(thread_turn_changes),
        )
        .route(
            "/api/threads/{thread_id}/turns/{turn_id}/file",
            get(thread_turn_file_diff),
        )
        .route("/api/threads/{thread_id}/setup", post(run_thread_setup))
        .route(
            "/api/threads/{thread_id}/watch",
            get(get_thread_watch).post(set_thread_watch),
        )
        .route(
            "/api/threads/{thread_id}/changes/file",
            get(thread_file_diff),
        )
        .route("/api/threads/{thread_id}/seen", post(mark_seen))
        .route("/api/threads/{thread_id}/merge-back", post(merge_back))
        .route("/api/threads/{thread_id}/revert", post(revert_thread))
}

async fn thread_state(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadState>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    Ok(Json(state.projection.snapshot(&thread_id).await?))
}

#[derive(Debug, Deserialize)]
struct OlderEventsQuery {
    before: u64,
}

/// The window just before `before`, for transcripts longer than the cold page.
async fn thread_events_older(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<OlderEventsQuery>,
) -> Result<Json<OlderEventsDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let page = state
        .events
        .older_page(&ThreadId::from(thread_id), query.before)
        .await?;
    Ok(Json(page))
}

async fn list_projects(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> Result<Json<ProjectsResponse>, ServerError> {
    Ok(Json(ProjectsResponse {
        projects: projects::list(&state.pool).await?,
    }))
}

async fn create_project(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>, ServerError> {
    let project = if request.create {
        let roots = crate::routes_fs::allowed_roots(&state).await?;
        projects::create_repository(&state.pool, &user, &request, &roots).await?
    } else {
        projects::create(&state.pool, &user, &request).await?
    };
    Ok(Json(ProjectResponse { project }))
}

async fn get_project(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(project_id): Path<String>,
) -> Result<Json<ProjectResponse>, ServerError> {
    Ok(Json(ProjectResponse {
        project: projects::get(&state.pool, &project_id).await?,
    }))
}

async fn update_project(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<ProjectResponse>, ServerError> {
    if !projects::can_edit(&state.pool, &project_id, &user).await? {
        return Err(ServerError(ApiError::forbidden(
            "only the project creator or an admin can edit this project",
        )));
    }
    Ok(Json(ProjectResponse {
        project: projects::update(&state.pool, &project_id, &request).await?,
    }))
}

#[derive(Debug, Deserialize)]
struct ThreadListQuery {
    include_archived: Option<bool>,
}

async fn list_threads(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<ThreadListQuery>,
) -> Result<Json<ThreadsResponse>, ServerError> {
    Ok(Json(ThreadsResponse {
        threads: threads::list(
            &state.pool,
            &project_id,
            query.include_archived.unwrap_or(false),
            &user,
            &state.policy,
        )
        .await?,
    }))
}

async fn create_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<String>,
    Json(request): Json<CreateThreadRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    let thread = create_thread_and_start(&state, &user, &project_id, &request).await?;
    Ok(Json(ThreadResponse { thread }))
}

#[derive(Debug, Deserialize)]
struct AgentSessionsQuery {
    agent: Option<String>,
}

async fn list_agent_sessions(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<AgentSessionsQuery>,
) -> Result<Json<AgentSessionsResponse>, ServerError> {
    let project = projects::row(&state.pool, &project_id).await?;
    let agent = query.agent.as_deref().unwrap_or("codex");
    let sessions =
        crate::agents::native::list(&state.agents, agent, FsPath::new(&project.path)).await?;
    // A bound session is already a thread. Offering it again can only race the
    // duplicate-session guard, so the picker and the sidebar list the rest.
    let bound: std::collections::HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT b.session_id FROM agent_bindings b
           JOIN threads t ON t.id = b.thread_id
          WHERE t.project_id = ? AND b.agent = ? AND b.native = 1
            AND b.session_id IS NOT NULL",
    )
    .bind(&project_id)
    .bind(agent)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .collect();
    Ok(Json(AgentSessionsResponse {
        sessions: sessions
            .into_iter()
            .filter(|session| !bound.contains(&session.id))
            .map(|session| AgentSessionDto {
                id: session.id,
                title: session.title,
                cwd: session.cwd,
                updated_at: session.updated_at,
            })
            .collect(),
    }))
}

/// Create a thread bound to an existing native session. It runs in the
/// session's own directory, so no worktree is created and the native id keeps
/// pointing at the same conversation.
async fn attach_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<String>,
    Json(request): Json<AttachThreadRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    let project = projects::row(&state.pool, &project_id).await?;
    let agent = request.agent.as_deref().unwrap_or("codex");
    let cwd = FsPath::new(&project.path);
    let session = SessionId(request.session_id);
    let sessions = crate::agents::native::list(&state.agents, agent, cwd).await?;
    let found = sessions
        .iter()
        .find(|candidate| candidate.id == session.0)
        .ok_or_else(|| {
            ServerError(ApiError::validation(
                "the agent does not know that session in this project",
            ))
        })?;
    if session_owner(&state, &session).await?.is_some() {
        return Err(ServerError(ApiError::conflict(
            "that agent session is already attached to a thread",
        )));
    }
    let thread = create_thread_and_start(
        &state,
        &user,
        &project_id,
        &CreateThreadRequest {
            title: Some(
                request
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|title| !title.is_empty())
                    .unwrap_or(&found.title)
                    .to_owned(),
            ),
            agent: Some(agent.to_owned()),
            model: None,
            effort: None,
            base_branch: None,
            initial_prompt: None,
            worktree: Some(false),
        },
    )
    .await?;
    crate::agents::native::import(&state.agents, &thread.id.0, &session).await?;
    Ok(Json(ThreadResponse { thread }))
}

async fn session_owner(
    state: &AppState,
    session: &SessionId,
) -> Result<Option<String>, ServerError> {
    Ok(sqlx::query_scalar(
        "SELECT thread_id FROM agent_bindings WHERE session_id = ? AND native = 1 LIMIT 1",
    )
    .bind(&session.0)
    .fetch_optional(&state.pool)
    .await?)
}

#[derive(Debug, Deserialize)]
struct AttachExistingSession {
    session_id: String,
}

/// Bind an already-created, empty thread to a native session. The same rules
/// as the project-level attach apply: no worktree, the session's own directory,
/// and no existing transcript.
async fn attach_existing_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<AttachExistingSession>,
) -> Result<Json<ThreadResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let binding = crate::bindings::load(&state.pool, &thread_id)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("thread not found")))?;
    if binding.session_id.is_some() {
        return Err(ServerError(ApiError::conflict(
            "this thread already has an agent session",
        )));
    }
    let thread = threads::get(&state.pool, &thread_id, &user, &state.policy).await?;
    if thread.worktree_path.is_some() {
        return Err(ServerError(ApiError::validation(
            "attaching requires a thread without a worktree",
        )));
    }
    if matches!(
        thread.status,
        threadbase_shared::ThreadStatus::Preparing
            | threadbase_shared::ThreadStatus::Running
            | threadbase_shared::ThreadStatus::RequiresAction
    ) {
        return Err(ServerError(ApiError::conflict(
            "wait for the current turn to finish before attaching",
        )));
    }
    let cwd = FsPath::new(&binding.cwd);
    let session = SessionId(request.session_id);
    let sessions = crate::agents::native::list(&state.agents, &binding.agent, cwd).await?;
    if !sessions.iter().any(|candidate| candidate.id == session.0) {
        return Err(ServerError(ApiError::validation(
            "the agent does not know that session in this project",
        )));
    }
    if session_owner(&state, &session).await?.is_some() {
        return Err(ServerError(ApiError::conflict(
            "that agent session is already attached to a thread",
        )));
    }
    let used: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE thread_id = ?
          AND kind IN ('message_delta','message_completed','turn_started',
                       'tool_call','tool_call_update')",
    )
    .bind(&thread_id)
    .fetch_one(&state.pool)
    .await?;
    if used > 0 {
        return Err(ServerError(ApiError::validation(
            "this thread already has a transcript; attach a new thread instead",
        )));
    }
    crate::agents::native::import(&state.agents, &thread_id, &session).await?;
    let thread = threads::get(&state.pool, &thread_id, &user, &state.policy).await?;
    Ok(Json(ThreadResponse { thread }))
}

/// Shared by the normal project route and the "start a thread from this tmux
/// session" route, so setup and agent kick semantics cannot drift.
pub(crate) async fn create_thread_and_start(
    state: &AppState,
    user: &CurrentUser,
    project_id: &str,
    request: &CreateThreadRequest,
) -> Result<ThreadDto, ServerError> {
    let thread = threads::create(
        &state.pool,
        &state.projection,
        &state.config.data_dir,
        user,
        project_id,
        request,
        &state.policy,
    )
    .await?;
    let manager = state.agents.clone();
    let thread_id = thread.id.0.clone();
    let has_initial_prompt = request
        .initial_prompt
        .as_deref()
        .is_some_and(|prompt| !prompt.trim().is_empty());
    let setup_started = crate::setup::maybe_start(state, &thread_id).await?;
    let thread = if setup_started {
        threads::get(&state.pool, &thread_id, user, &state.policy).await?
    } else {
        thread
    };
    // A thread without a first prompt starts its adapter on the first send.
    // Creating a dozen threads must not leave a dozen idle ACP processes.
    // Setup owns the kick when it runs: it must finish before the agent starts.
    if has_initial_prompt && !setup_started {
        tokio::spawn(async move {
            match manager.ensure_started(&thread_id).await {
                Ok(()) => manager.kick_queue(&thread_id),
                Err(error) => tracing::warn!(thread_id, %error, "agent startup failed"),
            }
        });
    }
    Ok(thread)
}

async fn list_all_threads(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ThreadListQuery>,
) -> Result<Json<ThreadsResponse>, ServerError> {
    Ok(Json(ThreadsResponse {
        threads: threads::list_all(
            &state.pool,
            query.include_archived.unwrap_or(false),
            &user,
            &state.policy,
        )
        .await?,
    }))
}

#[derive(Debug, Default, Deserialize)]
struct BundleQuery {
    /// `false` skips the git diff. The transcript poll only needs the thread
    /// read model; the Changes panel requests the full bundle.
    changes: Option<bool>,
}

async fn thread_bundle(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<BundleQuery>,
) -> Result<Json<threadbase_shared::ThreadBundleResponse>, ServerError> {
    let mut bundle = threads::bundle(
        &state.pool,
        &thread_id,
        &user,
        query.changes.unwrap_or(true),
        &state.policy,
    )
    .await?;
    let online = state
        .presence
        .online_user_ids(bundle.project.id.as_str())
        .await;
    for participant in &mut bundle.thread.participants {
        participant.online = online.contains(participant.user.id.as_str());
    }
    Ok(Json(threadbase_shared::ThreadBundleResponse { bundle }))
}

async fn get_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadResponse>, ServerError> {
    Ok(Json(ThreadResponse {
        thread: threads::get(&state.pool, &thread_id, &user, &state.policy).await?,
    }))
}

async fn thread_commits(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadHistoryResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    Ok(Json(ThreadHistoryResponse {
        history: diff::history(&state.pool, &row).await?,
    }))
}

async fn thread_commit_changes(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, sha)): Path<(String, String)>,
) -> Result<Json<ChangesDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    Ok(Json(diff::commit_changes(&row, &sha).await?))
}

#[derive(Debug, Deserialize)]
struct CommitFileQuery {
    path: String,
}

async fn thread_commit_file_diff(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, sha)): Path<(String, String)>,
    Query(query): Query<CommitFileQuery>,
) -> Result<Json<FileDiffDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    Ok(Json(diff::commit_file_diff(&row, &sha, &query.path).await?))
}

async fn update_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<UpdateThreadRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    if request.archived == Some(true) {
        // Authorize before touching the live adapter; archiving is also a stop.
        crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
        state.agents.stop(&thread_id).await;
    }
    let thread = threads::update(
        &state.pool,
        &state.projection,
        &thread_id,
        &user,
        &request,
        &state.policy,
    )
    .await?;
    if request.model.is_some() || request.effort.is_some() {
        // The UI sends an empty string to clear a field; never forward that to
        // the adapter as a literal model or effort value.
        let model = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let effort = request
            .effort
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if model.is_some() || effort.is_some() {
            // A thread that is idle has no adapter: the stored value is applied
            // when the next session starts, so silence is correct there. A live
            // adapter that refuses the value must not be quiet — that is how a
            // thread ends up running a different model than the one the picker
            // shows.
            if let Err(error) = state.agents.set_config(&thread_id, model, effort).await {
                tracing::warn!(thread_id, %error, "could not apply model/effort to agent");
                let _ = state
                    .events
                    .append(
                        &ThreadId::from(thread_id.clone()),
                        Actor::system(),
                        ThreadEvent::SystemNotice {
                            level: "warning".to_owned(),
                            message: format!(
                                "The running agent rejected the new model/effort ({error}). The choice is saved and applied again when the session restarts."
                            ),
                        },
                        EventMeta::default(),
                    )
                    .await;
            }
        }
    }
    if request.permission_mode.is_some() {
        // The provider is told as well, so a long turn stops asking for what the
        // thread now allows instead of relying on the server to answer each
        // request. Adapters without their own modes answer `Ok` and keep asking;
        // the policy already handles those.
        if let Err(error) = state
            .agents
            .set_permission_mode(&thread_id, thread.permission_mode)
            .await
        {
            tracing::warn!(thread_id, %error, "could not apply the permission mode to agent");
            let _ = state
                .events
                .append(
                    &ThreadId::from(thread_id.clone()),
                    Actor::system(),
                    ThreadEvent::SystemNotice {
                        level: "warning".to_owned(),
                        message: format!(
                            "The running agent did not accept the new permission mode ({error}). Requests are still answered by Threadbase's policy."
                        ),
                    },
                    EventMeta::default(),
                )
                .await;
        }
    }
    Ok(Json(ThreadResponse { thread }))
}

async fn thread_turn_changes(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, turn_id)): Path<(String, String)>,
) -> Result<Json<ChangesDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    Ok(Json(diff::turn_changes(&state.pool, &row, &turn_id).await?))
}

#[derive(Debug, Deserialize)]
struct TurnFileQuery {
    path: String,
}

async fn thread_turn_file_diff(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, turn_id)): Path<(String, String)>,
    Query(query): Query<TurnFileQuery>,
) -> Result<Json<FileDiffDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    Ok(Json(
        diff::turn_file_diff(&state.pool, &row, &turn_id, &query.path).await?,
    ))
}

async fn rewind_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<RewindRequest>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    state
        .agents
        .rewind_to_turn(&thread_id, &request.message_id, &user)
        .await?;
    Ok(Json(OkResponse { ok: true }))
}

async fn set_placement(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<PlacementRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let action = PlacementMachine::command(&request.action).ok_or_else(|| {
        ServerError(ApiError::validation(format!(
            "unknown placement command: {}",
            request.action
        )))
    })?;
    threads::place(
        &state.pool,
        &state.projection,
        &thread_id,
        action,
        request.until,
    )
    .await?;
    let thread = threads::get(&state.pool, &thread_id, &user, &state.policy).await?;
    Ok(Json(ThreadResponse { thread }))
}

#[derive(Debug, Deserialize)]
struct DeleteQuery {
    confirm: Option<bool>,
}

async fn delete_project(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Result<Json<OkResponse>, ServerError> {
    if query.confirm != Some(true) {
        return Err(ServerError(ApiError::validation(
            "removing a project requires confirm=true; its threads, worktrees and branches will be removed",
        )));
    }
    if !projects::can_edit(&state.pool, &project_id, &user).await? {
        return Err(ServerError(ApiError::forbidden(
            "only the project creator or an admin can remove this project",
        )));
    }
    let thread_ids = threads::ids_for_project(&state.pool, &project_id).await?;
    for thread_id in &thread_ids {
        state.agents.stop(thread_id).await;
    }
    projects::delete(&state.pool, &project_id).await?;
    for thread_id in thread_ids {
        let _ =
            tokio::fs::remove_dir_all(state.config.data_dir.join("attachments").join(thread_id))
                .await;
    }
    Ok(Json(OkResponse { ok: true }))
}

async fn delete_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Result<Json<OkResponse>, ServerError> {
    if query.confirm != Some(true) {
        return Err(ServerError(ApiError::validation(
            "deleting a thread requires confirm=true; its worktree and uncommitted changes will be removed",
        )));
    }
    crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
    state.agents.stop(&thread_id).await;
    threads::delete(&state.pool, &thread_id, &user).await?;
    let _ =
        tokio::fs::remove_dir_all(state.config.data_dir.join("attachments").join(&thread_id)).await;
    Ok(Json(OkResponse { ok: true }))
}

async fn get_thread_watch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<threadbase_shared::api::ThreadWatchResponse>, ServerError> {
    crate::authz::require_read(&state.pool, &thread_id, &user).await?;
    Ok(Json(
        crate::watchers::state(&state.pool, &thread_id, user.user.id.as_str()).await?,
    ))
}

async fn set_thread_watch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<threadbase_shared::api::ThreadWatchRequest>,
) -> Result<Json<threadbase_shared::api::ThreadWatchResponse>, ServerError> {
    crate::authz::require_read(&state.pool, &thread_id, &user).await?;
    Ok(Json(
        crate::watchers::set(&state.pool, &thread_id, user.user.id.as_str(), &request).await?,
    ))
}

async fn fork_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<ForkThreadRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    let thread = threads::fork(
        &state.pool,
        &state.events,
        &state.projection,
        &state.config.data_dir,
        &thread_id,
        &user,
        &request,
        &state.policy,
    )
    .await?;
    let thread = if crate::setup::maybe_start(&state, thread.id.as_str()).await? {
        threads::get(&state.pool, thread.id.as_str(), &user, &state.policy).await?
    } else {
        thread
    };
    Ok(Json(ThreadResponse { thread }))
}

async fn run_thread_setup(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<OkResponse>, ServerError> {
    crate::authz::require_write_role(&state.pool, &thread_id, &user).await?;
    crate::setup::rerun(&state, &thread_id).await?;
    Ok(Json(OkResponse { ok: true }))
}

#[derive(Debug, Deserialize)]
struct FileDiffQuery {
    kind: Option<String>,
    commit: Option<String>,
    path: String,
}

async fn revert_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    diff::revert_to_base(&row).await?;
    sqlx::query("UPDATE threads SET last_turn_sha = base_sha, updated_at = ? WHERE id = ?")
        .bind(threadbase_shared::now_ms())
        .bind(&thread_id)
        .execute(&state.pool)
        .await?;
    crate::auth::audit(
        &state.pool,
        Some(user.user.id.as_str()),
        "thread.reverted",
        Some("thread"),
        Some(&thread_id),
        None,
        None,
    )
    .await;
    Ok(Json(OkResponse { ok: true }))
}

async fn thread_file_diff(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Query(query): Query<FileDiffQuery>,
) -> Result<Json<FileDiffDto>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    Ok(Json(
        diff::file_diff(
            &row,
            &BaselineDto {
                kind: parse_kind(query.kind.as_deref()),
                commit: query.commit,
            },
            &query.path,
        )
        .await?,
    ))
}

async fn mark_seen(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<SeenRequest>,
) -> Result<Json<OkResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    let baseline = request.baseline.unwrap_or(BaselineDto {
        kind: BaselineKind::BaseBranch,
        commit: None,
    });
    diff::mark_seen(
        &state.pool,
        &row,
        &user,
        &request.paths,
        request.seen,
        &baseline,
    )
    .await?;
    Ok(Json(OkResponse { ok: true }))
}

async fn merge_back(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<MergeBackRequest>,
) -> Result<Json<MergeBackResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let row = threads::load_row(&state.pool, &thread_id).await?;
    let result = diff::merge_back(
        &state.pool,
        &row,
        &user,
        request.commit,
        request.commit_message.as_deref(),
    )
    .await?;
    if let Some(commit) = result.commit.as_deref() {
        sqlx::query(
            "UPDATE threads SET base_sha = ?, last_turn_sha = ?, updated_at = ? WHERE id = ?",
        )
        .bind(commit)
        .bind(commit)
        .bind(threadbase_shared::now_ms())
        .bind(&thread_id)
        .execute(&state.pool)
        .await?;
    }
    Ok(Json(result))
}

fn parse_kind(value: Option<&str>) -> BaselineKind {
    match value {
        Some("last_commit") => BaselineKind::LastCommit,
        Some("last_turn") => BaselineKind::LastTurn,
        Some("commit") => BaselineKind::Commit,
        _ => BaselineKind::BaseBranch,
    }
}
