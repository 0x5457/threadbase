import {
  Box,
  Button,
  Card,
  Group,
  Radio,
  Select,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { useState } from "react";
import type { Elicitation } from "../../api";
import { useDelayedFlag } from "../../lib/use-delayed-flag";

interface ElicitationProperty {
  type?: string;
  title?: string;
  description?: string;
  enum?: string[];
}

export function ElicitationCard({
  elicitation,
  busy,
  pendingAction = null,
  error = null,
  readOnly = false,
  onRespond,
}: {
  elicitation: Elicitation;
  busy: boolean;
  pendingAction?: "accept" | "decline" | null;
  error?: string | null;
  readOnly?: boolean;
  onRespond: (
    action: "accept" | "decline",
    values: Record<string, string>,
  ) => void;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const schema = (elicitation.requestedSchema ?? {}) as {
    properties?: Record<string, ElicitationProperty>;
    required?: string[];
  };
  const properties = Object.entries(schema.properties ?? {});
  const requiredKeys = schema.required ?? [];
  const required = new Set(requiredKeys);
  const missing = requiredKeys.some((key) => !values[key]);
  const showBusy = useDelayedFlag(busy, 300, 500);

  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Card withBorder p={0} className="approval-card">
          <Group h={27} px={9} gap={6} className="approval-head" wrap="nowrap">
            <span className="dot waiting" />
            <Text size="sm" c="dimmed">
              Input requested
            </Text>
            <Box style={{ flex: 1 }} />
            <Text size="sm" c="dimmed">
              {elicitation.agent}
            </Text>
          </Group>
          <Box p={9}>
            <Text size="md" mb={8}>
              {elicitation.message}
            </Text>
            {properties.length ? (
              <Stack gap="xs" mb="sm">
                {properties.map(([key, property]) =>
                  property.enum?.length && property.enum.length <= 4 ? (
                    <Radio.Group
                      key={key}
                      label={property.title ?? key}
                      description={property.description}
                      value={values[key] ?? ""}
                      onChange={(value) =>
                        setValues((current) => ({
                          ...current,
                          [key]: value,
                        }))
                      }
                      required={required.has(key)}
                    >
                      <Stack gap={5} mt={5}>
                        {property.enum.map((option) => (
                          <Radio
                            key={option}
                            value={option}
                            label={option}
                            disabled={readOnly}
                          />
                        ))}
                      </Stack>
                    </Radio.Group>
                  ) : property.enum?.length ? (
                    <Select
                      key={key}
                      label={property.title ?? key}
                      description={property.description}
                      data={property.enum}
                      value={values[key] ?? null}
                      disabled={readOnly}
                      onChange={(value) =>
                        setValues((current) => ({
                          ...current,
                          [key]: value ?? "",
                        }))
                      }
                      required={required.has(key)}
                    />
                  ) : (
                    <TextInput
                      key={key}
                      label={property.title ?? key}
                      description={property.description}
                      value={values[key] ?? ""}
                      disabled={readOnly}
                      onChange={(event) =>
                        setValues((current) => ({
                          ...current,
                          [key]: event.currentTarget.value,
                        }))
                      }
                      required={required.has(key)}
                    />
                  ),
                )}
              </Stack>
            ) : null}
            {readOnly ? (
              <Text size="xs" c="dimmed">
                Your account has read-only access, so responses are disabled.
                Ask an administrator to allow editing.
              </Text>
            ) : null}
            <Group
              justify="flex-end"
              gap={6}
              mt={readOnly ? 8 : 0}
              className="approval-actions"
            >
              <Button
                variant="subtle"
                color="gray"
                loading={showBusy && pendingAction === "decline"}
                disabled={readOnly || busy}
                onClick={() => onRespond("decline", values)}
              >
                Decline
              </Button>
              <Button
                variant="filled"
                color="brand"
                loading={showBusy && pendingAction === "accept"}
                disabled={readOnly || busy || missing}
                onClick={() => onRespond("accept", values)}
              >
                Continue
              </Button>
            </Group>
            {error ? (
              <Text size="xs" c="red" mt={8}>
                {error}
              </Text>
            ) : null}
          </Box>
        </Card>
      </div>
    </div>
  );
}
