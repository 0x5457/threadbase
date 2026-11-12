//! Pure diff parsing: hunks, line numbers and status.

use sha2::{Digest, Sha256};
use std::path::Path;
use threadbase_shared::{DiffHunkDto, DiffLineDto, DiffLineKind, FileStatus};
use unidiff::PatchSet;

pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

pub(super) fn hash_text(text: &str) -> String {
    hash_bytes(text.as_bytes())
}

pub(super) fn is_binary_bytes(bytes: &[u8]) -> bool {
    content_inspector::inspect(bytes).is_binary()
}

pub(super) async fn file_line_stats(path: &Path) -> (bool, u32) {
    let Ok(content) = tokio::fs::read(path).await else {
        return (false, 0);
    };
    if is_binary_bytes(&content) {
        return (true, 0);
    }
    let lines = content.iter().filter(|byte| **byte == b'\n').count();
    (false, u32::try_from(lines).unwrap_or(0))
}

pub(super) async fn is_binary_file(path: &Path) -> bool {
    let Ok(content) = tokio::fs::read(path).await else {
        return false;
    };
    is_binary_bytes(&content)
}

pub(super) fn parse_status(value: &str) -> FileStatus {
    match value.chars().next().unwrap_or('M') {
        'A' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        '?' => FileStatus::Untracked,
        _ => FileStatus::Modified,
    }
}

pub(super) fn parse_hunks(path: &str, patch: &str) -> Vec<DiffHunkDto> {
    let mut patch_set = PatchSet::new();
    if patch_set.parse(patch).is_err() {
        return Vec::new();
    }
    // `unidiff` does not retain the raw `@@` line, and the stored header is
    // part of the context hash comments are anchored to. Pair its hunks with
    // the header lines from git in the order both appear.
    let headers: Vec<&str> = patch
        .lines()
        .filter(|line| line.starts_with("@@"))
        .collect();
    let mut hunks = Vec::new();
    let parsed_hunks = patch_set.files().iter().flat_map(|file| file.hunks());
    for (header, hunk) in headers.into_iter().zip(parsed_hunks) {
        let lines = hunk
            .lines()
            .iter()
            .filter_map(|line| {
                let kind = match line.line_type.as_str() {
                    "+" => DiffLineKind::Add,
                    "-" => DiffLineKind::Delete,
                    " " => DiffLineKind::Context,
                    _ => return None,
                };
                Some(DiffLineDto {
                    kind,
                    old_line: line.source_line_no.and_then(to_u32),
                    new_line: line.target_line_no.and_then(to_u32),
                    content: line.value.clone(),
                })
            })
            .collect();
        hunks.push(DiffHunkDto {
            id: format!("h{}", hunks.len() + 1),
            header: header.to_owned(),
            context_hash: String::new(),
            old_start: to_u32(hunk.source_start).unwrap_or(u32::MAX),
            old_lines: to_u32(hunk.source_length).unwrap_or(u32::MAX),
            new_start: to_u32(hunk.target_start).unwrap_or(u32::MAX),
            new_lines: to_u32(hunk.target_length).unwrap_or(u32::MAX),
            lines,
        });
    }
    for hunk in &mut hunks {
        let mut value = format!("{path}:{}", hunk.header);
        for line in &hunk.lines {
            value.push('\n');
            value.push_str(&line.content);
        }
        hunk.context_hash = hash_text(&value)[..16].to_owned();
    }
    hunks
}

fn to_u32(value: usize) -> Option<u32> {
    u32::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/src/main.rs b/src/main.rs\n\
index 1111111..2222222 100644\n\
--- a/src/main.rs\n\
+++ b/src/main.rs\n\
@@ -1,3 +1,4 @@ fn main() {\n\
 line one\n\
-old\n\
+new\n\
+added\n\
 line three\n\
\\ No newline at end of file\n\
@@ -10,1 +11,2 @@\n\
 tail\n\
+extra\n";

    #[test]
    fn hunks_keep_git_headers_and_line_numbers() {
        let hunks = parse_hunks("src/main.rs", PATCH);
        assert_eq!(hunks.len(), 2);
        let first = &hunks[0];
        assert_eq!(first.id, "h1");
        assert_eq!(first.header, "@@ -1,3 +1,4 @@ fn main() {");
        assert_eq!(
            (
                first.old_start,
                first.old_lines,
                first.new_start,
                first.new_lines
            ),
            (1, 3, 1, 4)
        );
        let lines: Vec<_> = first
            .lines
            .iter()
            .map(|line| {
                (
                    line.kind,
                    line.old_line,
                    line.new_line,
                    line.content.as_str(),
                )
            })
            .collect();
        assert_eq!(
            lines,
            vec![
                (DiffLineKind::Context, Some(1), Some(1), "line one"),
                (DiffLineKind::Delete, Some(2), None, "old"),
                (DiffLineKind::Add, None, Some(2), "new"),
                (DiffLineKind::Add, None, Some(3), "added"),
                (DiffLineKind::Context, Some(3), Some(4), "line three"),
            ]
        );
        let second = &hunks[1];
        assert_eq!(second.id, "h2");
        assert_eq!(second.header, "@@ -10,1 +11,2 @@");
        assert_eq!(
            (
                second.old_start,
                second.old_lines,
                second.new_start,
                second.new_lines
            ),
            (10, 1, 11, 2)
        );
    }

    #[test]
    fn context_hash_matches_the_stored_header_and_contents() {
        let hunks = parse_hunks("src/main.rs", PATCH);
        let first = &hunks[0];
        let expected = format!(
            "src/main.rs:{}\nline one\nold\nnew\nadded\nline three",
            first.header
        );
        assert_eq!(first.context_hash, hash_text(&expected)[..16]);
    }
}
