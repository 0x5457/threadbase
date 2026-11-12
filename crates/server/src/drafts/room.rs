//! Live draft room: CRDT state, y-sync protocol handling and broadcast fan-out.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use threadbase_draft::DraftDoc;
use tokio::sync::broadcast;
use y_sync::sync::{DefaultProtocol, Message as SyncMessageEnvelope, Protocol, SyncMessage};
use yrs::encoding::read::Cursor;
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::Update;

/// One decoded inbound frame.
pub(crate) struct AppliedFrame {
    /// Replies addressed to the sending socket only.
    pub(crate) direct: Vec<Vec<u8>>,
    /// Validated document updates to persist and fan out to every peer.
    pub(crate) persisted: Vec<Vec<u8>>,
    /// False when the frame pushed the document past its size limits.
    pub(crate) limits_ok: bool,
}

pub(crate) struct DraftRoom {
    draft: DraftDoc,
    tx: broadcast::Sender<Vec<u8>>,
    /// Pause doc-update broadcasting while an inbound update is validated.
    broadcast_paused: Arc<AtomicBool>,
    _doc_sub: yrs::UpdateSubscription,
    _awareness_sub: y_sync::awareness::UpdateSubscription,
}

impl DraftRoom {
    pub(crate) fn new(mut draft: DraftDoc) -> Self {
        let (tx, _) = broadcast::channel(512);
        let broadcast_paused = Arc::new(AtomicBool::new(false));
        let doc_sub = forward_document_updates(&draft, tx.clone(), broadcast_paused.clone());
        let awareness_sub = forward_awareness(&mut draft, tx.clone());
        Self {
            draft,
            tx,
            broadcast_paused,
            _doc_sub: doc_sub,
            _awareness_sub: awareness_sub,
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }

    pub(crate) fn initial(&self) -> Vec<u8> {
        let mut encoder = EncoderV1::new();
        let protocol = DefaultProtocol;
        protocol
            .start(self.draft.awareness(), &mut encoder)
            .expect("encode initial sync");
        encoder.to_vec()
    }

    pub(crate) fn encode_full(&self) -> Vec<u8> {
        self.draft.encode_full()
    }

    pub(crate) fn peek_draft(&self) -> (String, Vec<String>) {
        (self.draft.text(), self.draft.authors())
    }

    pub(crate) fn take_draft(&mut self) -> (String, Vec<String>) {
        self.draft.take()
    }

    /// Apply an inbound sync frame. Doc updates are validated before they are
    /// broadcast to peers; on limit violation nothing is broadcast or returned
    /// for persistence, and `limits_ok` is false so the caller can drop the
    /// room.
    pub(crate) fn apply(&mut self, bytes: &[u8]) -> AppliedFrame {
        self.broadcast_paused.store(true, Ordering::SeqCst);
        let (direct, persisted) = self.apply_inner(bytes);
        let limits_ok = self.draft.limits_ok();
        if limits_ok {
            for update in &persisted {
                broadcast_update(&self.tx, update);
            }
        }
        self.broadcast_paused.store(false, Ordering::SeqCst);
        if limits_ok {
            AppliedFrame {
                direct,
                persisted,
                limits_ok,
            }
        } else {
            AppliedFrame {
                direct: Vec::new(),
                persisted: Vec::new(),
                limits_ok,
            }
        }
    }

    fn apply_inner(&mut self, bytes: &[u8]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let mut direct = Vec::new();
        let mut persisted = Vec::new();
        let mut decoder = DecoderV1::new(Cursor::new(bytes));
        let protocol = DefaultProtocol;
        while let Ok(message) = SyncMessageEnvelope::decode(&mut decoder) {
            match message {
                SyncMessageEnvelope::Sync(SyncMessage::SyncStep1(state_vector)) => {
                    if let Ok(Some(reply)) =
                        protocol.handle_sync_step1(self.draft.awareness(), state_vector)
                    {
                        direct.push(reply.encode_v1());
                    }
                }
                SyncMessageEnvelope::Sync(SyncMessage::SyncStep2(update))
                | SyncMessageEnvelope::Sync(SyncMessage::Update(update)) => {
                    let Ok(decoded) = Update::decode_v1(&update) else {
                        continue;
                    };
                    if protocol
                        .handle_update(self.draft.awareness_mut(), decoded)
                        .is_ok()
                    {
                        persisted.push(update);
                    }
                }
                SyncMessageEnvelope::Awareness(update) => {
                    let _ = protocol.handle_awareness_update(self.draft.awareness_mut(), update);
                }
                SyncMessageEnvelope::AwarenessQuery => {
                    if let Ok(Some(reply)) = protocol.handle_awareness_query(self.draft.awareness())
                    {
                        direct.push(reply.encode_v1());
                    }
                }
                SyncMessageEnvelope::Auth(_) | SyncMessageEnvelope::Custom(_, _) => {}
            }
        }
        (direct, persisted)
    }

    #[cfg(test)]
    pub(crate) fn draft_mut(&mut self) -> &mut DraftDoc {
        &mut self.draft
    }
}

fn forward_document_updates(
    draft: &DraftDoc,
    sink: broadcast::Sender<Vec<u8>>,
    paused: Arc<AtomicBool>,
) -> yrs::UpdateSubscription {
    draft
        .doc()
        .observe_update_v1(move |_, event| {
            if paused.load(Ordering::SeqCst) {
                return;
            }
            broadcast_update(&sink, &event.update);
        })
        .expect("document observer")
}

fn forward_awareness(
    draft: &mut DraftDoc,
    sink: broadcast::Sender<Vec<u8>>,
) -> y_sync::awareness::UpdateSubscription {
    draft.awareness_mut().on_update(move |awareness, event| {
        let mut changed = Vec::new();
        changed.extend_from_slice(event.added());
        changed.extend_from_slice(event.updated());
        changed.extend_from_slice(event.removed());
        if let Ok(update) = awareness.update_with_clients(changed) {
            let _ = sink.send(SyncMessageEnvelope::Awareness(update).encode_v1());
        }
    })
}

fn broadcast_update(sink: &broadcast::Sender<Vec<u8>>, update: &[u8]) {
    let message = SyncMessageEnvelope::Sync(SyncMessage::Update(update.to_vec()));
    let _ = sink.send(message.encode_v1());
}
