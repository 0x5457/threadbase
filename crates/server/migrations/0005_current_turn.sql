-- Persist the active turn id on the thread row so the shared ThreadState
-- projection does not need to derive it from the event log.
ALTER TABLE threads ADD COLUMN current_turn_id TEXT;
