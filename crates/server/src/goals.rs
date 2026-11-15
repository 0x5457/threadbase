//! Goal progress kept by the agent runtime inside the thread workspace.
//!
//! pi's `pi-goal-x` extension writes one JSON record per open goal to
//! `<cwd>/.pi/goals/active_goal_*.md` (the `.md` extension is historical: the
//! record is a JSON object followed by a human-readable progress section). The
//! file is the adapter's source of truth, so reading it gives the UI the
//! objective, task list and usage without mining the transcript.
//!
//! Adapters that do not manage goals simply have no such directory, which is
//! why every entry point returns `None` instead of an error.

use serde::Deserialize;
use std::fs;
use std::path::Path;
use threadbase_shared::model::{GoalDto, GoalTaskDto};

const GOALS_DIR: &str = ".pi/goals";
const ACTIVE_PREFIX: &str = "active_goal_";
const ACTIVE_SUFFIX: &str = ".md";

/// Read the most recently updated open goal in `cwd`.
///
/// The adapter may keep several open goals; the session's focused one is not
/// recorded on disk, so the newest record is the best available answer.
#[must_use]
pub fn read(cwd: &Path) -> Option<GoalDto> {
    let entries = fs::read_dir(cwd.join(GOALS_DIR)).ok()?;
    let mut newest: Option<GoalDto> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(ACTIVE_PREFIX) || !name.ends_with(ACTIVE_SUFFIX) {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Some(record) = parse(&text) else {
            continue;
        };
        let goal = record.into_dto();
        let newer = newest
            .as_ref()
            .is_none_or(|current| goal.updated_at > current.updated_at);
        if newer {
            newest = Some(goal);
        }
    }
    newest
}

/// Parse the leading JSON record; the trailing progress section is ignored.
fn parse(text: &str) -> Option<Record> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Record>()
        .next()?
        .ok()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(default)]
    id: String,
    #[serde(default)]
    objective: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    sisyphus: bool,
    #[serde(default)]
    auto_continue: bool,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    task_list: Option<TaskList>,
}

impl Record {
    fn into_dto(self) -> GoalDto {
        let usage = self.usage.unwrap_or_default();
        GoalDto {
            id: self.id,
            objective: self.objective,
            status: if self.status.is_empty() {
                "active".to_owned()
            } else {
                self.status
            },
            sisyphus: self.sisyphus,
            auto_continue: self.auto_continue,
            updated_at: self.updated_at,
            tokens_used: usage.tokens_used,
            active_seconds: usage.active_seconds,
            tasks: self
                .task_list
                .map(|list| list.tasks)
                .unwrap_or_default()
                .into_iter()
                .map(Task::into_dto)
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    #[serde(default)]
    tokens_used: u64,
    #[serde(default)]
    active_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskList {
    #[serde(default)]
    tasks: Vec<Task>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Task {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    verification_contract: Option<String>,
    #[serde(default)]
    subtasks: Vec<Task>,
}

impl Task {
    fn into_dto(self) -> GoalTaskDto {
        GoalTaskDto {
            id: self.id,
            title: self.title,
            status: if self.status.is_empty() {
                "pending".to_owned()
            } else {
                self.status
            },
            evidence: self.evidence,
            verification_contract: self.verification_contract,
            subtasks: self.subtasks.into_iter().map(Task::into_dto).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read;
    use std::fs;
    use std::path::Path;

    fn write_goal(dir: &Path, name: &str, record: &str) {
        let goals = dir.join(".pi/goals");
        fs::create_dir_all(&goals).expect("goals dir");
        fs::write(goals.join(name), record).expect("goal record");
    }

    const GOAL: &str = r#"{
  "version": 3,
  "id": "mabc-1",
  "objective": "Ship CSV export",
  "status": "paused",
  "autoContinue": true,
  "usage": { "tokensUsed": 1200, "activeSeconds": 90 },
  "sisyphus": true,
  "updatedAt": "2026-11-11T20:20:10.035Z",
  "taskList": {
    "tasks": [
      {
        "id": "one",
        "title": "Write parser",
        "status": "complete",
        "evidence": "tests pass",
        "subtasks": [{ "id": "one-a", "title": "quote fields", "status": "complete" }]
      },
      { "id": "two", "title": "Wire button", "status": "pending" }
    ]
  }
}

## Progress

- Status: paused
"#;

    #[test]
    fn reads_the_open_goal_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_20261111_mabc-1.md", GOAL);
        let goal = read(dir.path()).expect("goal");
        assert_eq!(goal.id, "mabc-1");
        assert_eq!(goal.objective, "Ship CSV export");
        assert_eq!(goal.status, "paused");
        assert!(goal.sisyphus);
        assert!(goal.auto_continue);
        assert_eq!(goal.tokens_used, 1200);
        assert_eq!(goal.active_seconds, 90);
        assert_eq!(goal.updated_at.as_deref(), Some("2026-11-11T20:20:10.035Z"));
        assert_eq!(goal.tasks.len(), 2);
        assert_eq!(goal.tasks[0].status, "complete");
        assert_eq!(goal.tasks[0].evidence.as_deref(), Some("tests pass"));
        assert_eq!(goal.tasks[0].subtasks.len(), 1);
        assert_eq!(goal.tasks[1].status, "pending");
    }

    #[test]
    fn workspace_without_goals_has_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read(dir.path()).is_none());
    }

    #[test]
    fn archived_goals_are_not_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "goal_20261111_mabc-1.md", GOAL);
        assert!(read(dir.path()).is_none());
    }

    #[test]
    fn newest_record_wins() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_old.md", GOAL);
        write_goal(
            dir.path(),
            "active_goal_new.md",
            &GOAL.replace("2026-11-11T20:20:10.035Z", "2026-11-12T08:00:00.000Z"),
        );
        let goal = read(dir.path()).expect("goal");
        assert_eq!(goal.updated_at.as_deref(), Some("2026-11-12T08:00:00.000Z"));
    }

    #[test]
    fn malformed_records_are_skipped() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_broken.md", "not json");
        write_goal(dir.path(), "active_goal_ok.md", GOAL);
        assert_eq!(read(dir.path()).expect("goal").id, "mabc-1");
    }
}
