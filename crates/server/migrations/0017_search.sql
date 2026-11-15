-- Full-text search over thread titles, prompts, agent messages and comments.
-- The application maintains this table; events remain the source of truth.
CREATE VIRTUAL TABLE search_fts USING fts5(
    thread_id UNINDEXED,
    kind UNINDEXED,
    ref_id UNINDEXED,
    title,
    body,
    ts UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);
