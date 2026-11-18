-- Guest links: one thread, one person, no account.
--
-- A link is a capability, not a user: the token is hashed at rest, belongs to a
-- single thread, expires, and can be revoked. The cookie a guest receives is a
-- separate session row so revoking the link ends every browser that used it.
CREATE TABLE guest_links (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER
);

CREATE INDEX idx_guest_links_thread ON guest_links(thread_id);

CREATE TABLE guest_sessions (
    token_hash TEXT PRIMARY KEY,
    link_id TEXT NOT NULL REFERENCES guest_links(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    last_seen_at INTEGER,
    user_agent TEXT
);

CREATE INDEX idx_guest_sessions_link ON guest_sessions(link_id);

-- A guest does not start a turn: the message waits until someone with an account
-- sends or dismisses it. `text` is kept after sending for the audit trail.
CREATE TABLE guest_messages (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    link_id TEXT NOT NULL REFERENCES guest_links(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    decided_at INTEGER,
    decided_by TEXT REFERENCES users(id)
);

CREATE INDEX idx_guest_messages_pending ON guest_messages(thread_id, status);
