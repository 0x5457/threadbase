//! Goal progress written by the agent runtime in the thread workspace.
//!
//! pi's `pi-goal-x` extension keeps one record per goal in
//! `<cwd>/.pi/goals/active_goal_*.md`. A goal belongs to the workspace but names
//! the session driving it (`scheduler.owner`), so only that thread reports it.
//! The record does not say whether the goal passed its audit; the newest verdict
//! in `.pi/goals/goal_events.jsonl` does. A missing directory means the adapter
//! does not use goals, so every entry point returns `None`.

use serde::Deserialize;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use threadbase_shared::model::{GoalAuditDto, GoalDto, GoalTaskDto};

const GOALS_DIR: &str = ".pi/goals";
const LEDGER: &str = "goal_events.jsonl";
const ACTIVE_PREFIX: &str = "active_goal_";
const ACTIVE_SUFFIX: &str = ".md";
/// The ledger is append-only and grows with every task; only its tail matters,
/// and the goal endpoint is polled while a thread is open.
const LEDGER_TAIL_BYTES: u64 = 64 * 1024;
/// The bar has room for one line; the full report stays in the adapter's ledger.
const AUDIT_SUMMARY_CHARS: usize = 200;

/// Read the most recently updated open goal in `cwd` that `session` owns.
///
/// `session` is the adapter session id from the thread's binding. A goal whose
/// record names another session belongs to that thread and is skipped here; a
/// record that names no session predates ownership tracking and is visible to
/// any started thread in the workspace.
#[must_use]
pub fn read(cwd: &Path, session: Option<&str>) -> Option<GoalDto> {
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
        if !record.visible_to(session) {
            continue;
        }
        let goal = record.into_dto();
        let newer = newest
            .as_ref()
            .is_none_or(|current| goal.updated_at > current.updated_at);
        if newer {
            newest = Some(goal);
        }
    }
    newest.map(|goal| attach_audit(cwd, goal))
}

/// True when the workspace has a goal that `session` keeps working on without
/// prompts, so the session is never idle. Evicting
/// the process would drop its scheduler mid-run. Only the owning session is
/// pinned — every other thread in the same directory must still be able to go
/// idle, or one goal would keep a whole project's adapters alive.
#[must_use]
pub fn runs_autonomously(cwd: &Path, session: Option<&str>) -> bool {
    read(cwd, session).is_some_and(|goal| goal.status == "active" && goal.auto_continue)
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
    /// The scheduler state, which is where the record names its session.
    #[serde(default)]
    scheduler: Option<Scheduler>,
}

impl Record {
    /// The session driving this goal, when the adapter reports one.
    fn owner(&self) -> Option<&str> {
        self.scheduler
            .as_ref()
            .and_then(|scheduler| scheduler.owner.as_deref())
            .filter(|owner| !owner.is_empty())
    }

    fn visible_to(&self, session: Option<&str>) -> bool {
        match self.owner() {
            Some(owner) => Some(owner) == session,
            // A thread that never started has no session and cannot own a
            // workspace goal; a started thread sees ownerless legacy records.
            None => session.is_some(),
        }
    }

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
            audit: None,
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
struct Scheduler {
    #[serde(default)]
    owner: Option<String>,
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

/// One ledger event. The ledger has eighteen kinds; only the audit verdicts
/// matter here, and unknown kinds simply fail the field checks.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LedgerEvent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    goal_id: Option<String>,
    #[serde(default)]
    verdict: Option<String>,
    #[serde(default)]
    report: Option<String>,
    #[serde(default)]
    at: Option<String>,
}

/// Attach the adapter's completion gate to a goal.
fn attach_audit(cwd: &Path, mut goal: GoalDto) -> GoalDto {
    goal.audit = read_ledger_tail(&cwd.join(GOALS_DIR).join(LEDGER))
        .and_then(|ledger| last_audit(&ledger, &goal.id));
    goal
}

/// The newest `audit_result` for `goal_id` in the ledger tail.
fn last_audit(ledger: &str, goal_id: &str) -> Option<GoalAuditDto> {
    ledger
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<LedgerEvent>(line).ok())
        .find(|event| event.kind == "audit_result" && event.goal_id.as_deref() == Some(goal_id))
        .map(|event| GoalAuditDto {
            verdict: event.verdict.unwrap_or_else(|| "error".to_owned()),
            at: event.at,
            summary: event.report.as_deref().and_then(report_summary),
        })
}

/// First meaningful line of an auditor report, short enough for a tooltip.
fn report_summary(report: &str) -> Option<String> {
    let line = report
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '*', '-', '>', ' '])
                .trim()
        })
        .find(|line| !line.is_empty())?;
    let mut summary: String = line.chars().take(AUDIT_SUMMARY_CHARS).collect();
    if line.chars().count() > AUDIT_SUMMARY_CHARS {
        summary.push('…');
    }
    Some(summary)
}

/// The tail of an append-only file, starting at a line boundary.
fn read_ledger_tail(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(LEDGER_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    // The window can cut through a multi-byte character as well as a line.
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        let index = text.find('\n')?;
        text.replace_range(..=index, "");
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::{read, runs_autonomously};
    use std::fs;
    use std::path::Path;

    fn write_goal(dir: &Path, name: &str, record: &str) {
        let goals = dir.join(".pi/goals");
        fs::create_dir_all(&goals).expect("goals dir");
        fs::write(goals.join(name), record).expect("goal record");
    }

    fn write_ledger(dir: &Path, lines: &[&str]) {
        let goals = dir.join(".pi/goals");
        fs::create_dir_all(&goals).expect("goals dir");
        fs::write(goals.join("goal_events.jsonl"), lines.join("\n")).expect("ledger");
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

    fn owned(owner: &str) -> String {
        GOAL.replace(
            "\"taskList\"",
            &format!("\"scheduler\": {{\"owner\": \"{owner}\"}},\n  \"taskList\""),
        )
    }

    #[test]
    fn reads_the_open_goal_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_20261111_mabc-1.md", GOAL);
        let goal = read(dir.path(), Some("sess-1")).expect("goal");
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
        assert!(goal.audit.is_none());
    }

    #[test]
    fn a_goal_belongs_to_the_session_that_drives_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_owned.md", &owned("sess-1"));
        assert!(read(dir.path(), Some("sess-1")).is_some());
        assert!(
            read(dir.path(), Some("sess-2")).is_none(),
            "another thread in the same directory must not see the goal"
        );
        assert!(
            read(dir.path(), None).is_none(),
            "a thread that never started has no session to match"
        );
    }

    #[test]
    fn an_ownerless_goal_stays_visible_to_started_threads() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_legacy.md", GOAL);
        assert!(read(dir.path(), Some("sess-1")).is_some());
        assert!(read(dir.path(), Some("sess-2")).is_some());
        assert!(read(dir.path(), None).is_none());
    }

    #[test]
    fn workspace_without_goals_has_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(read(dir.path(), Some("sess-1")).is_none());
    }

    #[test]
    fn archived_goals_are_not_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "goal_20261111_mabc-1.md", GOAL);
        assert!(read(dir.path(), Some("sess-1")).is_none());
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
        let goal = read(dir.path(), Some("sess-1")).expect("goal");
        assert_eq!(goal.updated_at.as_deref(), Some("2026-11-12T08:00:00.000Z"));
    }

    #[test]
    fn the_newest_visible_goal_wins() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_mine.md", GOAL);
        write_goal(
            dir.path(),
            "active_goal_theirs.md",
            &owned("sess-2").replace("2026-11-11T20:20:10.035Z", "2026-11-12T08:00:00.000Z"),
        );
        let goal = read(dir.path(), Some("sess-1")).expect("goal");
        assert_eq!(
            goal.updated_at.as_deref(),
            Some("2026-11-11T20:20:10.035Z"),
            "a newer goal owned by another session must not hide mine"
        );
    }

    #[test]
    fn the_last_audit_verdict_travels_with_the_goal() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_mine.md", GOAL);
        write_ledger(
            dir.path(),
            &[
                r#"{"type":"goal_created","goalId":"mabc-1","at":"2026-11-11T20:00:00.000Z"}"#,
                r#"{"type":"audit_started","goalId":"mabc-1","at":"2026-11-11T20:10:00.000Z"}"#,
                r###"{"type":"audit_result","goalId":"mabc-1","verdict":"disapproved","report":"## 审计结论：未完全达成\n\n- 三条不达标","at":"2026-11-11T20:20:00.000Z"}"###,
                r#"{"type":"audit_result","goalId":"other","verdict":"approved","at":"2026-11-11T20:30:00.000Z"}"#,
            ],
        );
        let audit = read(dir.path(), Some("sess-1"))
            .expect("goal")
            .audit
            .expect("audit");
        assert_eq!(audit.verdict, "disapproved");
        assert_eq!(audit.at.as_deref(), Some("2026-11-11T20:20:00.000Z"));
        assert_eq!(audit.summary.as_deref(), Some("审计结论：未完全达成"));
    }

    #[test]
    fn a_truncated_ledger_tail_still_parses_whole_events() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_goal(dir.path(), "active_goal_mine.md", GOAL);
        // A wide report pushes the tail window over the file start, so the
        // window opens in the middle of a line.
        let filler = format!(
            "{{\"type\":\"task_started\",\"goalId\":\"mabc-1\",\"at\":\"2026-11-11T19:00:00.000Z\",\"note\":\"{}\"}}",
            "x".repeat(70 * 1024)
        );
        write_ledger(
            dir.path(),
            &[
                &filler,
                r#"{"type":"audit_result","goalId":"mabc-1","verdict":"approved","at":"2026-11-11T20:20:00.000Z"}"#,
            ],
        );
        let audit = read(dir.path(), Some("sess-1"))
            .expect("goal")
            .audit
            .expect("audit");
        assert_eq!(audit.verdict, "approved");
    }

    #[test]
    fn only_active_auto_continue_goals_run_autonomously() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!runs_autonomously(dir.path(), Some("sess-1")));
        // The fixture is paused: nothing continues on its own.
        write_goal(dir.path(), "active_goal_paused.md", GOAL);
        assert!(!runs_autonomously(dir.path(), Some("sess-1")));
        write_goal(
            dir.path(),
            "active_goal_running.md",
            &owned("sess-1")
                .replace("\"status\": \"paused\"", "\"status\": \"active\"")
                .replace("2026-11-11T20:20:10.035Z", "2026-11-12T08:00:00.000Z"),
        );
        assert!(runs_autonomously(dir.path(), Some("sess-1")));
        assert!(
            !runs_autonomously(dir.path(), Some("sess-2")),
            "only the driving session is pinned; the rest of the project may idle"
        );
    }
}
