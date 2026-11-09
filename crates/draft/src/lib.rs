//! Per-thread draft document: a Yjs `Y.Array` of per-author blocks with a
//! `Y.Text` body. The event log is deliberately not a CRDT.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Array, Doc, GetString, Map, MapPrelim, MapRef, ReadTxn, Text, TextPrelim, TextRef, Transact,
    Update,
};

pub const MAX_THREAD_DRAFT_BYTES: usize = 200 * 1024;
pub const MAX_BLOCK_BYTES: usize = 16 * 1024;

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
}

#[derive(Debug, Clone)]
pub struct DraftDoc {
    doc: Doc,
}

impl Default for DraftDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl DraftDoc {
    #[must_use]
    pub fn new() -> Self {
        Self { doc: Doc::new() }
    }

    /// Load a persisted document snapshot.
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, DraftError> {
        let doc = Doc::new();
        if !bytes.is_empty() {
            let update = Update::decode_v1(bytes)
                .map_err(|error| DraftError::InvalidUpdate(error.to_string()))?;
            let mut txn = doc.transact_mut();
            txn.apply_update(update);
        }
        Ok(Self { doc })
    }

    #[must_use]
    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    #[must_use]
    pub fn state_vector(&self) -> Vec<u8> {
        let _ = self.doc.get_or_insert_array("blocks");
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    #[must_use]
    pub fn encode_update(&self, state_vector: &[u8]) -> Vec<u8> {
        let _ = self.doc.get_or_insert_array("blocks");
        let txn = self.doc.transact();
        let sv = yrs::StateVector::decode_v1(state_vector).unwrap_or_default();
        txn.encode_state_as_update_v1(&sv)
    }

    #[must_use]
    pub fn encode_full(&self) -> Vec<u8> {
        let _ = self.doc.get_or_insert_array("blocks");
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&yrs::StateVector::default())
    }

    pub fn apply_update(&mut self, bytes: &[u8]) -> Result<(), DraftError> {
        let update = Update::decode_v1(bytes)
            .map_err(|error| DraftError::InvalidUpdate(error.to_string()))?;
        let _ = self.doc.get_or_insert_array("blocks");
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update);
        Ok(())
    }

    #[must_use]
    pub fn blocks(&self) -> Vec<DraftBlock> {
        self.snapshot()
            .into_iter()
            .map(|(block, _)| block)
            .collect()
    }

    fn snapshot(&self) -> Vec<(DraftBlock, u32)> {
        let blocks = self.doc.get_or_insert_array("blocks");
        let txn = self.doc.transact();
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
        let blocks = self.doc.get_or_insert_array("blocks");
        let mut txn = self.doc.transact_mut();
        let position = blocks.len(&txn);
        let map = blocks.insert(&mut txn, position, MapPrelim::<i32>::from(HashMap::new()));
        map.insert(&mut txn, "id", id.to_owned());
        map.insert(&mut txn, "author", author.to_owned());
        map.insert(&mut txn, "createdAt", created_at);
        let body: TextRef = map.insert(&mut txn, "text", TextPrelim::new(""));
        body.push(&mut txn, text);
        Ok(())
    }

    /// Replace a block body. Used by the server-side send path and tests; live
    /// typing normally arrives as client CRDT updates.
    pub fn set_block_text(&mut self, block_id: &str, text: &str) -> Result<(), DraftError> {
        if text.len() > MAX_BLOCK_BYTES {
            return Err(DraftError::BlockTooLarge);
        }
        let blocks = self.doc.get_or_insert_array("blocks");
        let mut txn = self.doc.transact_mut();
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

    /// Remove the trailing run of non-empty blocks and return it in display
    /// order. Trailing empty blocks are removed as well.
    pub fn take_trailing(&mut self) -> Vec<DraftBlock> {
        let blocks = self.doc.get_or_insert_array("blocks");
        let snapshot = self.snapshot();
        let mut end = snapshot.len();
        while end > 0 && snapshot[end - 1].0.text.trim().is_empty() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && !snapshot[start - 1].0.text.trim().is_empty() {
            start -= 1;
        }
        let taken: Vec<DraftBlock> = snapshot[start..end]
            .iter()
            .map(|(block, _)| block.clone())
            .collect();
        let mut txn = self.doc.transact_mut();
        let len = blocks.len(&txn);
        if (start as u32) < len {
            blocks.remove_range(&mut txn, start as u32, len - start as u32);
        }
        drop(txn);
        taken
    }

    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.blocks().iter().map(|block| block.text.len()).sum()
    }
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
}
