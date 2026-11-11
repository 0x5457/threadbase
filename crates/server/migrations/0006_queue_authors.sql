-- Preserve the full draft author list with a queued input so the queue worker
-- can restore multi-author attribution when it sends.
ALTER TABLE queued_inputs ADD COLUMN authors_json TEXT;
