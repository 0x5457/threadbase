import { notifications } from "@mantine/notifications";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { setThreadPlacement } from "../api";
import type { ThreadSummary } from "../api";

/** The message to show, and the command that undoes it. */
const COPY: Record<
  string,
  { message: string; undo?: string; undoLabel?: string }
> = {
  settle: { message: "Thread settled", undo: "unsettle", undoLabel: "Un-settle" },
  unsettle: { message: "Thread back in the active list" },
  snooze: { message: "Thread snoozed for a day", undo: "wake", undoLabel: "Wake" },
  wake: { message: "Thread woken" },
};

/**
 * Moves a thread between sections, refreshes the lists, and offers an undo.
 *
 * These actions are reversible, so a short toast replaces a confirmation
 * dialog.
 */
export function useThreadPlacement() {
  const queryClient = useQueryClient();
  const place = useCallback(
    async (thread: ThreadSummary, action: string, until?: number | null) => {
      try {
        await setThreadPlacement(thread.id, action, until);
      } catch (cause) {
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not move the thread",
          color: "red",
        });
        return;
      }
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox-unread"] });
      void queryClient.invalidateQueries({ queryKey: ["thread", thread.id] });
      const copy = COPY[action];
      notifications.show({
        color: "teal",
        message: copy?.undo ? (
          <span className="undo-toast">
            {copy.message}
            <button
              type="button"
              className="undo-toast-action"
              onClick={() => void place(thread, copy.undo!)}
            >
              {copy.undoLabel ?? "Undo"}
            </button>
          </span>
        ) : (
          (copy?.message ?? "Thread moved")
        ),
      });
    },
    [queryClient],
  );
  return place;
}
