//! Thread-level roles and access checks.

use crate::auth::CurrentUser;
use crate::error::ServerError;
use sqlx::SqlitePool;
use threadbase_core::require_write;
use threadbase_shared::{ApiError, Role};

/// Return the effective role for a user, or `None` when the thread does not exist.
///
/// Every signed-in account sees every thread: the creator (and admins) own it,
/// other members can write anywhere, and a globally read-only account is clamped
/// to `viewer`.
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
    let created_by: Option<String> =
        sqlx::query_scalar("SELECT created_by FROM threads WHERE id = ?")
            .bind(thread_id)
            .fetch_optional(pool)
            .await?;
    let Some(created_by) = created_by else {
        return Ok(None);
    };
    if created_by == user.user.id.as_str() {
        return Ok(Some(Role::Owner));
    }
    Ok(Some(Role::Editor))
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
    use threadbase_shared::{now_ms, UserAccess, UserDto};
    use ulid::Ulid;

    async fn user(pool: &SqlitePool, admin: bool, access: UserAccess) -> CurrentUser {
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
                access,
                hidden_presence: false,
                created_at: now_ms(),
            },
            method: "github",
        }
    }

    async fn thread(pool: &SqlitePool, creator: &str) -> String {
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
            "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch, created_by, created_at, updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&thread)
        .bind(&project)
        .bind("t")
        .bind("idle")
        .bind("codex")
        .bind("thread/t")
        .bind("main")
        .bind(creator)
        .bind(now_ms())
        .bind(now_ms())
        .execute(pool)
        .await
        .unwrap();
        thread
    }

    #[tokio::test]
    async fn every_member_sees_every_thread() {
        let pool = db::connect_memory().await.unwrap();
        let creator = user(&pool, false, UserAccess::Member).await;
        let outsider = user(&pool, false, UserAccess::Member).await;
        let thread_id = thread(&pool, creator.user.id.as_str()).await;
        assert_eq!(
            role_for_user(&pool, &thread_id, &creator).await.unwrap(),
            Some(Role::Owner)
        );
        assert_eq!(
            role_for_user(&pool, &thread_id, &outsider).await.unwrap(),
            Some(Role::Editor)
        );
        assert!(require_write_role(&pool, &thread_id, &outsider)
            .await
            .is_ok());
        assert!(require_manage_role(&pool, &thread_id, &outsider)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn read_only_accounts_are_clamped_to_viewer() {
        let pool = db::connect_memory().await.unwrap();
        let creator = user(&pool, false, UserAccess::Member).await;
        let reader = user(&pool, false, UserAccess::ReadOnly).await;
        let thread_id = thread(&pool, creator.user.id.as_str()).await;
        assert_eq!(
            role_for_user(&pool, &thread_id, &reader).await.unwrap(),
            Some(Role::Viewer)
        );
        assert!(require_write_role(&pool, &thread_id, &reader)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn admins_are_owners_and_missing_threads_are_invisible() {
        let pool = db::connect_memory().await.unwrap();
        let owner = user(&pool, false, UserAccess::Member).await;
        let admin = user(&pool, true, UserAccess::Member).await;
        let thread_id = thread(&pool, owner.user.id.as_str()).await;
        assert_eq!(
            role_for_user(&pool, &thread_id, &admin).await.unwrap(),
            Some(Role::Owner)
        );
        assert_eq!(role_for_user(&pool, "missing", &admin).await.unwrap(), None);
    }
}
