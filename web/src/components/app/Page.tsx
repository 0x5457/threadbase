import {
  Anchor,
  Container,
  Group,
  ScrollArea,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { ChevronLeft } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";

export interface PageProps {
  /** Page heading. Uses the theme's h1 size, no inline font values. */
  title?: ReactNode;
  /** Optional leading icon inside the heading row. */
  icon?: ReactNode;
  description?: ReactNode;
  breadcrumb?: { to: string; label: string };
  /** Primary page action(s), right-aligned and wrapping under the title. */
  actions?: ReactNode;
  /** Container preset: `sm` 720, `md` 960, `lg` 1140 (Mantine scale). */
  size?: "sm" | "md" | "lg";
  children: ReactNode;
}

/**
 * Scrollable route body: one Mantine Container, one gutter scale, one heading
 * hierarchy. Every route renders through this so page width and gutters cannot
 * drift apart again.
 */
export function Page({
  title,
  icon,
  description,
  breadcrumb,
  actions,
  size = "md",
  children,
}: PageProps) {
  return (
    <ScrollArea h="100%">
      <Container
        size={size}
        px={{ base: "md", sm: "lg", lg: "xl" }}
        py={{ base: "md", sm: "lg", lg: "xl" }}
      >
        {breadcrumb ? (
          <Anchor
            component={Link}
            to={breadcrumb.to}
            size="xs"
            c="dimmed"
            mb="xs"
            display="inline-flex"
          >
            <Group gap={4} wrap="nowrap">
              <ChevronLeft size={12} aria-hidden />
              {breadcrumb.label}
            </Group>
          </Anchor>
        ) : null}

        {title ? (
          <Group
            justify="space-between"
            align="flex-start"
            gap="md"
            wrap="wrap"
            mb="lg"
          >
            <Stack gap={2} miw={0}>
              <Group gap="xs" wrap="nowrap">
                {icon}
                <Title order={1} lineClamp={1}>
                  {title}
                </Title>
              </Group>
              {description ? (
                <Text size="sm" c="dimmed">
                  {description}
                </Text>
              ) : null}
            </Stack>
            {actions ? (
              <Group gap="xs" wrap="wrap" justify="flex-end">
                {actions}
              </Group>
            ) : null}
          </Group>
        ) : null}

        {children}
      </Container>
    </ScrollArea>
  );
}
