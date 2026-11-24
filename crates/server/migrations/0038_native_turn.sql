-- Native turn cursor. Codex turn ids are UUIDv7, so "after this id" replaces
-- second-precision timestamps for incremental import.
ALTER TABLE agent_bindings ADD COLUMN native_turn_id TEXT;
