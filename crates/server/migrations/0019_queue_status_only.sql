-- A queue row's status already says whether it was superseded. Keeping a
-- second boolean let the two disagree (release, restart recovery and supersede
-- wrote different subsets), and every reader had to learn which one won.
ALTER TABLE queued_inputs DROP COLUMN superseded;
