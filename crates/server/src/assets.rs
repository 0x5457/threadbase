//! Web assets. Debug builds read `web/dist` from disk; release builds embed it
//! into the single binary.

use axum::body::Body;
#[cfg(debug_assertions)]
use axum::http::Uri;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

#[cfg(debug_assertions)]
pub fn serve(uri: &Uri) -> Response {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    serve_disk(&root, uri)
}

#[cfg(debug_assertions)]
fn serve_disk(root: &std::path::Path, uri: &Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let Ok(candidate) = threadbase_workspace::path::ensure_within(root, std::path::Path::new(path))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if candidate.is_file() {
        if let Ok(data) = std::fs::read(&candidate) {
            return file_response(path, data);
        }
    }
    if !path.contains('.') && root.join("index.html").is_file() {
        if let Ok(data) = std::fs::read(root.join("index.html")) {
            return file_response("index.html", data);
        }
    }
    if !root.join("index.html").is_file() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "Web assets are not built. Run `pnpm --dir web build` and restart Threadbase.",
        )
            .into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

#[cfg(not(debug_assertions))]
mod embedded {
    use super::{file_response, StatusCode};
    use axum::http::Uri;
    use axum::response::{IntoResponse, Response};
    use rust_embed::RustEmbed;

    #[derive(RustEmbed)]
    #[folder = "../../web/dist"]
    struct EmbeddedAssets;

    pub fn serve(uri: &Uri) -> Response {
        let path = uri.path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        if let Some(asset) = EmbeddedAssets::get(path) {
            return file_response(path, asset.data.into_owned());
        }
        if !path.contains('.') {
            if let Some(index) = EmbeddedAssets::get("index.html") {
                return file_response("index.html", index.data.into_owned());
            }
        }
        StatusCode::NOT_FOUND.into_response()
    }
}

#[cfg(not(debug_assertions))]
pub use embedded::serve;

fn file_response(path: &str, data: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, mime.as_ref()),
            (header::CACHE_CONTROL, cache),
        ],
        Body::from(data),
    )
        .into_response()
}

#[cfg(all(debug_assertions, test))]
mod tests {
    use super::*;

    fn root_with_index() -> (tempfile::TempDir, std::path::PathBuf) {
        let parent = tempfile::tempdir().unwrap();
        std::fs::write(parent.path().join("secret.txt"), "secret").unwrap();
        let root = parent.path().join("dist");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("index.html"), "<html></html>").unwrap();
        (parent, root)
    }

    #[test]
    fn traversal_outside_the_asset_root_is_not_found() {
        let (_parent, root) = root_with_index();
        let uri: Uri = "/../secret.txt".parse().unwrap();
        assert_eq!(serve_disk(&root, &uri).status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn dotless_paths_fall_back_to_the_spa_shell() {
        let (_parent, root) = root_with_index();
        let uri: Uri = "/settings".parse().unwrap();
        assert_eq!(serve_disk(&root, &uri).status(), StatusCode::OK);
    }
}
