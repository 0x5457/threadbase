//! Prompt images: storage, lookup and conversion to agent prompt parts.

use crate::error::ServerError;
use base64::Engine;
use sqlx::{FromRow, SqlitePool};
use std::path::{Path, PathBuf};
use threadbase_agent::PromptPart;
use threadbase_shared::{now_ms, ApiError, AttachmentDto};
use ulid::Ulid;

/// Images accepted in one prompt.
pub const MAX_IMAGES: usize = 4;
/// Decoded size cap per image.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, FromRow)]
struct Row {
    id: String,
    thread_id: String,
    mime_type: String,
    byte_size: i64,
    path: String,
}

/// A stored image plus the thread that owns it, for authz on the read route.
pub struct StoredAttachment {
    pub dto: AttachmentDto,
    pub thread_id: String,
    pub path: PathBuf,
}

/// Whitelisted image types, mapped to the extension used on disk.
#[must_use]
pub fn extension(mime_type: &str) -> Option<&'static str> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

fn to_dto(row: &Row) -> AttachmentDto {
    AttachmentDto {
        id: row.id.clone(),
        mime_type: row.mime_type.clone(),
        byte_size: row.byte_size,
    }
}

/// Persist one uploaded image under the data directory and record it.
pub async fn create(
    pool: &SqlitePool,
    data_dir: &Path,
    thread_id: &str,
    user_id: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<AttachmentDto, ServerError> {
    let normalized = mime_type.trim().to_ascii_lowercase();
    let extension = extension(&normalized)
        .ok_or_else(|| ServerError(ApiError::validation("unsupported image type")))?;
    if bytes.is_empty() {
        return Err(ServerError(ApiError::validation("image is empty")));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ServerError(ApiError::validation("image is too large")));
    }
    let id = Ulid::new().to_string();
    let dir = data_dir.join("attachments").join(thread_id);
    tokio::fs::create_dir_all(&dir).await.map_err(|error| {
        ServerError::internal(format!("could not create attachment dir: {error}"))
    })?;
    let path = dir.join(format!("{id}.{extension}"));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| ServerError::internal(format!("could not store image: {error}")))?;
    let byte_size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    if let Err(error) = sqlx::query(
        "INSERT INTO attachments(id, thread_id, mime_type, byte_size, path, created_by, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(thread_id)
    .bind(&normalized)
    .bind(byte_size)
    .bind(path.to_string_lossy().to_string())
    .bind(user_id)
    .bind(now_ms())
    .execute(pool)
    .await
    {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(error.into());
    }
    Ok(AttachmentDto {
        id,
        mime_type: normalized,
        byte_size,
    })
}

/// Resolve ids a prompt references, scoped to the thread and in send order.
pub async fn resolve(
    pool: &SqlitePool,
    thread_id: &str,
    ids: &[String],
) -> Result<Vec<AttachmentDto>, ServerError> {
    if ids.len() > MAX_IMAGES {
        return Err(ServerError(ApiError::validation("too many images")));
    }
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let row: Option<Row> = sqlx::query_as::<_, Row>(
            "SELECT id, thread_id, mime_type, byte_size, path FROM attachments
             WHERE id = ? AND thread_id = ?",
        )
        .bind(id)
        .bind(thread_id)
        .fetch_optional(pool)
        .await?;
        let Some(row) = row else {
            return Err(ServerError(ApiError::not_found("image not found")));
        };
        out.push(to_dto(&row));
    }
    Ok(out)
}

/// Look one image up for the read route.
pub async fn stored(pool: &SqlitePool, id: &str) -> Result<StoredAttachment, ServerError> {
    let row: Option<Row> = sqlx::query_as::<_, Row>(
        "SELECT id, thread_id, mime_type, byte_size, path FROM attachments WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Err(ServerError(ApiError::not_found("image not found")));
    };
    Ok(StoredAttachment {
        dto: to_dto(&row),
        thread_id: row.thread_id,
        path: PathBuf::from(row.path),
    })
}

/// Read the referenced images back as base64 prompt blocks. Called as the turn
/// starts, so a missing file fails the prompt instead of silently dropping it.
pub async fn prompt_parts(
    pool: &SqlitePool,
    attachments: &[AttachmentDto],
) -> Result<Vec<PromptPart>, ServerError> {
    let mut parts = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let stored = stored(pool, &attachment.id).await?;
        let bytes = tokio::fs::read(&stored.path).await.map_err(|error| {
            ServerError::internal(format!("could not read attachment: {error}"))
        })?;
        parts.push(PromptPart::Image {
            mime_type: stored.dto.mime_type,
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::TempDir;

    #[tokio::test]
    async fn stores_resolves_and_reads_back_an_image() {
        let pool = db::connect_memory().await.unwrap();
        let (_user, _project, thread) = crate::events::test_support::seed_thread(&pool).await;
        let user_id: String = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let data = TempDir::new().unwrap();
        let bytes = b"\x89PNG\r\n\x1a\nfake";

        let attachment = create(
            &pool,
            data.path(),
            thread.as_str(),
            &user_id,
            "image/png",
            bytes,
        )
        .await
        .unwrap();
        assert_eq!(attachment.mime_type, "image/png");
        assert_eq!(attachment.byte_size, i64::try_from(bytes.len()).unwrap());

        let resolved = resolve(&pool, thread.as_str(), std::slice::from_ref(&attachment.id))
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);

        let stored = stored(&pool, &attachment.id).await.unwrap();
        assert_eq!(stored.thread_id, thread.as_str());
        assert_eq!(tokio::fs::read(&stored.path).await.unwrap(), bytes);

        let parts = prompt_parts(&pool, &resolved).await.unwrap();
        assert!(matches!(
            &parts[0],
            PromptPart::Image { mime_type, .. } if mime_type == "image/png"
        ));
    }

    #[tokio::test]
    async fn rejects_unknown_types_and_cross_thread_ids() {
        let pool = db::connect_memory().await.unwrap();
        let (_user, _project, thread) = crate::events::test_support::seed_thread(&pool).await;
        let user_id: String = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let data = TempDir::new().unwrap();
        let error = create(
            &pool,
            data.path(),
            thread.as_str(),
            &user_id,
            "image/svg+xml",
            b"x",
        )
        .await
        .unwrap_err();
        assert_eq!(error.0.code, threadbase_shared::ErrorCode::Validation);

        let attachment = create(
            &pool,
            data.path(),
            thread.as_str(),
            &user_id,
            "image/png",
            b"x",
        )
        .await
        .unwrap();
        let error = resolve(&pool, "other-thread", &[attachment.id])
            .await
            .unwrap_err();
        assert_eq!(error.0.code, threadbase_shared::ErrorCode::NotFound);
    }
}
