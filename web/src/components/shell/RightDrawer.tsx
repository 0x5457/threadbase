import {
  AlertCircle,
  Check,
  FileCode2,
  GitCompareArrows,
  RefreshCw,
  ShieldCheck,
  X,
} from "lucide-react";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { FileChangeDto } from "@/api/types/FileChangeDto";
import type { FileStatus } from "@/api/types/FileStatus";
import type { ThreadBundleDto } from "@/api/types/ThreadBundleDto";
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

export type DrawerTab = "changes" | "approvals" | "files";

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
}

function fileStatusLabel(status: FileStatus): string {
  switch (status) {
    case "added":
      return "A";
    case "modified":
      return "M";
    case "deleted":
      return "D";
    case "renamed":
      return "R";
    case "untracked":
      return "U";
  }
}

function fileStatusClass(status: FileStatus): string {
  switch (status) {
    case "added":
    case "untracked":
      return "border-success/30 bg-success/10 text-success";
    case "deleted":
      return "border-danger/30 bg-danger/10 text-danger";
    case "renamed":
      return "border-info/30 bg-info/10 text-info";
    default:
      return "border-border bg-raised text-muted";
  }
}

function FileRow({ file }: { file: FileChangeDto }) {
  return (
    <li className="flex items-center gap-2 border-b border-border/60 px-3 py-2 last:border-b-0">
      <span
        className={cn(
          "flex size-5 shrink-0 items-center justify-center rounded border font-mono text-[10px] font-semibold",
          fileStatusClass(file.status),
        )}
        title={file.status}
      >
        {fileStatusLabel(file.status)}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate font-mono text-[11px] text-fg">{file.path}</p>
        {file.oldPath ? (
          <p className="truncate font-mono text-[10px] text-faint">
            from {file.oldPath}
          </p>
        ) : null}
      </div>
      {file.binary ? (
        <Badge size="sm" variant="outline">
          binary
        </Badge>
      ) : (
        <span className="shrink-0 font-mono text-[10px] tabular-nums">
          <span className="text-success">+{file.additions}</span>{" "}
          <span className="text-danger">−{file.deletions}</span>
        </span>
      )}
      {file.seen ? (
        <Check className="size-3 shrink-0 text-success" aria-label="Seen" />
      ) : null}
    </li>
  );
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
}: RightDrawerProps) {
  if (!open) {
    return null;
  }

  const changes = bundle?.changes;
  const files = changes?.files ?? [];
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
          <TabsList className="w-full justify-between">
            <TabsTrigger value="changes" className="flex-1 justify-center">
              <GitCompareArrows className="size-3" aria-hidden />
              Changes
              {changes ? (
                <span className="font-mono text-[9.5px] text-faint">
                  +{totalAdditions} −{totalDeletions}
                </span>
              ) : null}
            </TabsTrigger>
            <TabsTrigger value="approvals" className="flex-1 justify-center">
              <ShieldCheck className="size-3" aria-hidden />
              Approvals
              {pendingApprovals.length > 0 ? (
                <Badge size="sm" variant="warning">
                  {pendingApprovals.length}
                </Badge>
              ) : null}
            </TabsTrigger>
            <TabsTrigger value="files" className="flex-1 justify-center">
              <FileCode2 className="size-3" aria-hidden />
              Files
              {files.length > 0 ? (
                <span className="font-mono text-[9.5px] text-faint">
                  {files.length}
                </span>
              ) : null}
            </TabsTrigger>
          </TabsList>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          <TabsContent value="changes">
            {loading ? (
              <DrawerSkeleton />
            ) : error ? (
              <ErrorState
                error={error}
                title="Cannot load changes"
                onRetry={onRetry}
                compact
              />
            ) : !bundle ? (
              <EmptyState
                icon={GitCompareArrows}
                title="No thread open"
                description="Open a thread to review its changes."
                compact
              />
            ) : files.length === 0 ? (
              <EmptyState
                icon={GitCompareArrows}
                title="No file changes yet."
                description="Changes appear here after the agent edits files."
                compact
              />
            ) : (
              <div>
                <div className="space-y-2 border-b border-border px-3 py-3">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[10px] uppercase tracking-wider text-faint">
                      Baseline
                    </span>
                    <span className="truncate font-mono text-[10.5px] text-fg">
                      {changes?.baseLabel}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-2 text-[11px]">
                    <span className="text-muted">
                      {changes?.stats.files ?? files.length} files
                    </span>
                    <span className="font-mono">
                      <span className="text-success">+{totalAdditions}</span>{" "}
                      <span className="text-danger">−{totalDeletions}</span>
                    </span>
                  </div>
                  {changes && changes.stats.total > 0 ? (
                    <div className="space-y-1">
                      <div className="flex items-center justify-between text-[10px] text-faint">
                        <span>Seen</span>
                        <span>
                          {changes.stats.seen}/{changes.stats.total}
                        </span>
                      </div>
                      <div className="h-1 overflow-hidden rounded-full bg-raised">
                        <div
                          className="h-full rounded-full bg-accent"
                          style={{
                            width: `${Math.round(
                              (changes.stats.seen /
                                Math.max(1, changes.stats.total)) *
                                100,
                            )}%`,
                          }}
                        />
                      </div>
                    </div>
                  ) : null}
                </div>
                <ul>
                  {files.map((file) => (
                    <FileRow key={file.path} file={file} />
                  ))}
                </ul>
              </div>
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
                description="Agent permission requests will appear here."
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

          <TabsContent value="files">
            {loading ? (
              <DrawerSkeleton />
            ) : error ? (
              <ErrorState
                error={error}
                title="Cannot load files"
                onRetry={onRetry}
                compact
              />
            ) : files.length === 0 ? (
              <EmptyState
                icon={FileCode2}
                title="No changed files"
                description="Files touched by this thread will be listed here."
                compact
              />
            ) : (
              <ul>
                {files.map((file) => (
                  <FileRow key={file.path} file={file} />
                ))}
              </ul>
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
