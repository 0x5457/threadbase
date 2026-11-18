import { ActionIcon, Group, Text, Tooltip } from "@mantine/core";
import { IconCheck, IconX } from "@tabler/icons-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { decideGuestMessage, getState, listGuestMessages } from "../../api";

/**
 * Drafts left by guests on this thread.
 *
 * A guest cannot start a turn, so a draft waits here until a member sends or
 * dismisses it. Sending runs it as the member who pressed Send.
 */
export function GuestDrafts({
  threadId,
  canWrite,
}: {
  threadId: string;
  canWrite: boolean;
}) {
  const queryClient = useQueryClient();
  const state = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    staleTime: 60_000,
  });
  const enabled = state.data?.guestLinks ?? false;
  const pending = useQuery({
    queryKey: ["guest-messages", threadId],
    queryFn: () => listGuestMessages(threadId),
    enabled,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
  const decide = useMutation({
    mutationFn: ({
      messageId,
      action,
    }: {
      messageId: string;
      action: "send" | "dismiss";
    }) => decideGuestMessage(threadId, messageId, action),
    onSuccess: (response) => {
      queryClient.setQueryData(["guest-messages", threadId], response);
      void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
    },
  });

  const messages = pending.data?.messages ?? [];
  if (!enabled || messages.length === 0) return null;

  return (
    <div className="guest-drafts">
      {messages.map((message) => (
        <Group key={message.id} className="guest-draft" wrap="nowrap">
          <Text size="xs" className="guest-draft-text" title={message.text}>
            <Text component="span" size="xs" c="dimmed">
              Guest request ·{" "}
            </Text>
            {message.text}
          </Text>
          {canWrite ? (
            <Group gap={2} wrap="nowrap">
              <Tooltip label="Send as a turn">
                <ActionIcon
                  size={20}
                  aria-label="Send guest request"
                  disabled={decide.isPending}
                  onClick={() => decide.mutate({ messageId: message.id, action: "send" })}
                >
                  <IconCheck size={13} stroke={1.6} />
                </ActionIcon>
              </Tooltip>
              <Tooltip label="Dismiss">
                <ActionIcon
                  size={20}
                  aria-label="Dismiss guest request"
                  disabled={decide.isPending}
                  onClick={() =>
                    decide.mutate({ messageId: message.id, action: "dismiss" })
                  }
                >
                  <IconX size={13} stroke={1.6} />
                </ActionIcon>
              </Tooltip>
            </Group>
          ) : null}
        </Group>
      ))}
    </div>
  );
}
