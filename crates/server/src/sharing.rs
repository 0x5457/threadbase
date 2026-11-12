//! Thread membership, two-tier sharing and expiring invites.

use crate::auth::{audit, CurrentUser};
use crate::error::ServerError;
use crate::events::EventStore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use threadbase_shared::api::{CreateInviteRequest, InviteResponse};
use threadbase_shared::{
    now_ms, user_color, Actor, ApiError, InviteDto, ShareLevel, ThreadEvent, ThreadMemberDto,
    UserBrief,
};
use ulid::Ulid;

pub async fn add_member(
    pool: &SqlitePool,
    events: &EventStore,
    thread_id: &str,
    actor: &CurrentUser,
    username: &str,
    role: threadbase_shared::Role,
) -> Result<ThreadMemberDto, ServerError> {
    let target = crate::auth::load_user_by_username(pool, username.trim())
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("user not found")))?;
    let now = now_ms();
    sqlx::query(
        "INSERT INTO thread_members(thread_id, user_id, role, joined_at) VALUES(?, ?, ?, ?)
         ON CONFLICT(thread_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(thread_id)
    .bind(target.id.as_str())
    .bind(role.as_str())
    .bind(now)
    .execute(pool)
    .await?;
    let member = ThreadMemberDto {
        user: brief(&target),
        role,
        joined_at: now,
    };
    events
        .append(
            &threadbase_shared::ThreadId::from(thread_id.to_owned()),
            Actor::user(&actor_brief(actor)),
            ThreadEvent::MembersChanged {
                member: member.clone(),
                action: "added".into(),
            },
            Default::default(),
        )
        .await?;
    audit(
        pool,
        Some(actor.user.id.as_str()),
        "thread.member_added",
        Some("thread"),
        Some(thread_id),
        Some(target.id.as_str()),
        None,
    )
    .await;
    Ok(member)
}

pub async fn update_member(
    pool: &SqlitePool,
    events: &EventStore,
    thread_id: &str,
    actor: &CurrentUser,
    target_id: &str,
    role: threadbase_shared::Role,
) -> Result<ThreadMemberDto, ServerError> {
    if role != threadbase_shared::Role::Owner {
        let owners: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM thread_members WHERE thread_id = ? AND role = 'owner'",
        )
        .bind(thread_id)
        .fetch_one(pool)
        .await?;
        let current: Option<String> = sqlx::query_scalar(
            "SELECT role FROM thread_members WHERE thread_id = ? AND user_id = ?",
        )
        .bind(thread_id)
        .bind(target_id)
        .fetch_optional(pool)
        .await?;
        if owners <= 1 && current.as_deref() == Some("owner") {
            return Err(ServerError(ApiError::conflict(
                "a thread must keep at least one owner",
            )));
        }
    }
    let result =
        sqlx::query("UPDATE thread_members SET role = ? WHERE thread_id = ? AND user_id = ?")
            .bind(role.as_str())
            .bind(thread_id)
            .bind(target_id)
            .execute(pool)
            .await?;
    if result.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found("thread member not found")));
    }
    let user = crate::auth::load_user_by_id(pool, target_id)
        .await?
        .ok_or_else(|| ServerError(ApiError::not_found("user not found")))?;
    let member = ThreadMemberDto {
        user: brief(&user),
        role,
        joined_at: now_ms(),
    };
    events
        .append(
            &threadbase_shared::ThreadId::from(thread_id.to_owned()),
            Actor::user(&actor_brief(actor)),
            ThreadEvent::MembersChanged {
                member: member.clone(),
                action: "role_changed".into(),
            },
            Default::default(),
        )
        .await?;
    Ok(member)
}

pub async fn remove_member(
    pool: &SqlitePool,
    thread_id: &str,
    actor: &CurrentUser,
    target_id: &str,
) -> Result<(), ServerError> {
    let owner_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thread_members WHERE thread_id = ? AND role = 'owner'",
    )
    .bind(thread_id)
    .fetch_one(pool)
    .await?;
    let target_role: Option<String> =
        sqlx::query_scalar("SELECT role FROM thread_members WHERE thread_id = ? AND user_id = ?")
            .bind(thread_id)
            .bind(target_id)
            .fetch_optional(pool)
            .await?;
    if owner_count <= 1 && target_role.as_deref() == Some("owner") {
        return Err(ServerError(ApiError::conflict(
            "a thread must keep at least one owner",
        )));
    }
    sqlx::query("DELETE FROM thread_members WHERE thread_id = ? AND user_id = ?")
        .bind(thread_id)
        .bind(target_id)
        .execute(pool)
        .await?;
    audit(
        pool,
        Some(actor.user.id.as_str()),
        "thread.member_removed",
        Some("thread"),
        Some(thread_id),
        Some(target_id),
        None,
    )
    .await;
    Ok(())
}

pub async fn create_invite(
    pool: &SqlitePool,
    thread_id: &str,
    actor: &CurrentUser,
    request: &CreateInviteRequest,
) -> Result<InviteResponse, ServerError> {
    let role = request.role.unwrap_or(threadbase_shared::Role::Editor);
    if let Some(username) = request
        .username
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let target = crate::auth::load_user_by_username(pool, username.trim())
            .await?
            .ok_or_else(|| ServerError(ApiError::not_found("user not found")))?;
        let now = now_ms();
        sqlx::query(
            "INSERT INTO thread_members(thread_id, user_id, role, joined_at) VALUES(?, ?, ?, ?)
             ON CONFLICT(thread_id, user_id) DO UPDATE SET role = excluded.role",
        )
        .bind(thread_id)
        .bind(target.id.as_str())
        .bind(role.as_str())
        .bind(now)
        .execute(pool)
        .await?;
        let invite = InviteDto {
            id: Ulid::new().to_string().into(),
            thread_id: thread_id.to_owned().into(),
            role,
            email: request.email.clone(),
            created_at: now,
            expires_at: now + 14 * 24 * 60 * 60 * 1000,
            accepted_at: Some(now),
        };
        return Ok(InviteResponse {
            invite,
            token: String::new(),
        });
    }
    let token = format!("{}{}", Ulid::new(), Ulid::new());
    let hash = hash_token(&token);
    let id = Ulid::new().to_string();
    let now = now_ms();
    let expires_at = now + 14 * 24 * 60 * 60 * 1000;
    sqlx::query(
        "INSERT INTO invites(id, thread_id, token_hash, role, email, created_by, created_at, expires_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(thread_id)
    .bind(&hash)
    .bind(role.as_str())
    .bind(request.email.as_deref())
    .bind(actor.user.id.as_str())
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;
    audit(
        pool,
        Some(actor.user.id.as_str()),
        "thread.invite_created",
        Some("invite"),
        Some(&id),
        Some(&hash),
        None,
    )
    .await;
    Ok(InviteResponse {
        invite: InviteDto {
            id: id.into(),
            thread_id: thread_id.to_owned().into(),
            role,
            email: request.email.clone(),
            created_at: now,
            expires_at,
            accepted_at: None,
        },
        token,
    })
}

pub async fn accept_invite(
    pool: &SqlitePool,
    token: &str,
    user: &CurrentUser,
) -> Result<ThreadMemberDto, ServerError> {
    let hash = hash_token(token);
    let row: Option<(String, String, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, thread_id, role, expires_at, accepted_at FROM invites WHERE token_hash = ?",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await?;
    let Some((id, thread_id, role, expires_at, accepted_at)) = row else {
        return Err(ServerError(ApiError::not_found("invite not found")));
    };
    if accepted_at.is_some() {
        return Err(ServerError(ApiError::conflict("invite already used")));
    }
    if expires_at < now_ms() {
        return Err(ServerError(ApiError::conflict("invite expired")));
    }
    let role = threadbase_shared::Role::parse(&role);
    sqlx::query(
        "INSERT INTO thread_members(thread_id, user_id, role, joined_at) VALUES(?, ?, ?, ?)
         ON CONFLICT(thread_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(&thread_id)
    .bind(user.user.id.as_str())
    .bind(role.as_str())
    .bind(now_ms())
    .execute(pool)
    .await?;
    sqlx::query("UPDATE invites SET accepted_by = ?, accepted_at = ? WHERE id = ?")
        .bind(user.user.id.as_str())
        .bind(now_ms())
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(ThreadMemberDto {
        user: brief(&user.user),
        role,
        joined_at: now_ms(),
    })
}

pub async fn set_share_level(
    pool: &SqlitePool,
    thread_id: &str,
    level: ShareLevel,
) -> Result<(), ServerError> {
    sqlx::query("UPDATE threads SET share_level = ?, updated_at = ? WHERE id = ?")
        .bind(match level {
            ShareLevel::InviteOnly => "invite_only",
            ShareLevel::Workspace => "workspace",
        })
        .bind(now_ms())
        .bind(thread_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn brief(user: &threadbase_shared::UserDto) -> UserBrief {
    UserBrief {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: user.avatar_url.clone(),
        color: user_color(user.id.as_str()).to_owned(),
    }
}

fn actor_brief(user: &CurrentUser) -> UserBrief {
    brief(&user.user)
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

use axum::extract::{Path, State};
use axum::routing::{patch, post};
use axum::Json;
use axum::Router;
use threadbase_shared::api::{AddMemberRequest, UpdateMemberRequest, UpdateShareRequest};

pub fn router() -> Router<crate::app::AppState> {
    Router::new()
        .route("/api/threads/{thread_id}/members", post(add_member_route))
        .route(
            "/api/threads/{thread_id}/members/{user_id}",
            patch(update_member_route).delete(remove_member_route),
        )
        .route(
            "/api/threads/{thread_id}/invites",
            post(create_invite_route),
        )
        .route("/api/invites/{token}/accept", post(accept_invite_route))
        .route(
            "/api/threads/{thread_id}/invites/{invite_id}",
            axum::routing::delete(revoke_invite_route),
        )
        .route("/api/threads/{thread_id}/share", patch(set_share_route))
        .route(
            "/api/threads/{thread_id}/request-access",
            post(request_access_route),
        )
}

async fn add_member_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<AddMemberRequest>,
) -> Result<Json<threadbase_shared::ThreadMemberDto>, ServerError> {
    crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
    Ok(Json(
        add_member(
            &state.pool,
            &state.events,
            &thread_id,
            &user,
            &request.username,
            request.role,
        )
        .await?,
    ))
}

async fn update_member_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path((thread_id, target_id)): Path<(String, String)>,
    Json(request): Json<UpdateMemberRequest>,
) -> Result<Json<threadbase_shared::ThreadMemberDto>, ServerError> {
    crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
    Ok(Json(
        update_member(
            &state.pool,
            &state.events,
            &thread_id,
            &user,
            &target_id,
            request.role,
        )
        .await?,
    ))
}

async fn remove_member_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path((thread_id, target_id)): Path<(String, String)>,
) -> Result<Json<threadbase_shared::OkResponse>, ServerError> {
    crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
    remove_member(&state.pool, &thread_id, &user, &target_id).await?;
    Ok(Json(threadbase_shared::OkResponse { ok: true }))
}

async fn create_invite_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<CreateInviteRequest>,
) -> Result<Json<InviteResponse>, ServerError> {
    crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
    Ok(Json(
        create_invite(&state.pool, &thread_id, &user, &request).await?,
    ))
}

async fn revoke_invite_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path((thread_id, invite_id)): Path<(String, String)>,
) -> Result<Json<threadbase_shared::OkResponse>, ServerError> {
    crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
    revoke_invite(&state.pool, &thread_id, &invite_id, &user).await?;
    Ok(Json(threadbase_shared::OkResponse { ok: true }))
}

async fn accept_invite_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path(token): Path<String>,
) -> Result<Json<threadbase_shared::ThreadMemberDto>, ServerError> {
    Ok(Json(accept_invite(&state.pool, &token, &user).await?))
}

async fn set_share_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
    Json(request): Json<UpdateShareRequest>,
) -> Result<Json<threadbase_shared::OkResponse>, ServerError> {
    crate::authz::require_manage_role(&state.pool, &thread_id, &user).await?;
    set_share_level(&state.pool, &thread_id, request.share_level).await?;
    Ok(Json(threadbase_shared::OkResponse { ok: true }))
}

pub async fn revoke_invite(
    pool: &SqlitePool,
    thread_id: &str,
    invite_id: &str,
    actor: &CurrentUser,
) -> Result<(), ServerError> {
    let result = sqlx::query("DELETE FROM invites WHERE id = ? AND thread_id = ?")
        .bind(invite_id)
        .bind(thread_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ServerError(ApiError::not_found("invite not found")));
    }
    audit(
        pool,
        Some(actor.user.id.as_str()),
        "thread.invite_revoked",
        Some("invite"),
        Some(invite_id),
        None,
        None,
    )
    .await;
    Ok(())
}

async fn request_access_route(
    State(state): State<crate::app::AppState>,
    user: CurrentUser,
    Path(thread_id): Path<String>,
) -> Result<Json<threadbase_shared::OkResponse>, ServerError> {
    crate::authz::require_read(&state.pool, &thread_id, &user).await?;
    crate::inbox::request_access(&state.pool, &thread_id, &user).await?;
    Ok(Json(threadbase_shared::OkResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::events::test_support::seed_thread;

    #[tokio::test]
    async fn invite_accept_share_and_remove() {
        let pool = db::connect_memory().await.unwrap();
        let (owner_id, _, thread_id) = seed_thread(&pool).await;
        let owner = crate::auth::load_user_by_id(&pool, owner_id.as_str())
            .await
            .unwrap()
            .unwrap();
        let actor = CurrentUser {
            user: owner,
            method: "github",
        };
        let target = crate::auth::ensure_identity_user(
            &pool,
            &crate::auth::Identity::github("invitee", Some("Invitee"), None),
            &[],
        )
        .await
        .unwrap();
        let invite = create_invite(
            &pool,
            thread_id.as_str(),
            &actor,
            &threadbase_shared::api::CreateInviteRequest {
                email: None,
                username: None,
                role: Some(threadbase_shared::Role::Editor),
            },
        )
        .await
        .unwrap();
        assert!(!invite.token.is_empty());
        let invitee = CurrentUser {
            user: target.clone(),
            method: "github",
        };
        let member = accept_invite(&pool, &invite.token, &invitee).await.unwrap();
        assert_eq!(member.user.id, target.id);
        set_share_level(&pool, thread_id.as_str(), ShareLevel::Workspace)
            .await
            .unwrap();
        let level: String = sqlx::query_scalar("SELECT share_level FROM threads WHERE id = ?")
            .bind(thread_id.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(level, "workspace");
        let pending = create_invite(
            &pool,
            thread_id.as_str(),
            &actor,
            &threadbase_shared::api::CreateInviteRequest {
                email: Some("x@example.com".into()),
                username: None,
                role: Some(threadbase_shared::Role::Viewer),
            },
        )
        .await
        .unwrap();
        revoke_invite(
            &pool,
            thread_id.as_str(),
            pending.invite.id.as_str(),
            &actor,
        )
        .await
        .unwrap();
        let invites: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM invites WHERE thread_id = ?")
            .bind(thread_id.as_str())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(invites, 1);
        remove_member(&pool, thread_id.as_str(), &actor, target.id.as_str())
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM thread_members WHERE thread_id = ? AND user_id = ?",
        )
        .bind(thread_id.as_str())
        .bind(target.id.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }
}
