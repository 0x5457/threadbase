import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, Palette, Settings2, Shield, UserRound } from "lucide-react";
import { useState, type ComponentType, type ReactNode } from "react";
import { toast } from "sonner";
import { updateSettings } from "@/api/endpoints";
import { queryKeys, useMe, useWorkspaceState } from "@/api/queries";
import type { BaselineKind } from "@/api/types/BaselineKind";
import type { SettingsDto } from "@/api/types/SettingsDto";
import {
  Avatar,
  Badge,
  Button,
  Checkbox,
  ErrorState,
  Input,
  Skeleton,
} from "@/components/ui";
import { setTheme, useTheme, type Theme } from "@/lib/theme";
import { describeError } from "@/lib/errors";
import { cn } from "@/lib/utils";

const baselines: Array<{ value: BaselineKind; label: string }> = [
  { value: "base_branch", label: "Base branch" },
  { value: "last_commit", label: "Last commit" },
  { value: "last_turn", label: "Last turn" },
  { value: "commit", label: "Specific commit" },
];

function Section({
  icon: Icon,
  title,
  description,
  children,
}: {
  icon: ComponentType<{ className?: string }>;
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <section className="rounded-xl border border-border bg-panel">
      <div className="flex items-start gap-2.5 border-b border-border px-4 py-3">
        <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-raised text-muted">
          <Icon className="size-3.5" />
        </span>
        <div>
          <h2 className="text-[12px] font-semibold text-fg">{title}</h2>
          {description ? (
            <p className="mt-0.5 text-[11px] text-muted">{description}</p>
          ) : null}
        </div>
      </div>
      <div className="px-4 py-3.5">{children}</div>
    </section>
  );
}

function ThemePicker() {
  const theme = useTheme();
  const options: Array<{ value: Theme; label: string }> = [
    { value: "dark", label: "Dark" },
    { value: "light", label: "Light" },
  ];
  return (
    <div className="space-y-2">
      <p className="text-[11px] font-medium text-muted">Theme</p>
      <div className="flex gap-1.5">
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            onClick={() => setTheme(option.value)}
            className={cn(
              "flex h-8 min-w-20 items-center justify-center gap-1.5 rounded-md border px-3 text-[11.5px] transition-colors",
              theme === option.value
                ? "border-accent/50 bg-accent-soft text-fg"
                : "border-border bg-canvas text-muted hover:border-border-strong hover:text-fg",
            )}
          >
            {theme === option.value ? <Check className="size-3" /> : null}
            {option.label}
          </button>
        ))}
      </div>
      <p className="text-[10.5px] text-faint">
        Saved on this device. The server keeps its own appearance defaults.
      </p>
    </div>
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
      toast.success("Settings saved");
    },
    onError: (error) => {
      toast.error(
        error instanceof Error ? error.message : "Could not save settings.",
      );
    },
  });

  return (
    <div className="space-y-3">
      <Section
        icon={Palette}
        title="Thread defaults"
        description="Defaults apply to new threads. Existing threads keep their own settings."
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Default agent
            </span>
            <Input
              value={defaultAgent}
              onChange={(event) => setDefaultAgent(event.target.value)}
              placeholder="Agent name"
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Default model
            </span>
            <Input
              value={defaultModel}
              onChange={(event) => setDefaultModel(event.target.value)}
              placeholder="Agent default"
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Default diff baseline
            </span>
            <select
              value={defaultBaseline}
              onChange={(event) =>
                setDefaultBaseline(event.target.value as BaselineKind)
              }
              className="h-8 w-full rounded-md border border-border bg-canvas px-2 text-[12px] text-fg outline-none focus:border-accent/70"
            >
              {baselines.map((baseline) => (
                <option key={baseline.value} value={baseline.value}>
                  {baseline.label}
                </option>
              ))}
            </select>
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Transcript width
            </span>
            <select
              value={textWidth}
              onChange={(event) => setTextWidth(event.target.value)}
              className="h-8 w-full rounded-md border border-border bg-canvas px-2 text-[12px] text-fg outline-none focus:border-accent/70"
            >
              <option value="default">Default</option>
              <option value="full">Full</option>
              <option value="custom">Custom</option>
            </select>
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Subthreads per thread
            </span>
            <Input
              type="number"
              min={1}
              max={100}
              value={subthreadsPerThread}
              onChange={(event) => setSubthreadsPerThread(event.target.value)}
            />
          </label>
          <div className="flex items-center gap-2 self-end pb-1.5">
            <Checkbox
              id="send-with-modifier"
              checked={sendWithModifier}
              onCheckedChange={(checked) =>
                setSendWithModifier(checked === true)
              }
            />
            <label
              htmlFor="send-with-modifier"
              className="text-[11.5px] text-muted"
            >
              Send with modifier key
            </label>
          </div>
        </div>
      </Section>

      <Section
        icon={Shield}
        title="Presence and notifications"
        description="Controls what other workspace members can see."
      >
        <div className="space-y-3">
          <div className="flex items-start gap-2">
            <Checkbox
              id="hidden-presence"
              checked={hiddenPresence}
              onCheckedChange={(checked) =>
                setHiddenPresence(checked === true)
              }
            />
            <div>
              <label
                htmlFor="hidden-presence"
                className="text-[11.5px] text-fg"
              >
                Hide my presence
              </label>
              <p className="text-[10.5px] text-faint">
                Your focus and online status will not be shared.
              </p>
            </div>
          </div>
          <div className="flex items-start gap-2">
            <Checkbox
              id="browser-notifications"
              checked={browserNotifications}
              onCheckedChange={(checked) =>
                setBrowserNotifications(checked === true)
              }
            />
            <div>
              <label
                htmlFor="browser-notifications"
                className="text-[11.5px] text-fg"
              >
                Browser notifications
              </label>
              <p className="text-[10.5px] text-faint">
                Ask the browser for permission when a new mention or approval
                arrives.
              </p>
            </div>
          </div>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Webhook URL
            </span>
            <Input
              value={webhookUrl}
              onChange={(event) => setWebhookUrl(event.target.value)}
              placeholder="https://example.com/threadbase"
            />
            <span className="text-[10px] text-faint">
              Optional POST endpoint for approval and mention events.
            </span>
          </label>
        </div>
      </Section>

      {mutation.isError ? (
        <p role="alert" className="text-[11px] text-danger">
          {describeError(mutation.error, "Cannot save settings").description}
        </p>
      ) : null}
      <div className="flex justify-end">
        <Button
          variant="primary"
          size="md"
          loading={mutation.isPending}
          onClick={() => mutation.mutate()}
        >
          Save settings
        </Button>
      </div>
    </div>
  );
}

export function SettingsPage() {
  const me = useMe();
  const state = useWorkspaceState();

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <div className="mx-auto w-full max-w-3xl px-6 py-6">
        <div className="mb-5 space-y-1">
          <h1 className="flex items-center gap-2 text-[20px] font-semibold tracking-[-0.02em] text-fg">
            <Settings2 className="size-5 text-muted" aria-hidden />
            Settings
          </h1>
          <p className="text-[12px] text-muted">
            Workspace defaults, notifications and your local account.
          </p>
        </div>

        <div className="space-y-3">
          <Section
            icon={UserRound}
            title="Account"
            description="Identity used for this workspace session."
          >
            {me.isLoading ? (
              <div className="flex items-center gap-3">
                <Skeleton className="size-8 rounded-full" />
                <div className="space-y-1">
                  <Skeleton className="h-3.5 w-32" />
                  <Skeleton className="h-3 w-24" />
                </div>
              </div>
            ) : me.isError || !me.data ? (
              <ErrorState
                error={me.error}
                title="Cannot load account"
                onRetry={() => void me.refetch()}
                compact
              />
            ) : (
              <div className="flex flex-wrap items-center gap-3">
                <Avatar
                  person={{
                    id: me.data.user.id,
                    displayName: me.data.user.displayName,
                    avatarUrl: me.data.user.avatarUrl,
                  }}
                  size="lg"
                  online
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <p className="text-[12.5px] font-medium text-fg">
                      {me.data.user.displayName}
                    </p>
                    {me.data.user.isAdmin ? (
                      <Badge variant="accent" size="sm">
                        admin
                      </Badge>
                    ) : null}
                  </div>
                  <p className="text-[11px] text-muted">
                    @{me.data.user.username} · {me.data.authMethod}
                  </p>
                </div>
                <Badge variant="outline" size="sm">
                  {me.data.user.hiddenPresence
                    ? "presence hidden"
                    : "presence visible"}
                </Badge>
              </div>
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
            <div className="space-y-2 rounded-xl border border-border bg-panel p-4">
              <Skeleton className="h-4 w-32" />
              <Skeleton className="h-8 w-full" />
              <Skeleton className="h-8 w-2/3" />
            </div>
          ) : state.isError || !state.data ? (
            <div className="rounded-xl border border-border bg-panel">
              <ErrorState
                error={state.error}
                title="Cannot load workspace preferences"
                onRetry={() => void state.refetch()}
              />
            </div>
          ) : (
            <SettingsForm
              key={JSON.stringify(state.data.settings)}
              initial={state.data.settings}
            />
          )}
        </div>
      </div>
    </div>
  );
}
