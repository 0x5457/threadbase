import { notifications } from "@mantine/notifications";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { setThreadPlacement } from "../api";
import type { ThreadSummary } from "../api";

/** What undoing a parking command means, and what to tell the person. */
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
 * Move a thread between shelves, refresh the lists and offer the way back.
 *
 * Every command here is reversible, so the notification carries the inverse
 * command instead of an "are you sure" dialog: settling is the kind of thing a
 * person does while scrolling, and a modal would be worse than a 5-second undo.
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
