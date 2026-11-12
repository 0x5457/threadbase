import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Group,
  Loader,
  Modal,
  NavLink,
  ScrollArea,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import {
  IconAlertCircle,
  IconArrowUp,
  IconBrandGit,
  IconChevronRight,
  IconFolder,
  IconRefresh,
} from "@tabler/icons-react";
import { useCallback, useEffect, useState } from "react";
import {
  browseDirectories,
  createProject,
  type DirectoryListing,
  type Project,
} from "../api";

interface AddProjectModalProps {
  opened: boolean;
  onClose: () => void;
  onAdded?: (project: Project) => void;
}

export function AddProjectModal({
  opened,
  onClose,
  onAdded,
}: AddProjectModalProps) {
  const phone = useMediaQuery("(max-width: 48em)");
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [pathInput, setPathInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (path?: string) => {
    setLoading(true);
    setError(null);
    try {
      const next = await browseDirectories(path);
      setListing(next);
      setPathInput(next.path);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Cannot read directory");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!opened) return;
    setListing(null);
    setError(null);
    void load();
  }, [opened, load]);

  const add = async () => {
    if (!listing?.isGit) return;
    setAdding(true);
    setError(null);
    try {
      const { project } = await createProject(listing.path);
      notifications.show({
        message: `${project.name} added`,
        color: "teal",
      });
      onAdded?.(project);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Cannot add project");
    } finally {
      setAdding(false);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Add project"
      size="md"
      centered
      fullScreen={phone}
    >
      <Group gap="xs" wrap="nowrap" mb="xs">
        <TextInput
          flex={1}
          value={pathInput}
          onChange={(event) => setPathInput(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void load(pathInput);
          }}
          placeholder="/home/you/work/project"
          leftSection={<IconFolder size={14} stroke={1.5} />}
          rightSection={
            <Tooltip label="Open path">
              <ActionIcon
                size={20}
                aria-label="Open path"
                onClick={() => void load(pathInput)}
              >
                <IconChevronRight size={13} stroke={1.5} />
              </ActionIcon>
            </Tooltip>
          }
          aria-label="Directory path"
        />
        {listing?.parent ? (
          <Tooltip label="Parent directory">
            <ActionIcon
              size={30}
              variant="default"
              aria-label="Parent directory"
              onClick={() => void load(listing.parent ?? undefined)}
            >
              <IconArrowUp size={14} stroke={1.5} />
            </ActionIcon>
          </Tooltip>
        ) : null}
        <Tooltip label="Refresh">
          <ActionIcon
            size={30}
            variant="default"
            aria-label="Refresh"
            onClick={() => void load(listing?.path)}
          >
            <IconRefresh size={14} stroke={1.5} />
          </ActionIcon>
        </Tooltip>
      </Group>

      <Group gap={6} mb={6} wrap="nowrap">
        <Text size="sm" c="dimmed" className="picker-path">
          {listing?.path ?? "Choose a directory on this machine"}
        </Text>
        {listing?.isGit ? (
          <Badge
            variant="light"
            color="teal"
            leftSection={<IconBrandGit size={10} stroke={1.5} />}
          >
            git
          </Badge>
        ) : null}
      </Group>

      {error ? (
        <Alert
          icon={<IconAlertCircle size={14} />}
          color="red"
          mb="xs"
          title="Cannot open directory"
        >
          {error}
        </Alert>
      ) : null}

      <Box className="picker-list">
        {loading ? (
          <Group justify="center" py="xl">
            <Loader size="sm" />
          </Group>
        ) : (
          <ScrollArea h={phone ? "calc(100dvh - 300px)" : 280} type="hover">
            {listing?.parent ? (
              <NavLink
                component="button"
                className="side-nav picker-row"
                label=".."
                leftSection={<IconArrowUp size={14} stroke={1.5} />}
                onClick={() => void load(listing.parent ?? undefined)}
              />
            ) : null}
            {listing?.entries.map((entry) => (
              <NavLink
                key={entry.path}
                component="button"
                className="side-nav picker-row"
                label={entry.name}
                description={entry.isGit ? "git repository" : undefined}
                leftSection={
                  entry.isGit ? (
                    <IconBrandGit size={14} stroke={1.5} />
                  ) : (
                    <IconFolder size={14} stroke={1.5} />
                  )
                }
                rightSection={<IconChevronRight size={12} stroke={1.5} />}
                onClick={() => void load(entry.path)}
              />
            ))}
            {listing && listing.entries.length === 0 ? (
              <Text size="sm" c="dimmed" ta="center" py="xl">
                No subdirectories
              </Text>
            ) : null}
          </ScrollArea>
        )}
      </Box>

      <Group justify="space-between" mt="md" wrap="nowrap">
        <Text size="xs" c="dimmed">
          Projects must be git repositories.
        </Text>
        <Group gap="xs" wrap="nowrap">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button
            color="dark"
            onClick={() => void add()}
            loading={adding}
            disabled={!listing?.isGit || loading}
          >
            Use this directory
          </Button>
        </Group>
      </Group>
    </Modal>
  );
}
