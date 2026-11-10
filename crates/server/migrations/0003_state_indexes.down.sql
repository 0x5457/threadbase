-- Rollback for 0003_state_indexes
DROP INDEX IF EXISTS idx_approvals_thread_status;
DROP INDEX IF EXISTS idx_elicitations_thread_status;
DROP INDEX IF EXISTS idx_queued_inputs_thread_status;
DROP INDEX IF EXISTS idx_subagents_thread_status;
DROP INDEX IF EXISTS idx_comments_thread_status;
DROP INDEX IF EXISTS idx_inbox_user_read;
DROP INDEX IF EXISTS idx_thread_reads_thread_user;
