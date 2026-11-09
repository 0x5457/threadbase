import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, LockKeyhole, ShieldCheck } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { login, setupAdmin } from "@/api/endpoints";
import { queryKeys, useAuthStatus, useMe } from "@/api/queries";
import type { AuthMeDto } from "@/api/types/AuthMeDto";
import {
  Badge,
  Button,
  ErrorState,
  Input,
  LogoMark,
} from "@/components/ui";
import { ThemeToggle } from "@/components/shell/ThemeToggle";
import { describeError } from "@/lib/errors";

interface LoginLocationState {
  from?: string;
}

export function LoginPage() {
  const navigate = useNavigate();
  const location = useLocation();
  const queryClient = useQueryClient();
  const status = useAuthStatus();
  const me = useMe();
  const [displayName, setDisplayName] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [validation, setValidation] = useState<string | null>(null);

  const from =
    (location.state as LoginLocationState | null)?.from ?? "/";

  const completeLogin = (result: { user: AuthMeDto["user"] }) => {
    queryClient.setQueryData<AuthMeDto>(queryKeys.me, {
      user: result.user,
      authMethod: "password",
      tailscaleAvailable: false,
      needsSetup: false,
    });
    navigate(from, { replace: true });
  };

  const loginMutation = useMutation({
    mutationFn: () => login({ username: username.trim(), password }),
    onSuccess: completeLogin,
  });
  const setupMutation = useMutation({
    mutationFn: () =>
      setupAdmin({
        username: username.trim(),
        password,
        displayName: displayName.trim(),
      }),
    onSuccess: completeLogin,
  });

  if (status.isLoading || me.isLoading) {
    return <LoginLoading />;
  }

  if (me.data) {
    return <Navigate to={from} replace />;
  }

  if (status.isError) {
    return (
      <LoginFrame>
        <ErrorState
          error={status.error}
          title="Cannot reach Threadbase"
          description="The local service did not answer the auth status check."
          onRetry={() => void status.refetch()}
        />
      </LoginFrame>
    );
  }

  if (!status.data) {
    return <LoginLoading />;
  }

  const auth = status.data.status;
  const needsSetup = auth.needsSetup;
  const mutation = needsSetup ? setupMutation : loginMutation;

  const submit = () => {
    if (needsSetup) {
      if (!displayName.trim()) {
        setValidation("Enter a display name.");
        return;
      }
      if (username.trim().length < 3 || username.trim().length > 32) {
        setValidation("Username must be between 3 and 32 characters.");
        return;
      }
      if (!/^[A-Za-z0-9._-]+$/.test(username.trim())) {
        setValidation(
          "Username may contain letters, digits, dots, dashes and underscores.",
        );
        return;
      }
      if (password.length < 10) {
        setValidation("Password must be at least 10 characters.");
        return;
      }
      if (password !== confirmPassword) {
        setValidation("Passwords do not match.");
        return;
      }
    } else {
      if (!username.trim() || !password) {
        setValidation("Enter your username and password.");
        return;
      }
    }
    setValidation(null);
    mutation.mutate();
  };

  return (
    <LoginFrame>
      <div className="mb-5 space-y-1.5">
        <div className="flex items-center gap-2">
          <h1 className="text-[18px] font-semibold tracking-[-0.01em] text-fg">
            {needsSetup ? "Set up your workspace" : "Sign in to Threadbase"}
          </h1>
          {needsSetup ? (
            <Badge variant="accent" size="sm">
              first run
            </Badge>
          ) : null}
        </div>
        <p className="text-[12px] leading-relaxed text-muted">
          {needsSetup
            ? "Create the admin account for this local workspace. Only the machine running the service can complete setup."
            : "Use your Threadbase account to open shared threads and continue work."}
        </p>
      </div>

      {!needsSetup && auth.tailscaleHeader ? (
        <div className="mb-4 flex items-start gap-2 rounded-md border border-info/25 bg-info/5 px-2.5 py-2 text-[11px] text-muted">
          <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-info" />
          <span>
            Tailscale identity detected. If this browser is already trusted,
            the workspace opens automatically after sign-in.
          </span>
        </div>
      ) : null}

      {!auth.localLogin ? (
        <div className="mb-4 rounded-md border border-warning/30 bg-warning/5 px-2.5 py-2 text-[11px] text-muted">
          Local password login is disabled from this address. Open Threadbase
          through Tailscale or from the host machine.
        </div>
      ) : null}

      <form
        className="space-y-3"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        {needsSetup ? (
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Display name
            </span>
            <Input
              value={displayName}
              onChange={(event) => setDisplayName(event.target.value)}
              placeholder="Ada Lovelace"
              autoComplete="name"
              autoFocus
            />
          </label>
        ) : null}
        <label className="block space-y-1.5">
          <span className="text-[11px] font-medium text-muted">Username</span>
          <Input
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            placeholder="ada"
            autoComplete="username"
            autoCapitalize="none"
            spellCheck={false}
            autoFocus={!needsSetup}
          />
        </label>
        <label className="block space-y-1.5">
          <span className="text-[11px] font-medium text-muted">Password</span>
          <Input
            type="password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            placeholder={needsSetup ? "At least 10 characters" : "Your password"}
            autoComplete={needsSetup ? "new-password" : "current-password"}
          />
        </label>
        {needsSetup ? (
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Confirm password
            </span>
            <Input
              type="password"
              value={confirmPassword}
              onChange={(event) => setConfirmPassword(event.target.value)}
              placeholder="Repeat the password"
              autoComplete="new-password"
            />
          </label>
        ) : null}

        {validation ? (
          <p role="alert" className="text-[11px] text-danger">
            {validation}
          </p>
        ) : null}
        {mutation.isError ? (
          <p role="alert" className="text-[11px] text-danger">
            {describeError(
              mutation.error,
              needsSetup ? "Setup failed" : "Sign in failed",
            ).description}
          </p>
        ) : null}

        <Button
          type="submit"
          variant="primary"
          size="lg"
          className="w-full"
          loading={mutation.isPending}
          disabled={!auth.localLogin}
        >
          {needsSetup ? "Create workspace" : "Sign in"}
          {!mutation.isPending ? <ArrowRight aria-hidden /> : null}
        </Button>
      </form>

      <div className="mt-4 flex items-center gap-2 border-t border-border pt-3 text-[10.5px] text-faint">
        <LockKeyhole className="size-3" aria-hidden />
        <span>
          Sessions are stored locally. Tailscale is the only network boundary.
        </span>
      </div>
    </LoginFrame>
  );
}

function LoginFrame({ children }: { children: ReactNode }) {
  return (
    <div className="relative flex min-h-full items-center justify-center overflow-hidden bg-canvas p-6">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_50%_-10%,rgb(138_164_255/0.12),transparent_55%)]"
      />
      <div className="absolute right-3 top-3 z-10">
        <ThemeToggle />
      </div>
      <div className="relative w-full max-w-[400px]">
        <div className="mb-5 flex items-center gap-2.5">
          <span className="flex size-9 items-center justify-center rounded-xl border border-border bg-panel text-fg shadow-sm">
            <LogoMark size={20} />
          </span>
          <div>
            <p className="text-[13px] font-semibold tracking-tight text-fg">
              Threadbase
            </p>
            <p className="text-[10.5px] text-faint">
              Shared coding agent threads
            </p>
          </div>
        </div>
        <div className="rounded-xl border border-border bg-panel p-5 shadow-2xl shadow-black/10">
          {children}
        </div>
        <p className="mt-3 text-center text-[10px] text-faint">
          One local workspace. Every teammate on the same thread.
        </p>
      </div>
    </div>
  );
}

function LoginLoading() {
  return (
    <div className="flex min-h-full flex-col items-center justify-center gap-3 bg-canvas text-muted">
      <span className="flex size-10 items-center justify-center rounded-xl border border-border bg-panel">
        <LogoMark size={22} />
      </span>
      <p className="flex items-center gap-2 text-[12px]">
        <span className="size-1.5 animate-pulse rounded-full bg-accent" />
        Connecting to Threadbase…
      </p>
    </div>
  );
}
