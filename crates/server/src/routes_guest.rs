//! Guest endpoints: the link exchange, the guest's own read path, and the member
//! side that sends or dismisses a draft.
//!
//! Guest requests are not authenticated as a user. Everything a guest can reach
//! is derived from the cookie's link, so no handler here accepts a thread id from
//! a guest. Member endpoints live in the same module because they are the other
//! half of one feature, and they use the ordinary `CurrentUser` extractor.

use crate::app::AppState;
use crate::auth::CurrentUser;
use crate::authz;
use crate::error::ServerError;
use crate::guest;
use crate::threads;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use threadbase_shared::api::{
    GuestLinkResponse, GuestLinkStatus, GuestMessageDecision, GuestMessageDto, GuestMessageRequest,
    GuestMessagesResponse, GuestSessionRequest, GuestThreadResponse,
};
use threadbase_shared::{now_ms, ThreadId};

/// `enabled` comes from the caller so a disabled feature registers no routes at
/// all: axum runs a body extractor before the handler, so a guard inside the
/// handler would answer 422 and confirm the endpoint exists.
pub fn router(enabled: bool) -> Router<AppState> {
    if !enabled {
        return Router::new();
    }
    Router::new()
        .route("/api/guest/session", post(exchange))
        .route("/api/guest/thread", get(thread))
        .route("/api/guest/messages", post(draft))
        .route(
            "/api/threads/{thread_id}/guest-link",
            get(link_status).post(create_link).delete(revoke_link),
        )
        .route(
            "/api/threads/{thread_id}/guest-messages",
            get(pending_messages),
        )
        .route(
            "/api/threads/{thread_id}/guest-messages/{message_id}",
            post(decide_message),
        )
}

/// Exchange the link token for a cookie-bound session.
async fn exchange(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GuestSessionRequest>,
) -> Result<impl IntoResponse, ServerError> {
    let token = request.token.trim();
    let user_agent = headers
        .get("user-agent")
        .and_then(|value| value.to_str().ok());
    let (session, context) = guest::exchange(&state.pool, token, user_agent).await?;
    let cookie = crate::auth_session::cookie_header(
        guest::COOKIE,
        &session,
        state.config.cookie_secure,
        guest::LINK_SECONDS,
    );
    let body = guest_thread_at(&state, &context, 0).await?;
    Ok(([(axum::http::header::SET_COOKIE, cookie)], Json(body)))
}

#[derive(Debug, Deserialize)]
struct GuestThreadQuery {
    after: Option<u64>,
}

async fn thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GuestThreadQuery>,
) -> Result<Json<GuestThreadResponse>, ServerError> {
    Ok(Json(
        guest_thread(&state, &headers, query.after.unwrap_or(0)).await?,
    ))
}

async fn guest_thread(
    state: &AppState,
    headers: &HeaderMap,
    after: u64,
) -> Result<GuestThreadResponse, ServerError> {
    let context = guest::context(&state.pool, headers).await?;
    guest_thread_at(state, &context, after).await
}

async fn guest_thread_at(
    state: &AppState,
    context: &guest::GuestContext,
    after: u64,
) -> Result<GuestThreadResponse, ServerError> {
    let row = threads::load_row(&state.pool, &context.thread_id).await?;
    // `after == 0` returns the compacted tail, which is what a guest opening the
    // page needs; any other cursor continues the transcript.
    let mut page = state
        .events
        .page(&ThreadId::from(context.thread_id.clone()), after, 200)
        .await
        .map_err(|error| ServerError::internal(error.to_string()))?;
    // A guest reads the transcript, not the workbench. Filtering here rather than
    // in the UI is what makes it a boundary: queued member drafts, approval
    // requests, questions and subagent rows stay out of the payload entirely.
    page.events
        .retain(|envelope| guest_visible(&envelope.payload));
    Ok(GuestThreadResponse {
        title: row.title,
        agent: row.agent,
        model: row.model,
        expires_at: context.link_expires_at.min(context.session_expires_at),
        page,
    })
}

/// A guest may only leave a draft. Nothing runs until a person sends it.
/// Events that belong to the shared story of a thread.
fn guest_visible(payload: &threadbase_shared::ThreadEvent) -> bool {
    use threadbase_shared::ThreadEvent as E;
    matches!(
        payload,
        E::TurnStarted { .. }
            | E::MessageDelta { .. }
            | E::MessageCompleted { .. }
            | E::ThinkingDelta { .. }
            | E::ThinkingCompleted { .. }
            | E::ToolCall { .. }
            | E::ToolCallUpdate { .. }
            | E::PlanUpdated { .. }
            | E::UsageUpdated { .. }
            | E::TurnEnded { .. }
            | E::TurnError { .. }
            | E::AgentMessage { .. }
            | E::ContextCompacted { .. }
            | E::SystemNotice { .. }
    )
}

async fn draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<GuestMessageRequest>,
) -> Result<Json<GuestMessageDto>, ServerError> {
    let context = guest::context(&state.pool, &headers).await?;
    let id = guest::draft(&state.pool, &context, &request.text).await?;
    Ok(Json(GuestMessageDto {
        id,
        text: request.text.trim().to_owned(),
        status: threadbase_shared::GuestMessageStatus::Pending,
        created_at: now_ms(),
    }))
}

async fn link_status(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<GuestLinkStatus>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    let expires_at = guest::active_link(&state.pool, &thread_id).await?;
    Ok(Json(GuestLinkStatus {
        active: expires_at.is_some(),
        expires_at,
    }))
}

async fn create_link(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<GuestLinkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    let (token, expires_at) =
        guest::create_link(&state.pool, &thread_id, user.user.id.as_str()).await?;
    let origin = state
        .config
        .public_origin
        .clone()
        .unwrap_or_else(|| format!("http://{}", state.config.bind));
    Ok(Json(GuestLinkResponse {
        url: format!("{origin}/guest/{token}"),
        expires_at,
    }))
}

async fn revoke_link(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<GuestLinkResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    guest::revoke_thread_links(&state.pool, &thread_id).await?;
    Ok(Json(GuestLinkResponse {
        url: String::new(),
        expires_at: 0,
    }))
}

async fn pending_messages(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<GuestMessagesResponse>, ServerError> {
    authz::require_read(&state.pool, &thread_id, &user).await?;
    Ok(Json(GuestMessagesResponse {
        messages: guest::list_for_thread(&state.pool, &thread_id).await?,
    }))
}

async fn decide_message(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((thread_id, message_id)): Path<(String, String)>,
    Json(request): Json<GuestMessageDecision>,
) -> Result<Json<GuestMessagesResponse>, ServerError> {
    authz::require_write_role(&state.pool, &thread_id, &user).await?;
    match request.action {
        threadbase_shared::api::GuestMessageAction::Dismiss => {
            guest::dismiss(&state.pool, &thread_id, &message_id, user.user.id.as_str()).await?;
        }
        threadbase_shared::api::GuestMessageAction::Send => {
            // Claim first so two people pressing Send cannot start two turns.
            let text =
                guest::claim(&state.pool, &thread_id, &message_id, user.user.id.as_str()).await?;
            // The turn belongs to the person who pressed Send: guests have no
            // credentials and no role, so the transcript names a real member.
            if let Err(error) = state.agents.send(&thread_id, &user, &text, None).await {
                guest::release(&state.pool, &message_id).await;
                return Err(error);
            }
        }
    }
    Ok(Json(GuestMessagesResponse {
        messages: guest::list_for_thread(&state.pool, &thread_id).await?,
    }))
}
