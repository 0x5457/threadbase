import {
  AlertCircle,
  FileCode2,
  GitCompareArrows,
  MessageSquare,
  RefreshCw,
  Terminal,
  ShieldCheck,
  X,
} from "lucide-react";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { ThreadBundleDto } from "@/api/types/ThreadBundleDto";
import { ChangesPanel } from "@/components/thread/ChangesPanel";
import { CommentsPanel } from "@/components/thread/CommentsPanel";
import { FilesPanel } from "@/components/thread/FilesPanel";
import { TerminalPanel } from "@/components/thread/TerminalPanel";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  IconButton,
  Kbd,
  Skeleton,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  Tooltip,
} from "@/components/ui";
import { cn } from "@/lib/utils";

export type DrawerTab =
  | "changes"
  | "approvals"
  | "comments"
  | "files"
  | "terminal";

export interface RightDrawerProps {
  open: boolean;
  activeTab: DrawerTab;
  onTabChange: (tab: DrawerTab) => void;
  onClose: () => void;
  bundle?: ThreadBundleDto | null;
  loading?: boolean;
  error?: unknown;
  onRetry?: () => void;
  decidingApprovalId?: string | null;
  overlay?: boolean;
  onDecideApproval?: (approval: ApprovalDto, decision: ApprovalDecision) => void;
  threadId?: string;
  canWrite?: boolean;
}

function DrawerSkeleton() {
  return (
    <div className="space-y-2 px-3 py-3">
      <Skeleton className="h-5 w-32" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-10 w-full" />
      <Skeleton className="h-10 w-full" />
    </div>
  );
}

export function RightDrawer({
  open,
  activeTab,
  onTabChange,
  onClose,
  bundle,
  loading = false,
  error,
  onRetry,
  decidingApprovalId,
  overlay = true,
  onDecideApproval,
  threadId,
  canWrite = false,
}: RightDrawerProps) {
  if (!open) {
    return null;
  }

  const changes = bundle?.changes;
  const approvals = bundle?.approvals ?? [];
  const pendingApprovals = approvals.filter(
    (approval) => approval.status === "pending",
  );
  const totalAdditions = changes?.stats.additions ?? 0;
  const totalDeletions = changes?.stats.deletions ?? 0;

  return (
    <aside
      aria-label="Right drawer"
      className={cn(
        "flex flex-col border-l border-border bg-panel",
        overlay
          ? "absolute inset-y-0 right-0 z-40 w-[min(380px,calc(100vw-1rem))] shadow-2xl"
          : "relative h-full w-[380px] shrink-0",
      )}
    >
      <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border px-3">
        <p className="text-[12px] font-semibold text-fg">Review</p>
        <div className="ml-auto flex items-center gap-1">
          {onRetry ? (
            <Tooltip content="Refresh thread data">
              <IconButton label="Refresh thread data" size="sm" onClick={onRetry}>
                <RefreshCw />
              </IconButton>
            </Tooltip>
          ) : null}
          <IconButton label="Close right drawer" size="sm" onClick={onClose}>
            <X />
          </IconButton>
        </div>
      </div>

      <Tabs
        value={activeTab}
        onValueChange={(value) => onTabChange(value as DrawerTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <div className="shrink-0 border-b border-border px-2 py-1.5">
          <TabsList className="flex w-full gap-0.5">
            <TabsTrigger
              value="changes"
              aria-label="Changes"
              title="Changes"
              className="flex-1 justify-center px-1"
            >
              <GitCompareArrows className="size-3.5" aria-hidden />
              {changes ? (
                <span className="font-mono text-[9px] text-faint">
                  {totalAdditions + totalDeletions}
                </span>
              ) : null}
            </TabsTrigger>
            <TabsTrigger
              value="approvals"
              aria-label="Approvals"
              title="Approvals"
              className="relative flex-1 justify-center px-1"
            >
              <ShieldCheck className="size-3.5" aria-hidden />
              {pendingApprovals.length > 0 ? (
                <span className="absolute right-1 top-1 size-1.5 rounded-full bg-warning" />
              ) : null}
            </TabsTrigger>
            <TabsTrigger
              value="terminal"
              aria-label="Output"
              title="Agent output"
              className="flex-1 justify-center px-1"
            >
              <Terminal className="size-3.5" aria-hidden />
            </TabsTrigger>
            <TabsTrigger
              value="comments"
              aria-label="Comments"
              title="Comments"
              className="flex-1 justify-center px-1"
            >
              <MessageSquare className="size-3.5" aria-hidden />
            </TabsTrigger>
            <TabsTrigger
              value="files"
              aria-label="Files"
              title="Files"
              className="flex-1 justify-center px-1"
            >
              <FileCode2 className="size-3.5" aria-hidden />
            </TabsTrigger>
          </TabsList>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          <TabsContent value="changes">
            {threadId ? (
              <ChangesPanel threadId={threadId} canWrite={canWrite} />
            ) : (
              <EmptyState
                icon={GitCompareArrows}
                title="No thread open"
                compact
              />
            )}
          </TabsContent>

          <TabsContent value="approvals">
            {loading ? (
              <DrawerSkeleton />
            ) : error ? (
              <ErrorState
                error={error}
                title="Cannot load approvals"
                onRetry={onRetry}
                compact
              />
            ) : pendingApprovals.length === 0 ? (
              <EmptyState
                icon={ShieldCheck}
                title="No approvals waiting"
                compact
              />
            ) : (
              <ul className="space-y-2 p-2.5">
                {pendingApprovals.map((approval) => {
                  const deciding = decidingApprovalId === approval.id;
                  return (
                    <li
                      key={approval.id}
                      className="overflow-hidden rounded-lg border border-border bg-surface"
                    >
                      <div className="border-b border-border px-3 py-2">
                        <div className="flex items-start gap-2">
                          <AlertCircle className="mt-0.5 size-3.5 shrink-0 text-warning" />
                          <div className="min-w-0 flex-1">
                            <p className="text-[11.5px] font-medium text-fg">
                              {approval.title}
                            </p>
                            <p className="mt-0.5 text-[10px] text-faint">
                              {approval.agent}
                              {approval.subagentId
                                ? ` · subagent ${approval.subagentId}`
                                : ""}
                            </p>
                          </div>
                          <Badge size="sm" variant="warning">
                            pending
                          </Badge>
                        </div>
                      </div>
                      <div className="space-y-2 px-3 py-2">
                        {approval.command ? (
                          <pre className="overflow-x-auto rounded bg-canvas px-2 py-1.5 font-mono text-[10.5px] leading-relaxed text-fg">
                            {approval.command}
                          </pre>
                        ) : null}
                        {approval.cwd ? (
                          <p className="break-all font-mono text-[10px] text-faint">
                            cwd: {approval.cwd}
                          </p>
                        ) : null}
                        {approval.paths.length > 0 ? (
                          <ul className="space-y-0.5">
                            {approval.paths.map((path) => (
                              <li
                                key={path}
                                className="break-all font-mono text-[10px] text-muted"
                              >
                                {path}
                              </li>
                            ))}
                          </ul>
                        ) : null}
                      </div>
                      <div className="flex flex-wrap gap-1.5 border-t border-border bg-canvas/40 px-3 py-2">
                        {approval.options.map((option) => (
                          <Button
                            key={option.id}
                            size="sm"
                            variant={
                              option.kind === "deny" ? "danger" : "secondary"
                            }
                            loading={deciding}
                            disabled={!onDecideApproval}
                            onClick={() =>
                              onDecideApproval?.(approval, option.kind)
                            }
                          >
                            {option.label}
                          </Button>
                        ))}
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </TabsContent>

          <TabsContent value="terminal">
            {threadId ? (
              <TerminalPanel threadId={threadId} />
            ) : (
              <EmptyState
                icon={Terminal}
                title="No thread open"
                compact
              />
            )}
          </TabsContent>

          <TabsContent value="comments">
            {threadId ? (
              <CommentsPanel threadId={threadId} canWrite={canWrite} />
            ) : (
              <EmptyState
                icon={MessageSquare}
                title="No thread open"
                compact
              />
            )}
          </TabsContent>

          <TabsContent value="files">
            {threadId ? (
              <FilesPanel threadId={threadId} />
            ) : (
              <EmptyState
                icon={FileCode2}
                title="No thread open"
                compact
              />
            )}
          </TabsContent>
        </div>
      </Tabs>

      <div className="flex h-7 shrink-0 items-center gap-1 border-t border-border px-3 text-[10px] text-faint">
        <Kbd>⇧Esc</Kbd>
        <span>to close</span>
      </div>
    </aside>
  );
}
