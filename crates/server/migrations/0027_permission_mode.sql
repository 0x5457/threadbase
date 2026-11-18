-- How much an agent may do without asking. Both columns are nullable so a
-- thread can inherit its project and a project can inherit the instance default;
-- the effective mode is resolved (and clamped to the instance ceiling) on read,
-- never stored, so changing a project default moves every thread that inherits.
ALTER TABLE threads ADD COLUMN permission_mode TEXT;
ALTER TABLE projects ADD COLUMN permission_mode TEXT;
