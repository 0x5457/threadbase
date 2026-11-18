//! The published catalogue: every machine, state, command and fact.
//!
//! Transitions stay authoritative; this file only decides *which* machines are
//! published and in what order. State and action metadata is declared next to
//! each machine ([`States`], [`Actions`]), and every `to` in the published
//! graph is the result of calling the machine's own transition function, so
//! the table the clients read cannot drift from the code that enforces it.

use threadbase_shared::{
    MachineAction, MachineCommand, MachineSpec, MachineState, ThreadPlacement, ThreadStatus,
};

use super::*;

/// Build one published machine.
///
/// # Panics
///
/// When a machine lists a command that its own transition rejects. That pair is
/// a programming error: publishing it would offer clients a button that is
/// guaranteed to return 409.
fn spec<S, A>(
    id: &'static str,
    title: &'static str,
    summary: &'static str,
    transition: fn(S, &A) -> Result<S, TransitionError>,
) -> MachineSpec
where
    S: States,
    A: Actions<State = S>,
{
    MachineSpec {
        id: id.to_owned(),
        title: title.to_owned(),
        summary: summary.to_owned(),
        states: S::all()
            .iter()
            .map(|&state| MachineState {
                id: wire_id(state),
                label: state.label().to_owned(),
                terminal: state.terminal(),
                commands: A::commands(state)
                    .iter()
                    .map(|action| MachineCommand {
                        id: action.id().to_owned(),
                        to: match transition(state, action) {
                            Ok(next) => wire_id(next),
                            Err(error) => panic!("{id}: published command is illegal: {error}"),
                        },
                    })
                    .collect(),
            })
            .collect(),
        actions: A::ALL
            .iter()
            .map(|action| MachineAction {
                id: action.id().to_owned(),
                label: action.label().to_owned(),
                kind: action.kind(),
            })
            .collect(),
    }
}

/// The stored wire value of a state.
fn wire_id<S: Into<&'static str>>(state: S) -> String {
    let id: &'static str = state.into();
    id.to_owned()
}

/// The whole lifecycle catalogue. `threadbase machines` and
/// `GET /api/machines` return exactly this, and a test writes it to
/// `web/src/api/state-machines.json`.
#[must_use]
pub fn catalogue() -> Vec<MachineSpec> {
    vec![
        spec(
            ThreadMachine::ENTITY,
            "Thread",
            "A shared agent conversation: startup, turns, attention and archiving.",
            ThreadMachine::transition,
        ),
        spec(
            PlacementMachine::ENTITY,
            "Thread placement",
            "Where a thread sits between turns: active, snoozed or settled. Working is derived, never stored.",
            PlacementMachine::transition,
        ),
        spec(
            SetupMachine::ENTITY,
            "Worktree setup",
            "The project's setup command for this worktree; it gates the agent.",
            SetupMachine::transition,
        ),
        spec(
            ApprovalMachine::ENTITY,
            "Approval",
            "One agent permission request awaiting a human decision.",
            ApprovalMachine::transition,
        ),
        spec(
            ElicitationMachine::ENTITY,
            "Question",
            "One agent question awaiting a structured answer.",
            ElicitationMachine::transition,
        ),
        spec(
            SubagentMachine::ENTITY,
            "Subagent",
            "A delegation reported by the adapter; mirrored from tool calls when needed.",
            SubagentMachine::transition,
        ),
        spec(
            QueueMachine::ENTITY,
            "Queued input",
            "A message waiting for the thread's next turn or being delivered now.",
            QueueMachine::transition,
        ),
        spec(
            CommentMachine::ENTITY,
            "Comment",
            "A review note; pending comments are batched into the next prompt.",
            CommentMachine::transition,
        ),
        spec(
            WatchMachine::ENTITY,
            "Watching",
            "One user's notification relationship with a thread.",
            WatchMachine::transition,
        ),
        spec(
            TurnMachine::ENTITY,
            "Turn",
            "The single in-memory turn slot that serialises prompts for one thread.",
            TurnMachine::transition,
        ),
        spec(
            AuthSessionMachine::ENTITY,
            "Sign-in session",
            "One browser session cookie; live until it expires or is revoked.",
            AuthSessionMachine::transition,
        ),
        spec(
            PushMachine::ENTITY,
            "Web Push delivery",
            "One inbox row's delivery to browser push subscriptions, with retries.",
            PushMachine::transition,
        ),
        spec(
            ConnectionMachine::ENTITY,
            "Live connection",
            "The browser's WebSocket subscription to one thread's event stream.",
            ConnectionMachine::transition,
        ),
        spec(
            DraftMachine::ENTITY,
            "Shared draft",
            "The collaborative composer document; sending waits for the live phase.",
            DraftMachine::transition,
        ),
        spec(
            TerminalMachine::ENTITY,
            "Terminal",
            "One browser's attachment to the thread's tmux session.",
            TerminalMachine::transition,
        ),
    ]
}

/// Command ids for a thread state. The projection puts this on the wire so the
/// client never re-derives eligibility from status strings.
#[must_use]
pub fn thread_commands(state: ThreadStatus) -> Vec<String> {
    ThreadAction::commands(state)
        .iter()
        .map(|action| action.id().to_owned())
        .collect()
}

/// Command ids for a placement. Mirrors [`thread_commands`] for the shelves.
#[must_use]
pub fn placement_commands(placement: ThreadPlacement) -> Vec<String> {
    PlacementAction::commands(placement)
        .iter()
        .map(|action| action.id().to_owned())
        .collect()
}
