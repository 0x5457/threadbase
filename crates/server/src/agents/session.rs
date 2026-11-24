//! Per-thread session state and its locks.
//!
//! `harness` serializes ACP calls, `turn` holds the single turn slot and is
//! never held while a turn is prepared. A sender claims `starting` before any
//! git or database work, so a second sender queues instead of racing.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use threadbase_agent::{Capabilities, Harness, TurnHandle};
use threadbase_core::state::{TurnAction, TurnMachine};
use threadbase_shared::{now_ms, MessageId, TurnId, TurnState};
use tokio::sync::{Mutex, OwnedSemaphorePermit};

/// How long adapter output without a recorded turn still counts as that turn
/// running. Long enough to cover a slow tool call, short enough that a message
/// queued behind such a turn does not wait minutes.
const UNOWNED_TURN_MS: i64 = 15_000;

pub(crate) struct Session {
    pub(crate) capabilities: Capabilities,
    pub(crate) harness: Mutex<Box<dyn Harness>>,
    turn: Mutex<TurnSlot>,
    pub(crate) run: Arc<Mutex<RunState>>,
    /// Signalled by the pump task when its event stream closes. A rewind waits
    /// for this before deleting the tail, so a late idle update from the old
    /// adapter cannot land after the cut and resurrect part of the turn.
    pub(crate) pump_done: Arc<tokio::sync::Notify>,
    /// Set once the adapter is gone or being torn down, so a process exit is
    /// not mistaken for a crash and a dead session is never reused.
    stopped: AtomicBool,
    /// Text a fresh session receives before its first prompt: a forked
    /// thread's inherited transcript. Taken once, so a resumed session that
    /// already knows the conversation never gets it twice.
    history: Mutex<Option<String>>,
    /// Last adapter output that belonged to no recorded turn, in epoch ms.
    /// An adapter that resumes on its own (a subagent callback, a goal
    /// continuation) streams such output and ends it without a completion
    /// Threadbase can match, so recency is the only signal that it is over.
    unowned_output_ms: AtomicI64,
    /// Last harness interaction, used by the idle-session reaper.
    last_used_ms: AtomicI64,
}

impl Session {
    pub(crate) fn new(
        harness: Box<dyn Harness>,
        capabilities: Capabilities,
        run: Arc<Mutex<RunState>>,
    ) -> Self {
        Self {
            capabilities,
            harness: Mutex::new(harness),
            turn: Mutex::new(TurnSlot::default()),
            run,
            pump_done: Arc::new(tokio::sync::Notify::new()),
            stopped: AtomicBool::new(false),
            history: Mutex::new(None),
            unowned_output_ms: AtomicI64::new(0),
            last_used_ms: AtomicI64::new(now_ms()),
        }
    }

    /// Remember adapter output that belongs to no recorded turn.
    pub(crate) fn note_unowned_output(&self) {
        self.unowned_output_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub(crate) fn clear_unowned_output(&self) {
        self.unowned_output_ms.store(0, Ordering::Relaxed);
    }

    /// Whether the adapter is still finishing a turn Threadbase has no record
    /// of. A prompt issued now would take that turn's completion for its own
    /// and fail with no output, so callers have to wait instead.
    pub(crate) fn unowned_turn(&self) -> bool {
        let at = self.unowned_output_ms.load(Ordering::Relaxed);
        at != 0 && now_ms().saturating_sub(at) < UNOWNED_TURN_MS
    }

    /// Hand the next prompt the transcript this session was forked from.
    pub(crate) async fn set_history(&self, text: String) {
        *self.history.lock().await = Some(text);
    }

    pub(crate) async fn take_history(&self) -> Option<String> {
        self.history.lock().await.take()
    }

    /// Put the context back when the prompt never reached the adapter; the
    /// next turn still needs the conversation the fork copied.
    pub(crate) async fn restore_history(&self, text: Option<String>) {
        if text.is_some() {
            *self.history.lock().await = text;
        }
    }

    pub(crate) fn touch(&self) {
        self.last_used_ms.store(now_ms(), Ordering::Relaxed);
    }

    /// Milliseconds since the last interaction; `i64::MAX` when stopped.
    pub(crate) fn idle_ms(&self) -> i64 {
        if self.is_stopped() {
            return i64::MAX;
        }
        now_ms().saturating_sub(self.last_used_ms.load(Ordering::Relaxed))
    }

    /// Mark the session before the harness is stopped, or when its process
    /// exited unexpectedly, so it is never handed out again.
    pub(crate) fn mark_stopped(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Kill the adapter behind this session. `mark_stopped` comes first so the
    /// dying process is not read as a crash and the session is never reused.
    pub(crate) async fn stop(&self) {
        self.mark_stopped();
        let mut harness = self.harness.lock().await;
        let _ = harness.shutdown().await;
    }

    /// Claim the right to prepare the next turn. The returned token must be
    /// handed to `begin_turn` or released; it is the only way to start a
    /// prompt, so two input paths cannot prepare turns concurrently.
    pub(crate) async fn claim_start(&self) -> Option<TurnStartClaim<'_>> {
        if self.is_stopped() {
            return None;
        }
        self.touch();
        self.turn
            .lock()
            .await
            .claim()
            .then_some(TurnStartClaim { session: self })
    }

    pub(crate) async fn abandon_start(&self) {
        self.turn.lock().await.abandon_start();
    }

    /// Install the prompt's handle and report whether the turn already ended
    /// while the start claim was held.
    pub(crate) async fn install_turn(
        &self,
        handle: TurnHandle,
        permit: Option<OwnedSemaphorePermit>,
    ) -> TurnInstall {
        self.touch();
        self.turn.lock().await.install(handle, permit)
    }

    pub(crate) async fn clear_turn(&self) {
        self.touch();
        self.turn.lock().await.clear();
    }

    /// Take the turn for stop-and-send or cancellation without setting the
    /// cancel flag used for turns that are still starting.
    pub(crate) async fn take_turn(&self) -> Option<TurnHandle> {
        self.touch();
        self.turn.lock().await.take_running()
    }

    /// Take a running turn, or remember that a turn which is still being
    /// prepared must be cancelled as soon as its prompt is issued.
    pub(crate) async fn take_turn_for_cancel(&self) -> Option<TurnHandle> {
        self.touch();
        self.turn.lock().await.take_for_cancel()
    }

    /// Reconcile the in-memory handle with the persisted thread status. A
    /// handle is stale once the thread is no longer running.
    pub(crate) async fn observe_running(&self, persisted_running: bool) -> bool {
        self.touch();
        self.turn.lock().await.observe_running(persisted_running)
    }
}

/// The claimed right to start one turn. Holding this token means the turn
/// slot's `starting` flag is set; the token does not hold the mutex.
pub(crate) struct TurnStartClaim<'a> {
    session: &'a Session,
}

impl<'a> TurnStartClaim<'a> {
    pub(crate) fn session(&self) -> &'a Session {
        self.session
    }

    pub(crate) async fn release(self) {
        self.session.abandon_start().await;
    }
}

/// The result of installing a prompt handle. A turn can finish before the
/// starter gets here when the harness emits its completion events immediately.
pub(crate) enum TurnInstall {
    Running { cancel_requested: bool },
    AlreadyFinished,
}

/// The one in-memory turn slot for a thread.
///
/// The shape mirrors [`TurnMachine`] exactly; legality is asked of the machine
/// so the persisted status rules and the concurrency rules cannot disagree.
#[derive(Default)]
enum TurnSlot {
    #[default]
    Empty,
    Starting {
        /// A cancel arrived before the prompt handle existed.
        cancel_requested: bool,
        /// A completion event arrived before the handle existed.
        completed_while_starting: bool,
    },
    Running {
        handle: TurnHandle,
        /// Global turn-concurrency permit, released when the turn clears.
        _permit: Option<OwnedSemaphorePermit>,
    },
}

impl TurnSlot {
    fn state(&self) -> TurnState {
        match self {
            Self::Empty => TurnState::Empty,
            Self::Starting { .. } => TurnState::Starting,
            Self::Running { .. } => TurnState::Running,
        }
    }

    /// Ask the machine whether `action` is legal here. Every method below
    /// returns early when it is not, so the enum can never reach a shape the
    /// machine would reject.
    fn allows(&self, action: &TurnAction) -> bool {
        TurnMachine::transition(self.state(), action).is_ok()
    }

    fn claim(&mut self) -> bool {
        if !self.allows(&TurnAction::Claim) {
            return false;
        }
        *self = Self::Starting {
            cancel_requested: false,
            completed_while_starting: false,
        };
        true
    }

    fn abandon_start(&mut self) {
        if self.allows(&TurnAction::Abandon) {
            *self = Self::Empty;
        }
    }

    fn install(&mut self, handle: TurnHandle, permit: Option<OwnedSemaphorePermit>) -> TurnInstall {
        let Self::Starting {
            cancel_requested,
            completed_while_starting,
        } = std::mem::take(self)
        else {
            debug_assert!(false, "install without a start claim");
            return TurnInstall::AlreadyFinished;
        };
        if completed_while_starting {
            return TurnInstall::AlreadyFinished;
        }
        *self = Self::Running {
            handle,
            _permit: permit,
        };
        TurnInstall::Running { cancel_requested }
    }

    fn clear(&mut self) {
        if self.allows(&TurnAction::End) {
            match std::mem::take(self) {
                // The starter still has to observe the completion.
                Self::Starting {
                    cancel_requested, ..
                } => {
                    *self = Self::Starting {
                        cancel_requested,
                        completed_while_starting: true,
                    };
                }
                _ => *self = Self::Empty,
            }
        }
    }

    fn take_for_cancel(&mut self) -> Option<TurnHandle> {
        if !self.allows(&TurnAction::Cancel) {
            return None;
        }
        match self {
            Self::Starting {
                cancel_requested, ..
            } => {
                *cancel_requested = true;
                None
            }
            Self::Running { .. } => match std::mem::take(self) {
                Self::Running { handle, .. } => Some(handle),
                _ => None,
            },
            Self::Empty => None,
        }
    }

    /// Take a running turn without arming the cancel flag. A turn that is
    /// still starting has no handle yet, so stop-and-send falls back to the
    /// queued path.
    fn take_running(&mut self) -> Option<TurnHandle> {
        match std::mem::take(self) {
            Self::Running { handle, .. } => Some(handle),
            other => {
                *self = other;
                None
            }
        }
    }

    fn observe_running(&mut self, persisted_running: bool) -> bool {
        if !self.allows(&TurnAction::Sync {
            running: persisted_running,
        }) {
            return false;
        }
        if persisted_running {
            return matches!(self, Self::Running { .. });
        }
        if matches!(self, Self::Running { .. }) {
            *self = Self::Empty;
        }
        false
    }
}

#[derive(Default)]
pub(crate) struct RunState {
    pub(crate) turn_id: Option<TurnId>,
    pub(crate) thinking_id: Option<MessageId>,
    /// Agent messages streamed this turn, in arrival order. The protocol names
    /// each message, so one turn can carry several transcript rows.
    pub(crate) agent_messages: Vec<(String, OpenMessage)>,
    /// Tool calls already inserted by this turn, so an upsert knows whether it
    /// creates the row or patches it.
    pub(crate) tools: std::collections::HashSet<String>,
    /// Something reached the transcript during this turn. A turn that ends with
    /// `end_turn` and nothing to show for it died upstream.
    pub(crate) saw_output: bool,
}

/// One agent message being streamed, keyed by the protocol's stable id.
#[derive(Default)]
pub(crate) struct OpenMessage {
    pub(crate) id: MessageId,
    pub(crate) text: String,
    /// The text last written to the transcript. An upsert writes immediately;
    /// the turn end only writes again when the text changed since.
    pub(crate) completed: Option<String>,
}

impl RunState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// The transcript row for one protocol message id, created on first sight.
    pub(crate) fn open_message(&mut self, protocol_id: &str) -> &mut OpenMessage {
        let index = match self
            .agent_messages
            .iter()
            .position(|(id, _)| id == protocol_id)
        {
            Some(index) => index,
            None => {
                self.agent_messages
                    .push((protocol_id.to_owned(), OpenMessage::default()));
                self.agent_messages.len() - 1
            }
        };
        &mut self.agent_messages[index].1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(turn_id: &str) -> TurnHandle {
        TurnHandle {
            turn_id: turn_id.to_owned(),
        }
    }

    #[test]
    fn only_one_sender_can_claim_a_start() {
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        assert!(!slot.claim(), "a claimed preparation must stay exclusive");
        assert!(!slot.observe_running(false));
        assert!(!slot.claim(), "the claim survives a stale-status check");
    }

    #[test]
    fn installing_the_prompt_handle_releases_the_start_claim() {
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        assert!(matches!(
            slot.install(handle("turn-1"), None),
            TurnInstall::Running {
                cancel_requested: false
            }
        ));
        assert_eq!(slot.state(), TurnState::Running);
        assert!(!slot.claim(), "a running turn cannot be started again");
        assert!(slot.observe_running(true));
    }

    #[test]
    fn abandoning_a_start_allows_the_next_sender_to_claim() {
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        slot.abandon_start();
        assert!(slot.claim());
    }

    #[test]
    fn cancel_during_preparation_is_delivered_after_the_prompt() {
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        assert!(slot.take_for_cancel().is_none());
        assert!(
            matches!(
                slot.install(handle("turn-1"), None),
                TurnInstall::Running {
                    cancel_requested: true
                }
            ),
            "the starter must observe the cancel that arrived during preparation"
        );
    }

    #[test]
    fn a_turn_that_finishes_while_starting_is_not_installed() {
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        slot.clear();
        assert!(matches!(
            slot.install(handle("turn-1"), None),
            TurnInstall::AlreadyFinished
        ));
        assert_eq!(
            slot.state(),
            TurnState::Empty,
            "a completed turn stays completed"
        );
    }

    #[tokio::test]
    async fn clearing_a_turn_releases_the_concurrency_permit() {
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = slots.clone().try_acquire_owned().unwrap();
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        assert!(matches!(
            slot.install(handle("turn-1"), Some(permit)),
            TurnInstall::Running { .. }
        ));
        assert!(slots.clone().try_acquire_owned().is_err());
        slot.clear();
        assert!(slots.clone().try_acquire_owned().is_ok());
    }

    #[test]
    fn stale_turn_handles_are_cleared_when_the_thread_is_not_running() {
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        assert!(matches!(
            slot.install(handle("turn-1"), None),
            TurnInstall::Running { .. }
        ));
        assert!(!slot.observe_running(false));
        assert_eq!(slot.state(), TurnState::Empty);
    }
}
