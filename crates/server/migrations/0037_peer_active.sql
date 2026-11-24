-- Another client is driving this thread's native session. The composer uses
-- it to stop the second writer before the agent rejects the turn.
ALTER TABLE agent_bindings ADD COLUMN peer_active INTEGER NOT NULL DEFAULT 0;
