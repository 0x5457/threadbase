import { ChevronLeft, Folder, FolderGit2, FolderPlus } from "lucide-react";
import {
  ActionIcon,
  Badge,
  Box,
  Button,
  Group,
  Loader,
  Modal,
  ScrollArea,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { createProject } from "@/api/endpoints";
import { queryKeys, useDirectoryListing } from "@/api/queries";
import type { ProjectDto } from "@/api/types/ProjectDto";
import { describeError } from "@/lib/errors";
import { useDebouncedValue } from "@/lib/hooks";
import { basename } from "@/lib/format";

export interface NewProjectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: (project: ProjectDto) => void;
}

export function NewProjectDialog({
  open,
  onOpenChange,
  onCreated,
}: NewProjectDialogProps) {
  const queryClient = useQueryClient();
  const [path, setPath] = useState("");
  const [active, setActive] = useState(0);
  const debounced = useDebouncedValue(path, 180);
  const listing = useDirectoryListing(debounced);
  const input = useRef<HTMLInputElement>(null);
  const initialized = useRef(false);
  const mutation = useMutation({
    mutationFn: () => createProject({ path: path.trim(), name: null }),
    onSuccess: (response) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.projects });
      initialized.current = false;
      onCreated(response.project);
      onOpenChange(false);
    },
  });

  useEffect(() => {
    if (!open) {
      initialized.current = false;
      return;
    }
    if (!initialized.current && listing.data) {
      initialized.current = true;
      setPath(listing.data.path);
    }
  }, [open, listing.data]);

  const entries = listing.data?.entries ?? [];
  const choices = [
    ...(listing.data?.parent
      ? [{ name: "..", path: listing.data.parent, isGit: false }]
      : []),
    ...entries,
  ];
  const isRepository = listing.data?.isGit ?? false;
  const selected = choices[active];

  const go = (next: string) => {
    setPath(next);
    setActive(0);
    window.setTimeout(() => input.current?.focus(), 0);
  };

  const submit = () => {
    if (path.trim()) {
      mutation.mutate();
    }
  };

  return (
    <Modal
      opened={open}
      onClose={() => onOpenChange(false)}
      title="Add project"
      size="sm"
    >
      <Stack gap="xs">
        <Group gap="xs" align="flex-start">
          {listing.data?.parent ? (
            <ActionIcon
              variant="default"
              size="md"
              className="tb-hit-40"
              aria-label="Parent directory"
              onClick={() => go(listing.data?.parent ?? "")}
            >
              <ChevronLeft size={14} aria-hidden />
            </ActionIcon>
          ) : (
            <Box w={28} h={28} style={{ flexShrink: 0 }} />
          )}
          <TextInput
            ref={input}
            autoFocus
            value={path}
            onChange={(event) => {
              setPath(event.target.value);
              setActive(0);
            }}
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setActive((value) => Math.min(value + 1, choices.length - 1));
              } else if (event.key === "ArrowUp") {
                event.preventDefault();
                setActive((value) => Math.max(value - 1, 0));
              } else if (event.key === "ArrowRight") {
                event.preventDefault();
                if (selected) {
                  go(selected.path);
                }
              } else if (event.key === "Enter") {
                event.preventDefault();
                // An explicit repository path wins over the hovered entry:
                // Enter should add the repo the user typed, not walk to `..`.
                if (isRepository) {
                  submit();
                } else if (selected) {
                  go(selected.path);
                }
              }
            }}
            aria-label="Project path"
            placeholder="/home/you/code"
            spellCheck={false}
            size="sm"
            ff="var(--mantine-font-family-monospace)"
          />
        </Group>

        {isRepository ? (
          <Button
            variant="light"
            color="blue"
            size="sm"
            fullWidth
            justify="flex-start"
            leftSection={<FolderGit2 size={14} aria-hidden />}
            onClick={submit}
          >
            <Group gap="xs" wrap="nowrap" style={{ flex: 1 }}>
              <Text size="sm" truncate="end" style={{ flex: 1 }}>
                Use {basename(path)}
              </Text>
              <Badge variant="light" color="blue" size="xs">
                git
              </Badge>
            </Group>
          </Button>
        ) : null}

        {listing.isError ? (
          <Text size="sm" c="red" px="xs">
            {describeError(listing.error, "Cannot open directory").description}
          </Text>
        ) : listing.isLoading ? (
          <Box ta="center" py="xl">
            <Loader size="sm" color="gray" />
          </Box>
        ) : (
          <ScrollArea h={256} type="hover">
            <Stack gap={2} py="xs">
              {choices.map((entry, index) => (
                <Button
                  key={`${entry.path}:${index}`}
                  variant="subtle"
                  color="gray"
                  size="sm"
                  justify="flex-start"
                  fullWidth
                  data-dir-active={index === active}
                  bg={
                    index === active
                      ? "var(--mantine-color-default-hover)"
                      : undefined
                  }
                  leftSection={
                    entry.isGit ? (
                      <FolderGit2 size={14} aria-hidden />
                    ) : (
                      <Folder size={14} aria-hidden />
                    )
                  }
                  rightSection={
                    entry.isGit ? (
                      <Badge variant="outline" color="gray" size="xs">
                        git
                      </Badge>
                    ) : undefined
                  }
                  onMouseEnter={() => setActive(index)}
                  onClick={() => go(entry.path)}
                >
                  <Text size="sm" truncate="end">
                    {entry.name}
                  </Text>
                </Button>
              ))}
              {choices.length === 0 ? (
                <Group gap="xs" px="xs" py="xs">
                  <FolderPlus size={14} aria-hidden />
                  <Text size="sm" c="dimmed">
                    No subdirectories
                  </Text>
                </Group>
              ) : null}
            </Stack>
          </ScrollArea>
        )}

        {mutation.isError ? (
          <Text size="sm" c="red" role="alert" px="xs">
            {describeError(mutation.error, "Cannot add project").description}
          </Text>
        ) : null}
      </Stack>

      <Group justify="flex-end" gap="xs" mt="sm">
        <Button variant="default" size="sm" onClick={() => onOpenChange(false)}>
          Cancel
        </Button>
        <Button
          variant="filled"
          size="sm"
          loading={mutation.isPending}
          disabled={!path.trim()}
          onClick={submit}
        >
          Add {path.trim() ? basename(path) : "project"}
        </Button>
      </Group>
    </Modal>
  );
}
