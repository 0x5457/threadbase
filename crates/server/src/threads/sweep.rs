//! Wakes due snoozes and settles idle threads.
//!
//! The rule is `should_auto_settle` in `threadbase-core`; this file collects the
//! facts and calls the same `place()` a click does, so a concurrent prompt wins.

use super::place;
use crate::error::ServerError;
use crate::projection::Projection;
use sqlx::{FromRow, SqlitePool};
use std::time::Duration;
use threadbase_core::state::{should_auto_settle, IdleFacts, PlacementAction};
use threadbase_shared::{now_ms, ErrorCode, ThreadPlacement, ThreadStatus};
use tokio::task::JoinHandle;

/// How often both rules run.
const PERIOD: Duration = Duration::from_secs(60);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepReport {
    pub woken: u32,
    pub settled: u32,
}

/// Runs both rules once. `idle_ms = 0` disables auto-settling; snoozes still
/// wake, because a person asked to see the thread again.
pub async fn sweep(
    pool: &SqlitePool,
    projection: &Projection,
    idle_ms: i64,
    now: i64,
) -> Result<SweepReport, ServerError> {
    let mut report = SweepReport::default();

    let due: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM threads
          WHERE placement = 'snoozed'
            AND snooze_until IS NOT NULL
            AND snooze_until <= ?
            AND status != 'archived'
          ORDER BY snooze_until",
    )
    .bind(now)
    .fetch_all(pool)
    .await?;
    for thread_id in due {
        match place(
            pool,
            projection,
            &thread_id,
            PlacementAction::SnoozeElapsed,
            None,
        )
        .await
        {
            Ok(_) => report.woken += 1,
            // The thread changed between the query and the write; the next
            // run will pick it up.
            Err(error) if error.0.code == ErrorCode::Conflict => {}
            Err(error) => tracing::warn!(thread_id, %error, "could not wake a snoozed thread"),
        }
    }

    if idle_ms <= 0 {
        return Ok(report);
    }
    for candidate in candidates(pool).await? {
        let facts = IdleFacts {
            status: ThreadStatus::Idle,
            placement: ThreadPlacement::Active,
            auto_settle: true,
            working: candidate.queued_inputs > 0 || candidate.active_subagents > 0,
            pending: candidate.pending_requests(),
            last_activity_at: candidate.last_activity_at,
            placement_at: candidate.placement_at,
        };
        if !should_auto_settle(facts, now, idle_ms) {
            continue;
        }
        match place(
            pool,
            projection,
            &candidate.id,
            PlacementAction::AutoSettle,
            None,
        )
        .await
        {
            Ok(_) => {
                report.settled += 1;
                tracing::info!(thread_id = %candidate.id, "thread settled after inactivity");
            }
            Err(error) if error.0.code == ErrorCode::Conflict => {}
            Err(error) => {
                tracing::warn!(thread_id = %candidate.id, %error, "auto-settle failed");
            }
        }
    }
    Ok(report)
}

/// Starts the background loop. The first run waits one interval so a restart
/// does not settle a wave of threads during startup recovery.
#[must_use]
pub fn spawn_sweeper(pool: SqlitePool, projection: Projection, idle_ms: i64) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(PERIOD).await;
            match sweep(&pool, &projection, idle_ms, now_ms()).await {
                Ok(report) if report != SweepReport::default() => {
                    tracing::info!(
                        woken = report.woken,
                        settled = report.settled,
                        "placement sweep"
                    );
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "placement sweep failed"),
            }
        }
    })
}

/// Idle threads the sweep may settle. The rest is filtered in SQL.
#[derive(FromRow)]
struct Candidate {
    id: String,
    last_activity_at: i64,
    placement_at: i64,
    pending_approvals: i64,
    pending_elicitations: i64,
    queued_inputs: i64,
    active_subagents: i64,
}

impl Candidate {
    fn pending_requests(&self) -> u32 {
        u32::try_from(self.pending_approvals + self.pending_elicitations).unwrap_or(u32::MAX)
    }
}

async fn candidates(pool: &SqlitePool) -> Result<Vec<Candidate>, ServerError> {
    Ok(sqlx::query_as::<_, Candidate>(
        "SELECT t.id, COALESCE(t.last_activity_at, t.updated_at) AS last_activity_at,
            t.placement_at,
            (SELECT COUNT(*) FROM approvals a
              WHERE a.thread_id = t.id AND a.status = 'pending') AS pending_approvals,
            (SELECT COUNT(*) FROM elicitations e
              WHERE e.thread_id = t.id AND e.status = 'pending') AS pending_elicitations,
            (SELECT COUNT(*) FROM queued_inputs q
              WHERE q.thread_id = t.id AND q.status = 'queued') AS queued_inputs,
            (SELECT COUNT(*) FROM subagents s
              WHERE s.thread_id = t.id AND s.status IN ('running','requires_action')) AS active_subagents
         FROM threads t
         WHERE t.placement = 'active' AND t.status = 'idle' AND t.auto_settle = 'default'
         ORDER BY last_activity_at",
    )
    .fetch_all(pool)
    .await?)
}
