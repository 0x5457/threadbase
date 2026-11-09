-- Threadbase initial schema.
-- Timestamps are Unix milliseconds. JSON columns are TEXT.

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name TEXT NOT NULL,
    avatar_url TEXT,
    password_hash TEXT,
    is_admin INTEGER NOT NULL DEFAULT 0,
    tailscale_login TEXT UNIQUE,
    hidden_presence INTEGER NOT NULL DEFAULT 0,
    settings_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL
);

CREATE TABLE auth_sessions (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER,
    last_seen_at INTEGER,
    user_agent TEXT,
    ip TEXT
);

CREATE INDEX idx_auth_sessions_user ON auth_sessions(user_id);

CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    remote_url TEXT,
    default_branch TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    parent_thread_id TEXT REFERENCES threads(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    agent TEXT NOT NULL,
    model TEXT,
    effort TEXT,
    branch TEXT NOT NULL,
    base_branch TEXT NOT NULL,
    base_sha TEXT,
    last_turn_sha TEXT,
    worktree_path TEXT,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER,
    pinned INTEGER NOT NULL DEFAULT 0,
    share_level TEXT NOT NULL DEFAULT 'invite_only',
    last_activity_at INTEGER
);

CREATE INDEX idx_threads_project ON threads(project_id, archived_at);
CREATE INDEX idx_threads_updated ON threads(updated_at);

CREATE TABLE thread_members (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, user_id)
);

CREATE INDEX idx_thread_members_user ON thread_members(user_id);

CREATE TABLE agent_bindings (
    thread_id TEXT PRIMARY KEY REFERENCES threads(id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    adapter_version TEXT,
    protocol_version TEXT,
    session_id TEXT,
    cwd TEXT NOT NULL,
    config_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'idle',
    last_error TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE thread_counters (
    thread_id TEXT PRIMARY KEY REFERENCES threads(id) ON DELETE CASCADE,
    last_seq INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE events (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    ts INTEGER NOT NULL,
    actor_kind TEXT NOT NULL,
    actor_id TEXT,
    actor_name TEXT,
    origin TEXT NOT NULL,
    subagent_id TEXT,
    parent_subagent_id TEXT,
    tool_call_id TEXT,
    delivery TEXT,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (thread_id, seq)
);

CREATE INDEX idx_events_ts ON events(thread_id, ts);

CREATE TABLE snapshots (
    thread_id TEXT PRIMARY KEY REFERENCES threads(id) ON DELETE CASCADE,
    upto_seq INTEGER NOT NULL,
    state_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE subagents (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    parent_id TEXT,
    title TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL,
    cwd TEXT,
    can_cancel INTEGER NOT NULL DEFAULT 0,
    usage_json TEXT,
    last_seq INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_subagents_thread ON subagents(thread_id, parent_id);

CREATE TABLE queued_inputs (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES users(id),
    text TEXT NOT NULL,
    status TEXT NOT NULL,
    position INTEGER NOT NULL,
    superseded INTEGER NOT NULL DEFAULT 0,
    turn_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_queued_inputs_thread ON queued_inputs(thread_id, status, position);

CREATE TABLE approvals (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    subagent_id TEXT,
    turn_id TEXT,
    tool_call_id TEXT,
    agent TEXT NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL,
    command TEXT,
    cwd TEXT,
    paths_json TEXT NOT NULL DEFAULT '[]',
    options_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL,
    decision TEXT,
    scope TEXT,
    decided_by TEXT REFERENCES users(id),
    decided_at INTEGER,
    note TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_approvals_thread ON approvals(thread_id, status);
CREATE INDEX idx_approvals_pending ON approvals(status, created_at);

CREATE TABLE approval_rules (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    thread_id TEXT REFERENCES threads(id) ON DELETE CASCADE,
    agent TEXT NOT NULL,
    tool TEXT NOT NULL,
    command_prefix TEXT,
    scope TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_approval_rules_project ON approval_rules(project_id, agent, tool);

CREATE TABLE elicitations (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    subagent_id TEXT,
    tool_call_id TEXT,
    agent TEXT NOT NULL,
    message TEXT NOT NULL,
    mode TEXT NOT NULL,
    schema_json TEXT NOT NULL,
    blocking INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL,
    response_json TEXT,
    responded_by TEXT REFERENCES users(id),
    responded_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_elicitations_thread ON elicitations(thread_id, status);

CREATE TABLE comments (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES comments(id) ON DELETE CASCADE,
    author_id TEXT NOT NULL REFERENCES users(id),
    body TEXT NOT NULL,
    anchor_kind TEXT NOT NULL,
    anchor_json TEXT NOT NULL,
    anchor_changed INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    delivered_seq INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_comments_thread ON comments(thread_id, status, created_at);

CREATE TABLE draft_updates (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    update_blob BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, seq)
);

CREATE TABLE draft_snapshots (
    thread_id TEXT PRIMARY KEY REFERENCES threads(id) ON DELETE CASCADE,
    state_blob BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL,
    path TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);

CREATE TABLE invites (
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

CREATE INDEX idx_invites_thread ON invites(thread_id);

CREATE TABLE seen_files (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, user_id, path)
);

CREATE TABLE thread_reads (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    last_read_seq INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, user_id)
);

CREATE TABLE inbox (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    approval_id TEXT,
    elicitation_id TEXT,
    comment_id TEXT,
    actor_id TEXT REFERENCES users(id),
    summary TEXT NOT NULL,
    read_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_inbox_user ON inbox(user_id, read_at, created_at);

CREATE TABLE audit_log (
    id TEXT PRIMARY KEY,
    actor_id TEXT REFERENCES users(id),
    action TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    detail_json TEXT,
    ip TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_audit_created ON audit_log(created_at);

CREATE TABLE user_settings (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    PRIMARY KEY (user_id, key)
);

CREATE TABLE webhooks (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    events_json TEXT NOT NULL DEFAULT '[]',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);
