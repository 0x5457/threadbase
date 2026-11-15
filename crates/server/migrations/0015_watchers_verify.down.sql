ALTER TABLE projects DROP COLUMN verify_command;
DROP INDEX IF EXISTS idx_thread_watchers_user;
DROP TABLE IF EXISTS thread_watchers;
