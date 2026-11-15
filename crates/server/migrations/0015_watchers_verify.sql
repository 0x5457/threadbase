-- Thread watchers drive notifications: every thread starts with its creator
-- watching, and anyone who speaks in the thread is added automatically. Mute
-- is per watcher, so a large team can share one thread list without sharing
-- the same inbox noise.
CREATE TABLE thread_watchers (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    muted INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, user_id)
);

CREATE INDEX idx_thread_watchers_user ON thread_watchers(user_id);

INSERT INTO thread_watchers(thread_id, user_id, muted, created_at)
SELECT id, created_by, 0, created_at FROM threads;

-- Optional command run in the thread worktree before its changes are accepted
-- by merge-back. NULL means "no verification configured".
ALTER TABLE projects ADD COLUMN verify_command TEXT;
