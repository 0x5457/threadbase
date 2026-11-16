/**
 * The composer's reserved space while its chunk or editor loads. Keeping this
 * in its own module matters: the real Composer pulls Tiptap, the CRDT draft
 * provider and the suggestion stack, and the transcript should paint before
 * that download even starts.
 */
export function ComposerSkeleton({
  agent = "agent",
  onActivate,
}: {
  agent?: string;
  /** A tap mounts the real composer immediately instead of waiting for idle. */
  onActivate?: () => void;
}) {
  return (
    <section className="composer" onPointerDown={onActivate}>
      <div className="composer-inner">
        <div className="composer-row">
          <span />
          <div className="composer-input">
            <span className="composer-placeholder">Message {agent}…</span>
          </div>
        </div>
        <div className="composer-tools" aria-hidden="true" />
      </div>
    </section>
  );
}
