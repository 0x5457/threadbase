ALTER TABLE inbox DROP COLUMN pushed;
DROP INDEX IF EXISTS idx_push_subscriptions_user;
DROP TABLE IF EXISTS push_subscriptions;
DROP TABLE IF EXISTS push_config;
