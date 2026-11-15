-- `agent_bindings.status` and `last_error` were written but never read: the
-- thread status machine owns lifecycle, and the binding is only useful for the
-- resume ids (`agent`, `cwd`, `session_id`). The remaining columns were never
-- written at all. Drop them so there is no second place to look for state.
ALTER TABLE agent_bindings DROP COLUMN status;
ALTER TABLE agent_bindings DROP COLUMN last_error;
ALTER TABLE agent_bindings DROP COLUMN adapter_version;
ALTER TABLE agent_bindings DROP COLUMN protocol_version;
ALTER TABLE agent_bindings DROP COLUMN config_json;
