import {
  ActionIcon,
  Badge,
  Box,
  Button,
  Drawer,
  Kbd,
  Paper,
  ScrollArea,
  Skeleton,
  Tabs,
  Text,
  Textarea,
  Tooltip,
} from "@mantine/core";
import {
  AlertCircle,
  ChevronRight,
  FileCode2,
  GitCompareArrows,
  MessageSquareText,
  RefreshCw,
  ShieldCheck,
  Terminal,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { ThreadBundleDto } from "@/api/types/ThreadBundleDto";
import { ChangesPanel } from "@/components/thread/ChangesPanel";
import { CommentsPanel } from "@/components/thread/CommentsPanel";
import { FilesPanel } from "@/components/thread/FilesPanel";
import { TerminalPanel } from "@/components/thread/TerminalPanel";
import { EmptyState, ErrorState } from "@/components/app";

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
  /** Desktop uses the persistent AppShell aside; phone/tablet use a sheet. */
  desktop?: boolean;
  phone?: boolean;
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
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        padding: 12,
      }}
    >
      <Skeleton height={20} width={128} />
      <Skeleton height={64} />
      <Skeleton height={40} />
      <Skeleton height={40} />
    </div>
  );
}

function ReviewPanel({
  activeTab,
  onTabChange,
  onClose,
  bundle,
  loading = false,
  error,
  onRetry,
  decidingApprovalId,
  onDecideApproval,
  threadId,
  canWrite = false,
}: Omit<RightDrawerProps, "open" | "desktop" | "phone">) {
  const [denyApproval, setDenyApproval] = useState<ApprovalDto | null>(null);
  const [denyNote, setDenyNote] = useState("");
  const panelRef = useRef<HTMLDivElement>(null);

  // The drawer mounts when it opens; land keyboard focus on the active tab so
  // the panel is operable without walking the transcript first.
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      panelRef.current
        ?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')
        ?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  const changes = bundle?.changes;
  const approvals = bundle?.approvals ?? [];
  const pendingApprovals = approvals.filter(
    (approval) => approval.status === "pending",
  );

  const totalAdditions = changes?.stats.additions ?? 0;
  const totalDeletions = changes?.stats.deletions ?? 0;

  return (
    <div
      ref={panelRef}
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        minHeight: 0,
        background: "var(--mantine-color-body)",
        paddingBottom: "env(safe-area-inset-bottom)",
      }}
    >
      <div
        style={{
          display: "flex",
          height: 44,
          flexShrink: 0,
          alignItems: "center",
          gap: 8,
          paddingInline: 12,
        }}
      >
        <Text size="sm" fw={600}>
          Review
        </Text>
        <div
          style={{
            marginLeft: "auto",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          {onRetry ? (
            <Tooltip label="Refresh thread data">
              <ActionIcon
                variant="default"
                size="lg"
                className="tb-hit-40"
                aria-label="Refresh thread data"
                onClick={onRetry}
              >
                <RefreshCw size={14} aria-hidden />
              </ActionIcon>
            </Tooltip>
          ) : null}
          <ActionIcon
            variant="default"
            size="lg"
            className="tb-hit-40"
            aria-label="Close right drawer"
            onClick={onClose}
          >
            <X size={14} aria-hidden />
          </ActionIcon>
        </div>
      </div>

      <Tabs
        value={activeTab}
        onChange={(value) => {
          if (value) {
            onTabChange(value as DrawerTab);
          }
        }}
        variant="pills"
        keepMounted={false}
        style={{
          display: "flex",
          minHeight: 0,
          flex: 1,
          flexDirection: "column",
        }}
      >
        <div
          style={{
            flexShrink: 0,
            paddingInline: 12,
            paddingBottom: 8,
          }}
        >
          <Tabs.List
            grow
            style={{
              padding: 2,
              background: "var(--mantine-color-default-hover)",
              borderRadius: 8,
            }}
          >
            <Tabs.Tab
              value="changes"
              aria-label="Changes"
              title="Changes"
              leftSection={<GitCompareArrows size={14} aria-hidden />}
            >
              {changes ? (
                <Text size="xs" ff="monospace" c="dimmed">
                  {totalAdditions + totalDeletions}
                </Text>
              ) : null}
            </Tabs.Tab>
            <Tabs.Tab
              value="approvals"
              aria-label="Approvals"
              title="Approvals"
              leftSection={<ShieldCheck size={14} aria-hidden />}
              rightSection={
                pendingApprovals.length > 0 ? (
                  <span
                    aria-hidden
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: "var(--mantine-color-yellow-filled)",
                    }}
                  />
                ) : undefined
              }
            />
            <Tabs.Tab
              value="terminal"
              aria-label="Output"
              title="Agent output"
              leftSection={<Terminal size={14} aria-hidden />}
            />
            <Tabs.Tab
              value="comments"
              aria-label="Comments"
              title="Comments"
              leftSection={<MessageSquareText size={14} aria-hidden />}
            />
            <Tabs.Tab
              value="files"
              aria-label="Files"
              title="Files"
              leftSection={<FileCode2 size={14} aria-hidden />}
            />
          </Tabs.List>
        </div>

        <ScrollArea
          style={{ flex: 1, minHeight: 0 }}
          scrollbars="y"
          type="scroll"
        >
          <Tabs.Panel value="changes">
            {threadId ? (
              <ChangesPanel threadId={threadId} canWrite={canWrite} />
            ) : (
              <EmptyState
                icon={GitCompareArrows}
                title="No thread open"
                compact
              />
            )}
          </Tabs.Panel>

          <Tabs.Panel value="approvals">
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
              <ul
                style={{
                  margin: 0,
                  padding: 10,
                  listStyle: "none",
                  display: "flex",
                  flexDirection: "column",
                  gap: 8,
                }}
              >
                {pendingApprovals.map((approval) => {
                  const deciding = decidingApprovalId === approval.id;
                  return (
                    <li key={approval.id}>
                      <Paper withBorder radius="md" p="sm">
                        <div
                          style={{
                            display: "flex",
                            alignItems: "flex-start",
                            gap: 8,
                            borderBottom:
                              "1px solid var(--mantine-color-default-border)",
                            paddingBottom: 10,
                            marginBottom: 10,
                          }}
                        >
                          <AlertCircle
                            size={14}
                            aria-hidden
                            style={{
                              marginTop: 2,
                              flexShrink: 0,
                              color: "var(--mantine-color-yellow-filled)",
                            }}
                          />
                          <div style={{ minWidth: 0, flex: 1 }}>
                            <Text size="sm" fw={500}>
                              {approval.title}
                            </Text>
                            <Text size="xs" c="dimmed" mt={2}>
                              {approval.agent}
                              {approval.subagentId
                                ? ` · subagent ${approval.subagentId}`
                                : ""}
                            </Text>
                          </div>
                          <Badge color="yellow" size="sm">
                            pending
                          </Badge>
                        </div>

                        <div
                          style={{
                            display: "flex",
                            flexDirection: "column",
                            gap: 8,
                          }}
                        >
                          {approval.command ? (
                            <pre
                              style={{
                                margin: 0,
                                overflowX: "auto",
                                borderRadius: "var(--mantine-radius-md)",
                                background:
                                  "var(--mantine-color-default-hover)",
                                padding: "6px 8px",
                                fontFamily:
                                  "var(--mantine-font-family-monospace)",
                                fontSize: "var(--mantine-font-size-md)",
                                lineHeight: 1.5,
                                color: "var(--mantine-color-text)",
                              }}
                            >
                              {approval.command}
                            </pre>
                          ) : null}
                          {approval.cwd ? (
                            <Text
                              size="xs"
                              ff="monospace"
                              c="dimmed"
                              style={{ overflowWrap: "anywhere" }}
                            >
                              cwd: {approval.cwd}
                            </Text>
                          ) : null}
                          {approval.paths.length > 0 ? (
                            <ul
                              style={{
                                margin: 0,
                                padding: 0,
                                listStyle: "none",
                                display: "flex",
                                flexDirection: "column",
                                gap: 2,
                              }}
                            >
                              {approval.paths.map((path) => (
                                <li
                                  key={path}
                                  style={{
                                    fontFamily:
                                      "var(--mantine-font-family-monospace)",
                                    fontSize: "var(--mantine-font-size-md)",
                                    color: "var(--mantine-color-dimmed)",
                                    overflowWrap: "anywhere",
                                  }}
                                >
                                  {path}
                                </li>
                              ))}
                            </ul>
                          ) : null}
                          {approval.input ? (
                            <details>
                              <summary
                                style={{
                                  display: "flex",
                                  alignItems: "center",
                                  gap: "0.5rem",
                                  cursor: "pointer",
                                  fontSize: "var(--mantine-font-size-md)",
                                  color: "var(--mantine-color-dimmed)",
                                }}
                              >
                                <ChevronRight
                                  size={12}
                                  data-disclosure-chevron
                                  aria-hidden
                                  style={{ flexShrink: 0 }}
                                />
                                Full parameters
                              </summary>
                              <pre
                                style={{
                                  margin: "4px 0 0",
                                  maxHeight: 224,
                                  overflow: "auto",
                                  whiteSpace: "pre-wrap",
                                  overflowWrap: "anywhere",
                                  borderRadius: "var(--mantine-radius-md)",
                                  background:
                                    "var(--mantine-color-default-hover)",
                                  padding: 8,
                                  fontFamily:
                                    "var(--mantine-font-family-monospace)",
                                  fontSize: "var(--mantine-font-size-md)",
                                  color: "var(--mantine-color-text)",
                                }}
                              >
                                {approval.input}
                              </pre>
                            </details>
                          ) : null}
                        </div>

                        <div
                          style={{
                            display: "flex",
                            flexDirection: "column",
                            gap: 6,
                            borderTop:
                              "1px solid var(--mantine-color-default-border)",
                            paddingTop: 10,
                            marginTop: 10,
                          }}
                        >
                          {denyApproval?.id === approval.id &&
                          pendingApprovals.some(
                            (entry) => entry.id === denyApproval.id,
                          ) ? (
                            <>
                              <Textarea
                                autoFocus
                                value={denyNote}
                                onChange={(event) =>
                                  setDenyNote(event.target.value)
                                }
                                placeholder="Why deny?"
                                rows={2}
                                size="sm"
                              />
                              <div
                                style={{
                                  display: "flex",
                                  gap: 6,
                                }}
                              >
                                <Button
                                  size="sm"
                                  variant="subtle"
                                  className="tb-action-40"
                                  disabled={deciding}
                                  onClick={() => {
                                    setDenyApproval(null);
                                    setDenyNote("");
                                  }}
                                >
                                  Cancel
                                </Button>
                                <Button
                                  size="sm"
                                  color="red"
                                  className="tb-action-40"
                                  loading={deciding}
                                  disabled={deciding || !denyNote.trim()}
                                  onClick={() =>
                                    onDecideApproval?.(
                                      approval,
                                      "deny",
                                      denyNote.trim(),
                                    )
                                  }
                                >
                                  Deny with reason
                                </Button>
                              </div>
                            </>
                          ) : (
                            <div
                              style={{
                                display: "flex",
                                flexWrap: "wrap",
                                gap: 6,
                              }}
                            >
                              {approval.options.map((option) => (
                                <Button
                                  key={option.id}
                                  size="sm"
                                  className="tb-action-40"
                                  color={
                                    option.kind === "deny" ? "red" : "gray"
                                  }
                                  variant={
                                    option.kind === "deny"
                                      ? "outline"
                                      : option.kind === "allow_once"
                                        ? "filled"
                                        : "light"
                                  }
                                  loading={deciding}
                                  disabled={!onDecideApproval || deciding}
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
                      </Paper>
                    </li>
                  );
                })}
              </ul>
            )}
          </Tabs.Panel>

          <Tabs.Panel value="terminal">
            {threadId ? (
              <TerminalPanel threadId={threadId} />
            ) : (
              <EmptyState icon={Terminal} title="No thread open" compact />
            )}
          </Tabs.Panel>

          <Tabs.Panel value="comments">
            {threadId ? (
              <CommentsPanel threadId={threadId} canWrite={canWrite} />
            ) : (
              <EmptyState icon={MessageSquareText} title="No thread open" compact />
            )}
          </Tabs.Panel>

          <Tabs.Panel value="files">
            {threadId ? (
              <FilesPanel threadId={threadId} />
            ) : (
              <EmptyState icon={FileCode2} title="No thread open" compact />
            )}
          </Tabs.Panel>
        </ScrollArea>
      </Tabs>

      <Box
        visibleFrom="sm"
        style={{
          display: "flex",
          height: 28,
          flexShrink: 0,
          alignItems: "center",
          gap: 4,
          borderTop: "1px solid var(--mantine-color-default-border)",
          paddingInline: 12,
        }}
      >
        <Kbd>⇧Esc</Kbd>
        <Text size="xs" c="dimmed">
          to close
        </Text>
      </Box>
    </div>
  );
}

export function RightDrawer({
  open,
  desktop = false,
  phone = false,
  onClose,
  ...panelProps
}: RightDrawerProps) {
  if (!open) {
    return null;
  }

  const panel = <ReviewPanel {...panelProps} onClose={onClose} />;

  if (desktop) {
    return (
      <Box
        component="aside"
        aria-label="Right drawer"
        data-drawer
        h="100%"
        w={360}
        style={{
          flexShrink: 0,
          minWidth: 0,
          overflow: "hidden",
          borderLeft: "1px solid var(--mantine-color-default-border)",
        }}
      >
        {panel}
      </Box>
    );
  }

  return (
    <Drawer
      opened
      onClose={onClose}
      position="right"
      size={phone ? "100%" : 420}
      padding={0}
      withCloseButton={false}
      overlayProps={{ backgroundOpacity: 0.55, blur: 2 }}
      aria-label="Review panel"
      styles={{
        content: {
          display: "flex",
          flexDirection: "column",
          height: "100%",
          overflow: "hidden",
        },
        body: {
          display: "flex",
          flex: 1,
          flexDirection: "column",
          minHeight: 0,
          padding: 0,
        },
      }}
    >
      <Box component="aside" aria-label="Right drawer" data-drawer h="100%">
        {panel}
      </Box>
    </Drawer>
  );
}
