import {
  Box,
  Button,
  Card,
  Group,
  Select,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { useState } from "react";
import type { Elicitation } from "../../api";

interface ElicitationProperty {
  type?: string;
  title?: string;
  description?: string;
  enum?: string[];
}

export function ElicitationCard({
  elicitation,
  busy,
  onRespond,
}: {
  elicitation: Elicitation;
  busy: boolean;
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
  const required = schema.required ?? [];
  const missing = required.some((key) => !values[key]);

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
                  property.enum?.length ? (
                    <Select
                      key={key}
                      label={property.title ?? key}
                      description={property.description}
                      data={property.enum}
                      value={values[key] ?? null}
                      onChange={(value) =>
                        setValues((current) => ({
                          ...current,
                          [key]: value ?? "",
                        }))
                      }
                      required={required.includes(key)}
                    />
                  ) : (
                    <TextInput
                      key={key}
                      label={property.title ?? key}
                      description={property.description}
                      value={values[key] ?? ""}
                      onChange={(event) =>
                        setValues((current) => ({
                          ...current,
                          [key]: event.currentTarget.value,
                        }))
                      }
                      required={required.includes(key)}
                    />
                  ),
                )}
              </Stack>
            ) : null}
            <Group justify="flex-end" gap={6}>
              <Button
                variant="subtle"
                color="gray"
                loading={busy}
                onClick={() => onRespond("decline", values)}
              >
                Decline
              </Button>
              <Button
                variant="filled"
                color="dark"
                loading={busy}
                disabled={missing}
                onClick={() => onRespond("accept", values)}
              >
                Continue
              </Button>
            </Group>
          </Box>
        </Card>
      </div>
    </div>
  );
}
