-- Placement is where a person parked a thread between turns: `active`,
-- `snoozed` or `settled`. It is not a status. `working` and "needs you" are
-- derived when the list is built from the thread status plus queued input and
-- live subagents, so a parked thread cannot claim to be running.
--
-- `archived` intentionally stays in `threads.status`: it changes what the agent
-- may accept, which is a status question, while placement changes nothing about
-- the agent. `placement_at` is the timestamp the shelf is sorted by.
ALTER TABLE threads ADD COLUMN placement TEXT NOT NULL DEFAULT 'active';
ALTER TABLE threads ADD COLUMN placement_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN snooze_until INTEGER;

UPDATE threads SET placement_at = COALESCE(last_activity_at, updated_at)
 WHERE placement_at = 0;

CREATE INDEX idx_threads_placement ON threads(placement, snooze_until);
