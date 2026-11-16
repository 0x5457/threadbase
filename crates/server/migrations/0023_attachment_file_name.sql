-- Original names make non-image attachments readable in the composer and in
-- history; older rows keep NULL and render as generic attachments.
ALTER TABLE attachments ADD COLUMN file_name TEXT;
