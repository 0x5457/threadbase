-- A tmux session can be the origin of one thread. The session itself lives in
-- tmux, so the link stores a fingerprint (created_at) and is ignored once that
-- socket/id pair is reused by a different session.
CREATE TABLE tmux_links (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
    socket TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_name TEXT NOT NULL,
    session_created_at INTEGER NOT NULL,
    cwd TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX idx_tmux_links_session
    ON tmux_links(socket, session_id, session_created_at);
CREATE INDEX idx_tmux_links_thread ON tmux_links(thread_id);
