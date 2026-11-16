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
use threadbase_shared::api::{
    CreateProjectRequest, CreateThreadRequest, ForkThreadRequest, MergeBackRequest, SeenRequest,
    ThreadState, UpdateProjectRequest, UpdateThreadRequest,
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
            "/api/threads/{thread_id}",
            get(get_thread).patch(update_thread).delete(delete_thread),
        )
        .route("/api/threads/{thread_id}/fork", post(fork_thread))
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
        project: projects::update(
            &state.pool,
            &project_id,
            request.name.as_deref(),
            request.remote_url.as_deref(),
            request.path.as_deref(),
            request.verify_command.as_deref(),
            request.setup_command.as_deref(),
        )
        .await?,
    }))
}

#[derive(Debug, Deserialize)]
struct ThreadListQuery {
    include_archived: Option<bool>,
}

async fn list_threads(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<ThreadListQuery>,
) -> Result<Json<ThreadsResponse>, ServerError> {
    Ok(Json(ThreadsResponse {
        threads: threads::list(
            &state.pool,
            &project_id,
            query.include_archived.unwrap_or(false),
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
        threads::get(&state.pool, &thread_id, user).await?
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
    _user: CurrentUser,
    Query(query): Query<ThreadListQuery>,
) -> Result<Json<ThreadsResponse>, ServerError> {
    Ok(Json(ThreadsResponse {
        threads: threads::list_all(&state.pool, query.include_archived.unwrap_or(false)).await?,
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
        thread: threads::get(&state.pool, &thread_id, &user).await?,
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
    let thread =
        threads::update(&state.pool, &state.projection, &thread_id, &user, &request).await?;
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
        &state.projection,
        &state.config.data_dir,
        &thread_id,
        &user,
        &request,
    )
    .await?;
    let thread = if crate::setup::maybe_start(&state, thread.id.as_str()).await? {
        threads::get(&state.pool, thread.id.as_str(), &user).await?
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
