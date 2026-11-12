DROP INDEX IF EXISTS idx_users_access;

ALTER TABLE users DROP COLUMN access;
