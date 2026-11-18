import { Alert, Button, Loader, Text, Textarea } from "@mantine/core";
import { useEffect, useMemo, useRef, useState } from "react";
import type { EventEnvelope } from "../../api/types/EventEnvelope";
import type { GuestThreadResponse } from "../../api/types/GuestThreadResponse";
import { getGuestThread, sendGuestDraft, startGuestSession } from "../../api";
import {
  createThreadView,
  extendThreadView,
  type ThreadViewState,
} from "../../lib/thread-events";
import { Message } from "../thread/Message";
import { Tool } from "../thread/Tool";

type Phase =
  | { kind: "loading" }
  | { kind: "ready"; thread: GuestThreadResponse }
  | { kind: "error"; message: string };

/**
 * The guest's whole product surface: one thread's transcript, and a draft that a
 * person has to send. There is no nav, no diff, no file tree, no terminal — a
 * link is a door, not a seat.
 */
export function GuestThread({ token }: { token: string }) {
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [events, setEvents] = useState<EventEnvelope[]>([]);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const cursor = useRef(0);
  const scroller = useRef<HTMLDivElement | null>(null);

  // The token in the URL is exchanged once for an HttpOnly cookie, so later
  // polling does not carry the capability in a query string.
  useEffect(() => {
    let cancelled = false;
    void startGuestSession(token)
      .then((thread) => {
        if (cancelled) return;
        setPhase({ kind: "ready", thread });
        setEvents(thread.page.events);
        cursor.current = thread.page.lastSeq;
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        setPhase({
          kind: "error",
          message:
            cause instanceof Error ? cause.message : "This link is not valid",
        });
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  // Poll for new events. A guest has no WebSocket: the live path is
  // member-only, and one GET every two seconds is cheap next to a stream.
  useEffect(() => {
    if (phase.kind !== "ready") return;
    const timer = window.setInterval(() => {
      void getGuestThread(cursor.current)
        .then((thread) => {
          if (thread.page.events.length === 0) {
            cursor.current = Math.max(cursor.current, thread.page.lastSeq);
            return;
          }
          setEvents((current) => [...current, ...thread.page.events]);
          cursor.current = Math.max(cursor.current, thread.page.lastSeq);
        })
        .catch(() => {
          setNotice("Updates stopped. Reload to reconnect.");
        });
    }, 2_000);
    return () => window.clearInterval(timer);
  }, [phase.kind]);

  const view: ThreadViewState = useMemo(
    () =>
      extendThreadView(
        createThreadView(
          phase.kind === "ready" ? phase.thread.agent : "Agent",
        ),
        events,
      ),
    [events, phase],
  );

  useEffect(() => {
    const node = scroller.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [view.items.length]);

  if (phase.kind === "loading") {
    return (
      <div className="guest-shell">
        <Loader size="sm" />
      </div>
    );
  }
  if (phase.kind === "error") {
    return (
      <div className="guest-shell">
        <Alert color="red" title="This link is not valid">
          {phase.message}
        </Alert>
      </div>
    );
  }

  const expires = new Date(phase.thread.expiresAt);
  const submit = async () => {
    const text = draft.trim();
    if (!text || sending) return;
    setSending(true);
    setNotice(null);
    try {
      await sendGuestDraft(text);
      setDraft("");
      setSent(true);
    } catch (cause) {
      setNotice(
        cause instanceof Error ? cause.message : "Could not send the message",
      );
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="guest-shell">
      <header className="guest-head">
        <div>
          <Text size="sm" fw={500}>
            {phase.thread.title}
          </Text>
          <Text size="xs" c="dimmed">
            {phase.thread.model ?? phase.thread.agent} · shared thread · link
            expires {expires.toLocaleString()}
          </Text>
        </div>
      </header>
      <div className="guest-transcript" ref={scroller}>
        {view.items.map((item, index) => {
          switch (item.kind) {
            case "message":
              return (
                <Message
                  key={`m:${item.id}`}
                  item={item}
                  active={false}
                  onComment={() => {}}
                  onLongPress={() => {}}
                  showComment={false}
                />
              );
            case "tool":
              return (
                <Tool
                  key={`t:${item.call.id}`}
                  item={item}
                  expanded={false}
                  onToggleExpanded={() => {}}
                />
              );
            case "notice":
              return (
                <Text
                  key={`n:${index}`}
                  size="xs"
                  c={item.level === "error" ? "red" : "dimmed"}
                  ta="center"
                  py={4}
                >
                  {item.text}
                </Text>
              );
            case "thinking":
              return (
                <Text key={`th:${item.id}`} size="xs" c="dimmed" py={2}>
                  Thought for a moment · {item.tools} tool
                  {item.tools === 1 ? "" : "s"}
                </Text>
              );
            default:
              return null;
          }
        })}
      </div>
      <div className="guest-composer">
        {sent ? (
          <Text size="xs" c="dimmed">
            Sent. Someone on the team has to approve it before the agent runs.
          </Text>
        ) : null}
        {notice ? (
          <Text size="xs" c="red">
            {notice}
          </Text>
        ) : null}
        <Textarea
          autosize
          minRows={1}
          maxRows={6}
          placeholder="Ask a question about this thread…"
          value={draft}
          onChange={(event) => setDraft(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              void submit();
            }
          }}
        />
        <Button
          size="compact-sm"
          loading={sending}
          disabled={draft.trim().length === 0}
          onClick={() => void submit()}
        >
          Send for approval
        </Button>
      </div>
    </div>
  );
}
