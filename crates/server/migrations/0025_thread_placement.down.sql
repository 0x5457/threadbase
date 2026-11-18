DROP INDEX IF EXISTS idx_threads_placement;
ALTER TABLE threads DROP COLUMN snooze_until;
ALTER TABLE threads DROP COLUMN placement_at;
ALTER TABLE threads DROP COLUMN placement;
