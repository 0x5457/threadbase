-- ACP v2 permission requests carry supporting copy beside the title; the
-- transcript shows it under the approval prompt.
ALTER TABLE approvals ADD COLUMN description TEXT;
