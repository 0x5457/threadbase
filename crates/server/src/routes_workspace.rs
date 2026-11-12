//! Project, thread and change-review routes.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use crate::diff;
use crate::error::ServerError;
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
    ApiError, BaselineDto, BaselineKind, FileDiffDto, MergeBackResponse, OkResponse,
    ProjectResponse, ProjectsResponse, ThreadResponse, ThreadsResponse,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/threads", get(list_all_threads))
        .route("/api/threads/{thread_id}/bundle", get(thread_bundle))
        .route("/api/threads/{thread_id}/state", get(thread_state))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/{project_id}",
            get(get_project).patch(update_project),
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
    Ok(Json(ProjectResponse {
        project: projects::create(&state.pool, &user, &request).await?,
    }))
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
    user: CurrentUser,
    Path(project_id): Path<String>,
    Query(query): Query<ThreadListQuery>,
) -> Result<Json<ThreadsResponse>, ServerError> {
    Ok(Json(ThreadsResponse {
        threads: threads::list(
            &state.pool,
            &project_id,
            &user,
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
    let thread = threads::create(
        &state.pool,
        &state.projection,
        &state.config.data_dir,
        &user,
        &project_id,
        &request,
    )
    .await?;
    let manager = state.agents.clone();
    let thread_id = thread.id.0.clone();
    let has_initial_prompt = request
        .initial_prompt
        .as_deref()
        .is_some_and(|prompt| !prompt.trim().is_empty());
    tokio::spawn(async move {
        match manager.ensure_started(&thread_id).await {
            Ok(()) if has_initial_prompt => manager.kick_queue(&thread_id),
            Ok(()) => {}
            Err(error) => tracing::warn!(thread_id, %error, "agent startup failed"),
        }
    });
    Ok(Json(ThreadResponse { thread }))
}

async fn list_all_threads(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ThreadListQuery>,
) -> Result<Json<ThreadsResponse>, ServerError> {
    Ok(Json(ThreadsResponse {
        threads: threads::list_all(&state.pool, &user, query.include_archived.unwrap_or(false))
            .await?,
    }))
}

async fn thread_bundle(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<threadbase_shared::ThreadBundleResponse>, ServerError> {
    let mut bundle = threads::bundle(&state.pool, &thread_id, &user).await?;
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

async fn update_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<UpdateThreadRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
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
            if let Err(error) = state.agents.set_config(&thread_id, model, effort).await {
                tracing::warn!(thread_id, %error, "could not apply model/effort to agent");
            }
        }
    }
    Ok(Json(ThreadResponse { thread }))
}

#[derive(Debug, Deserialize)]
struct DeleteQuery {
    confirm: Option<bool>,
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
    threads::delete(&state.pool, &thread_id, &user).await?;
    Ok(Json(OkResponse { ok: true }))
}

async fn fork_thread(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<ForkThreadRequest>,
) -> Result<Json<ThreadResponse>, ServerError> {
    Ok(Json(ThreadResponse {
        thread: threads::fork(
            &state.pool,
            &state.projection,
            &state.config.data_dir,
            &thread_id,
            &user,
            &request,
        )
        .await?,
    }))
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
