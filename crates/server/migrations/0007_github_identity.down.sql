DROP INDEX IF EXISTS idx_auth_sessions_expires;
ALTER TABLE auth_sessions DROP COLUMN method;
DROP INDEX IF EXISTS idx_users_identity_key;
ALTER TABLE users DROP COLUMN identity_key;
ALTER TABLE users DROP COLUMN identity_source;
ALTER TABLE users ADD COLUMN password_hash TEXT;
