import { Avatar, Indicator } from "@mantine/core";
import type { ThreadSummary } from "../../api";

function initials(name: string) {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

export function Participants({
  participants,
  onlineUserIds,
}: {
  participants: ThreadSummary["participants"];
  onlineUserIds?: string[];
}) {
  if (!participants.length) return null;

  return (
    <Avatar.Group spacing={-4} visibleFrom="xs">
      {participants.slice(0, 3).map((participant, index) => (
        <Indicator
          key={participant.user.id}
          color="green"
          size={5}
          offset={1}
          position="bottom-end"
          disabled={
            onlineUserIds
              ? !onlineUserIds.includes(participant.user.id)
              : !participant.online
          }
          withBorder={false}
        >
          <Avatar
            size={18}
            radius="xl"
            color={index === 0 ? "brand" : "blue"}
            title={participant.user.displayName}
            src={participant.user.avatarUrl ?? undefined}
          >
            {initials(participant.user.displayName)}
          </Avatar>
        </Indicator>
      ))}
    </Avatar.Group>
  );
}
