import {
  Button,
  Card,
  Group,
  Loader,
  NumberInput,
  SegmentedControl,
  Select,
  Stack,
  Switch,
  Text,
  TextInput,
  useMantineColorScheme,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { getMe, getState, updateSettings, type Settings } from "../api";
import { PeopleSection } from "./PeopleSection";

const agents = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude Code" },
  { value: "opencode", label: "opencode" },
  { value: "pi", label: "pi" },
];

const baselines = [
  { value: "base_branch", label: "Base branch" },
  { value: "last_commit", label: "Last commit" },
  { value: "last_turn", label: "Last turn" },
];

export function SettingsView() {
  const queryClient = useQueryClient();
  const { colorScheme, setColorScheme } = useMantineColorScheme();
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
  });
  const state = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    staleTime: 10_000,
  });
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (state.data) setSettings(state.data.settings);
  }, [state.data]);

  const patch = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((current) =>
      current ? { ...current, [key]: value } : current,
    );
  };

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await updateSettings({
        ...settings,
        theme: colorScheme === "dark" ? "dark" : "light",
      });
      notifications.show({ message: "Settings saved", color: "teal" });
      void queryClient.invalidateQueries({ queryKey: ["state"] });
      void queryClient.invalidateQueries({ queryKey: ["thread"] });
      void queryClient.invalidateQueries({ queryKey: ["threads"] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
    } catch (cause) {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not save settings",
        color: "red",
      });
    } finally {
      setSaving(false);
    }
  };

  if (state.isPending || !settings) {
    return (
      <Group justify="center" py="xl">
        <Loader size="sm" />
      </Group>
    );
  }

  return (
    <div className="settings-view">
      <div className="settings-head">
        <Text size="md" fw={500}>
          Settings
        </Text>
        <Text size="sm" c="dimmed">
          Workspace defaults and personal preferences.
        </Text>
      </div>
      <div className="settings-body">
        <Stack gap="md">
          <Card withBorder p="md" className="settings-card">
            <Stack gap="sm">
              <Text size="sm" fw={500}>
                Appearance
              </Text>
              <Group justify="space-between" align="center">
                <Stack gap={0}>
                  <Text size="sm">Theme</Text>
                  <Text size="xs" c="dimmed">
                    Stored on this device and in your workspace settings.
                  </Text>
                </Stack>
                <SegmentedControl
                  size="xs"
                  value={colorScheme === "dark" ? "dark" : "light"}
                  onChange={(value) => {
                    setColorScheme(value as "light" | "dark");
                    patch("theme", value);
                  }}
                  data={[
                    { label: "Light", value: "light" },
                    { label: "Dark", value: "dark" },
                  ]}
                />
              </Group>
              <Group justify="space-between" align="center">
                <Stack gap={0}>
                  <Text size="sm">Transcript width</Text>
                  <Text size="xs" c="dimmed">
                    How wide the thread document can grow.
                  </Text>
                </Stack>
                <Select
                  size="xs"
                  w={130}
                  data={[
                    { value: "default", label: "Default" },
                    { value: "full", label: "Full" },
                  ]}
                  value={settings.textWidth}
                  onChange={(value) => patch("textWidth", value ?? "default")}
                  allowDeselect={false}
                />
              </Group>
            </Stack>
          </Card>

          <Card withBorder p="md" className="settings-card">
            <Stack gap="sm">
              <Text size="sm" fw={500}>
                Thread defaults
              </Text>
              <Group grow align="flex-start">
                <Select
                  label="Default agent"
                  data={agents}
                  value={settings.defaultAgent}
                  onChange={(value) => patch("defaultAgent", value ?? "codex")}
                  allowDeselect={false}
                />
                <TextInput
                  label="Default model"
                  description="Used for new threads; change it per thread in the composer"
                  placeholder="Agent default"
                  value={settings.defaultModel ?? ""}
                  onChange={(event) =>
                    patch("defaultModel", event.currentTarget.value)
                  }
                />
              </Group>
              <Group grow align="flex-start">
                <Select
                  label="Default diff baseline"
                  data={baselines}
                  value={settings.defaultBaseline}
                  onChange={(value) =>
                    patch(
                      "defaultBaseline",
                      (value ?? "base_branch") as Settings["defaultBaseline"],
                    )
                  }
                  allowDeselect={false}
                />
                <NumberInput
                  label="Subthreads per thread"
                  min={3}
                  max={100}
                  value={settings.subthreadsPerThread}
                  onChange={(value) =>
                    patch(
                      "subthreadsPerThread",
                      typeof value === "number" ? value : 10,
                    )
                  }
                />
              </Group>
              <Switch
                color="dark"
                label="Send with modifier key"
                description="Require ⌘/Ctrl+Enter to send."
                checked={settings.sendWithModifier}
                onChange={(event) =>
                  patch("sendWithModifier", event.currentTarget.checked)
                }
              />
            </Stack>
          </Card>

          <Card withBorder p="md" className="settings-card">
            <Stack gap="sm">
              <Text size="sm" fw={500}>
                Presence and notifications
              </Text>
              <Switch
                color="dark"
                label="Hide my presence"
                description="Other people won't see when you are viewing a thread."
                checked={settings.hiddenPresence}
                onChange={(event) =>
                  patch("hiddenPresence", event.currentTarget.checked)
                }
              />
              <Switch
                color="dark"
                label="Browser notifications"
                description="Notify me when an agent needs approval or input."
                checked={settings.browserNotifications}
                onChange={(event) => {
                  const enabled = event.currentTarget.checked;
                  if (
                    enabled &&
                    typeof Notification !== "undefined" &&
                    Notification.permission === "default"
                  ) {
                    void Notification.requestPermission();
                  }
                  patch("browserNotifications", enabled);
                }}
              />
              <TextInput
                label="Webhook URL"
                placeholder="https://example.com/threadbase"
                value={settings.webhookUrl ?? ""}
                onChange={(event) =>
                  patch("webhookUrl", event.currentTarget.value)
                }
              />
            </Stack>
          </Card>

          {me.data?.user.isAdmin ? (
            <PeopleSection currentUserId={me.data.user.id} />
          ) : null}

          <Group justify="flex-end">
            <Button color="dark" loading={saving} onClick={() => void save()}>
              Save settings
            </Button>
          </Group>
        </Stack>
      </div>
    </div>
  );
}
