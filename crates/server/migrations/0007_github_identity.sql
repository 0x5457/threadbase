-- Replace forwarded Tailscale identity headers with GitHub OAuth sessions.

ALTER TABLE users ADD COLUMN identity_source TEXT;
ALTER TABLE users ADD COLUMN identity_key TEXT;

-- Legacy logins are `login@provider`. A GitHub-backed tailnet maps cleanly onto
-- GitHub OAuth (`login@github` -> `github:login`), so existing users keep their
-- threads, memberships and admin bit across the switch.
UPDATE users
SET identity_source = 'github',
    identity_key = 'github:' || substr(tailscale_login, 1, length(tailscale_login) - 7)
WHERE tailscale_login LIKE '%@github';

UPDATE users
SET identity_source = 'legacy',
    identity_key = 'legacy:' || tailscale_login
WHERE identity_key IS NULL AND tailscale_login IS NOT NULL;

CREATE UNIQUE INDEX idx_users_identity_key ON users(identity_key)
WHERE identity_key IS NOT NULL;

-- Password login was removed before it shipped, so the column is dead weight.
ALTER TABLE users DROP COLUMN password_hash;

-- `tailscale_login` cannot be dropped: it is declared UNIQUE, and SQLite
-- refuses DROP COLUMN on indexed columns. Nothing reads it any more.
ALTER TABLE auth_sessions ADD COLUMN method TEXT NOT NULL DEFAULT 'github';

CREATE INDEX idx_auth_sessions_expires ON auth_sessions(expires_at);
