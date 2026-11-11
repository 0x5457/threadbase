import { Avatar, Group, Indicator, Text, Tooltip, UnstyledButton } from "@mantine/core";

export interface AvatarPerson {
  id: string;
  displayName: string;
  avatarUrl?: string | null;
  color?: string | null;
  online?: boolean;
}

export interface AvatarStackProps {
  people: AvatarPerson[];
  max?: number;
  size?: "xs" | "sm" | "md" | "lg";
  className?: string;
  onSelect?: (person: AvatarPerson) => void;
}

const sizeMap = { xs: 16, sm: 20, md: 24, lg: 32 } as const;

export function AvatarStack({
  people,
  max = 3,
  size = "sm",
  className,
  onSelect,
}: AvatarStackProps) {
  const visible = people.slice(0, max);
  const hidden = people.slice(max);
  if (people.length === 0) {
    return null;
  }
  const px = sizeMap[size];

  return (
    <Group gap={4} wrap="nowrap" className={className}>
      <Avatar.Group spacing="xs">
        {visible.map((person) => {
          const avatar = (
            <Indicator
              inline
              disabled={!person.online}
              position="bottom-end"
              size={Math.max(8, Math.round(px * 0.35))}
              color="green"
              withBorder
            >
              <Avatar
                src={person.avatarUrl ?? null}
                alt={person.displayName}
                name={person.displayName}
                color="initials"
                size={px}
                radius="xl"
              />
            </Indicator>
          );

          return onSelect ? (
            <UnstyledButton
              key={person.id}
              onClick={() => onSelect(person)}
              title={`Follow ${person.displayName}`}
            >
              {avatar}
            </UnstyledButton>
          ) : (
            <Tooltip key={person.id} label={person.displayName}>
              {avatar}
            </Tooltip>
          );
        })}
      </Avatar.Group>
      {hidden.length > 0 ? (
        <Text
          size="xs"
          c="dimmed"
          title={hidden.map((person) => person.displayName).join(", ")}
        >
          +{hidden.length}
        </Text>
      ) : null}
    </Group>
  );
}
