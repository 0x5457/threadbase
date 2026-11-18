use threadbase_shared::ActionKind;
use threadbase_shared::ThreadPlacement;
use threadbase_shared::ThreadSection;
use threadbase_shared::ThreadStatus;

use super::{guard, illegal, Actions, States, TransitionError};

// ---------------------------------------------------------------------------
// Placement: where a thread sits between turns
// ---------------------------------------------------------------------------

/// Commands and facts that move a thread between shelves.
///
/// Placement is a *human* decision about attention, so it is stored, while the
/// two sections that describe live work (`Working` and "needs you") are
/// derived by [`section`]. That split is what keeps a settled thread from
/// claiming to run, and a running thread from hiding in a shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementAction {
    Settle,
    Unsettle,
    Snooze,
    Wake,
    /// Fact: the inactivity/PR rules parked an idle thread.
    AutoSettle,
    /// Fact: a snooze deadline passed.
    SnoozeElapsed,
}

pub struct PlacementMachine;

impl PlacementMachine {
    pub const ENTITY: &'static str = "placement";

    pub fn transition(
        current: ThreadPlacement,
        action: &PlacementAction,
    ) -> Result<ThreadPlacement, TransitionError> {
        use ThreadPlacement::{Active, Settled, Snoozed};
        let next = match action {
            PlacementAction::Settle => match current {
                Active | Settled => Settled,
                Snoozed => return Err(illegal(Self::ENTITY, current, "settle")),
            },
            PlacementAction::Unsettle => match current {
                Settled | Active => Active,
                Snoozed => return Err(illegal(Self::ENTITY, current, "unsettle")),
            },
            PlacementAction::Snooze => match current {
                Active | Settled | Snoozed => Snoozed,
            },
            PlacementAction::Wake => match current {
                Snoozed | Active => Active,
                Settled => return Err(illegal(Self::ENTITY, current, "wake")),
            },
            PlacementAction::AutoSettle => match current {
                Active | Settled => Settled,
                Snoozed => return Err(illegal(Self::ENTITY, current, "auto_settle")),
            },
            PlacementAction::SnoozeElapsed => match current {
                Snoozed | Active => Active,
                Settled => return Err(illegal(Self::ENTITY, current, "snooze_elapsed")),
            },
        };
        Ok(next)
    }

    /// Why a command is illegal from this placement, in language a toast can
    /// show. The machine keeps the copy next to the rule so the two cannot
    /// drift apart.
    #[must_use]
    pub fn refusal(action: &PlacementAction, current: ThreadPlacement) -> &'static str {
        match (action, current) {
            (PlacementAction::Settle, ThreadPlacement::Snoozed) => {
                "wake this thread before settling it"
            }
            (PlacementAction::Unsettle, ThreadPlacement::Snoozed) => "this thread is snoozed",
            (PlacementAction::Wake, ThreadPlacement::Settled) => {
                "this thread is settled; un-settle it instead"
            }
            _ => "this thread is already in that state",
        }
    }

    /// Resolve the command id a client sent. Facts are not invocable, so a
    /// payload can never park a thread by claiming `auto_settle`.
    #[must_use]
    pub fn command(id: &str) -> Option<PlacementAction> {
        PlacementAction::ALL
            .iter()
            .copied()
            .find(|action| action.kind() == ActionKind::Command && action.id() == id)
    }
}

impl States for ThreadPlacement {
    fn all() -> &'static [Self] {
        &[Self::Active, Self::Snoozed, Self::Settled]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Snoozed => "Snoozed",
            Self::Settled => "Settled",
        }
    }
}

impl Actions for PlacementAction {
    type State = ThreadPlacement;

    const ALL: &'static [Self] = &[
        Self::Settle,
        Self::Unsettle,
        Self::Snooze,
        Self::Wake,
        Self::AutoSettle,
        Self::SnoozeElapsed,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Settle => "settle",
            Self::Unsettle => "unsettle",
            Self::Snooze => "snooze",
            Self::Wake => "wake",
            Self::AutoSettle => "auto_settle",
            Self::SnoozeElapsed => "snooze_elapsed",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Settle => "Settle",
            Self::Unsettle => "Un-settle",
            Self::Snooze => "Snooze",
            Self::Wake => "Wake",
            Self::AutoSettle => "Settled automatically",
            Self::SnoozeElapsed => "Snooze ended",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Settle | Self::Unsettle | Self::Snooze | Self::Wake => ActionKind::Command,
            Self::AutoSettle | Self::SnoozeElapsed => ActionKind::Fact,
        }
    }

    fn commands(state: ThreadPlacement) -> &'static [Self] {
        match state {
            ThreadPlacement::Active => &[Self::Settle, Self::Snooze],
            ThreadPlacement::Settled => &[Self::Unsettle, Self::Snooze],
            ThreadPlacement::Snoozed => &[Self::Wake],
        }
    }
}

/// Whether the thread is doing work that a shelf must not hide.
///
/// `Error`, `Interrupted` and `RequiresAction` are deliberately excluded: those
/// are the states where the thread needs a person, so they belong in the active
/// list no matter how much background work is still nominally queued.
#[must_use]
pub fn is_working(status: ThreadStatus, queued_inputs: u32, active_subagents: u32) -> bool {
    match status {
        ThreadStatus::Preparing | ThreadStatus::Running => true,
        ThreadStatus::Idle => queued_inputs > 0 || active_subagents > 0,
        ThreadStatus::RequiresAction
        | ThreadStatus::Interrupted
        | ThreadStatus::Error
        | ThreadStatus::Archived => false,
    }
}

/// Sort rank of a section. Pinned work outranks a shelf, and closed work sinks
/// below everything, because that is the order a person scans the sidebar in.
#[must_use]
pub fn section_rank(section: ThreadSection) -> u8 {
    match section {
        ThreadSection::Pinned => 0,
        ThreadSection::Active => 1,
        ThreadSection::Working => 2,
        ThreadSection::Snoozed => 3,
        ThreadSection::Settled => 4,
        ThreadSection::Archived => 5,
    }
}

/// Whether the thread is waiting on a person rather than on the agent. These
/// rows sort to the top of the active list: an approval nobody can see is a
/// turn nobody can finish.
#[must_use]
pub fn needs_attention(status: ThreadStatus, pending: u32) -> bool {
    pending > 0
        || matches!(
            status,
            ThreadStatus::RequiresAction | ThreadStatus::Error | ThreadStatus::Interrupted
        )
}

/// The row a thread renders in.
///
/// Precedence is the product rule, in one place: closed work leaves every
/// shelf, a pin outranks a shelf, then the parked shelves, then live work.
#[must_use]
pub fn section(
    status: ThreadStatus,
    placement: ThreadPlacement,
    pinned: bool,
    working: bool,
) -> ThreadSection {
    if status == ThreadStatus::Archived {
        return ThreadSection::Archived;
    }
    if pinned {
        return ThreadSection::Pinned;
    }
    match placement {
        ThreadPlacement::Snoozed => ThreadSection::Snoozed,
        ThreadPlacement::Settled => ThreadSection::Settled,
        ThreadPlacement::Active if working => ThreadSection::Working,
        ThreadPlacement::Active => ThreadSection::Active,
    }
}

/// Everything the inactivity sweeper knows about one thread.
///
/// The sweeper cannot answer "should this settle" from a single row: idle status
/// is necessary but not sufficient, and a thread that nobody is waiting on may
/// still be running a subagent. Passing the facts keeps the policy pure and the
/// SQL to a prefilter.
#[derive(Debug, Clone, Copy)]
pub struct IdleFacts {
    pub status: ThreadStatus,
    pub placement: ThreadPlacement,
    /// `false` when the thread opted out with "never auto-settle".
    pub auto_settle: bool,
    pub working: bool,
    pub pending: u32,
    pub last_activity_at: i64,
    /// When the thread last moved between shelves.
    pub placement_at: i64,
}

/// Whether inactivity alone may park this thread.
///
/// Only `idle` qualifies. `error` and `interrupted` need a person, `working`
/// covers running turns, queued input and live subagents, and anything with an
/// unanswered request is by definition not done.
///
/// The quiet period also counts from the last *move*, not only from the last
/// message: a thread that was just woken or un-settled has been looked at, so
/// parking it again in the same breath would hide it before anyone saw it.
#[must_use]
pub fn should_auto_settle(facts: IdleFacts, now: i64, idle_ms: i64) -> bool {
    let quiet_since = facts.last_activity_at.max(facts.placement_at);
    idle_ms > 0
        && facts.auto_settle
        && facts.placement == ThreadPlacement::Active
        && facts.status == ThreadStatus::Idle
        && !facts.working
        && facts.pending == 0
        && now.saturating_sub(quiet_since) >= idle_ms
}

/// Why a thread cannot be settled right now, for the 409 the client shows.
#[must_use]
pub fn settle_blocker(
    status: ThreadStatus,
    queued_inputs: u32,
    active_subagents: u32,
    pending: u32,
) -> Option<&'static str> {
    if pending > 0 {
        return Some("answer the pending approval or question before settling");
    }
    if is_working(status, queued_inputs, active_subagents) {
        return Some("stop the turn before settling");
    }
    None
}

/// Guard for commands whose side effects require an unsettled thread.
pub fn check_settle_allowed(
    status: ThreadStatus,
    queued_inputs: u32,
    active_subagents: u32,
    pending: u32,
) -> Result<(), TransitionError> {
    match settle_blocker(status, queued_inputs, active_subagents, pending) {
        Some(reason) => Err(guard(PlacementMachine::ENTITY, reason)),
        None => Ok(()),
    }
}
