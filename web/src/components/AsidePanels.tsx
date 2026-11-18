import { Suspense, lazy } from "react";
import { Center, Loader } from "@mantine/core";
import type { CommitSummary } from "../api";
import { HistoryPanel } from "./HistoryPanel";

// The diff surface pulls in the editor and highlighter stacks. Keeping it out
// of the shell chunk is what makes the first paint after sign-in cheap.
const RealChangesPanel = lazy(() =>
  import("./RealChangesPanel").then((module) => ({
    default: module.RealChangesPanel,
  })),
);
const FilesPanel = lazy(() =>
  import("./FilesPanel").then((module) => ({
    default: module.FilesPanel,
  })),
);

export type AsideTab = "changes" | "files" | "history";
/** What the Changes panel is showing: the worktree, one commit, or one turn's
 *  span. A turn span is wider than a commit — it includes uncommitted edits and
 *  every commit the turn made — so it is its own view, not a baseline flag. */
export type AsideView =
  | { kind: "current" }
  | { kind: "commit"; commit: CommitSummary }
  | { kind: "turn"; turnId: string; label: string };

/**
 * The changes/files/history stack, shared by the wide aside and the phone
 * drawer. The two placements differ only in what closing means, so that is
 * the one callback each caller supplies.
 */
export function AsidePanels({
  threadId,
  tab,
  view,
  filesOpened,
  historyOpened,
  showTurn,
  showChanges,
  showFiles,
  showHistory,
  onSelectCommit,
  onClose,
}: {
  threadId: string;
  tab: AsideTab;
  view: AsideView;
  /** The file browser is mounted on first use only; until then it costs a
   *  directory request and DOM that a thread open does not need. */
  filesOpened: boolean;
  /** History is likewise mounted on first use, and keeps its scroll when the
   *  user switches back to Changes. */
  historyOpened: boolean;
  showTurn: (turnId: string) => void;
  showChanges: () => void;
  showFiles: () => void;
  showHistory: () => void;
  onSelectCommit: (commit: CommitSummary) => void;
  onClose: () => void;
}) {
  return (
    <Suspense
      fallback={
        <Center h="100%" py="xl">
          <Loader size="sm" />
        </Center>
      }
    >
      {/* Panels stay mounted: switching back to Changes reuses the parsed
          diff instead of re-tokenizing it on the main thread. */}
      <div
        className="aside-panel"
        style={{ display: tab === "changes" ? undefined : "none" }}
      >
        <RealChangesPanel
          threadId={threadId}
          active={tab === "changes"}
          commit={view.kind === "commit" ? view.commit : null}
          turn={view.kind === "turn" ? view : null}
          onShowTurn={showTurn}
          onShowCurrent={showChanges}
          onShowFiles={showFiles}
          onShowHistory={showHistory}
          onClose={onClose}
        />
      </div>
      {filesOpened ? (
        <div
          className="aside-panel"
          style={{ display: tab === "files" ? undefined : "none" }}
        >
          <FilesPanel
            threadId={threadId}
            onShowChanges={showChanges}
            onClose={onClose}
          />
        </div>
      ) : null}
      {historyOpened ? (
        <div
          className="aside-panel"
          style={{ display: tab === "history" ? undefined : "none" }}
        >
          <HistoryPanel
            threadId={threadId}
            active={tab === "history"}
            selectedSha={view.kind === "commit" ? view.commit.sha : null}
            onSelect={onSelectCommit}
            onShowChanges={showChanges}
            onClose={onClose}
          />
        </div>
      ) : null}
    </Suspense>
  );
}
