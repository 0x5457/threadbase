import {
  Archive,
  ChevronDown,
  ChevronLeft,
  Columns2,
  FolderGit2,
  GitBranch,
  MessageSquareText,
  Pin,
  Plus,
} from "lucide-react";
import {
  Anchor,
  Badge,
  Box,
  Button,
  Group,
  Menu,
  ScrollArea,
  Select,
  Skeleton,
  Stack,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import { useState } from "react";
import { Link, useNavigate, useOutletContext, useParams } from "react-router-dom";
import {
  useProject,
  useProjectOverview,
  useProjectThreads,
} from "@/api/queries";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { ThreadStatusBadge } from "@/components/thread/ThreadStatusBadge";
import {
  AvatarStack,
  EmptyState,
  ErrorState,
  PageContainer,
} from "@/components/app";
import { DataBadge, ListRow } from "@/components/ui";
import { formatRelativeTime } from "@/lib/format";
import { layout, fontSizes } from "@/lib/design";
import type {
  NewThreadOptions,
  WorkspaceOutletContext,
} from "./workspace-context";

function threadPath(thread: ThreadDto): string {
  return `/p/${encodeURIComponent(thread.projectId)}/t/${encodeURIComponent(
    thread.id,
  )}`;
}

const agentOptions = [
  { value: "default", label: "Default" },
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude Code" },
  { value: "opencode", label: "opencode" },
  { value: "pi", label: "pi" },
];

/** Single source for both the per-row and the summary diff readout. */
function DiffStat({
  additions,
  deletions,
}: {
  additions: number;
  deletions: number;
}) {
  return (
    <Group gap={4} wrap="nowrap" className="tb-mono" fz={fontSizes.xs}>
      <Text size="xs" c="green">
        +{additions}
      </Text>
      <Text size="xs" c="red">
        −{deletions}
      </Text>
    </Group>
  );
}

function ThreadRow({
  thread,
  subthreadCount,
}: {
  thread: ThreadDto;
  subthreadCount: number;
}) {
  const people = thread.participants.map((participant) => ({
    id: participant.user.id,
    displayName: participant.user.displayName,
    avatarUrl: participant.user.avatarUrl,
    color: participant.user.color,
  }));

  return (
    <ListRow
      href={threadPath(thread)}
      icon={<MessageSquareText size={layout.icon.sm} aria-hidden />}
      meta={
        <Group gap="sm" visibleFrom="sm" wrap="nowrap">
          <ThreadStatusBadge status={thread.status} />
          {thread.pendingApprovals > 0 ? (
            <DataBadge tone="warning">{thread.pendingApprovals}</DataBadge>
          ) : null}
          <DiffStat
            additions={thread.changes.additions}
            deletions={thread.changes.deletions}
          />
          {people.length > 0 ? (
            <AvatarStack people={people} max={3} size="xs" />
          ) : null}
        </Group>
      }
    >
      <Box style={{ display: "flex", flexDirection: "column", gap: 1, minWidth: 0 }}>
        <Group gap={6} wrap="nowrap">
          <Text size="sm" fw={500} truncate="end" style={{ flex: 1 }}>
            {thread.title}
          </Text>
          {thread.pinned ? (
            <Pin size={layout.icon.xs} aria-label="Pinned" style={{ flexShrink: 0 }} />
          ) : null}
        </Group>
        <Group gap="sm" wrap="nowrap">
          <Text size="xs" c="dimmed">
            {thread.agent}
          </Text>
          {subthreadCount > 0 ? (
            <Text size="xs" c="dimmed">
              {subthreadCount} subthreads
            </Text>
          ) : null}
          <Text size="xs" c="dimmed">
            {formatRelativeTime(thread.updatedAt)}
          </Text>
          <Group gap="xs" wrap="nowrap" hiddenFrom="sm">
            <ThreadStatusBadge status={thread.status} />
            <DiffStat
              additions={thread.changes.additions}
              deletions={thread.changes.deletions}
            />
          </Group>
        </Group>
      </Box>
    </ListRow>
  );
}

export function ProjectPage() {
  const { projectId } = useParams();
  const { openNewThread } = useOutletContext<WorkspaceOutletContext>();
  const navigate = useNavigate();
  const [showArchived, setShowArchived] = useState(false);
  const [newAgent, setNewAgent] = useState("");
  const [newModel, setNewModel] = useState("");
  const [newBranch, setNewBranch] = useState("");
  const [newThreadOpen, setNewThreadOpen] = useState(false);
  const project = useProject(projectId);
  const threads = useProjectThreads(projectId, showArchived);
  const overview = useProjectOverview(projectId);

  if (!projectId) {
    return (
      <Stack align="center" justify="center" h="100%" p="lg">
        <ErrorState
          title="Project not found"
          description="The URL does not contain a project id."
        />
      </Stack>
    );
  }

  if (project.isLoading) {
    return (
      <PageContainer>
        <Stack gap="sm">
          <Skeleton h={24} w={192} />
          <Skeleton h={16} w={288} />
          <Skeleton h={96} />
          <Skeleton h={160} />
        </Stack>
      </PageContainer>
    );
  }

  if (project.isError || !project.data) {
    return (
      <Stack align="center" justify="center" h="100%" p="lg">
        <ErrorState
          error={project.error}
          title="Cannot open project"
          onRetry={() => void project.refetch()}
          action={
            <Anchor component={Link} to="/" size="sm" c="dimmed">
              <Group gap={4}>
                <ChevronLeft size={layout.icon.xs} aria-hidden />
                Back to projects
              </Group>
            </Anchor>
          }
        />
      </Stack>
    );
  }

  const data = project.data.project;
  const threadList = threads.data?.threads ?? [];
  const rootThreads = threadList
    .filter(
      (thread) =>
        !thread.parentThreadId ||
        !threadList.some((candidate) => candidate.id === thread.parentThreadId),
    )
    .sort((a, b) => Number(b.updatedAt) - Number(a.updatedAt));
  const childCount = new Map<string, number>();
  for (const thread of threadList) {
    if (thread.parentThreadId) {
      childCount.set(
        thread.parentThreadId,
        (childCount.get(thread.parentThreadId) ?? 0) + 1,
      );
    }
  }
  const active = threadList.filter(
    (thread) => thread.status === "running" || thread.status === "preparing",
  ).length;
  const waiting = threadList.reduce(
    (total, thread) => total + thread.pendingApprovals,
    0,
  );
  const additions = threadList.reduce(
    (total, thread) => total + thread.changes.additions,
    0,
  );
  const deletions = threadList.reduce(
    (total, thread) => total + thread.changes.deletions,
    0,
  );

  return (
    <ScrollArea h="100%">
      <PageContainer>
        <Anchor component={Link} to="/" size="sm" c="dimmed">
          <Group gap={4}>
            <ChevronLeft size={layout.icon.xs} aria-hidden />
            Projects
          </Group>
        </Anchor>

        <Group
          justify="space-between"
          align="flex-start"
          wrap="wrap"
          gap="md"
          mt="sm"
        >
          <Group align="flex-start" gap="xs" style={{ minWidth: 0 }}>
            <FolderGit2
              size={layout.icon.md}
              aria-hidden
              style={{ flexShrink: 0, marginTop: 3 }}
            />
            <Stack gap={2} style={{ minWidth: 0 }}>
              <Group gap="xs" wrap="wrap">
                <Title order={1}>{data.name}</Title>
                {!data.available ? (
                  <Badge variant="light" color="red" size="xs">
                    path unavailable
                  </Badge>
                ) : null}
              </Group>
              <Text size="xs" c="dimmed" className="tb-mono" truncate="end">
                {data.path}
              </Text>
              <Group gap={4}>
                <GitBranch size={layout.icon.xs} aria-hidden />
                <Text size="xs" c="dimmed">
                  {data.defaultBranch}
                </Text>
              </Group>
            </Stack>
          </Group>
          <Group gap="xs" wrap="wrap" style={{ flexShrink: 0 }}>
            <Button
              variant="default"
              leftSection={<Columns2 size={layout.icon.sm} aria-hidden />}
              onClick={() =>
                navigate(`/p/${encodeURIComponent(projectId)}/compare`)
              }
            >
              Compare
            </Button>
            <Button
              variant="default"
              leftSection={<Archive size={layout.icon.sm} aria-hidden />}
              onClick={() => setShowArchived((value) => !value)}
            >
              {showArchived ? "Hide archived" : "Archived"}
            </Button>
            <Button.Group>
              <Button
                variant="filled"
                leftSection={<Plus size={layout.icon.sm} aria-hidden />}
                onClick={() => openNewThread(data.id)}
              >
                New thread
              </Button>
              <Menu
                opened={newThreadOpen}
                onChange={setNewThreadOpen}
                position="bottom-end"
                // The dropdown hosts a form (agent/model/branch). A nested
                // Select portals its options outside this dropdown, which
                // Mantine treats as an outside click and would unmount the
                // form mid-edit.
                closeOnClickOutside={false}
                closeOnItemClick={false}
              >
                <Menu.Target>
                  <Button
                    variant="filled"
                    aria-label="New thread options"
                    px="xs"
                  >
                    <ChevronDown size={layout.icon.sm} aria-hidden />
                  </Button>
                </Menu.Target>
                <Menu.Dropdown>
                  <Stack gap="sm" p="xs" w={256}>
                    <Select
                      label="Agent"
                      value={newAgent || "default"}
                      onChange={(value) =>
                        setNewAgent(value === "default" ? "" : (value ?? ""))
                      }
                      data={agentOptions}
                    />
                    <TextInput
                      label="Model"
                      value={newModel}
                      onChange={(event) => setNewModel(event.target.value)}
                      placeholder="Agent default"
                    />
                    <TextInput
                      label="Base branch"
                      value={newBranch}
                      onChange={(event) => setNewBranch(event.target.value)}
                      placeholder={data.defaultBranch}
                      className="tb-mono"
                    />
                    <Button
                      onClick={() => {
                        const options: NewThreadOptions = {
                          agent: newAgent || null,
                          model: newModel || null,
                          baseBranch: newBranch || null,
                        };
                        openNewThread(data.id, options);
                        setNewThreadOpen(false);
                      }}
                    >
                      Create
                    </Button>
                  </Stack>
                </Menu.Dropdown>
              </Menu>
            </Button.Group>
          </Group>
        </Group>

        <Group gap="md" mt="lg" wrap="wrap">
          <DataBadge tone="neutral">{threadList.length} threads</DataBadge>
          {active > 0 ? (
            <DataBadge tone="info" dot>
              {active} running
            </DataBadge>
          ) : null}
          {waiting > 0 ? (
            <DataBadge tone="warning" dot>
              {waiting} waiting
            </DataBadge>
          ) : null}
          {additions > 0 || deletions > 0 ? (
            <DiffStat additions={additions} deletions={deletions} />
          ) : null}
        </Group>

        {overview.data?.overview.recentActivity[0] ? (
          <Text size="xs" c="dimmed" mt="xs" truncate="end">
            Last: {overview.data.overview.recentActivity[0].actor.displayName}{" "}
            {overview.data.overview.recentActivity[0].summary} ·{" "}
            {formatRelativeTime(overview.data.overview.recentActivity[0].ts)}
          </Text>
        ) : null}

        <Box mt="md">
          {threads.isLoading ? (
            <Stack gap="xs">
              {Array.from({ length: 3 }).map((_, index) => (
                <Skeleton key={index} h={layout.rowHeight} />
              ))}
            </Stack>
          ) : threads.isError ? (
            <ErrorState
              error={threads.error}
              title="Cannot load threads"
              onRetry={() => void threads.refetch()}
            />
          ) : rootThreads.length === 0 ? (
            <EmptyState icon={MessageSquareText} title="No threads" compact />
          ) : (
            <Stack gap={0}>
              {rootThreads.map((thread) => (
                <ThreadRow
                  key={thread.id}
                  thread={thread}
                  subthreadCount={childCount.get(thread.id) ?? 0}
                />
              ))}
            </Stack>
          )}
        </Box>
      </PageContainer>
    </ScrollArea>
  );
}
