import { FilesBrowser } from "./files/FilesBrowser";

export function FilesPanel({
  threadId,
  onClose,
  onShowChanges,
}: {
  threadId: string;
  onClose?: () => void;
  onShowChanges?: () => void;
}) {
  // Keying the browser by thread clears the browsed path when the panel
  // switches to a different worktree.
  return (
    <FilesBrowser
      key={threadId}
      threadId={threadId}
      onClose={onClose}
      onShowChanges={onShowChanges}
    />
  );
}
