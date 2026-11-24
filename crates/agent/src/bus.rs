//! The event bus shared by every transport. Broadcast, never persisted here:
//! the server pump owns the translation into thread events.

use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::RawAgentEvent;

const EVENT_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
pub(crate) struct EventBus {
    sender: Arc<Mutex<Option<broadcast::Sender<RawAgentEvent>>>>,
}

impl EventBus {
    pub(crate) fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            sender: Arc::new(Mutex::new(Some(sender))),
        }
    }

    pub(crate) fn send(&self, event: RawAgentEvent) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = sender.as_ref() {
            let _ = sender.send(event);
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<RawAgentEvent> {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match sender.as_ref() {
            Some(sender) => sender.subscribe(),
            None => {
                let (sender, receiver) = broadcast::channel(1);
                drop(sender);
                receiver
            }
        }
    }

    /// The adapter is gone; subscribers see the channel close.
    pub(crate) fn close(&self) {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}
