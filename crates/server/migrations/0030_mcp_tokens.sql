-- Tokens for outside agents driving Threadbase over MCP.
--
-- A token is a member's delegated authority with explicit scopes, stored only as
-- a hash. It is minted from the settings page by a signed-in person, so no local
-- process can grant itself one, and it can be revoked at any time.
CREATE TABLE mcp_tokens (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    scopes TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER
);

CREATE INDEX idx_mcp_tokens_user ON mcp_tokens(user_id);
