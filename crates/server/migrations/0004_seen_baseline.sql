-- Scope Seen state to a comparison baseline so switching baselines does not
-- inherit or lose review progress.
ALTER TABLE seen_files RENAME TO seen_files_old;

CREATE TABLE seen_files (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    baseline TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, user_id, path, baseline)
);

INSERT INTO seen_files(thread_id, user_id, path, content_hash, baseline, seen_at)
    SELECT thread_id, user_id, path, content_hash, '', seen_at FROM seen_files_old;

DROP TABLE seen_files_old;
