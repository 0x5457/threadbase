-- A cold send stores its message before the adapter exists so nothing is lost
-- if startup fails. Until the worker takes it, that row is not waiting behind
-- anything: it is the message starting the thread, so the queue bar and the
-- "N messages queued" count must not show it.
ALTER TABLE queued_inputs ADD COLUMN starting INTEGER NOT NULL DEFAULT 0;
