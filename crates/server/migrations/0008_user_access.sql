-- Per-user access level.
--
--   pending   – signed in, waiting for an administrator to approve them
--   read_only – approved to read, cannot change anything
--   member    – normal use
--   disabled  – access revoked; sessions are revoked on the transition
--
-- Existing accounts keep their previous (full) access.
ALTER TABLE users ADD COLUMN access TEXT NOT NULL DEFAULT 'member';

CREATE INDEX idx_users_access ON users(access);
