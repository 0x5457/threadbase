import {
  ActionIcon,
  Drawer,
  Group,
  Loader,
  Modal,
  ScrollArea,
  Text,
  TextInput,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { useQuery } from "@tanstack/react-query";
import { IconSearch, IconX } from "@tabler/icons-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { listSessions, type SessionSummary } from "../../api";
import { sessionKey, sessionMatches } from "../../lib/sessions";
import { SheetGrabber } from "../SheetGrabber";
import { SessionRow } from "./SessionRow";

function SwitcherBody({
  opened,
  onClose,
  onOpenThread,
  onOpenThreadShell,
  onOpenTmux,
}: {
  opened: boolean;
  onClose: () => void;
  onOpenThread: (threadId: string) => void;
  onOpenThreadShell: (threadId: string) => void;
  onOpenTmux: (socket: string, id: string) => void;
}) {
  const [needle, setNeedle] = useState("");
  const [active, setActive] = useState(-1);
  const listRef = useRef<HTMLDivElement>(null);
  const sessions = useQuery({
    queryKey: ["sessions"],
    queryFn: listSessions,
    enabled: opened,
    staleTime: 2_000,
  });
  const filtered = useMemo(() => {
    const value = needle.trim().toLowerCase();
    return (sessions.data?.sessions ?? []).filter((session) =>
      sessionMatches(session, value),
    );
  }, [needle, sessions.data?.sessions]);
  const activeIndex =
    active < 0 || filtered.length === 0
      ? -1
      : Math.min(active, filtered.length - 1);

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>('[data-active="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  const open = (session: SessionSummary) => {
    if (session.kind === "thread") onOpenThread(session.id);
    else onOpenTmux(session.socket, session.id);
    onClose();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" && filtered.length > 0) {
      event.preventDefault();
      setActive(Math.min((activeIndex < 0 ? -1 : activeIndex) + 1, filtered.length - 1));
      return;
    }
    if (event.key === "ArrowUp" && filtered.length > 0) {
      event.preventDefault();
      setActive((current) => Math.max((current < 0 ? 0 : current) - 1, 0));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      const session = filtered[activeIndex < 0 ? 0 : activeIndex];
      if (session) open(session);
    }
  };

  return (
    <div className="session-switcher">
      <TextInput
        autoFocus
        aria-label="Search sessions to switch"
        placeholder="Search threads and tmux sessions"
        value={needle}
        onChange={(event) => {
          setNeedle(event.currentTarget.value);
          setActive(-1);
        }}
        onKeyDown={onKeyDown}
        leftSection={<IconSearch size={14} stroke={1.5} />}
        size="sm"
      />
      {sessions.isPending ? (
        <Group justify="center" py="xl">
          <Loader size="sm" />
        </Group>
      ) : sessions.isError ? (
        <Text size="sm" c="red" py="md">
          {sessions.error instanceof Error
            ? sessions.error.message
            : "Could not load sessions"}
        </Text>
      ) : filtered.length === 0 ? (
        <Text size="sm" c="dimmed" py="md">
          {needle.trim() ? "No matching sessions." : "No sessions yet."}
        </Text>
      ) : (
        <ScrollArea.Autosize mah="50vh" type="hover" className="session-switcher-list">
          <div ref={listRef}>
            {filtered.map((session, index) => (
              <SessionRow
                key={sessionKey(session)}
                session={session}
                compact
                active={index === activeIndex}
                onHover={() => setActive(index)}
                onOpen={() => open(session)}
                onOpenLinkedThread={(threadId) => {
                  onOpenThread(threadId);
                  onClose();
                }}
                onOpenShell={
                  session.kind === "thread"
                    ? () => {
                        onOpenThreadShell(session.id);
                        onClose();
                      }
                    : undefined
                }
              />
            ))}
          </div>
        </ScrollArea.Autosize>
      )}
      <Text size="xs" c="dimmed" className="session-switcher-hint">
        Threads open the transcript. Tmux opens a shared shell; take over the
        keyboard when you need it.
      </Text>
    </div>
  );
}

/**
 * Compact switcher over the same `sessions` query as the full surface. It is
 * a bottom sheet on phones (the terminal drawer is usually full-screen there)
 * and a centered modal on desktop.
 */
export function SessionSwitcher({
  opened,
  onClose,
  onOpenThread,
  onOpenThreadShell,
  onOpenTmux,
}: {
  opened: boolean;
  onClose: () => void;
  onOpenThread: (threadId: string) => void;
  onOpenThreadShell: (threadId: string) => void;
  onOpenTmux: (socket: string, id: string) => void;
}) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const body = (
    <SwitcherBody
      opened={opened}
      onClose={onClose}
      onOpenThread={onOpenThread}
      onOpenThreadShell={onOpenThreadShell}
      onOpenTmux={onOpenTmux}
    />
  );
  if (phone) {
    return (
      <Drawer
        opened={opened}
        onClose={onClose}
        position="bottom"
        size="min(560px, 78%)"
        padding={0}
        withCloseButton={false}
        className="session-switcher-sheet"
      >
        <div className="sheet-stack">
          <SheetGrabber onClose={onClose} />
          <div className="session-switcher-sheet-head">
            <Text size="sm" fw={500}>
              Switch session
            </Text>
            <ActionIcon aria-label="Close session switcher" onClick={onClose}>
              <IconX size={14} stroke={1.5} />
            </ActionIcon>
          </div>
          <div className="session-switcher-sheet-body">{body}</div>
        </div>
      </Drawer>
    );
  }
  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Switch session"
      size="md"
      centered
    >
      {body}
    </Modal>
  );
}
