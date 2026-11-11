//! Delivery policy for messages sent while a turn may be running.

use threadbase_shared::DeliveryMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendAction {
    /// Start a regular prompt; no turn is running.
    Prompt,
    /// Fold the input into the running turn.
    Inject,
    /// Cancel the running turn and immediately prompt with the input.
    StopAndSend,
    /// Persist the input in the thread queue.
    Queue,
}

#[derive(Debug, Clone, Copy)]
pub struct DeliveryContext {
    pub turn_running: bool,
    pub steering_supported: bool,
}

impl DeliveryContext {
    #[must_use]
    pub fn idle() -> Self {
        Self {
            turn_running: false,
            steering_supported: false,
        }
    }
}

/// Resolve a requested delivery mode against the current agent capabilities.
///
/// The default priority is steering, then queue. Stopping a running turn is
/// destructive (it cancels pending approvals/elicitations), so it is only used
/// when the caller explicitly asks for it.
#[must_use]
pub fn resolve_delivery(mode: DeliveryMode, ctx: DeliveryContext) -> SendAction {
    match mode {
        DeliveryMode::Queue => SendAction::Queue,
        DeliveryMode::Inject => {
            if ctx.turn_running && ctx.steering_supported {
                SendAction::Inject
            } else if ctx.turn_running {
                SendAction::Queue
            } else {
                SendAction::Prompt
            }
        }
        DeliveryMode::StopAndSend => {
            if ctx.turn_running {
                SendAction::StopAndSend
            } else {
                SendAction::Prompt
            }
        }
        DeliveryMode::Auto => {
            if !ctx.turn_running {
                SendAction::Prompt
            } else if ctx.steering_supported {
                SendAction::Inject
            } else {
                SendAction::Queue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_auto_starts_a_prompt() {
        assert_eq!(
            resolve_delivery(DeliveryMode::Auto, DeliveryContext::idle()),
            SendAction::Prompt
        );
    }

    #[test]
    fn running_auto_prefers_steering() {
        let ctx = DeliveryContext {
            turn_running: true,
            steering_supported: true,
        };
        assert_eq!(
            resolve_delivery(DeliveryMode::Auto, ctx),
            SendAction::Inject
        );
    }

    #[test]
    fn running_auto_without_steering_queues() {
        let ctx = DeliveryContext {
            turn_running: true,
            steering_supported: false,
        };
        assert_eq!(resolve_delivery(DeliveryMode::Auto, ctx), SendAction::Queue);
    }

    #[test]
    fn explicit_inject_without_steering_falls_back_to_queue() {
        let ctx = DeliveryContext {
            turn_running: true,
            steering_supported: false,
        };
        assert_eq!(
            resolve_delivery(DeliveryMode::Inject, ctx),
            SendAction::Queue
        );
    }

    #[test]
    fn explicit_queue_always_queues() {
        assert_eq!(
            resolve_delivery(DeliveryMode::Queue, DeliveryContext::idle()),
            SendAction::Queue
        );
    }
}
