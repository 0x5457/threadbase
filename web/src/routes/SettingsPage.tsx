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
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
} from "@/components/ui";
import { setTheme, useTheme, type Theme } from "@/lib/theme";
import { describeError } from "@/lib/errors";
import { ApprovalRulesSection } from "@/components/workspace/ApprovalRulesSection";
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
    <section className="rounded-xl border border-border/40 bg-card/30">
      <div className="flex items-start gap-2.5 border-b border-border/40 px-4 py-3">
        <span className="flex size-7 shrink-0 items-center justify-center text-muted-foreground">
          <Icon className="size-3.5" />
        </span>
        <div>
          <h2 className="text-[13px] font-semibold text-foreground">{title}</h2>
          {description ? (
            <p className="mt-0.5 text-[11.5px] text-muted-foreground">{description}</p>
          ) : null}
        </div>
      </div>
      <div className="px-4 py-4">{children}</div>
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
      <p className="text-[13px] font-medium text-muted-foreground">Theme</p>
      <div className="flex gap-1.5">
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            onClick={() => setTheme(option.value)}
            aria-pressed={theme === option.value}
            className={cn(
              "flex h-8 min-w-20 items-center justify-center gap-1.5 rounded-md border px-3 text-[13px] transition-colors",
              theme === option.value
                ? "border-primary/50 bg-primary/10 text-fg"
                : "border-border bg-canvas text-muted-foreground hover:border-border-strong hover:text-fg",
            )}
          >
            {theme === option.value ? <Check className="size-3" /> : null}
            {option.label}
          </button>
        ))}
      </div>
      <p className="text-[13px] text-faint">
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
            <span className="text-[13px] font-medium text-muted-foreground">
              Default agent
            </span>
            <Input
              value={defaultAgent}
              onChange={(event) => setDefaultAgent(event.target.value)}
              placeholder="Agent name"
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[13px] font-medium text-muted-foreground">
              Default model
            </span>
            <Input
              value={defaultModel}
              onChange={(event) => setDefaultModel(event.target.value)}
              placeholder="Agent default"
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[13px] font-medium text-muted-foreground">
              Default diff baseline
            </span>
            <Select
              value={defaultBaseline}
              onValueChange={(value) =>
                setDefaultBaseline(value as BaselineKind)
              }
            >
              <SelectTrigger className="h-9 w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {baselines.map((baseline) => (
                  <SelectItem key={baseline.value} value={baseline.value}>
                    {baseline.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </label>
          <label className="block space-y-1.5">
            <span className="text-[13px] font-medium text-muted-foreground">
              Transcript width
            </span>
            <Select value={textWidth} onValueChange={setTextWidth}>
              <SelectTrigger className="h-9 w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="default">Default</SelectItem>
                <SelectItem value="full">Full</SelectItem>
                <SelectItem value="custom">Custom</SelectItem>
              </SelectContent>
            </Select>
          </label>
          <label className="block space-y-1.5">
            <span className="text-[13px] font-medium text-muted-foreground">
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
              className="text-[13px] text-muted-foreground"
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
                className="text-[13px] text-fg"
              >
                Hide my presence
              </label>
              <p className="text-[13px] text-faint">
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
                className="text-[13px] text-fg"
              >
                Browser notifications
              </label>
              <p className="text-[13px] text-faint">
                Ask the browser for permission when a new mention or approval
                arrives.
              </p>
            </div>
          </div>
          <label className="block space-y-1.5">
            <span className="text-[13px] font-medium text-muted-foreground">
              Webhook URL
            </span>
            <Input
              value={webhookUrl}
              onChange={(event) => setWebhookUrl(event.target.value)}
              placeholder="https://example.com/threadbase"
            />
            <span className="text-[13px] text-faint">
              Optional POST endpoint for approval and mention events.
            </span>
          </label>
        </div>
      </Section>

      {mutation.isError ? (
        <p role="alert" className="text-[13px] text-danger">
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
      <div className="mx-auto w-full max-w-3xl px-6 py-10">
        <div className="mb-6 space-y-1">
          <h1 className="flex items-center gap-2 text-[24px] font-semibold tracking-[-0.03em] text-foreground">
            <Settings2 className="size-5 text-muted-foreground" aria-hidden />
            Settings
          </h1>
          <p className="text-[12px] text-muted-foreground">
            Workspace defaults and your local account.
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
                    <p className="text-[13px] font-medium text-fg">
                      {me.data.user.displayName}
                    </p>
                    {me.data.user.isAdmin ? (
                      <Badge variant="accent" size="sm">
                        admin
                      </Badge>
                    ) : null}
                  </div>
                  <p className="text-[13px] text-muted-foreground">
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
            <div className="space-y-2 rounded-xl border border-border/40 bg-card/30 p-4">
              <Skeleton className="h-4 w-32" />
              <Skeleton className="h-8 w-full" />
              <Skeleton className="h-8 w-2/3" />
            </div>
          ) : state.isError || !state.data ? (
            <div className="rounded-xl border border-border/40 bg-card/30">
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

          {me.data?.user.isAdmin ? (
            <Section
              icon={Shield}
              title="Standing approval rules"
              description="Rules created by “Allow for thread”. Revoke them when an agent should ask again."
            >
              <ApprovalRulesSection />
            </Section>
          ) : null}
        </div>
      </div>
    </div>
  );
}
