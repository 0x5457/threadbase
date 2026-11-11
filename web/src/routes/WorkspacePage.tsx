import { AlertTriangle, FolderGit2, FolderPlus } from "lucide-react";
import {
  Box,
  Button,
  Group,
  ScrollArea,
  Skeleton,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { useOutletContext } from "react-router-dom";
import { useProjects, useWorkspaceThreads } from "@/api/queries";
import type { ProjectDto } from "@/api/types/ProjectDto";
import { EmptyState, ErrorState, PageContainer } from "@/components/app";
import { DataBadge, ListRow } from "@/components/ui";
import { formatRelativeTime } from "@/lib/format";
import { layout, fontSizes } from "@/lib/design";
import type { WorkspaceOutletContext } from "./workspace-context";

function ProjectRow({ project }: { project: ProjectDto }) {
  return (
    <ListRow
      href={`/p/${encodeURIComponent(project.id)}`}
      icon={<FolderGit2 size={layout.icon.md} aria-hidden />}
      meta={
        <Group gap="md" visibleFrom="sm" wrap="nowrap">
          <DataBadge tone="neutral">{project.activeThreads} threads</DataBadge>
          {project.waitingApprovals > 0 ? (
            <DataBadge tone="warning">
              {project.waitingApprovals} waiting
            </DataBadge>
          ) : null}
          <span
            className="tb-mono"
            style={{
              fontSize: fontSizes.xs,
              color: "var(--mantine-color-dimmed)",
            }}
          >
            {formatRelativeTime(project.lastActivityAt)}
          </span>
        </Group>
      }
    >
      <Box
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 1,
          minWidth: 0,
        }}
      >
        <Group gap={6} wrap="nowrap">
          <Text size="sm" fw={500} truncate="end">
            {project.name}
          </Text>
          {!project.available ? (
            <AlertTriangle
              size={layout.icon.sm}
              aria-label="Project path is unavailable"
              color="var(--mantine-color-yellow-filled)"
              style={{ flexShrink: 0 }}
            />
          ) : null}
        </Group>
        <Text
          size="xs"
          c="dimmed"
          truncate="end"
          className="tb-mono"
        >
          {project.path}
        </Text>
        {/* Narrow screens have no meta column, so keep the counts in the row. */}
        <Text size="xs" c="dimmed" truncate="end" hiddenFrom="sm">
          {project.activeThreads} threads
          {project.waitingApprovals > 0
            ? ` · ${project.waitingApprovals} waiting`
            : ""}
        </Text>
      </Box>
    </ListRow>
  );
}

export function WorkspacePage() {
  const { openNewProject } = useOutletContext<WorkspaceOutletContext>();
  const projects = useProjects();
  const threads = useWorkspaceThreads();
  const projectList = projects.data?.projects ?? [];

  return (
    <ScrollArea h="100%">
      <PageContainer>
        <Group justify="space-between" align="center" gap="sm">
          <Group gap="xs" wrap="nowrap">
            <FolderGit2 size={layout.icon.md} aria-hidden />
            <Title order={1}>Projects</Title>
          </Group>
          <Button
            variant="filled"
            className="tb-action-40"
            leftSection={<FolderPlus size={layout.icon.sm} aria-hidden />}
            onClick={openNewProject}
          >
            Add project
          </Button>
        </Group>

        <Box mt="lg">
          {projects.isLoading ? (
            <Stack gap="xs">
              {Array.from({ length: 4 }).map((_, index) => (
                <Skeleton key={index} h={layout.rowHeight} />
              ))}
            </Stack>
          ) : projects.isError ? (
            <ErrorState
              error={projects.error}
              title="Cannot load projects"
              onRetry={() => void projects.refetch()}
            />
          ) : projectList.length === 0 ? (
            <EmptyState icon={FolderPlus} title="No projects" compact />
          ) : (
            <Stack gap={0} mt="xs">
              {projectList.map((project) => (
                <ProjectRow key={project.id} project={project} />
              ))}
            </Stack>
          )}
          {threads.isError ? (
            <Text size="xs" c="yellow" mt="sm">
              {threads.error instanceof Error
                ? threads.error.message
                : "Thread counts unavailable."}
            </Text>
          ) : null}
        </Box>
      </PageContainer>
    </ScrollArea>
  );
}
