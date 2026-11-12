-- Several indexes were superseded by later migrations or no longer match a
-- query (they share a prefix with a better index, so writes paid twice).
DROP INDEX IF EXISTS idx_approvals_thread_status;
DROP INDEX IF EXISTS idx_elicitations_thread_status;
DROP INDEX IF EXISTS idx_queued_inputs_thread_status;
DROP INDEX IF EXISTS idx_comments_thread_status;
DROP INDEX IF EXISTS idx_inbox_user_read;
DROP INDEX IF EXISTS idx_approvals_pending;
DROP INDEX IF EXISTS idx_events_ts;
DROP INDEX IF EXISTS idx_subagents_thread;
