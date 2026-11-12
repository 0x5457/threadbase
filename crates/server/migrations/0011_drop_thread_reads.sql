-- Thread read markers belonged to routes that no longer exist; unread state is
-- tracked by the inbox, not by a per-thread event cursor.
DROP TABLE IF EXISTS thread_reads;
