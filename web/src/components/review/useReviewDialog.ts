import { notifications } from "@mantine/notifications";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { createComment, getThreadBundle } from "../../api";
import type { ReviewVerdict } from "./ReviewVerdictForm";

export function useReviewDialog(
  threadId: string | null,
  opened: boolean,
  onClose: () => void,
) {
  const queryClient = useQueryClient();
  const [verdict, setVerdict] = useState<ReviewVerdict>("approve");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!opened) return;
    setVerdict("approve");
    setNote("");
    setBusy(false);
  }, [opened, threadId]);

  const bundle = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId as string),
    enabled: opened && Boolean(threadId),
    staleTime: 2_000,
  });
  const data = bundle.data?.bundle;
  const pending = (data?.approvals ?? []).filter(
    (approval) => approval.status === "pending",
  );

  const submit = async () => {
    if (!threadId) return;
    setBusy(true);
    const decision = verdict === "approve" ? "approved" : "changes requested";
    try {
      const body = `Review: ${decision}\n\n${note.trim()}`.trimEnd();
      await createComment(threadId, body, { kind: "thread" }, false);
      await queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
      notifications.show({
        message: `Review recorded: ${decision}`,
        color: verdict === "approve" ? "teal" : "orange",
      });
      setNote("");
      onClose();
    } catch (cause) {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not record review",
        color: "red",
      });
    } finally {
      setBusy(false);
    }
  };

  return {
    bundle,
    data,
    pending,
    verdict,
    setVerdict,
    note,
    setNote,
    busy,
    submit,
  };
}
