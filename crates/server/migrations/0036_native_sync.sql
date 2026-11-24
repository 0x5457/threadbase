-- Watermark for native sessions: the completion time of the newest turn this
-- thread has imported or mirrored. Resume imports only newer turns.
ALTER TABLE agent_bindings ADD COLUMN native_synced_at INTEGER NOT NULL DEFAULT 0;
