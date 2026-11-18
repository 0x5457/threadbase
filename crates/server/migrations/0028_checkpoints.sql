-- One row per turn: the worktree snapshot taken immediately before that turn
-- started. Rewinding to a turn means `git reset --hard` to this commit, so the
-- mapping has to be durable. The History panel used to guess it from commit and
-- prompt timestamps; a guess is not something to base a destructive operation on.
CREATE TABLE checkpoints (
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    turn_id TEXT NOT NULL,
    sha TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (thread_id, turn_id)
);
