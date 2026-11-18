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
import { memo, useEffect, useState } from "react";
import { getMe, getState, updateSettings, type Settings } from "../api";
import {
  currentPushState,
  disablePush,
  enablePush,
  pushSupported,
  type PushState,
} from "../lib/push";
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

export const SettingsView = memo(function SettingsView() {
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
  // Last persisted snapshot, so the bar can tell an actual edit from a refetch
  // and Discard has something to return to.
  const [saved, setSaved] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [pushState, setPushState] = useState<PushState>(() =>
    pushSupported() ? "off" : "unsupported",
  );
  const [pushBusy, setPushBusy] = useState(false);

  useEffect(() => {
    if (state.data) {
      setSettings(state.data.settings);
      setSaved(state.data.settings);
    }
  }, [state.data]);

  useEffect(() => {
    if (!pushSupported()) {
      setPushState("unsupported");
      return;
    }
    void currentPushState()
      .then(setPushState)
      .catch(() => setPushState("off"));
  }, []);

  const patch = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((current) =>
      current ? { ...current, [key]: value } : current,
    );
  };

  const save = async () => {
    if (!settings) return;
    const payload: Settings = {
      ...settings,
      theme: colorScheme === "dark" ? "dark" : "light",
    };
    setSaving(true);
    try {
      await updateSettings(payload);
      // The state query refetch is async; mark this exact payload saved now so
      // the sticky bar disappears as soon as the request resolves. Only the
      // theme is merged back so an edit made while the save was in flight is
      // not overwritten.
      setSaved(payload);
      setSettings((current) =>
        current ? { ...current, theme: payload.theme } : current,
      );
      notifications.show({ message: "Settings saved", color: "teal" });
      void queryClient.invalidateQueries({ queryKey: ["state"] });
      void queryClient.invalidateQueries({ queryKey: ["thread"] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox-unread"] });
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

  // The header toggle changes the live scheme without touching the settings
  // object; count that as unsaved too, otherwise leaving Settings would keep
  // the server on the old theme.
  const dirty = saved
    ? JSON.stringify(settings) !== JSON.stringify(saved) ||
      settings.theme !== colorScheme
    : false;

  return (
    <div className="settings-view">
      <div className="settings-scroll">
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
                    Zed&apos;s One Light / One Dark palette, stored in your
                    workspace settings.
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
                    { label: "One Light", value: "light" },
                    { label: "One Dark", value: "dark" },
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
                label="In-app browser notifications"
                description="Show a notification while Threadbase is open and an agent needs approval or input."
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
              <Group justify="space-between" align="center">
                <Stack gap={0}>
                  <Text size="sm">Push notifications</Text>
                  <Text size="xs" c="dimmed">
                    Get notified when an agent needs input, finishes or fails,
                    even when Threadbase is closed.
                  </Text>
                </Stack>
                {pushState === "unsupported" ? (
                  <Text size="xs" c="dimmed">
                    Not supported by this browser
                  </Text>
                ) : pushState === "denied" ? (
                  <Text size="xs" c="red">
                    Blocked in browser settings
                  </Text>
                ) : (
                  <Button
                    size="xs"
                    variant={pushState === "on" ? "default" : "filled"}
                    color="dark"
                    loading={pushBusy}
                    onClick={() => {
                      setPushBusy(true);
                      const action =
                        pushState === "on" ? disablePush() : enablePush();
                      void action
                        .then(() => currentPushState())
                        .then(setPushState)
                        .catch((cause: unknown) =>
                          notifications.show({
                            message:
                              cause instanceof Error
                                ? cause.message
                                : "Could not update push notifications",
                            color: "red",
                          }),
                        )
                        .finally(() => setPushBusy(false));
                    }}
                  >
                    {pushState === "on" ? "Disable push" : "Enable push"}
                  </Button>
                )}
              </Group>
            </Stack>
          </Card>

          {me.data?.user.isAdmin ? (
            <PeopleSection currentUserId={me.data.user.id} />
          ) : null}
        </Stack>
      </div>
      </div>
      {dirty ? (
        <div className="settings-savebar">
          <Text size="xs" c="dimmed">
            Unsaved changes
          </Text>
          <Group gap="xs">
            <Button
              variant="default"
              size="xs"
              disabled={saving}
              onClick={() => {
                if (!saved) return;
                setSettings(saved);
                setColorScheme(saved.theme === "dark" ? "dark" : "light");
              }}
            >
              Discard
            </Button>
            <Button
              color="dark"
              size="xs"
              loading={saving}
              onClick={() => void save()}
            >
              Save changes
            </Button>
          </Group>
        </div>
      ) : null}
    </div>
  );
});
