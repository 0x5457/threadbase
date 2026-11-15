ALTER TABLE agent_bindings ADD COLUMN config_json TEXT NOT NULL DEFAULT '{}';
ALTER TABLE agent_bindings ADD COLUMN protocol_version TEXT;
ALTER TABLE agent_bindings ADD COLUMN adapter_version TEXT;
ALTER TABLE agent_bindings ADD COLUMN last_error TEXT;
ALTER TABLE agent_bindings ADD COLUMN status TEXT NOT NULL DEFAULT 'idle';
