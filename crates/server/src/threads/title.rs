//! Thread titles: what a first prompt earns, and what a paste leaves behind.
//!
//! A title is read in a one-line sidebar slot, so it is the prompt's first line
//! with the markup a copy-paste drags along removed. Nothing here parses HTML:
//! the goal is prose, not a document.

use crate::error::ServerError;
use sqlx::SqlitePool;

/// Longest title a prompt may earn, in characters.
const MAX: usize = 60;

/// The title a first prompt earns: its first line that still says something once
/// tags are gone. `None` when the prompt is only markup or whitespace.
#[must_use]
pub fn title_from_prompt(text: &str) -> Option<String> {
    let stripped = strip_tags(text);
    let line = stripped
        .lines()
        .map(plain_line)
        .find(|line| !line.is_empty())?;
    Some(truncate(&line))
}

/// Repairs titles written before the prompt cleaner existed: a title that still
/// carries markup is replaced by the prose inside it. Idempotent, and it never
/// touches a title that reads fine.
pub async fn backfill_titles(pool: &SqlitePool) -> Result<u32, ServerError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, title FROM threads
         WHERE instr(title, '<') > 0 OR instr(title, '&') > 0",
    )
    .fetch_all(pool)
    .await?;
    let mut repaired = 0;
    for (id, title) in rows {
        let cleaned = plain_line(&strip_tags(&title));
        if cleaned.is_empty() || cleaned == title {
            continue;
        }
        sqlx::query("UPDATE threads SET title = ? WHERE id = ?")
            .bind(&cleaned)
            .bind(&id)
            .execute(pool)
            .await?;
        repaired += 1;
    }
    Ok(repaired)
}

/// Drops `<tag …>` spans, keeps a literal `<` that starts prose (`1 < 3`) and the
/// text of an autolink (`<https://…>`).
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let starts_tag = after
            .starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!' || c == '?');
        match (starts_tag, after.find('>')) {
            (true, Some(close)) if !after[..close].contains('<') => {
                let inner = &after[..close];
                if inner.contains("://") {
                    out.push_str(inner);
                }
                rest = &after[close + 1..];
            }
            // A tag cut off by the title budget (`<link title="null" rel="noopene`)
            // is still a tag: it carries attribute syntax, not prose.
            (true, None) if after.contains(['=', '"', '\'']) || after.starts_with('/') => {
                rest = "";
            }
            _ => {
                out.push('<');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// One line as a reader sees it: entities decoded, runs of whitespace collapsed.
fn plain_line(line: &str) -> String {
    decode_entities(line)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_entities(value: &str) -> String {
    const ENTITIES: &[(&str, &str)] = &[
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&nbsp;", " "),
    ];
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        match ENTITIES.iter().find(|(entity, _)| tail.starts_with(entity)) {
            Some((entity, replacement)) => {
                out.push_str(replacement);
                rest = &tail[entity.len()..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn truncate(line: &str) -> String {
    let mut title: String = line.chars().take(MAX).collect();
    if line.chars().count() > MAX {
        title.push('…');
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_drops_the_markup_a_paste_drags_in() {
        assert_eq!(
            title_from_prompt(
                r#"看下能不能把这个集成到系统<link title="null" target="_blank" rel="noopener">看看"#
            )
            .as_deref(),
            Some("看下能不能把这个集成到系统看看")
        );
    }

    #[test]
    fn title_drops_a_tag_the_budget_cut_in_half() {
        assert_eq!(
            title_from_prompt(
                "看下能不能把这个集成到系统<link title=\"null\" target=\"_blank\" rel=\"noopene"
            )
            .as_deref(),
            Some("看下能不能把这个集成到系统")
        );
    }

    #[test]
    fn title_skips_lines_that_are_only_markup() {
        assert_eq!(
            title_from_prompt("<div class=\"x\"></div>\n  real title  ").as_deref(),
            Some("real title")
        );
        assert_eq!(title_from_prompt("<!-- nothing -->"), None);
        assert_eq!(title_from_prompt("   \n\t\n"), None);
    }

    #[test]
    fn title_keeps_a_literal_angle_bracket_and_autolinks() {
        assert_eq!(
            title_from_prompt("1 < 3 没问题").as_deref(),
            Some("1 < 3 没问题")
        );
        assert_eq!(
            title_from_prompt("see <https://example.com> here").as_deref(),
            Some("see https://example.com here")
        );
    }

    #[test]
    fn title_decodes_entities_and_collapses_whitespace() {
        assert_eq!(
            title_from_prompt("A &amp; B&nbsp;&nbsp;  C").as_deref(),
            Some("A & B C")
        );
    }

    #[test]
    fn title_stops_at_the_sidebar_budget() {
        let long = "长".repeat(MAX + 20);
        let title = title_from_prompt(&long).unwrap();
        assert_eq!(title.chars().count(), MAX + 1);
        assert!(title.ends_with('…'));
        let exact = "长".repeat(MAX);
        assert_eq!(title_from_prompt(&exact).as_deref(), Some(exact.as_str()));
    }
}
