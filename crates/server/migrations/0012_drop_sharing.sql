-- Sharing is gone: every signed-in account sees every thread, so threads need
-- no share level, membership rows, or invite tokens. Member-change events are
-- removed with the feature so replay never meets a variant the code no longer
-- knows.
DELETE FROM events WHERE kind = 'members_changed';
DELETE FROM inbox WHERE kind IN ('invite', 'access');

DROP INDEX IF EXISTS idx_invites_thread;
DROP TABLE IF EXISTS invites;

DROP INDEX IF EXISTS idx_thread_members_user;
DROP TABLE IF EXISTS thread_members;

ALTER TABLE threads DROP COLUMN share_level;
