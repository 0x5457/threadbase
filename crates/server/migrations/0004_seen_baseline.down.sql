ALTER TABLE seen_files RENAME TO seen_files_old;

CREATE TABLE seen_files (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, user_id, path)
);

INSERT INTO seen_files(thread_id, user_id, path, content_hash, seen_at)
    SELECT thread_id, user_id, path, content_hash, seen_at
    FROM seen_files_old
    WHERE baseline = '';

DROP TABLE seen_files_old;
