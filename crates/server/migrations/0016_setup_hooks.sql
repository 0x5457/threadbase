-- Project setup hook and per-thread setup state. Setup runs once per worktree
-- before the agent is allowed to start, so dependency installs and env files
-- are ready without paying the cost inside the first turn.
ALTER TABLE projects ADD COLUMN setup_command TEXT;

ALTER TABLE threads ADD COLUMN setup_status TEXT NOT NULL DEFAULT 'none';
ALTER TABLE threads ADD COLUMN setup_error TEXT;
