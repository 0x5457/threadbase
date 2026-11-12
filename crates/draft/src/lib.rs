//! Per-thread draft document: a Yjs `Y.Array` of per-author blocks with a
//! `Y.Text` body. The event log is deliberately not a CRDT.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use y_sync::awareness::Awareness;
use yrs::updates::decoder::Decode;
use yrs::{
    Array, ArrayRef, Doc, GetString, Map, MapPrelim, MapRef, ReadTxn, Text, TextPrelim, TextRef,
    Transact, Update,
};

pub const MAX_THREAD_DRAFT_BYTES: usize = 200 * 1024;
pub const MAX_BLOCK_BYTES: usize = 16 * 1024;
/// Maximum binary frame accepted on the draft WebSocket.
pub const MAX_WS_MESSAGE_BYTES: usize = 256 * 1024;
/// Maximum single CRDT update blob accepted from a client.
pub const MAX_UPDATE_BYTES: usize = 128 * 1024;

#[derive(Debug, Error)]
pub enum DraftError {
    #[error("draft block exceeds {MAX_BLOCK_BYTES} bytes")]
    BlockTooLarge,
    #[error("thread draft exceeds {MAX_THREAD_DRAFT_BYTES} bytes")]
    ThreadTooLarge,
    #[error("invalid CRDT update: {0}")]
    InvalidUpdate(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftBlock {
    pub id: String,
    pub author: String,
    pub text: String,
    pub created_at: i64,
    /// Sent blocks stay in the CRDT as tombstones; deleting array parents while
    /// a client is mid-edit can create dangling updates in Yjs.
    #[serde(skip)]
    pub sent: bool,
}

#[derive(Debug)]
pub struct DraftDoc {
    awareness: Awareness,
}

impl Default for DraftDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl DraftDoc {
    #[must_use]
    pub fn new() -> Self {
        Self {
            awareness: Awareness::new(Doc::new()),
        }
    }

    /// Load a persisted document snapshot.
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, DraftError> {
        let mut draft = Self::new();
        if !bytes.is_empty() {
            draft.apply_update(bytes)?;
        }
        Ok(draft)
    }

    #[must_use]
    pub fn doc(&self) -> &Doc {
        self.awareness.doc()
    }

    #[must_use]
    pub fn awareness(&self) -> &Awareness {
        &self.awareness
    }

    pub fn awareness_mut(&mut self) -> &mut Awareness {
        &mut self.awareness
    }

    /// Encode the complete document as a single v1 update.
    #[must_use]
    pub fn encode_full(&self) -> Vec<u8> {
        self.ensure_blocks();
        let txn = self.awareness.doc().transact();
        txn.encode_state_as_update_v1(&yrs::StateVector::default())
    }

    pub fn apply_update(&mut self, bytes: &[u8]) -> Result<(), DraftError> {
        let update = Update::decode_v1(bytes)
            .map_err(|error| DraftError::InvalidUpdate(error.to_string()))?;
        self.ensure_blocks();
        let mut txn = self.awareness.doc().transact_mut();
        txn.apply_update(update);
        Ok(())
    }

    #[must_use]
    pub fn blocks(&self) -> Vec<DraftBlock> {
        self.pending_blocks(false)
    }

    /// Non-destructive view of the pending (non-empty, unsent) blocks.
    /// Used by the send path to read the text before committing to send.
    #[must_use]
    pub fn peek_blocks(&self) -> Vec<DraftBlock> {
        self.pending_blocks(true)
    }

    /// Pending blocks in display order; `require_text` drops empty blocks.
    fn pending_blocks(&self, require_text: bool) -> Vec<DraftBlock> {
        let mut blocks: Vec<DraftBlock> = self
            .snapshot()
            .into_iter()
            .map(|(block, _)| block)
            .filter(|block| !block.sent && (!require_text || !block.text.trim().is_empty()))
            .collect();
        sort_blocks(&mut blocks);
        blocks
    }

    /// Check the thread-level and per-block size limits.
    #[must_use]
    pub fn limits_ok(&self) -> bool {
        let snapshot = self.snapshot();
        let total: usize = snapshot.iter().map(|(block, _)| block.text.len()).sum();
        total <= MAX_THREAD_DRAFT_BYTES
            && snapshot
                .iter()
                .all(|(block, _)| block.text.len() <= MAX_BLOCK_BYTES)
    }

    fn ensure_blocks(&self) -> ArrayRef {
        self.awareness.doc().get_or_insert_array("blocks")
    }

    fn snapshot(&self) -> Vec<(DraftBlock, u32)> {
        let blocks = self.ensure_blocks();
        let txn = self.awareness.doc().transact();
        let mut out = Vec::new();
        for index in 0..blocks.len(&txn) {
            let Some(value) = blocks.get(&txn, index) else {
                continue;
            };
            let Ok(map) = value.cast::<MapRef>() else {
                continue;
            };
            out.push((
                DraftBlock {
                    id: read_string(&txn, &map, "id"),
                    author: read_string(&txn, &map, "author"),
                    text: read_text(&txn, &map),
                    created_at: read_i64(&txn, &map, "createdAt"),
                    sent: map.get(&txn, "sentAt").is_some(),
                },
                index,
            ));
        }
        out
    }

    pub fn append_block(
        &mut self,
        id: &str,
        author: &str,
        text: &str,
        created_at: i64,
    ) -> Result<(), DraftError> {
        if text.len() > MAX_BLOCK_BYTES {
            return Err(DraftError::BlockTooLarge);
        }
        if self.total_bytes() + text.len() > MAX_THREAD_DRAFT_BYTES {
            return Err(DraftError::ThreadTooLarge);
        }
        let blocks = self.ensure_blocks();
        let mut txn = self.awareness.doc().transact_mut();
        let position = blocks.len(&txn);
        let map = blocks.insert(&mut txn, position, MapPrelim::<i32>::from(HashMap::new()));
        map.insert(&mut txn, "id", id.to_owned());
        map.insert(&mut txn, "author", author.to_owned());
        map.insert(&mut txn, "createdAt", created_at);
        let body: TextRef = map.insert(&mut txn, "text", TextPrelim::new(""));
        body.push(&mut txn, text);
        Ok(())
    }

    /// Replace a block body. Test-only helper that simulates a remote text
    /// edit; live typing normally arrives as client CRDT updates.
    #[cfg(test)]
    pub fn set_block_text(&mut self, block_id: &str, text: &str) -> Result<(), DraftError> {
        if text.len() > MAX_BLOCK_BYTES {
            return Err(DraftError::BlockTooLarge);
        }
        let blocks = self.ensure_blocks();
        let mut txn = self.awareness.doc().transact_mut();
        for index in 0..blocks.len(&txn) {
            let Some(value) = blocks.get(&txn, index) else {
                continue;
            };
            let Ok(map) = value.cast::<MapRef>() else {
                continue;
            };
            if read_string(&txn, &map, "id") != block_id {
                continue;
            }
            if let Some(body) = map
                .get(&txn, "text")
                .and_then(|value| value.cast::<TextRef>().ok())
            {
                let current = body.get_string(&txn);
                if !current.is_empty() {
                    body.remove_range(&mut txn, 0, current.len() as u32);
                }
                body.push(&mut txn, text);
            }
            return Ok(());
        }
        Ok(())
    }

    /// Remove all non-empty blocks and return them in deterministic display
    /// order. Empty blocks are dropped. The CRDT array itself may converge in a
    /// different order, so display order comes from `createdAt`, then `id`.
    pub fn take_trailing(&mut self) -> Vec<DraftBlock> {
        let taken = self.pending_blocks(true);
        let blocks = self.ensure_blocks();
        let ids: HashSet<String> = taken.iter().map(|block| block.id.clone()).collect();
        let mut txn = self.awareness.doc().transact_mut();
        for index in 0..blocks.len(&txn) {
            let Some(value) = blocks.get(&txn, index) else {
                continue;
            };
            let Ok(map) = value.cast::<MapRef>() else {
                continue;
            };
            let id = read_string(&txn, &map, "id");
            if !ids.contains(&id) {
                continue;
            }
            if let Some(body) = map
                .get(&txn, "text")
                .and_then(|value| value.cast::<TextRef>().ok())
            {
                let len = body.len(&txn);
                if len > 0 {
                    body.remove_range(&mut txn, 0, len);
                }
            }
            map.insert(&mut txn, "sentAt", true);
        }
        drop(txn);
        taken
    }

    fn total_bytes(&self) -> usize {
        self.blocks().iter().map(|block| block.text.len()).sum()
    }

    /// Remove every block, used after a send transaction has consumed them.
    pub fn reset(&mut self) {
        let blocks = self.ensure_blocks();
        let mut txn = self.awareness.doc().transact_mut();
        let len = blocks.len(&txn);
        if len > 0 {
            blocks.remove_range(&mut txn, 0, len);
        }
    }
}

fn sort_blocks(blocks: &mut [DraftBlock]) {
    blocks.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn read_string<T: ReadTxn>(txn: &T, map: &MapRef, key: &str) -> String {
    map.get(txn, key)
        .and_then(|value| value.cast::<String>().ok())
        .unwrap_or_default()
}

fn read_i64<T: ReadTxn>(txn: &T, map: &MapRef, key: &str) -> i64 {
    map.get(txn, key)
        .and_then(|value| value.cast::<i64>().ok())
        .unwrap_or_default()
}

fn read_text<T: ReadTxn>(txn: &T, map: &MapRef) -> String {
    map.get(txn, "text")
        .and_then(|value| value.cast::<TextRef>().ok())
        .map(|text| text.get_string(txn))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips_encoded_state() {
        let mut doc = DraftDoc::new();
        doc.append_block("b1", "alice", "hello", 1).unwrap();
        doc.append_block("b2", "bob", "world", 2).unwrap();
        let encoded = doc.encode_full();
        let restored = DraftDoc::from_snapshot(&encoded).unwrap();
        assert_eq!(restored.blocks(), doc.blocks());
        assert_eq!(
            restored.encode_full(),
            encoded,
            "snapshot must re-encode byte-identically"
        );
    }

    #[test]
    fn empty_snapshot_loads_an_empty_draft() {
        let doc = DraftDoc::from_snapshot(&[]).unwrap();
        assert!(doc.blocks().is_empty());
        assert!(doc.limits_ok());
    }

    #[test]
    fn thread_size_limit_is_enforced() {
        let mut doc = DraftDoc::new();
        let block = "x".repeat(MAX_BLOCK_BYTES);
        let full_blocks = MAX_THREAD_DRAFT_BYTES / MAX_BLOCK_BYTES;
        for index in 0..full_blocks {
            doc.append_block(&format!("b{index}"), "alice", &block, index as i64)
                .unwrap();
        }
        assert!(doc.limits_ok());
        let result = doc.append_block("overflow", "alice", &block, 99);
        assert!(matches!(result, Err(DraftError::ThreadTooLarge)));
    }

    #[test]
    fn blocks_round_trip_and_converge() {
        let mut a = DraftDoc::new();
        a.append_block("b1", "alice", "hello", 1).unwrap();
        a.append_block("b2", "bob", "world", 2).unwrap();
        let update = a.encode_full();
        let mut b = DraftDoc::new();
        b.apply_update(&update).unwrap();
        assert_eq!(a.blocks(), b.blocks());
    }

    #[test]
    fn trailing_blocks_are_taken_in_order() {
        let mut doc = DraftDoc::new();
        doc.append_block("b1", "alice", "one", 1).unwrap();
        doc.append_block("b2", "bob", "two", 2).unwrap();
        let taken = doc.take_trailing();
        assert_eq!(taken.len(), 2);
        assert_eq!(taken[0].text, "one");
        assert_eq!(taken[1].text, "two");
        assert!(doc.blocks().is_empty());
    }

    #[test]
    fn empty_tail_blocks_are_dropped() {
        let mut doc = DraftDoc::new();
        doc.append_block("b1", "alice", "one", 1).unwrap();
        doc.append_block("b2", "bob", "", 2).unwrap();
        let taken = doc.take_trailing();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].text, "one");
    }

    #[test]
    fn block_size_limit_is_enforced() {
        let mut doc = DraftDoc::new();
        let result = doc.append_block("b1", "alice", &"x".repeat(MAX_BLOCK_BYTES + 1), 1);
        assert!(matches!(result, Err(DraftError::BlockTooLarge)));
    }

    #[test]
    fn set_block_text_replaces_content() {
        let mut doc = DraftDoc::new();
        doc.append_block("b1", "alice", "hello", 1).unwrap();
        doc.set_block_text("b1", "goodbye").unwrap();
        assert_eq!(doc.blocks()[0].text, "goodbye");
    }

    #[test]
    fn peek_blocks_is_non_destructive() {
        let mut doc = DraftDoc::new();
        doc.append_block("b1", "alice", "one", 1).unwrap();
        doc.append_block("b2", "bob", "", 2).unwrap();
        let peeked = doc.peek_blocks();
        assert_eq!(peeked.len(), 1);
        assert_eq!(peeked[0].text, "one");
        assert_eq!(doc.blocks().len(), 2, "peek must not consume blocks");
        assert_eq!(
            doc.peek_blocks().len(),
            1,
            "peek must remain non-destructive"
        );
    }
}

#[cfg(test)]
mod concurrent_tests {
    use super::*;

    #[tokio::test]
    async fn two_senders_take_the_same_blocks_exactly_once() {
        let mut doc = DraftDoc::new();
        doc.append_block("b1", "alice", "hello", 1).unwrap();
        let shared = std::sync::Arc::new(tokio::sync::Mutex::new(doc));
        let left = shared.clone();
        let right = shared.clone();
        let a = tokio::spawn(async move { left.lock().await.take_trailing() });
        let b = tokio::spawn(async move { right.lock().await.take_trailing() });
        let a = a.await.unwrap();
        let b = b.await.unwrap();
        assert_eq!(
            a.len() + b.len(),
            1,
            "only one sender may consume the drafts"
        );
    }

    #[test]
    fn text_edits_converge_after_both_updates() {
        let mut a = DraftDoc::new();
        a.append_block("b1", "alice", "hello", 1).unwrap();
        let update = a.encode_full();
        let mut b = DraftDoc::new();
        b.apply_update(&update).unwrap();
        // Both replicas edit the same Y.Text; applying each other's updates must converge.
        a.set_block_text("b1", "hello from a").unwrap();
        b.set_block_text("b1", "hello from b").unwrap();
        let a_update = a.encode_full();
        let b_update = b.encode_full();
        a.apply_update(&b_update).unwrap();
        b.apply_update(&a_update).unwrap();
        assert_eq!(a.blocks(), b.blocks());
    }
}
