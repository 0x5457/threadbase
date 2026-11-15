DROP INDEX idx_inbox_push;

ALTER TABLE inbox ADD COLUMN pushed INTEGER NOT NULL DEFAULT 0;
UPDATE inbox SET pushed = 1 WHERE push_status IN ('sent', 'skipped');

ALTER TABLE inbox DROP COLUMN push_error;
ALTER TABLE inbox DROP COLUMN push_next_at;
ALTER TABLE inbox DROP COLUMN push_attempts;
ALTER TABLE inbox DROP COLUMN push_status;
