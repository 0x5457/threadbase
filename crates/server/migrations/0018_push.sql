-- Web Push: one VAPID keypair for this instance and per-user browser
-- subscriptions. Inbox rows carry a delivery flag so the push worker can run
-- independently of the request path that created the notification.
CREATE TABLE push_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    private_key TEXT NOT NULL,
    public_key TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE push_subscriptions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    endpoint TEXT NOT NULL UNIQUE,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_push_subscriptions_user ON push_subscriptions(user_id);

ALTER TABLE inbox ADD COLUMN pushed INTEGER NOT NULL DEFAULT 0;
-- Never push notifications that were already sitting in an inbox before the
-- feature existed.
UPDATE inbox SET pushed = 1;
