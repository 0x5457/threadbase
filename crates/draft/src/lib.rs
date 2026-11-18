//! Per-thread draft: one shared `Y.XmlFragment` for the composer and a `Y.Map`
//! of contributors. The event log is not a CRDT.
//!
//! Schema (shared with `web/src/lib/draft.ts`):
//!
//!   Y.Doc
//!   ├─ XmlFragment "content"  – the composer document (y-prosemirror shape)
//!   └─ Map "editors"          – user id → last local edit timestamp

use thiserror::Error;
use y_sync::awareness::Awareness;
use yrs::updates::decoder::Decode;
use yrs::{
    Doc, GetString, Map, ReadTxn, Transact, Update, XmlElementPrelim, XmlFragment, XmlNode,
    XmlTextPrelim,
};

/// Root type holding the shared composer document.
pub const DRAFT_FIELD: &str = "content";
/// Root type mapping every user id that edited the pending draft to a timestamp.
pub const EDITORS_MAP: &str = "editors";
pub const MAX_THREAD_DRAFT_BYTES: usize = 200 * 1024;
/// Maximum binary frame accepted on the draft WebSocket.
pub const MAX_WS_MESSAGE_BYTES: usize = 256 * 1024;
/// Maximum single CRDT update blob accepted from a client.
pub const MAX_UPDATE_BYTES: usize = 128 * 1024;

#[derive(Debug, Error)]
pub enum DraftError {
    #[error("invalid CRDT update: {0}")]
    InvalidUpdate(String),
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
        let txn = self.awareness.doc().transact();
        txn.encode_state_as_update_v1(&yrs::StateVector::default())
    }

    pub fn apply_update(&mut self, bytes: &[u8]) -> Result<(), DraftError> {
        let update = Update::decode_v1(bytes)
            .map_err(|error| DraftError::InvalidUpdate(error.to_string()))?;
        let mut txn = self.awareness.doc().transact_mut();
        txn.apply_update(update);
        Ok(())
    }

    /// Plain text of the shared document, one line per block node. Matches the
    /// editor's `getText()` closely enough for prompts and size limits.
    #[must_use]
    pub fn text(&self) -> String {
        let fragment = self.doc().get_or_insert_xml_fragment(DRAFT_FIELD);
        let txn = self.doc().transact();
        let mut lines = Vec::new();
        for index in 0..fragment.len(&txn) {
            if let Some(node) = fragment.get(&txn, index) {
                lines.push(node_text(&txn, &node));
            }
        }
        lines.join("\n")
    }

    /// Every user id that edited the pending draft, in stable order.
    #[must_use]
    pub fn authors(&self) -> Vec<String> {
        let editors = self.doc().get_or_insert_map(EDITORS_MAP);
        let txn = self.doc().transact();
        let mut ids: Vec<String> = editors.keys(&txn).map(str::to_owned).collect();
        ids.sort();
        ids
    }

    /// Check the thread-level size limit.
    #[must_use]
    pub fn limits_ok(&self) -> bool {
        self.text().len() <= MAX_THREAD_DRAFT_BYTES
    }

    /// Read the pending draft and clear it in one transaction: the send path
    /// consumes the shared document so every open composer empties at once.
    pub fn take(&mut self) -> (String, Vec<String>) {
        let text = self.text();
        let authors = self.authors();
        if text.is_empty() && authors.is_empty() {
            return (text, authors);
        }
        let fragment = self.doc().get_or_insert_xml_fragment(DRAFT_FIELD);
        let editors = self.doc().get_or_insert_map(EDITORS_MAP);
        let mut txn = self.doc().transact_mut();
        let len = fragment.len(&txn);
        if len > 0 {
            fragment.remove_range(&mut txn, 0, len);
        }
        let keys: Vec<String> = editors.keys(&txn).map(str::to_owned).collect();
        for key in keys {
            editors.remove(&mut txn, &key);
        }
        (text, authors)
    }
}

fn node_text<T: ReadTxn>(txn: &T, node: &XmlNode) -> String {
    match node {
        XmlNode::Text(text) => text.get_string(txn),
        XmlNode::Element(element) => {
            // Tiptap's `hardBreak` carries no text of its own.
            if element.tag().as_ref() == "hardBreak" {
                return "\n".to_owned();
            }
            let mut out = String::new();
            for index in 0..element.len(txn) {
                if let Some(child) = element.get(txn, index) {
                    out.push_str(&node_text(txn, &child));
                }
            }
            out
        }
        XmlNode::Fragment(fragment) => {
            let mut lines = Vec::new();
            for index in 0..fragment.len(txn) {
                if let Some(child) = fragment.get(txn, index) {
                    lines.push(node_text(txn, &child));
                }
            }
            lines.join("\n")
        }
    }
}

/// Append one paragraph per line and record the author as an editor. Test and
/// fixture helper: live edits arrive as client CRDT updates.
pub fn seed_text(doc: &mut DraftDoc, author: &str, lines: &[&str]) -> Result<(), DraftError> {
    let fragment = doc.doc().get_or_insert_xml_fragment(DRAFT_FIELD);
    let editors = doc.doc().get_or_insert_map(EDITORS_MAP);
    let mut txn = doc.doc().transact_mut();
    for line in lines {
        let paragraph = fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        paragraph.push_back(&mut txn, XmlTextPrelim::new(*line));
    }
    editors.insert(&mut txn, author.to_owned(), 1i64);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips_encoded_state() {
        let mut doc = DraftDoc::new();
        seed_text(&mut doc, "alice", &["hello"]).unwrap();
        seed_text(&mut doc, "bob", &["world"]).unwrap();
        let encoded = doc.encode_full();
        let restored = DraftDoc::from_snapshot(&encoded).unwrap();
        assert_eq!(restored.text(), "hello\nworld");
        assert_eq!(restored.authors(), doc.authors());
        assert_eq!(restored.encode_full(), encoded);
    }

    #[test]
    fn empty_snapshot_loads_an_empty_draft() {
        let doc = DraftDoc::from_snapshot(&[]).unwrap();
        assert!(doc.text().is_empty());
        assert!(doc.authors().is_empty());
        assert!(doc.limits_ok());
    }

    #[test]
    fn block_nodes_join_with_newlines() {
        let mut doc = DraftDoc::new();
        seed_text(&mut doc, "alice", &["first", "second"]).unwrap();
        assert_eq!(doc.text(), "first\nsecond");
    }

    #[test]
    fn take_reads_and_clears_text_and_editors() {
        let mut doc = DraftDoc::new();
        seed_text(&mut doc, "alice", &["one"]).unwrap();
        seed_text(&mut doc, "bob", &["two"]).unwrap();
        let (text, authors) = doc.take();
        assert_eq!(text, "one\ntwo");
        assert_eq!(authors, vec!["alice".to_owned(), "bob".to_owned()]);
        assert!(doc.text().is_empty());
        assert!(doc.authors().is_empty());
        let (again, again_authors) = doc.take();
        assert!(again.is_empty() && again_authors.is_empty());
    }

    #[test]
    fn thread_size_limit_is_enforced() {
        let mut doc = DraftDoc::new();
        let line = "x".repeat(MAX_THREAD_DRAFT_BYTES);
        seed_text(&mut doc, "alice", &[&line]).unwrap();
        assert!(doc.limits_ok());
        seed_text(&mut doc, "bob", &["more"]).unwrap();
        assert!(!doc.limits_ok());
    }

    #[test]
    fn edits_converge_across_replicas() {
        let mut a = DraftDoc::new();
        seed_text(&mut a, "alice", &["hello"]).unwrap();
        let mut b = DraftDoc::new();
        b.apply_update(&a.encode_full()).unwrap();

        let fragment = b.doc().get_or_insert_xml_fragment(DRAFT_FIELD);
        let paragraph = fragment.get(&b.doc().transact(), 0).unwrap();
        let XmlNode::Element(element) = paragraph else {
            panic!("paragraph expected");
        };
        let mut txn = b.doc().transact_mut();
        element.push_back(&mut txn, XmlTextPrelim::new(" from b"));
        drop(txn);

        let a_update = a.encode_full();
        let b_update = b.encode_full();
        a.apply_update(&b_update).unwrap();
        b.apply_update(&a_update).unwrap();
        assert_eq!(a.text(), b.text());
        assert!(a.text().contains("hello"));
        assert!(a.text().contains("from b"));
    }

    #[test]
    fn concurrent_take_reports_exactly_once() {
        let mut doc = DraftDoc::new();
        seed_text(&mut doc, "alice", &["hello"]).unwrap();
        let first = doc.take();
        let second = doc.take();
        assert_eq!(first.0, "hello");
        assert!(second.0.is_empty());
    }

    #[test]
    fn hard_breaks_become_newlines() {
        let doc = DraftDoc::new();
        let fragment = doc.doc().get_or_insert_xml_fragment(DRAFT_FIELD);
        let mut txn = doc.doc().transact_mut();
        let paragraph = fragment.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        paragraph.push_back(&mut txn, XmlTextPrelim::new("a"));
        paragraph.push_back(&mut txn, XmlElementPrelim::empty("hardBreak"));
        paragraph.push_back(&mut txn, XmlTextPrelim::new("b"));
        drop(txn);
        assert_eq!(doc.text(), "a\nb");
    }
}
