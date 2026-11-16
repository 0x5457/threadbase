-- The sidebar needs a cheap "has a draft" marker without loading every CRDT
-- room. The flag is maintained on empty <-> non-empty transitions and backfilled
-- from persisted draft rows at startup.
ALTER TABLE threads ADD COLUMN has_draft INTEGER NOT NULL DEFAULT 0;
