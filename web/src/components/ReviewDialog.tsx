import {
  Avatar,
  Badge,
  Box,
  Button,
  Card,
  Group,
  Loader,
  Modal,
  SegmentedControl,
  Stack,
  Text,
  Textarea,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import {
  IconAlertTriangle,
  IconArrowsDiff,
  IconFileDiff,
  IconMessageCircle,
} from "@tabler/icons-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState, type ReactNode } from "react";
import {
  createComment,
  getThreadBundle,
  type Approval,
  type Comment,
  type ThreadChanges,
} from "../api";

const COMMENT_LIMIT = 6;

function initials(name: string) {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

function SectionHead({
  icon,
  label,
  count,
  trailing,
}: {
  icon: ReactNode;
  label: string;
  count?: number;
  trailing?: ReactNode;
}) {
  return (
    <div className="changes-head">
      {icon}
      <Text size="sm" fw={500}>
        {label}
      </Text>
      {typeof count === "number" ? (
        <Badge size="xs" variant="light" color="gray">
          {count}
        </Badge>
      ) : null}
      <Box style={{ flex: 1 }} />
      {trailing}
    </div>
  );
}

function ChangesSummary({
  changes,
  onOpenChanges,
}: {
  changes: ThreadChanges;
  onOpenChanges?: () => void;
}) {
  return (
    <div className="changes">
      <SectionHead
        icon={<IconFileDiff size={13} stroke={1.5} />}
        label="Changes"
        trailing={
          <Group gap={8} wrap="nowrap">
            <Text size="sm" className="stat-add">
              +{changes.stats.additions}
            </Text>
            <Text size="sm" className="stat-del">
              −{changes.stats.deletions}
            </Text>
            {onOpenChanges ? (
              <Button
                size="compact-xs"
                variant="default"
                leftSection={<IconArrowsDiff size={12} stroke={1.5} />}
                onClick={onOpenChanges}
              >
                Open changes
              </Button>
            ) : null}
          </Group>
        }
      />
      <Text size="xs" c="dimmed" px={10} pt={6}>
        Since {changes.baseLabel} · {changes.files.length} file
        {changes.files.length === 1 ? "" : "s"}
      </Text>
      {changes.files.length === 0 ? (
        <Text size="sm" c="dimmed" px={10} py="sm">
          No file changes yet
        </Text>
      ) : (
        changes.files.map((file) => (
          <div key={file.path} className="file-head">
            <Text size="sm" className="dir" ff="monospace">
              {file.path}
            </Text>
            <Text size="xs" className="stat-add">
              +{file.additions}
            </Text>
            <Text size="xs" className="stat-del">
              −{file.deletions}
            </Text>
            <Text size="xs" c="dimmed">
              {file.status}
            </Text>
          </div>
        ))
      )}
    </div>
  );
}

function ApprovalSummary({ approvals }: { approvals: Approval[] }) {
  return (
    <div>
      <SectionHead
        icon={<IconAlertTriangle size={13} stroke={1.5} />}
        label="Pending approvals"
        count={approvals.length}
      />
      {approvals.length === 0 ? (
        <Text size="sm" c="dimmed" px={10} py="sm">
          Nothing waiting on you
        </Text>
      ) : (
        <Stack gap={4} p={6}>
          {approvals.map((approval) => (
            <Card key={approval.id} withBorder p={6} className="approval-card">
              <Group gap={6} wrap="nowrap">
                <Text size="sm" flex={1} lineClamp={1}>
                  {approval.title}
                </Text>
                <Badge size="xs" variant="light" color="orange">
                  {approval.kind}
                </Badge>
              </Group>
            </Card>
          ))}
        </Stack>
      )}
    </div>
  );
}

function CommentSummary({ comments }: { comments: Comment[] }) {
  const visible = comments.slice(-COMMENT_LIMIT);
  return (
    <div>
      <SectionHead
        icon={<IconMessageCircle size={13} stroke={1.5} />}
        label="Comments"
        count={comments.length}
        trailing={
          comments.length > visible.length ? (
            <Text size="xs" c="dimmed">
              latest {visible.length} of {comments.length}
            </Text>
          ) : null
        }
      />
      {visible.length === 0 ? (
        <Text size="sm" c="dimmed" px={10} py="sm">
          No comments yet
        </Text>
      ) : (
        <div>
          {visible.map((comment) => (
            <div key={comment.id} className="comment-row">
              <Card withBorder p={6} className="comment-card">
                <Group gap={6} align="flex-start" wrap="nowrap">
                  <Avatar
                    size={14}
                    radius="xl"
                    color="brand"
                    src={comment.author.avatarUrl ?? undefined}
                  >
                    {initials(comment.author.displayName)}
                  </Avatar>
                  <div className="comment-text">
                    <Text size="sm" fw={500} component="span">
                      {comment.author.displayName}
                    </Text>
                    <Text
                      size="sm"
                      mt={2}
                      style={{ whiteSpace: "pre-wrap" }}
                    >
                      {comment.body}
                    </Text>
                  </div>
                </Group>
              </Card>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function ReviewDialog({
  threadId,
  opened,
  onClose,
  onOpenChanges,
}: {
  threadId: string | null;
  opened: boolean;
  onClose: () => void;
  onOpenChanges?: () => void;
}) {
  const phone = useMediaQuery("(max-width: 48em)");
  const queryClient = useQueryClient();
  const [verdict, setVerdict] = useState<"approve" | "request_changes">(
    "approve",
  );
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!opened) return;
    setVerdict("approve");
    setNote("");
    setBusy(false);
  }, [opened, threadId]);

  const bundle = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId as string),
    enabled: opened && Boolean(threadId),
    staleTime: 2_000,
  });
  const data = bundle.data?.bundle;
  const pending = (data?.approvals ?? []).filter(
    (approval) => approval.status === "pending",
  );

  const submit = async () => {
    if (!threadId) return;
    setBusy(true);
    const decision = verdict === "approve" ? "approved" : "changes requested";
    try {
      const body = `Review: ${decision}\n\n${note.trim()}`.trimEnd();
      await createComment(threadId, body, { kind: "thread" }, false);
      await queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
      notifications.show({
        message: `Review recorded: ${decision}`,
        color: verdict === "approve" ? "teal" : "orange",
      });
      setNote("");
      onClose();
    } catch (cause) {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not record review",
        color: "red",
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Review thread"
      size="lg"
      centered
      fullScreen={phone}
    >
      {bundle.isPending && threadId ? (
        <Group justify="center" py="xl">
          <Loader size="sm" />
        </Group>
      ) : bundle.isError ? (
        <Text size="sm" c="red">
          {bundle.error instanceof Error
            ? bundle.error.message
            : "Cannot load thread"}
        </Text>
      ) : data ? (
        <Stack gap="md">
          <ChangesSummary changes={data.changes} onOpenChanges={onOpenChanges} />
          <ApprovalSummary approvals={pending} />
          <CommentSummary comments={data.comments} />

          <div>
            <SectionHead label="Verdict" icon={null} />
            <Stack gap="xs" p={6}>
              <SegmentedControl
                size="xs"
                fullWidth
                aria-label="Review verdict"
                value={verdict}
                onChange={(value) =>
                  setVerdict(value as "approve" | "request_changes")
                }
                data={[
                  { label: "Approve", value: "approve" },
                  { label: "Request changes", value: "request_changes" },
                ]}
              />
              <Textarea
                label="Review note"
                description="Recorded as a thread comment for the agent."
                autosize
                minRows={2}
                maxRows={6}
                value={note}
                onChange={(event) => setNote(event.currentTarget.value)}
              />
              <Group justify="flex-end">
                <Button variant="default" onClick={onClose} disabled={busy}>
                  Cancel
                </Button>
                <Button
                  color="dark"
                  loading={busy}
                  disabled={!threadId || busy}
                  onClick={() => void submit()}
                >
                  Submit review
                </Button>
              </Group>
            </Stack>
          </div>
        </Stack>
      ) : (
        <Text size="sm" c="dimmed">
          Open a thread to review it
        </Text>
      )}
    </Modal>
  );
}
