ALTER TABLE queued_inputs ADD COLUMN superseded INTEGER NOT NULL DEFAULT 0;
UPDATE queued_inputs SET superseded = 1 WHERE status = 'superseded';
