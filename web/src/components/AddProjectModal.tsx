import {
  ActionIcon,
  Badge,
  Box,
  Button,
  Group,
  Modal,
  NavLink,
  ScrollArea,
  SegmentedControl,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import {
  IconArrowUp,
  IconBrandGit,
  IconChevronRight,
  IconFolder,
  IconRefresh,
} from "@tabler/icons-react";
import { join } from "pathe";
import { useCallback, useEffect, useState } from "react";
import {
  browseDirectories,
  createProject,
  type DirectoryListing,
  type Project,
} from "../api";
import { isImeConfirming } from "../lib/keyboard";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import { ListSkeleton } from "./loading/Skeleton";
import { LoadError } from "./loading/LoadError";

interface AddProjectModalProps {
  opened: boolean;
  onClose: () => void;
  onAdded?: (project: Project) => void;
}

type Mode = "existing" | "new";

/** A folder name, not a path: the parent comes from the picker. */
function validProjectName(name: string) {
  return (
    name.length > 0 &&
    name !== "." &&
    name !== ".." &&
    !name.startsWith(".") &&
    !/[/\\\0]/.test(name)
  );
}

export function AddProjectModal({
  opened,
  onClose,
  onAdded,
}: AddProjectModalProps) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [pathInput, setPathInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [adding, setAdding] = useState(false);
  const showSkeleton = useDelayedFlag(loading, 300, 500);
  const showAdding = useDelayedFlag(adding, 300, 500);
  const [mode, setMode] = useState<Mode>("existing");
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<{ title: string; message: string } | null>(
    null,
  );

  const load = useCallback(async (path?: string) => {
    setLoading(true);
    setError(null);
    try {
      const next = await browseDirectories(path);
      setListing(next);
      setPathInput(next.path);
    } catch (cause) {
      setError({
        title: "Cannot open directory",
        message:
          cause instanceof Error ? cause.message : "Cannot read directory",
      });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!opened) return;
    setListing(null);
    setError(null);
    setMode("existing");
    setNewName("");
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
      setError({
        title: "Cannot add project",
        message: cause instanceof Error ? cause.message : "Cannot add project",
      });
    } finally {
      setAdding(false);
    }
  };

  const createNew = async () => {
    const parent = listing?.path;
    const name = newName.trim();
    if (!parent || !validProjectName(name)) {
      setError({
        title: "Cannot create project",
        message: "Enter a folder name without slashes or a leading dot.",
      });
      return;
    }
    setAdding(true);
    setError(null);
    try {
      const { project } = await createProject(join(parent, name), {
        name,
        create: true,
      });
      notifications.show({
        message: `${project.name} created`,
        color: "teal",
      });
      onAdded?.(project);
      onClose();
    } catch (cause) {
      setError({
        title: "Cannot create project",
        message:
          cause instanceof Error ? cause.message : "Cannot create project",
      });
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
      closeOnClickOutside={!adding}
      closeOnEscape={!adding}
      withCloseButton={!adding}
    >
      <SegmentedControl
        fullWidth
        size="xs"
        mb="sm"
        value={mode}
        onChange={(value) => {
          setMode(value as Mode);
          setError(null);
        }}
        data={[
          { label: "Existing repository", value: "existing" },
          { label: "New project", value: "new" },
        ]}
      />

      <Group gap="xs" wrap="nowrap" mb="xs">
        <TextInput
          flex={1}
          value={pathInput}
          onChange={(event) => setPathInput(event.currentTarget.value)}
          onKeyDown={(event) => {
            // Enter is also how a CJK IME commits a candidate; submitting here
            // would browse a half-typed path.
            if (event.key !== "Enter" || isImeConfirming(event)) return;
            event.preventDefault();
            void load(pathInput);
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
          aria-label={mode === "new" ? "Parent directory" : "Directory path"}
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
        <Box mb="xs">
          <LoadError
            title={error.title}
            message={error.message}
            onRetry={() => void load(pathInput || listing?.path)}
            compact
          />
        </Box>
      ) : null}

      <Box className="picker-list">
        {loading && showSkeleton ? (
          <ListSkeleton
            label="Loading folders"
            rows={7}
            avatar
            avatarSize={14}
            lines={1}
            className="picker-skeleton"
          />
        ) : (
          <ScrollArea
            h={phone ? "calc(100dvh - 340px)" : mode === "new" ? 220 : 280}
            type="hover"
          >
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

      {mode === "new" ? (
        <Box mt="sm">
          <TextInput
            label="Project name"
            placeholder="my-project"
            value={newName}
            onChange={(event) => setNewName(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || isImeConfirming(event)) return;
              event.preventDefault();
              void createNew();
            }}
            data-autofocus
            aria-label="Project name"
          />
          <Text size="xs" c="dimmed" mt={4} className="picker-path">
            {listing
              ? join(listing.path, newName.trim() || "…")
              : "Choose a parent directory"}
          </Text>
        </Box>
      ) : null}

      <Group justify="space-between" mt="md" wrap="nowrap">
        <Text size="xs" c="dimmed">
          {mode === "new"
            ? "Creates the folder, runs git init and commits a README."
            : "Projects must be git repositories."}
        </Text>
        <Group gap="xs" wrap="nowrap">
          <Button variant="default" onClick={onClose} disabled={adding}>
            Cancel
          </Button>
          <Button
            color="dark"
            onClick={() => void (mode === "new" ? createNew() : add())}
            loading={showAdding}
            disabled={
              adding ||
              loading ||
              (mode === "new"
                ? !listing || !validProjectName(newName.trim())
                : !listing?.isGit)
            }
          >
            {mode === "new" ? "Create project" : "Use this directory"}
          </Button>
        </Group>
      </Group>
    </Modal>
  );
}
