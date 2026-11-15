-- A boolean could not tell "delivered" from "gave up after a transport
-- failure", so a failed push was marked done and never retried. Push now has
-- explicit states, an attempt counter, and a next-attempt timestamp the worker
-- polls.
ALTER TABLE inbox ADD COLUMN push_status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE inbox ADD COLUMN push_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inbox ADD COLUMN push_next_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inbox ADD COLUMN push_error TEXT;

-- Never deliver a backlog of old notifications on the first tick after an
-- upgrade. Every row that exists now is marked skipped; new rows default to
-- pending.
UPDATE inbox SET push_status = 'skipped';

ALTER TABLE inbox DROP COLUMN pushed;

CREATE INDEX idx_inbox_push ON inbox(push_status, push_next_at);
