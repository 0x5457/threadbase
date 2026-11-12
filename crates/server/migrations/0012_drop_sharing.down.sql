ALTER TABLE threads ADD COLUMN share_level TEXT NOT NULL DEFAULT 'invite_only';

CREATE TABLE IF NOT EXISTS thread_members (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_members_user ON thread_members(user_id);

CREATE TABLE IF NOT EXISTS invites (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    email TEXT,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    accepted_by TEXT REFERENCES users(id),
    accepted_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_invites_thread ON invites(thread_id);
