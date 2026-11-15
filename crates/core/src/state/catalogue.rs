//! The published catalogue: every state, command and fact.

use threadbase_shared::{MachineAction, MachineCommand, MachineSpec, MachineState, ThreadStatus};

use super::*;

// ---------------------------------------------------------------------------
// Catalogue: states, commands and facts, published to every consumer
// ---------------------------------------------------------------------------
macro_rules! action {
    ($action:expr) => {
        MachineAction {
            id: $action.id().to_owned(),
            label: $action.label().to_owned(),
            kind: $action.kind(),
        }
    };
}

/// `commands` pairs each command with the state it lands on; the catalogue
/// test checks every pair against the machine's transition function.
fn state(id: &str, label: &str, terminal: bool, commands: &[(&str, &str)]) -> MachineState {
    MachineState {
        id: id.to_owned(),
        label: label.to_owned(),
        terminal,
        commands: commands
            .iter()
            .map(|(id, to)| MachineCommand {
                id: (*id).to_owned(),
                to: (*to).to_owned(),
            })
            .collect(),
    }
}
fn spec(
    id: &str,
    title: &str,
    summary: &str,
    states: Vec<MachineState>,
    actions: Vec<MachineAction>,
) -> MachineSpec {
    MachineSpec {
        id: id.to_owned(),
        title: title.to_owned(),
        summary: summary.to_owned(),
        states,
        actions,
    }
}

/// The whole lifecycle catalogue. `threadbase machines` and
/// `GET /api/machines` return exactly this, and a test writes it to
/// `web/src/api/state-machines.json`.
#[must_use]
pub fn catalogue() -> Vec<MachineSpec> {
    vec![
        spec(
            "thread",
            "Thread",
            "A shared agent conversation: startup, turns, attention and archiving.",
            vec![
                state(
                    "preparing",
                    "Preparing",
                    false,
                    &[("restart", "preparing"), ("archive", "archived")],
                ),
                state(
                    "idle",
                    "Idle",
                    false,
                    &[
                        ("prompt", "running"),
                        ("restart", "preparing"),
                        ("archive", "archived"),
                    ],
                ),
                state(
                    "running",
                    "Running",
                    false,
                    &[
                        ("prompt", "running"),
                        ("cancel", "idle"),
                        ("restart", "preparing"),
                        ("archive", "archived"),
                    ],
                ),
                state(
                    "requires_action",
                    "Needs you",
                    false,
                    &[
                        ("prompt", "running"),
                        ("cancel", "idle"),
                        ("restart", "preparing"),
                        ("archive", "archived"),
                    ],
                ),
                state(
                    "interrupted",
                    "Interrupted",
                    false,
                    &[
                        ("prompt", "running"),
                        ("restart", "preparing"),
                        ("archive", "archived"),
                    ],
                ),
                state(
                    "error",
                    "Error",
                    false,
                    &[
                        ("prompt", "running"),
                        ("restart", "preparing"),
                        ("archive", "archived"),
                    ],
                ),
                state("archived", "Archived", false, &[("unarchive", "idle")]),
            ],
            vec![
                action!(ThreadAction::Create),
                action!(ThreadAction::Restart),
                action!(ThreadAction::StartSucceeded),
                action!(ThreadAction::StartFailed),
                action!(ThreadAction::Prompt),
                action!(ThreadAction::ApprovalPending),
                action!(ThreadAction::ElicitationPending),
                action!(ThreadAction::ApprovalResolved {
                    turn_active: true,
                    remaining_pending: 0,
                }),
                action!(ThreadAction::ElicitationResolved {
                    turn_active: false,
                    remaining_pending: 0,
                }),
                action!(ThreadAction::TurnEnded),
                action!(ThreadAction::TurnError),
                action!(ThreadAction::ProcessExited),
                action!(ThreadAction::AgentFailed),
                action!(ThreadAction::Cancel),
                action!(ThreadAction::Archive),
                action!(ThreadAction::Unarchive),
            ],
        ),
        spec(
            "setup",
            "Worktree setup",
            "The project's setup command for this worktree; it gates the agent.",
            vec![
                state("none", "Not configured", false, &[("start", "pending")]),
                state("pending", "Queued", false, &[]),
                state("running", "Running", false, &[]),
                state("succeeded", "Succeeded", false, &[("start", "pending")]),
                state("failed", "Failed", false, &[("start", "pending")]),
            ],
            vec![
                action!(SetupAction::Start),
                action!(SetupAction::Started),
                action!(SetupAction::Finish),
                action!(SetupAction::Fail),
                action!(SetupAction::Interrupt),
            ],
        ),
        spec(
            "approval",
            "Approval",
            "One agent permission request awaiting a human decision.",
            vec![
                state(
                    "pending",
                    "Pending",
                    false,
                    &[
                        ("allow", "allowed"),
                        ("deny", "denied"),
                        ("cancel", "cancelled"),
                    ],
                ),
                state("allowed", "Allowed", true, &[]),
                state("denied", "Denied", true, &[]),
                state("cancelled", "Cancelled", true, &[]),
            ],
            vec![
                action!(ApprovalAction::Create),
                action!(ApprovalAction::Allow),
                action!(ApprovalAction::Deny),
                action!(ApprovalAction::Cancel),
            ],
        ),
        spec(
            "elicitation",
            "Question",
            "One agent question awaiting a structured answer.",
            vec![
                state(
                    "pending",
                    "Pending",
                    false,
                    &[
                        ("accept", "accepted"),
                        ("decline", "declined"),
                        ("cancel", "cancelled"),
                    ],
                ),
                state("accepted", "Answered", true, &[]),
                state("declined", "Declined", true, &[]),
                state("cancelled", "Cancelled", true, &[]),
            ],
            vec![
                action!(ElicitationAction::Create),
                action!(ElicitationAction::Accept),
                action!(ElicitationAction::Decline),
                action!(ElicitationAction::Cancel),
            ],
        ),
        spec(
            "subagent",
            "Subagent",
            "A delegation reported by the adapter; mirrored from tool calls when needed.",
            vec![
                state("idle", "Idle", false, &[("cancel", "cancelled")]),
                state("running", "Running", false, &[("cancel", "cancelled")]),
                state(
                    "requires_action",
                    "Needs input",
                    false,
                    &[("cancel", "cancelled")],
                ),
                state("cancelled", "Cancelled", true, &[]),
                state("error", "Error", true, &[]),
            ],
            vec![
                action!(SubagentAction::Create),
                action!(SubagentAction::Run),
                action!(SubagentAction::Idle),
                action!(SubagentAction::RequireAction),
                action!(SubagentAction::Resolve),
                action!(SubagentAction::Fail),
                action!(SubagentAction::Cancel),
            ],
        ),
        spec(
            "queue",
            "Queued input",
            "A message waiting for the thread's next turn or being delivered now.",
            vec![
                state("queued", "Queued", false, &[("delete", "dropped")]),
                state("sending", "Sending", false, &[]),
                state("sent", "Sent", true, &[]),
                state("superseded", "Superseded", true, &[]),
                state("dropped", "Removed", false, &[("restore", "queued")]),
            ],
            vec![
                action!(QueueAction::Enqueue),
                action!(QueueAction::Dispatch),
                action!(QueueAction::PromptSucceeded),
                action!(QueueAction::PromptFailed),
                action!(QueueAction::Release),
                action!(QueueAction::Supersede),
                action!(QueueAction::Delete),
                action!(QueueAction::Restore),
            ],
        ),
        spec(
            "comment",
            "Comment",
            "A review note; pending comments are batched into the next prompt.",
            vec![
                state(
                    "pending",
                    "Pending",
                    false,
                    &[("deliver", "delivered"), ("resolve", "resolved")],
                ),
                state("delivered", "Delivered", false, &[("resolve", "resolved")]),
                state("resolved", "Resolved", true, &[]),
            ],
            vec![
                action!(CommentAction::CreatePending),
                action!(CommentAction::CreateDelivered),
                action!(CommentAction::Deliver),
                action!(CommentAction::Resolve),
            ],
        ),
        spec(
            "watch",
            "Watching",
            "One user's notification relationship with a thread.",
            vec![
                state(
                    "not_watching",
                    "Not watching",
                    false,
                    &[("watch", "watching")],
                ),
                state(
                    "watching",
                    "Watching",
                    false,
                    &[("mute", "muted"), ("unwatch", "not_watching")],
                ),
                state(
                    "muted",
                    "Muted",
                    false,
                    &[("unmute", "watching"), ("unwatch", "not_watching")],
                ),
            ],
            vec![
                action!(WatchAction::Watch),
                action!(WatchAction::Mute),
                action!(WatchAction::Unmute),
                action!(WatchAction::Unwatch),
            ],
        ),
        spec(
            "turn",
            "Turn",
            "The single in-memory turn slot that serialises prompts for one thread.",
            vec![
                state("empty", "Idle", false, &[]),
                state("starting", "Starting", false, &[("cancel", "starting")]),
                state("running", "Running", false, &[("cancel", "empty")]),
            ],
            vec![
                action!(TurnAction::Claim),
                action!(TurnAction::Abandon),
                action!(TurnAction::Install),
                action!(TurnAction::InstallAfterFinish),
                action!(TurnAction::Complete),
                action!(TurnAction::End),
                action!(TurnAction::Cancel),
                action!(TurnAction::Sync { running: true }),
            ],
        ),
        spec(
            "auth_session",
            "Sign-in session",
            "One browser session cookie; live until it expires or is revoked.",
            vec![
                state(
                    "active",
                    "Active",
                    false,
                    &[("touch", "active"), ("revoke", "revoked")],
                ),
                state("expired", "Expired", true, &[]),
                state("revoked", "Revoked", true, &[]),
            ],
            vec![
                action!(AuthSessionAction::Create),
                action!(AuthSessionAction::Touch),
                action!(AuthSessionAction::Revoke),
                action!(AuthSessionAction::Expire),
            ],
        ),
        spec(
            "push",
            "Web Push delivery",
            "One inbox row's delivery to browser push subscriptions, with retries.",
            vec![
                state("pending", "Pending", false, &[]),
                state("sent", "Sent", true, &[]),
                state("failed", "Failed", true, &[("retry", "pending")]),
                state("skipped", "Skipped", true, &[("retry", "pending")]),
            ],
            vec![
                action!(PushAction::Enqueue),
                action!(PushAction::Deliver),
                action!(PushAction::Fail),
                action!(PushAction::GiveUp),
                action!(PushAction::Skip),
                action!(PushAction::Retry),
            ],
        ),
        spec(
            "connection",
            "Live connection",
            "The browser's WebSocket subscription to one thread's event stream.",
            vec![
                state("idle", "Idle", false, &[]),
                state("loading", "Loading", false, &[]),
                state("live", "Live", false, &[]),
                state(
                    "reconnecting",
                    "Reconnecting",
                    false,
                    &[("retry", "loading")],
                ),
                state("failed", "Failed", false, &[("retry", "loading")]),
            ],
            vec![
                action!(ConnectionAction::Connect),
                action!(ConnectionAction::Opened),
                action!(ConnectionAction::Ready),
                action!(ConnectionAction::Lost),
                action!(ConnectionAction::Fail),
                action!(ConnectionAction::Retry),
                action!(ConnectionAction::Close),
            ],
        ),
        spec(
            "draft",
            "Shared draft",
            "The collaborative composer document; sending waits for the live phase.",
            vec![
                state("idle", "Idle", false, &[]),
                state("connecting", "Connecting", false, &[]),
                state("syncing", "Syncing", false, &[]),
                state("live", "Live", false, &[]),
                state("offline", "Offline", false, &[("retry", "connecting")]),
            ],
            vec![
                action!(DraftAction::Connect),
                action!(DraftAction::Connected),
                action!(DraftAction::Synced),
                action!(DraftAction::Lost),
                action!(DraftAction::Close),
                action!(DraftAction::Retry),
            ],
        ),
        spec(
            "terminal",
            "Terminal",
            "One browser's attachment to the thread's tmux session.",
            vec![
                state("connecting", "Connecting", false, &[]),
                state("live", "Live", false, &[("drive", "live")]),
                state("exited", "Exited", true, &[("reconnect", "connecting")]),
                state("error", "Error", true, &[("reconnect", "connecting")]),
            ],
            vec![
                action!(TerminalAction::Connect),
                action!(TerminalAction::Attached),
                action!(TerminalAction::Exit),
                action!(TerminalAction::Fail),
                action!(TerminalAction::Reconnect),
                action!(TerminalAction::Drive),
            ],
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
