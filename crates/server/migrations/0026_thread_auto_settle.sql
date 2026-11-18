-- Whether the inactivity sweeper may park a thread on its own. This is a policy
-- flag, not a placement: `never` still allows a person to settle the thread, and
-- `default` only settles it while it is idle with nobody waiting on it.
ALTER TABLE threads ADD COLUMN auto_settle TEXT NOT NULL DEFAULT 'default';
