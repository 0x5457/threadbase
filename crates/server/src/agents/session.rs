//! Per-thread session state and its lock boundaries.
//!
//! Three concerns are deliberately separate so a slow turn cannot block
//! approvals, config reads or cancellation:
//! - `capabilities` is immutable after startup,
//! - `harness` serializes ACP IPC,
//! - `turn` only holds the single turn slot and is never held while a turn is
//!   being prepared.
//!
//! A sender claims the `starting` slot before doing any git, database or
//! prompt preparation. Other senders observe the claim as busy and queue
//! instead, which preserves the single-turn invariant.

use std::sync::Arc;
use threadbase_agent::{Capabilities, Harness, TurnHandle};
use threadbase_shared::{MessageId, TurnId};
use tokio::sync::Mutex;

pub(crate) struct Session {
    pub(crate) capabilities: Capabilities,
    pub(crate) harness: Mutex<Box<dyn Harness>>,
    turn: Mutex<TurnSlot>,
    pub(crate) run: Arc<Mutex<RunState>>,
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
        }
    }

    /// Claim the right to prepare the next turn. The returned token must be
    /// handed to `begin_turn` or released; it is the only way to start a
    /// prompt, so two input paths cannot prepare turns concurrently.
    pub(crate) async fn claim_start(&self) -> Option<TurnStartClaim<'_>> {
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
    pub(crate) async fn install_turn(&self, handle: TurnHandle) -> TurnInstall {
        self.turn.lock().await.install(handle)
    }

    pub(crate) async fn clear_turn(&self) {
        self.turn.lock().await.clear();
    }

    /// Take the turn for stop-and-send or cancellation without setting the
    /// cancel flag used for turns that are still starting.
    pub(crate) async fn take_turn(&self) -> Option<TurnHandle> {
        self.turn.lock().await.turn.take()
    }

    /// Take a running turn, or remember that a turn which is still being
    /// prepared must be cancelled as soon as its prompt is issued.
    pub(crate) async fn take_turn_for_cancel(&self) -> Option<TurnHandle> {
        self.turn.lock().await.take_for_cancel()
    }

    /// Reconcile the in-memory handle with the persisted thread status. A
    /// handle is stale once the thread is no longer running.
    pub(crate) async fn observe_running(&self, persisted_running: bool) -> bool {
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

#[derive(Default)]
struct TurnSlot {
    turn: Option<TurnHandle>,
    starting: bool,
    cancel_requested: bool,
    completed_while_starting: bool,
}

impl TurnSlot {
    fn claim(&mut self) -> bool {
        if self.turn.is_some() || self.starting {
            return false;
        }
        self.starting = true;
        self.cancel_requested = false;
        self.completed_while_starting = false;
        true
    }

    fn abandon_start(&mut self) {
        self.starting = false;
        self.cancel_requested = false;
        self.completed_while_starting = false;
    }

    fn install(&mut self, handle: TurnHandle) -> TurnInstall {
        self.starting = false;
        if self.completed_while_starting {
            self.completed_while_starting = false;
            return TurnInstall::AlreadyFinished;
        }
        self.turn = Some(handle);
        TurnInstall::Running {
            cancel_requested: std::mem::take(&mut self.cancel_requested),
        }
    }

    fn clear(&mut self) {
        if self.starting {
            self.completed_while_starting = true;
        } else {
            self.turn = None;
        }
    }

    fn take_for_cancel(&mut self) -> Option<TurnHandle> {
        if self.starting {
            self.cancel_requested = true;
        }
        self.turn.take()
    }

    fn observe_running(&mut self, persisted_running: bool) -> bool {
        if persisted_running {
            self.turn.is_some()
        } else {
            self.turn = None;
            false
        }
    }
}

#[derive(Default)]
pub(crate) struct RunState {
    pub(crate) turn_id: Option<TurnId>,
    pub(crate) agent_message_id: Option<MessageId>,
    pub(crate) thinking_id: Option<MessageId>,
    pub(crate) agent_text: String,
}

impl RunState {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
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
            slot.install(handle("turn-1")),
            TurnInstall::Running {
                cancel_requested: false
            }
        ));
        assert!(slot.turn.is_some());
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
                slot.install(handle("turn-1")),
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
            slot.install(handle("turn-1")),
            TurnInstall::AlreadyFinished
        ));
        assert!(slot.turn.is_none(), "a completed turn stays completed");
    }

    #[test]
    fn stale_turn_handles_are_cleared_when_the_thread_is_not_running() {
        let mut slot = TurnSlot::default();
        assert!(slot.claim());
        assert!(matches!(
            slot.install(handle("turn-1")),
            TurnInstall::Running { .. }
        ));
        assert!(!slot.observe_running(false));
        assert!(slot.turn.is_none());
    }
}
