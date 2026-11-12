import { Badge, Box, Text } from "@mantine/core";
import type { ReactNode } from "react";

export function SectionHead({
  icon,
  label,
  count,
  trailing,
}: {
  icon: ReactNode;
  label: string;
  count?: number;
  trailing?: ReactNode;
}) {
  return (
    <div className="changes-head">
      {icon}
      <Text size="sm" fw={500}>
        {label}
      </Text>
      {typeof count === "number" ? (
        <Badge size="xs" variant="light" color="gray">
          {count}
        </Badge>
      ) : null}
      <Box style={{ flex: 1 }} />
      {trailing}
    </div>
  );
}
