-- A queued prompt remembers its images by id; the bytes already live in the
-- attachments table, so only the references need storing.
ALTER TABLE queued_inputs ADD COLUMN attachments_json TEXT;
