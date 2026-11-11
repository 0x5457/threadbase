import {
  Badge,
  Group,
  Kbd,
  Skeleton,
  Text,
  VisuallyHidden,
} from "@mantine/core";
import { createSpotlight, Spotlight } from "@mantine/spotlight";
import { CornerDownLeft } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useWorkspaceSearch } from "@/api/queries";
import type { SearchResultDto } from "@/api/types/SearchResultDto";
import { ErrorState } from "@/components/app";
import { useDebouncedValue } from "@/lib/hooks";
import { ApiError } from "@/lib/api";

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function resultPath(result: SearchResultDto): string | null {
  if (result.projectId && result.threadId) {
    return `/p/${encodeURIComponent(result.projectId)}/t/${encodeURIComponent(
      result.threadId,
    )}`;
  }
  if (result.projectId) {
    return `/p/${encodeURIComponent(result.projectId)}`;
  }
  return null;
}

function resultIconLabel(kind: string): string {
  switch (kind) {
    case "project":
      return "Project";
    case "thread":
      return "Thread";
    case "file":
      return "File";
    case "comment":
      return "Comment";
    case "message":
      return "Message";
    default:
      return kind;
  }
}

export function CommandPalette({ open, onOpenChange }: CommandPaletteProps) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const debouncedQuery = useDebouncedValue(query, 180);
  const search = useWorkspaceSearch(debouncedQuery);
  const trimmed = query.trim();
  const searching = trimmed.length > 0 && debouncedQuery !== query;
  const [store, controls] = useMemo(() => createSpotlight(), []);

  useEffect(() => {
    if (open) {
      controls.open();
    } else {
      controls.close();
    }
  }, [open, controls]);

  const closePalette = () => {
    setQuery("");
    onOpenChange(false);
  };

  const select = (result: SearchResultDto) => {
    const path = resultPath(result);
    if (!path) {
      return;
    }
    closePalette();
    navigate(path);
  };

  const results = useMemo(() => {
    if (searching || search.isLoading || search.isError) {
      return [];
    }
    if (trimmed.length === 0) {
      return [];
    }
    return search.data?.results ?? [];
  }, [searching, search.isLoading, search.isError, trimmed, search.data]);

  const nothingFound = searching || search.isLoading ? (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 6,
        padding: 6,
      }}
    >
      <Skeleton height={32} />
      <Skeleton height={32} />
      <Skeleton height={32} width="75%" />
    </div>
  ) : search.isError ? (
    <ErrorState
      error={search.error}
      title="Search failed"
      description={
        search.error instanceof ApiError
          ? search.error.message
          : undefined
      }
      compact
    />
  ) : trimmed.length === 0 ? (
    <div
      style={{
        padding: "24px 12px",
        textAlign: "center",
      }}
    >
      <Text size="xs" c="dimmed">
        Search across projects, threads and files.
      </Text>
      <Text size="xs" c="dimmed" mt={4}>
        Results are requested from the workspace search API.
      </Text>
    </div>
  ) : (
    <Text
      size="xs"
      c="dimmed"
      style={{
        padding: "24px 12px",
        textAlign: "center",
      }}
    >
      No results for “{trimmed}”.
    </Text>
  );

  return (
    <Spotlight.Root
      store={store}
      shortcut={null}
      query={query}
      onQueryChange={setQuery}
      title={<VisuallyHidden>Search Threadbase</VisuallyHidden>}
      onSpotlightClose={() => onOpenChange(false)}
      clearQueryOnClose
    >
      <Spotlight.Search
        autoFocus
        size="md"
        placeholder="Search projects, threads and files…"
      />
      <VisuallyHidden role="status" aria-live="polite">
        {searching || search.isLoading
          ? "Searching…"
          : search.isError
            ? "Search failed"
            : trimmed.length === 0
              ? "Type to search"
              : `${results.length} result${results.length === 1 ? "" : "s"}`}
      </VisuallyHidden>
      {results.length > 0 ? (
        <Spotlight.ActionsList>
          {results.map((result) => {
            const path = resultPath(result);
            return (
              <Spotlight.Action
                key={`${result.kind}:${result.id}`}
                id={`${result.kind}:${result.id}`}
                label={result.title}
                description={result.subtitle ?? undefined}
                disabled={!path}
                leftSection={
                  <Badge variant="outline" size="sm">
                    {resultIconLabel(result.kind)}
                  </Badge>
                }
                rightSection={<CornerDownLeft size={12} aria-hidden />}
                onClick={() => select(result)}
              />
            );
          })}
        </Spotlight.ActionsList>
      ) : (
        <Spotlight.Empty>{nothingFound}</Spotlight.Empty>
      )}
      <Spotlight.Footer>
        <Group
          justify="space-between"
          px="sm"
          py="xs"
          gap="xs"
          wrap="nowrap"
        >
          <Group gap="xs" wrap="nowrap">
            <Group gap={4} wrap="nowrap">
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd>
              <Text size="xs" c="dimmed">
                navigate
              </Text>
            </Group>
            <Group gap={4} wrap="nowrap">
              <Kbd>↵</Kbd>
              <Text size="xs" c="dimmed">
                open
              </Text>
            </Group>
          </Group>
          <Text size="xs" c="dimmed">
            Workspace search
          </Text>
        </Group>
      </Spotlight.Footer>
    </Spotlight.Root>
  );
}
