-- Tables that no code path reads or writes. Removing them keeps the schema
-- honest: every table has an owner.
DROP TABLE IF EXISTS snapshots;
DROP TABLE IF EXISTS attachments;
DROP TABLE IF EXISTS webhooks;
DROP TABLE IF EXISTS user_settings;
