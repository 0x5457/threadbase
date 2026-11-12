-- Images attached to a prompt live on disk; the row keeps the lookup, mime and
-- owning thread so a URL can serve them back after a reload.
CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    path TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_attachments_thread ON attachments(thread_id);
