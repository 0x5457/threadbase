import { HelpCircle, MessageSquare, ShieldAlert } from "lucide-react";
import { useState } from "react";
import {
  ActionIcon,
  Anchor,
  Badge,
  Box,
  Button,
  Checkbox,
  Divider,
  Flex,
  Paper,
  Select,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { ElicitationDto } from "@/api/types/ElicitationDto";
import type { CommentAnchor } from "@/api/types/CommentAnchor";

interface SchemaProperty {
  type?: string;
  title?: string;
  description?: string;
  enum?: unknown[];
  default?: unknown;
}

interface FormSchema {
  properties?: Record<string, SchemaProperty>;
  required?: string[];
}

const SECRET_MARKERS = ["password", "token", "secret", "api_key", "apikey"];

export function ElicitationCard({
  elicitation,
  resolvedAction,
  responding = false,
  onRespond,
  onComment,
}: {
  elicitation: ElicitationDto;
  resolvedAction: ElicitationAction | null;
  responding?: boolean;
  onRespond?: (
    id: string,
    action: ElicitationAction,
    values: Record<string, unknown>,
  ) => void;
  onComment?: (anchor: CommentAnchor) => void;
}) {
  const schema = (elicitation.requestedSchema ?? {}) as FormSchema;
  const fields = Object.entries(schema.properties ?? {});
  const authorizationUrl =
    elicitation.mode === "url"
      ? (elicitation.requestedSchema as { url?: string } | null)?.url
      : undefined;
  const [values, setValues] = useState<Record<string, unknown>>(() => {
    const initial: Record<string, unknown> = {};
    for (const [key, property] of fields) {
      if (property.default !== undefined) {
        initial[key] = property.default;
      }
    }
    return initial;
  });
  const secret = fields.some(([key]) =>
    SECRET_MARKERS.some((marker) => key.toLowerCase().includes(marker)),
  );
  const pending = elicitation.status === "pending" && resolvedAction === null;
  const missingRequired = (schema.required ?? []).filter((key) => {
    const value = values[key];
    return value === undefined || value === null || value === "";
  });

  return (
    <Flex gap="md">
      <Box w={28} />
      <Box flex={1} miw={0}>
        <Paper
          withBorder
          p="sm"
          radius="md"
          bg="var(--mantine-color-blue-light)"
        >
          <Flex align="center" gap="xs" wrap="wrap">
            <HelpCircle
              size={14}
              aria-hidden
              style={{ color: "var(--mantine-color-blue-light-color)" }}
            />
            <Text size="sm" fw={500}>
              Agent needs input
            </Text>
            {onComment ? (
              <ActionIcon
                variant="subtle"
                color="gray"
                size="xs"
                className="tb-hit-40"
                aria-label="Comment on question"
                onClick={() =>
                  onComment({
                    kind: "elicitation",
                    elicitationId: elicitation.id,
                  })
                }
              >
                <MessageSquare size={14} aria-hidden />
              </ActionIcon>
            ) : null}
            <Badge
              variant="light"
              color={pending ? "blue" : "gray"}
              size="xs"
              style={{ marginLeft: "auto" }}
            >
              {resolvedAction ?? elicitation.status}
            </Badge>
          </Flex>
          <Divider
            my="xs"
            color="var(--mantine-color-blue-light-hover)"
          />
          <Stack gap="xs">
            <Text size="sm">{elicitation.message}</Text>
            {authorizationUrl ? (
              <Paper withBorder p="xs" radius="sm">
                <Text
                  fz="xs"
                  ff="monospace"
                  c="dimmed"
                  display="block"
                  style={{ wordBreak: "break-all" }}
                >
                  {authorizationUrl}
                </Text>
                <Anchor
                  href={authorizationUrl}
                  target="_blank"
                  rel="noreferrer"
                  size="sm"
                  c="blue.6"
                >
                  Open authorization page
                </Anchor>
              </Paper>
            ) : null}
            <Text size="sm" c="dimmed">
              {elicitation.agent}
              {elicitation.blocking ? "" : " · non-blocking"}
            </Text>
            {secret ? (
              <Paper
                withBorder
                p="xs"
                radius="sm"
                bg="var(--mantine-color-yellow-light)"
              >
                <Flex align="flex-start" gap="xs">
                  <ShieldAlert
                    size={14}
                    aria-hidden
                    style={{
                      marginTop: 2,
                      flexShrink: 0,
                      color: "var(--mantine-color-yellow-light-color)",
                    }}
                  />
                  <Text size="sm" c="yellow-light-color">
                    This form asks for a secret. Threadbase will not submit
                    passwords or tokens through elicitation.
                  </Text>
                </Flex>
              </Paper>
            ) : null}
            {pending ? (
              <>
                {fields.map(([key, property]) => (
                  <Box component="label" key={key} display="block">
                    <Text size="sm" c="dimmed" display="block" mb={4}>
                      {property.title ?? key}
                    </Text>
                    {property.type === "boolean" ? (
                      <Checkbox
                        checked={Boolean(values[key])}
                        onChange={(event) =>
                          setValues((current) => ({
                            ...current,
                            [key]: event.currentTarget.checked,
                          }))
                        }
                      />
                    ) : property.enum ? (
                      <Select
                        value={String(values[key] ?? "")}
                        onChange={(next) => {
                          if (next !== null) {
                            setValues((current) => ({
                              ...current,
                              [key]: next,
                            }));
                          }
                        }}
                        data={property.enum.map((option) => ({
                          value: String(option),
                          label: String(option),
                        }))}
                        placeholder="Select…"
                      />
                    ) : (
                      <TextInput
                        type={property.type === "number" ? "number" : "text"}
                        value={String(values[key] ?? "")}
                        onChange={(event) =>
                          setValues((current) => ({
                            ...current,
                            [key]:
                              property.type === "number"
                                ? event.target.value === ""
                                  ? undefined
                                  : Number(event.target.value)
                                : event.target.value,
                          }))
                        }
                        disabled={secret}
                      />
                    )}
                    {property.description ? (
                      <Text size="sm" c="dimmed" display="block" mt={4}>
                        {property.description}
                      </Text>
                    ) : null}
                  </Box>
                ))}
                <Flex
                  gap="xs"
                  wrap="wrap"
                  mt="xs"
                  pt="xs"
                  style={{
                    borderTop:
                      "1px solid var(--mantine-color-blue-light-hover)",
                  }}
                >
                  <Button
                    size="xs"
                    disabled={
                      secret || !onRespond || responding || missingRequired.length > 0
                    }
                    loading={responding}
                    onClick={() =>
                      onRespond?.(elicitation.id, "accept", values)
                    }
                  >
                    Submit
                  </Button>
                  <Button
                    size="xs"
                    variant="outline"
                    disabled={responding}
                    onClick={() =>
                      onRespond?.(elicitation.id, "decline", {})
                    }
                  >
                    Decline
                  </Button>
                  <Button
                    size="xs"
                    variant="subtle"
                    color="dark"
                    disabled={responding}
                    onClick={() => onRespond?.(elicitation.id, "cancel", {})}
                  >
                    Cancel
                  </Button>
                  {missingRequired.length > 0 && !secret ? (
                    <Text size="xs" c="dimmed" role="status">
                      Fill in{" "}
                      {missingRequired
                        .map((key) => schema.properties?.[key]?.title ?? key)
                        .join(", ")}{" "}
                      to submit.
                    </Text>
                  ) : null}
                </Flex>
              </>
            ) : elicitation.respondedBy ? (
              <Text size="sm" c="dimmed">
                Answered by {elicitation.respondedBy.displayName}
              </Text>
            ) : null}
          </Stack>
        </Paper>
      </Box>
    </Flex>
  );
}
