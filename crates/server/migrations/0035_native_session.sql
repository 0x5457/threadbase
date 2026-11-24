-- A thread attached to an agent's own session store. Its history cannot be
-- truncated or forked locally without diverging from the agent.
ALTER TABLE agent_bindings ADD COLUMN native INTEGER NOT NULL DEFAULT 0;
