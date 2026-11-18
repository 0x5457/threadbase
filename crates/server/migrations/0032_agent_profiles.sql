-- Workspace-configured ACP agents layered on the built-in profiles.
CREATE TABLE agent_profiles (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    command TEXT NOT NULL,
    args_json TEXT NOT NULL DEFAULT '[]',
    env_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL
);
