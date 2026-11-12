//! Thread-level roles and access checks.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_core::require_write;
use threadbase_shared::{ApiError, Role, ShareLevel};

/// Return the effective role for a user, or `None` when the thread is not visible.
///
/// A globally read-only account is clamped to `viewer` even when a thread row
/// says `editor`: the thread grant cannot raise the account-wide ceiling.
pub async fn role_for_user(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<Option<Role>, ServerError> {
    let role = raw_role_for_user(pool, thread_id, user).await?;
    Ok(role.map(|role| clamp_role(user, role)))
}

fn clamp_role(user: &CurrentUser, role: Role) -> Role {
    if user.is_admin() {
        return Role::Owner;
    }
    match user.user.access {
        threadbase_shared::UserAccess::Member => role,
        _ => Role::Viewer,
    }
}

async fn raw_role_for_user(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<Option<Role>, ServerError> {
    if user.is_admin() {
        return Ok(Some(Role::Owner));
    }
    let member: Option<String> =
        sqlx::query_scalar("SELECT role FROM thread_members WHERE thread_id = ? AND user_id = ?")
            .bind(thread_id)
            .bind(user.user.id.as_str())
            .fetch_optional(pool)
            .await?;
    if let Some(role) = member {
        return Ok(Some(Role::parse(&role)));
    }
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT created_by, share_level FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(pool)
            .await?;
    let Some((created_by, share_level)) = row else {
        return Ok(None);
    };
    if created_by == user.user.id.as_str() {
        return Ok(Some(Role::Owner));
    }
    if ShareLevel::parse(&share_level) == ShareLevel::Workspace {
        return Ok(Some(Role::Viewer));
    }
    Ok(None)
}

pub async fn require_read(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<Role, ServerError> {
    match role_for_user(pool, thread_id, user).await? {
        Some(role) => Ok(role),
        None => Err(ServerError(ApiError::not_found("thread not found"))),
    }
}

pub async fn require_write_role(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<Role, ServerError> {
    let role = require_read(pool, thread_id, user).await?;
    require_write(role).map_err(|_| ServerError(ApiError::forbidden("viewers cannot write")))?;
    Ok(role)
}

pub async fn require_manage_role(
    pool: &SqlitePool,
    thread_id: &str,
    user: &CurrentUser,
) -> Result<Role, ServerError> {
    let role = require_read(pool, thread_id, user).await?;
    if role.can_manage() {
        Ok(role)
    } else {
        Err(ServerError(ApiError::forbidden(
            "only the thread owner can manage this",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::CurrentUser;
    use crate::db;
    use threadbase_shared::{now_ms, UserDto};
    use ulid::Ulid;

    async fn user(pool: &SqlitePool, admin: bool) -> CurrentUser {
        let id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO users(id, username, display_name, is_admin, created_at) VALUES(?,?,?,?,?)",
        )
        .bind(&id)
        .bind(format!("u{}", &id[20..]))
        .bind("User")
        .bind(i64::from(admin))
        .bind(now_ms())
        .execute(pool)
        .await
        .unwrap();
        CurrentUser {
            user: UserDto {
                id: id.into(),
                username: "u".into(),
                display_name: "User".into(),
                avatar_url: None,
                is_admin: admin,
                access: threadbase_shared::UserAccess::Member,
                hidden_presence: false,
                created_at: now_ms(),
            },
            method: "github",
        }
    }

    async fn thread(pool: &SqlitePool, creator: &str, share: &str) -> String {
        let project = Ulid::new().to_string();
        let thread = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO projects(id, name, path, default_branch, created_by, created_at, updated_at) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(&project)
        .bind("p")
        .bind(format!("/tmp/{project}"))
        .bind("main")
        .bind(creator)
        .bind(now_ms())
        .bind(now_ms())
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch, created_by, share_level, created_at, updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&thread)
        .bind(&project)
        .bind("t")
        .bind("idle")
        .bind("codex")
        .bind("thread/t")
        .bind("main")
        .bind(creator)
        .bind(share)
        .bind(now_ms())
        .bind(now_ms())
        .execute(pool)
        .await
        .unwrap();
        thread
    }

    #[tokio::test]
    async fn creator_is_owner_and_workspace_sharing_is_read_only() {
        let pool = db::connect_memory().await.unwrap();
        let creator = user(&pool, false).await;
        let outsider = user(&pool, false).await;
        let thread_id = thread(&pool, creator.user.id.as_str(), "invite_only").await;
        assert_eq!(
            role_for_user(&pool, &thread_id, &creator).await.unwrap(),
            Some(Role::Owner)
        );
        assert_eq!(
            role_for_user(&pool, &thread_id, &outsider).await.unwrap(),
            None
        );
        sqlx::query("UPDATE threads SET share_level = 'workspace' WHERE id = ?")
            .bind(&thread_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            role_for_user(&pool, &thread_id, &outsider).await.unwrap(),
            Some(Role::Viewer)
        );
        assert!(require_write_role(&pool, &thread_id, &outsider)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn explicit_membership_beats_workspace_share() {
        let pool = db::connect_memory().await.unwrap();
        let creator = user(&pool, false).await;
        let editor = user(&pool, false).await;
        let thread_id = thread(&pool, creator.user.id.as_str(), "workspace").await;
        sqlx::query(
            "INSERT INTO thread_members(thread_id, user_id, role, joined_at) VALUES(?,?,?,?)",
        )
        .bind(&thread_id)
        .bind(editor.user.id.as_str())
        .bind("editor")
        .bind(now_ms())
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            role_for_user(&pool, &thread_id, &editor).await.unwrap(),
            Some(Role::Editor)
        );
        assert!(require_write_role(&pool, &thread_id, &editor).await.is_ok());
    }

    #[tokio::test]
    async fn admins_bypass_membership() {
        let pool = db::connect_memory().await.unwrap();
        let owner = user(&pool, false).await;
        let admin = user(&pool, true).await;
        let thread_id = thread(&pool, owner.user.id.as_str(), "invite_only").await;
        assert_eq!(
            role_for_user(&pool, &thread_id, &admin).await.unwrap(),
            Some(Role::Owner)
        );
    }
}
