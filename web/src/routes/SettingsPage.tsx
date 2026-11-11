import { Check, Palette, Settings2, Shield, UserRound } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import {
  Avatar,
  Badge,
  Box,
  Button,
  Checkbox,
  Group,
  Indicator,
  Paper,
  ScrollArea,
  Select,
  SimpleGrid,
  Skeleton,
  Stack,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { updateSettings } from "@/api/endpoints";
import { queryKeys, useMe, useWorkspaceState } from "@/api/queries";
import type { BaselineKind } from "@/api/types/BaselineKind";
import type { SettingsDto } from "@/api/types/SettingsDto";
import { ErrorState, PageContainer } from "@/components/app";
import { ApprovalRulesSection } from "@/components/workspace/ApprovalRulesSection";
import { setTheme, useTheme, type Theme } from "@/lib/theme";
import { describeError } from "@/lib/errors";
import { layout } from "@/lib/design";

const baselines: Array<{ value: BaselineKind; label: string }> = [
  { value: "base_branch", label: "Base branch" },
  { value: "last_commit", label: "Last commit" },
  { value: "last_turn", label: "Last turn" },
  { value: "commit", label: "Specific commit" },
];

const textWidths = [
  { value: "default", label: "Default" },
  { value: "full", label: "Full" },
  { value: "custom", label: "Custom" },
];

function Section({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: LucideIcon;
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <Paper withBorder>
      <Box px="md" pt="md">
        <Group gap="xs" align="center" wrap="nowrap">
          <Icon
            size={layout.icon.sm}
            aria-hidden
            style={{ color: "var(--mantine-color-dimmed)", flexShrink: 0 }}
          />
          <Text size="md" fw={600}>
            {title}
          </Text>
        </Group>
        {description ? (
          <Text size="xs" c="dimmed" mt={2}>
            {description}
          </Text>
        ) : null}
      </Box>
      <Box px="md" pb="md" pt="sm">
        {children}
      </Box>
    </Paper>
  );
}

function ThemePicker() {
  const theme = useTheme();
  const options: Array<{ value: Theme; label: string }> = [
    { value: "dark", label: "Dark" },
    { value: "light", label: "Light" },
  ];
  return (
    <Stack gap="xs">
      <Text size="xs" c="dimmed">
        Theme
      </Text>
      <Group gap="xs">
        {options.map((option) => (
          <Button
            key={option.value}
            type="button"
            size="xs"
            variant={theme === option.value ? "default" : "subtle"}
            color={theme === option.value ? undefined : "gray"}
            onClick={() => setTheme(option.value)}
            aria-pressed={theme === option.value}
            leftSection={
              theme === option.value ? <Check size={12} aria-hidden /> : undefined
            }
            maw={80}
          >
            {option.label}
          </Button>
        ))}
      </Group>
      <Text size="xs" c="dimmed">
        Saved on this device. The server keeps its own appearance defaults.
      </Text>
    </Stack>
  );
}

function SettingsForm({ initial }: { initial: SettingsDto }) {
  const queryClient = useQueryClient();
  const [textWidth, setTextWidth] = useState(initial.textWidth);
  const [subthreadsPerThread, setSubthreadsPerThread] = useState(
    String(initial.subthreadsPerThread),
  );
  const [sendWithModifier, setSendWithModifier] = useState(
    initial.sendWithModifier,
  );
  const [defaultAgent, setDefaultAgent] = useState(initial.defaultAgent);
  const [defaultModel, setDefaultModel] = useState(initial.defaultModel ?? "");
  const [defaultBaseline, setDefaultBaseline] = useState<BaselineKind>(
    initial.defaultBaseline,
  );
  const [hiddenPresence, setHiddenPresence] = useState(initial.hiddenPresence);
  const [browserNotifications, setBrowserNotifications] = useState(
    initial.browserNotifications,
  );
  const [webhookUrl, setWebhookUrl] = useState(initial.webhookUrl ?? "");
  const mutation = useMutation({
    mutationFn: () =>
      updateSettings({
        theme: null,
        textWidth,
        subthreadsPerThread: Number.parseInt(subthreadsPerThread, 10) || null,
        sendWithModifier,
        defaultAgent: defaultAgent.trim() ? defaultAgent.trim() : null,
        defaultModel: defaultModel.trim() ? defaultModel.trim() : null,
        defaultBaseline,
        hiddenPresence,
        webhookUrl: webhookUrl.trim() ? webhookUrl.trim() : null,
        browserNotifications,
      }),
    onSuccess: (response) => {
      queryClient.setQueryData(queryKeys.state, response);
      notifications.show({
        message: "Settings saved",
        color: "green",
      });
    },
    onError: (error) => {
      notifications.show({
        message:
          error instanceof Error ? error.message : "Could not save settings.",
        color: "red",
      });
    },
  });

  return (
    <Stack gap="sm">
      <Section
        icon={Palette}
        title="Thread defaults"
        description="Defaults apply to new threads. Existing threads keep their own settings."
      >
        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="sm" verticalSpacing="sm">
          <TextInput
            label="Default agent"
            value={defaultAgent}
            onChange={(event) => setDefaultAgent(event.target.value)}
            placeholder="Agent name"
          />
          <TextInput
            label="Default model"
            value={defaultModel}
            onChange={(event) => setDefaultModel(event.target.value)}
            placeholder="Agent default"
          />
          <Select
            label="Default diff baseline"
            value={defaultBaseline}
            onChange={(value) =>
              setDefaultBaseline((value ?? initial.defaultBaseline) as BaselineKind)
            }
            data={baselines}
          />
          <Select
            label="Transcript width"
            value={textWidth}
            onChange={(value) => setTextWidth(value ?? "default")}
            data={textWidths}
          />
        </SimpleGrid>
        <Stack gap="sm" mt="sm">
          <TextInput
            label="Subthreads per thread"
            type="number"
            min={1}
            max={100}
            value={subthreadsPerThread}
            onChange={(event) => setSubthreadsPerThread(event.target.value)}
            w={160}
          />
          <Checkbox
            label="Send with modifier key"
            checked={sendWithModifier}
            onChange={(event) => setSendWithModifier(event.target.checked)}
          />
        </Stack>
      </Section>

      <Section
        icon={Shield}
        title="Presence and notifications"
        description="Controls what other workspace members can see."
      >
        <Stack gap="sm">
          <Checkbox
            label="Hide my presence"
            description="Your focus and online status will not be shared."
            checked={hiddenPresence}
            onChange={(event) => setHiddenPresence(event.target.checked)}
          />
          <Checkbox
            label="Browser notifications"
            description="Ask the browser for permission when a new mention or approval arrives."
            checked={browserNotifications}
            onChange={(event) => setBrowserNotifications(event.target.checked)}
          />
          <TextInput
            label="Webhook URL"
            value={webhookUrl}
            onChange={(event) => setWebhookUrl(event.target.value)}
            placeholder="https://example.com/threadbase"
            description="Optional POST endpoint for approval and mention events."
          />
        </Stack>
      </Section>

      {mutation.isError ? (
        <Text size="sm" c="red" role="alert">
          {describeError(mutation.error, "Cannot save settings").description}
        </Text>
      ) : null}
      <Group justify="flex-end">
        <Button
          variant="filled"
          size="sm"
          loading={mutation.isPending}
          onClick={() => mutation.mutate()}
        >
          Save settings
        </Button>
      </Group>
    </Stack>
  );
}

export function SettingsPage() {
  const me = useMe();
  const state = useWorkspaceState();

  return (
    <ScrollArea h="100%">
      <PageContainer>
        <Stack gap={2} mb="lg">
          <Group gap="xs">
            <Settings2 size={layout.icon.md} aria-hidden />
            <Title order={1}>Settings</Title>
          </Group>
          <Text size="xs" c="dimmed">
            Workspace defaults and your local account.
          </Text>
        </Stack>

        <Stack gap="sm">
          <Section
            icon={UserRound}
            title="Account"
            description="Identity used for this workspace session."
          >
            {me.isLoading ? (
              <Group gap="sm">
                <Skeleton h={32} w={32} radius="xl" />
                <Stack gap={4}>
                  <Skeleton h={14} w={128} />
                  <Skeleton h={12} w={96} />
                </Stack>
              </Group>
            ) : me.isError || !me.data ? (
              <ErrorState
                error={me.error}
                title="Cannot load account"
                onRetry={() => void me.refetch()}
                compact
              />
            ) : (
              <Group gap="sm" wrap="wrap">
                <Indicator
                  inline
                  size={10}
                  color="green"
                  withBorder
                  position="bottom-end"
                >
                  <Avatar
                    src={me.data.user.avatarUrl ?? null}
                    name={me.data.user.displayName}
                    color="initials"
                    size="lg"
                    radius="xl"
                  />
                </Indicator>
                <Stack gap={2} style={{ flex: 1, minWidth: 0 }}>
                  <Group gap={6} wrap="wrap">
                    <Text size="sm" fw={500}>
                      {me.data.user.displayName}
                    </Text>
                    {me.data.user.isAdmin ? (
                      <Badge variant="light" color="blue" size="xs">
                        admin
                      </Badge>
                    ) : null}
                  </Group>
                  <Text size="sm" c="dimmed">
                    @{me.data.user.username} · {me.data.authMethod}
                  </Text>
                </Stack>
                <Badge variant="outline" color="gray" size="xs">
                  {me.data.user.hiddenPresence
                    ? "presence hidden"
                    : "presence visible"}
                </Badge>
              </Group>
            )}
          </Section>

          <Section
            icon={Palette}
            title="Appearance"
            description="Theme is stored locally in this browser."
          >
            <ThemePicker />
          </Section>

          {state.isLoading ? (
            <Paper withBorder radius="md" p="sm">
              <Stack gap="xs">
                <Skeleton h={16} w={128} />
                <Skeleton h={32} />
                <Skeleton h={32} w="66%" />
              </Stack>
            </Paper>
          ) : state.isError || !state.data ? (
            <Paper withBorder radius="md">
              <ErrorState
                error={state.error}
                title="Cannot load workspace preferences"
                onRetry={() => void state.refetch()}
              />
            </Paper>
          ) : (
            <SettingsForm
              key={JSON.stringify(state.data.settings)}
              initial={state.data.settings}
            />
          )}

          {me.data?.user.isAdmin ? (
            <Section
              icon={Shield}
              title="Standing approval rules"
              description="Rules created by “Allow for thread”. Revoke them when an agent should ask again."
            >
              <ApprovalRulesSection />
            </Section>
          ) : null}
        </Stack>
      </PageContainer>
    </ScrollArea>
  );
}
