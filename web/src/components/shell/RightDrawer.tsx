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
import { useState } from "react";
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
  ScrollArea,
  Skeleton,
  Tabs,
  Textarea,
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
  onDecideApproval?: (
    approval: ApprovalDto,
    decision: ApprovalDecision,
    note?: string,
  ) => void;
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
  const [denyApproval, setDenyApproval] = useState<ApprovalDto | null>(null);
  const [denyNote, setDenyNote] = useState("");
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
        "flex flex-col border-l border-border/50 bg-background",
        overlay
          ? "absolute inset-y-0 right-0 z-40 w-[min(380px,calc(100vw-1rem))] shadow-2xl"
          : "relative h-full w-[360px] shrink-0",
      )}
    >
      <div className="flex h-11 shrink-0 items-center gap-2 px-4">
        <p className="text-[12px] font-semibold text-foreground">Review</p>
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
        <div className="shrink-0 px-3 pb-2">
          <TabsList className="flex h-8 w-full gap-0.5 rounded-lg bg-muted/50 p-0.5">
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

        <ScrollArea className="min-h-0 flex-1">
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
                      className="overflow-hidden rounded-xl border border-border/40 bg-card/40"
                    >
                      <div className="border-b border-border/40 px-3 py-2.5">
                        <div className="flex items-start gap-2">
                          <AlertCircle className="mt-0.5 size-3.5 shrink-0 text-warning" />
                          <div className="min-w-0 flex-1">
                            <p className="text-[13px] font-medium text-fg">
                              {approval.title}
                            </p>
                            <p className="mt-0.5 text-[13px] text-faint">
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
                      <div className="space-y-2 px-3 py-2.5">
                        {approval.command ? (
                          <pre className="overflow-x-auto rounded bg-canvas px-2 py-1.5 font-mono text-[13px] leading-relaxed text-fg">
                            {approval.command}
                          </pre>
                        ) : null}
                        {approval.cwd ? (
                          <p className="break-all font-mono text-[13px] text-faint">
                            cwd: {approval.cwd}
                          </p>
                        ) : null}
                        {approval.paths.length > 0 ? (
                          <ul className="space-y-0.5">
                            {approval.paths.map((path) => (
                              <li
                                key={path}
                                className="break-all font-mono text-[13px] text-muted-foreground"
                              >
                                {path}
                              </li>
                            ))}
                          </ul>
                        ) : null}
                        {approval.input ? (
                          <details className="text-[13px] text-muted-foreground">
                            <summary className="cursor-pointer text-faint hover:text-fg">
                              Full parameters
                            </summary>
                            <pre className="mt-1 max-h-56 overflow-auto whitespace-pre-wrap break-all rounded bg-canvas p-2 font-mono text-[13px] text-fg">
                              {approval.input}
                            </pre>
                          </details>
                        ) : null}
                      </div>
                      <div className="space-y-1.5 border-t border-border/40 px-3 py-2.5">
                        {denyApproval?.id === approval.id ? (
                          <>
                            <Textarea
                              autoFocus
                              value={denyNote}
                              onChange={(event) => setDenyNote(event.target.value)}
                              placeholder="Why deny?"
                              rows={2}
                              className="text-[13px]"
                            />
                            <div className="flex gap-1.5">
                              <Button
                                size="sm"
                                variant="ghost"
                                onClick={() => {
                                  setDenyApproval(null);
                                  setDenyNote("");
                                }}
                              >
                                Cancel
                              </Button>
                              <Button
                                size="sm"
                                variant="danger"
                                loading={deciding}
                                disabled={!denyNote.trim()}
                                onClick={() => {
                                  onDecideApproval?.(
                                    approval,
                                    "deny",
                                    denyNote.trim(),
                                  );
                                  setDenyApproval(null);
                                  setDenyNote("");
                                }}
                              >
                                Deny with reason
                              </Button>
                            </div>
                          </>
                        ) : (
                          <div className="flex flex-wrap gap-1.5">
                            {approval.options.map((option) => (
                              <Button
                                key={option.id}
                                size="sm"
                                variant={
                                  option.kind === "deny" ? "danger" : "secondary"
                                }
                                loading={deciding}
                                disabled={!onDecideApproval}
                                onClick={() => {
                                  if (option.kind === "deny") {
                                    setDenyApproval(approval);
                                    setDenyNote("");
                                  } else {
                                    onDecideApproval?.(approval, option.kind);
                                  }
                                }}
                              >
                                {option.label}
                              </Button>
                            ))}
                          </div>
                        )}
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
        </ScrollArea>
      </Tabs>

      <div className="flex h-7 shrink-0 items-center gap-1 border-t border-border/50 px-3 text-[11px] text-faint">
        <Kbd>⇧Esc</Kbd>
        <span>to close</span>
      </div>
    </aside>
  );
}
