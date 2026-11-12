//! Skill discovery for the composer.
//!
//! Skills live in agent-specific folders next to the project or in the user's
//! home. We read the frontmatter from each `SKILL.md` so the picker can show a
//! name and a one-line description.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use threadbase_shared::SkillDto;

const MAX_SKILLS: usize = 100;

struct Root {
    path: PathBuf,
    source: &'static str,
}

pub fn list(project: &Path) -> Vec<SkillDto> {
    let mut roots = vec![
        Root {
            path: project.join(".claude/skills"),
            source: "project · claude",
        },
        Root {
            path: project.join(".agents/skills"),
            source: "project · agents",
        },
        Root {
            path: project.join(".codex/skills"),
            source: "project · codex",
        },
        Root {
            path: project.join(".pi/skills"),
            source: "project · pi",
        },
        Root {
            path: project.join("skills"),
            source: "project",
        },
    ];

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        roots.extend([
            Root {
                path: home.join(".claude/skills"),
                source: "user · claude",
            },
            Root {
                path: home.join(".agents/skills"),
                source: "user · agents",
            },
            Root {
                path: home.join(".codex/skills"),
                source: "user · codex",
            },
            Root {
                path: home.join(".pi/agent/skills"),
                source: "user · pi",
            },
        ]);
    }

    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    for root in roots {
        collect_root(&root, &mut seen, &mut skills);
        if skills.len() >= MAX_SKILLS {
            break;
        }
    }
    skills.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    skills.truncate(MAX_SKILLS);
    skills
}

fn collect_root(root: &Root, seen: &mut HashSet<PathBuf>, skills: &mut Vec<SkillDto>) {
    let Ok(entries) = std::fs::read_dir(&root.path) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if skills.len() >= MAX_SKILLS {
            return;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(canonical.clone()) {
            continue;
        }
        let Some(file) = skill_file(&path) else {
            continue;
        };
        if let Some(skill) = parse_skill(&file, &canonical, root.source) {
            skills.push(skill);
        }
    }
}

fn skill_file(directory: &Path) -> Option<PathBuf> {
    for name in ["SKILL.md", "skill.md", "README.md"] {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let mut markdown = std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "md"))?;
    markdown.set_extension("md");
    markdown.is_file().then_some(markdown)
}

fn parse_skill(file: &Path, directory: &Path, source: &str) -> Option<SkillDto> {
    let content = std::fs::read_to_string(file).ok()?;
    let fallback_name = directory
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_owned());
    let mut name = fallback_name;
    let mut description = String::new();
    let mut body: Vec<&str> = Vec::new();

    let mut lines = content.lines();
    let first = lines.next();
    if first.is_some_and(|line| line.trim() == "---") {
        let mut frontmatter = Vec::new();
        for line in lines.by_ref() {
            if line.trim() == "---" {
                break;
            }
            frontmatter.push(line);
        }
        body.extend(lines);
        parse_frontmatter(&frontmatter, &mut name, &mut description);
    } else {
        body.extend(first);
        body.extend(lines);
    }

    if description.is_empty() {
        let mut first_heading = None;
        let mut first_line = None;
        for line in &body {
            let trimmed = line.trim();
            if first_heading.is_none() {
                if let Some(heading) = trimmed.strip_prefix("# ") {
                    first_heading = Some(heading.trim().to_owned());
                }
            }
            if first_line.is_none()
                && !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("```")
            {
                first_line = Some(trimmed.to_owned());
            }
        }
        description = first_heading
            .or(first_line)
            .unwrap_or_else(|| "Agent skill".to_owned());
    }

    if description.chars().count() > 180 {
        description = description.chars().take(177).collect::<String>();
        description.push('…');
    }

    Some(SkillDto {
        name,
        description,
        source: source.to_owned(),
        path: file.to_string_lossy().to_string(),
    })
}

fn parse_frontmatter(lines: &[&str], name: &mut String, description: &mut String) {
    let mut description_lines: Option<Vec<String>> = None;

    for line in lines {
        let trimmed = line.trim();
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(block) = description_lines.as_mut() {
                if !trimmed.is_empty() {
                    block.push(trimmed.to_owned());
                }
                continue;
            }
        } else if let Some(block) = description_lines.take() {
            *description = block.join(" ");
        }

        if let Some(value) = trimmed.strip_prefix("name:") {
            let value = value.trim().trim_matches(['"', '\'']);
            if !value.is_empty() {
                *name = value.to_owned();
            }
        } else if let Some(value) = trimmed.strip_prefix("description:") {
            let value = value.trim().trim_matches(['"', '\'']);
            if value == ">" || value == "|" {
                description_lines = Some(Vec::new());
            } else if !value.is_empty() {
                *description = value.to_owned();
            }
        }
    }

    if let Some(block) = description_lines.take() {
        *description = block.join(" ");
    }
}
