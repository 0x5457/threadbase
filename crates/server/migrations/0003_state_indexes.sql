-- Indexes for lifecycle state queries (thread status, pending items, queue).
CREATE INDEX IF NOT EXISTS idx_approvals_thread_status ON approvals(thread_id, status);
CREATE INDEX IF NOT EXISTS idx_elicitations_thread_status ON elicitations(thread_id, status);
CREATE INDEX IF NOT EXISTS idx_queued_inputs_thread_status ON queued_inputs(thread_id, status);
CREATE INDEX IF NOT EXISTS idx_subagents_thread_status ON subagents(thread_id, status);
CREATE INDEX IF NOT EXISTS idx_comments_thread_status ON comments(thread_id, status);
CREATE INDEX IF NOT EXISTS idx_inbox_user_read ON inbox(user_id, read_at);
CREATE INDEX IF NOT EXISTS idx_thread_reads_thread_user ON thread_reads(thread_id, user_id);
