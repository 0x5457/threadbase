//! Prompt attachments: storage, lookup, download headers and agent prompt parts.
//!
//! Images stay inline in the chat. Every other file is stored under the data
//! directory, served only as a download, and described to the agent with an
//! absolute path; small text files are also inlined so common pastes need no
//! extra file read.

use crate::error::ServerError;
use base64::Engine;
use sqlx::{FromRow, SqlitePool};
use std::path::{Path, PathBuf};
use threadbase_agent::PromptPart;
use threadbase_shared::{now_ms, ApiError, AttachmentDto};
use tokio::io::AsyncReadExt;
use ulid::Ulid;

/// Attachments accepted in one prompt.
pub const MAX_ATTACHMENTS: usize = 8;
/// Decoded size cap per image.
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Decoded size cap per non-image file.
pub const MAX_FILE_BYTES: usize = 25 * 1024 * 1024;
/// Text files up to this size are inlined in the prompt instead of only linked.
const MAX_INLINE_TEXT_BYTES: usize = 32 * 1024;
/// Larger text files get this much preview plus the absolute path.
const TEXT_PREVIEW_BYTES: usize = 4 * 1024;

#[derive(Debug, FromRow)]
struct Row {
    id: String,
    thread_id: String,
    mime_type: String,
    byte_size: i64,
    path: String,
    file_name: Option<String>,
}

/// A stored attachment plus the thread that owns it, for authz on the read route.
pub struct StoredAttachment {
    pub dto: AttachmentDto,
    pub thread_id: String,
    pub path: PathBuf,
}

/// Inline-previewable image types, mapped to the extension used on disk.
/// SVG and other image MIME types are accepted as ordinary download files.
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

#[must_use]
pub fn is_image_mime(mime_type: &str) -> bool {
    extension(mime_type).is_some()
}

fn to_dto(row: &Row) -> AttachmentDto {
    AttachmentDto {
        id: row.id.clone(),
        mime_type: row.mime_type.clone(),
        byte_size: row.byte_size,
        file_name: row.file_name.clone(),
    }
}

/// Keep only a displayable base name; never use this for the disk path.
#[must_use]
pub fn clean_file_name(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let base = raw.rsplit(['/', '\\']).next()?.trim();
    let cleaned: String = base
        .chars()
        .filter(|character| !character.is_control())
        .collect();
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return None;
    }
    Some(cleaned.chars().take(200).collect())
}

fn safe_extension(file_name: &str) -> Option<String> {
    let extension = file_name.rsplit_once('.')?.1;
    let extension = extension.to_ascii_lowercase();
    if (1..=12).contains(&extension.len())
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        Some(extension)
    } else {
        None
    }
}

fn infer_image_mime(file_name: &str) -> Option<&'static str> {
    match file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        Some("gif") => Some("image/gif"),
        _ => None,
    }
}

fn normalize_mime(mime_type: &str) -> String {
    let value = mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if value.is_empty() || value.len() > 128 || !value.contains('/') {
        "application/octet-stream".to_owned()
    } else {
        value
    }
}

fn default_file_name(image_extension: Option<&str>) -> String {
    image_extension.map_or_else(
        || "attachment.bin".to_owned(),
        |extension| format!("image.{extension}"),
    )
}

/// Persist one uploaded file under the data directory and record it.
pub async fn create(
    pool: &SqlitePool,
    data_dir: &Path,
    thread_id: &str,
    user_id: &str,
    mime_type: &str,
    file_name: Option<&str>,
    bytes: &[u8],
) -> Result<AttachmentDto, ServerError> {
    let cleaned_name = clean_file_name(file_name);
    let mut normalized = normalize_mime(mime_type);
    if normalized == "application/octet-stream" {
        if let Some(inferred) = cleaned_name.as_deref().and_then(infer_image_mime) {
            normalized = inferred.to_owned();
        }
    }
    let image_extension = extension(&normalized);
    let limit = if image_extension.is_some() {
        MAX_IMAGE_BYTES
    } else {
        MAX_FILE_BYTES
    };
    if bytes.is_empty() {
        return Err(ServerError(ApiError::validation("attachment is empty")));
    }
    if bytes.len() > limit {
        let kind = if image_extension.is_some() {
            "image is too large (max 8 MB)"
        } else {
            "file is too large (max 25 MB)"
        };
        return Err(ServerError(ApiError::validation(kind)));
    }
    let file_name =
        clean_file_name(file_name).unwrap_or_else(|| default_file_name(image_extension));
    let disk_extension = image_extension
        .map(str::to_owned)
        .or_else(|| safe_extension(&file_name))
        .unwrap_or_else(|| "bin".to_owned());
    let id = Ulid::new().to_string();
    let dir = data_dir.join("attachments").join(thread_id);
    tokio::fs::create_dir_all(&dir).await.map_err(|error| {
        ServerError::internal(format!("could not create attachment dir: {error}"))
    })?;
    let path = dir.join(format!("{id}.{disk_extension}"));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| ServerError::internal(format!("could not store attachment: {error}")))?;
    let byte_size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
    if let Err(error) = sqlx::query(
        "INSERT INTO attachments(id, thread_id, mime_type, byte_size, path, file_name, created_by, created_at)
         VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(thread_id)
    .bind(&normalized)
    .bind(byte_size)
    .bind(path.to_string_lossy().to_string())
    .bind(&file_name)
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
        file_name: Some(file_name),
    })
}

/// Resolve ids a prompt references, scoped to the thread and in send order.
pub async fn resolve(
    pool: &SqlitePool,
    thread_id: &str,
    ids: &[String],
) -> Result<Vec<AttachmentDto>, ServerError> {
    if ids.len() > MAX_ATTACHMENTS {
        return Err(ServerError(ApiError::validation(
            "too many attachments (max 8)",
        )));
    }
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let row: Option<Row> = sqlx::query_as::<_, Row>(
            "SELECT id, thread_id, mime_type, byte_size, path, file_name
               FROM attachments WHERE id = ? AND thread_id = ?",
        )
        .bind(id)
        .bind(thread_id)
        .fetch_optional(pool)
        .await?;
        let Some(row) = row else {
            return Err(ServerError(ApiError::not_found("attachment not found")));
        };
        out.push(to_dto(&row));
    }
    Ok(out)
}

/// Look one attachment up for the read route.
pub async fn stored(pool: &SqlitePool, id: &str) -> Result<StoredAttachment, ServerError> {
    let row: Option<Row> = sqlx::query_as::<_, Row>(
        "SELECT id, thread_id, mime_type, byte_size, path, file_name
           FROM attachments WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Err(ServerError(ApiError::not_found("attachment not found")));
    };
    Ok(StoredAttachment {
        dto: to_dto(&row),
        thread_id: row.thread_id,
        path: PathBuf::from(row.path),
    })
}

fn is_text_like(mime_type: &str, file_name: Option<&str>) -> bool {
    let mime = mime_type.to_ascii_lowercase();
    if mime.starts_with("text/") {
        return true;
    }
    if matches!(
        mime.as_str(),
        "application/json"
            | "application/xml"
            | "application/yaml"
            | "application/x-yaml"
            | "application/toml"
            | "application/x-toml"
            | "application/javascript"
            | "application/typescript"
            | "application/x-sh"
            | "application/sql"
    ) {
        return true;
    }
    let Some(extension) = file_name
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
    else {
        return false;
    };
    matches!(
        extension.as_str(),
        "txt"
            | "md"
            | "markdown"
            | "json"
            | "jsonl"
            | "yaml"
            | "yml"
            | "toml"
            | "csv"
            | "tsv"
            | "log"
            | "xml"
            | "html"
            | "css"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "py"
            | "rs"
            | "go"
            | "java"
            | "c"
            | "h"
            | "cpp"
            | "hpp"
            | "sh"
            | "bash"
            | "zsh"
            | "sql"
            | "ini"
            | "conf"
            | "env"
            | "diff"
            | "patch"
    )
}

async fn read_prefix(path: &Path, limit: usize) -> Result<Vec<u8>, ServerError> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| ServerError::internal(format!("could not read attachment: {error}")))?;
    let mut buffer = Vec::new();
    file.take(u64::try_from(limit).unwrap_or(u64::MAX))
        .read_to_end(&mut buffer)
        .await
        .map_err(|error| ServerError::internal(format!("could not read attachment: {error}")))?;
    Ok(buffer)
}

fn file_prompt(
    dto: &AttachmentDto,
    path: &Path,
    bytes: i64,
    text: Option<(String, bool)>,
) -> String {
    let name = dto.file_name.as_deref().unwrap_or("attachment");
    let header = format!(
        "Attached file: {name}\nType: {}\nSize: {bytes} bytes\nPath: {}",
        dto.mime_type,
        path.display()
    );
    match text {
        Some((content, true)) => format!(
            "{header}\n\n--- file content ---\n{content}\n--- end file ---"
        ),
        Some((content, false)) => format!(
            "{header}\n\n--- first {} bytes ---\n{content}\n--- end preview ---\nUse your file tools to read the complete file at the path above.",
            content.len()
        ),
        None => format!(
            "{header}\n\nThis is a binary file. Use your file tools to inspect it at the path above."
        ),
    }
}

/// Read attachments back as agent prompt parts. Images use ACP image blocks;
/// every other file becomes a text part with a path (and inline content when it
/// is a reasonably sized text file). Called as the turn starts, so a missing
/// file fails the prompt instead of silently dropping it.
pub async fn prompt_parts(
    pool: &SqlitePool,
    attachments: &[AttachmentDto],
) -> Result<Vec<PromptPart>, ServerError> {
    let mut parts = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let stored = stored(pool, &attachment.id).await?;
        if is_image_mime(&stored.dto.mime_type) {
            let bytes = tokio::fs::read(&stored.path).await.map_err(|error| {
                ServerError::internal(format!("could not read attachment: {error}"))
            })?;
            parts.push(PromptPart::Image {
                mime_type: stored.dto.mime_type.clone(),
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            });
            continue;
        }
        let size = stored.dto.byte_size;
        let text = if is_text_like(&stored.dto.mime_type, stored.dto.file_name.as_deref()) {
            if size >= 0 && size <= MAX_INLINE_TEXT_BYTES as i64 {
                let bytes = tokio::fs::read(&stored.path).await.map_err(|error| {
                    ServerError::internal(format!("could not read attachment: {error}"))
                })?;
                Some((String::from_utf8_lossy(&bytes).into_owned(), true))
            } else {
                let bytes = read_prefix(&stored.path, TEXT_PREVIEW_BYTES).await?;
                Some((String::from_utf8_lossy(&bytes).into_owned(), false))
            }
        } else {
            tokio::fs::metadata(&stored.path).await.map_err(|error| {
                ServerError::internal(format!("could not read attachment: {error}"))
            })?;
            None
        };
        parts.push(PromptPart::text(file_prompt(
            &stored.dto,
            &stored.path,
            size,
            text,
        )));
    }
    Ok(parts)
}

/// Download header for non-image attachments. The ASCII fallback keeps old
/// clients working; `filename*` preserves the original name.
#[must_use]
pub fn content_disposition(file_name: Option<&str>) -> String {
    let name = file_name.unwrap_or("attachment");
    let fallback: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    let fallback = if fallback.trim_matches('_').is_empty() {
        "attachment".to_owned()
    } else {
        fallback
    };
    let encoded: String = url::form_urlencoded::byte_serialize(name.as_bytes())
        .collect::<String>()
        .replace('+', "%20");
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::TempDir;

    async fn seeded() -> (SqlitePool, TempDir, String, String) {
        let pool = db::connect_memory().await.unwrap();
        let (_user, _project, thread) = crate::events::test_support::seed_thread(&pool).await;
        let user_id: String = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let data = TempDir::new().unwrap();
        (pool, data, thread.to_string(), user_id)
    }

    #[tokio::test]
    async fn stores_resolves_and_reads_back_an_image() {
        let (pool, data, thread, user_id) = seeded().await;
        let bytes = b"\x89PNG\r\n\x1a\nfake";
        let attachment = create(
            &pool,
            data.path(),
            thread.as_str(),
            &user_id,
            "image/png",
            Some("pixel.png"),
            bytes,
        )
        .await
        .unwrap();
        assert_eq!(attachment.mime_type, "image/png");
        assert_eq!(attachment.file_name.as_deref(), Some("pixel.png"));
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
    async fn stores_a_text_file_and_inlines_it_in_the_prompt() {
        let (pool, data, thread, user_id) = seeded().await;
        let attachment = create(
            &pool,
            data.path(),
            thread.as_str(),
            &user_id,
            "text/markdown",
            Some("notes.md"),
            b"# Notes\nhello",
        )
        .await
        .unwrap();
        assert_eq!(attachment.file_name.as_deref(), Some("notes.md"));
        let parts = prompt_parts(&pool, &[attachment]).await.unwrap();
        let PromptPart::Text(text) = &parts[0] else {
            panic!("expected a text prompt part");
        };
        assert!(text.contains("notes.md"), "{text}");
        assert!(text.contains("hello"), "{text}");
        assert!(text.contains("file content"), "{text}");
    }

    #[tokio::test]
    async fn oversized_files_and_too_many_attachments_are_rejected() {
        let (pool, data, thread, user_id) = seeded().await;
        let too_large = vec![0u8; MAX_FILE_BYTES + 1];
        let error = create(
            &pool,
            data.path(),
            thread.as_str(),
            &user_id,
            "application/pdf",
            Some("manual.pdf"),
            &too_large,
        )
        .await
        .unwrap_err();
        assert_eq!(error.0.code, threadbase_shared::ErrorCode::Validation);

        let ids: Vec<String> = (0..=MAX_ATTACHMENTS)
            .map(|_| "missing".to_owned())
            .collect();
        let error = resolve(&pool, thread.as_str(), &ids).await.unwrap_err();
        assert_eq!(error.0.code, threadbase_shared::ErrorCode::Validation);
    }

    #[tokio::test]
    async fn rejects_cross_thread_ids() {
        let (pool, data, thread, user_id) = seeded().await;
        let attachment = create(
            &pool,
            data.path(),
            thread.as_str(),
            &user_id,
            "text/plain",
            Some("a.txt"),
            b"a",
        )
        .await
        .unwrap();
        let other_thread = ulid::Ulid::new().to_string();
        let error = resolve(&pool, &other_thread, std::slice::from_ref(&attachment.id))
            .await
            .unwrap_err();
        assert_eq!(error.0.code, threadbase_shared::ErrorCode::NotFound);
    }

    #[test]
    fn content_disposition_sanitizes_names() {
        let value = content_disposition(Some("a \"quote\".txt"));
        assert!(value.starts_with("attachment; filename=\"a__quote_.txt\""));
        assert!(value.contains("filename*=UTF-8''a%20%22quote%22.txt"));
    }
}
