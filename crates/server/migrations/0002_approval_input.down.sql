-- Rollback for 0002_approval_input
ALTER TABLE approvals DROP COLUMN input_json;
