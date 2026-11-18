import {
  Alert,
  Button,
  Center,
  Code,
  Group,
  Loader,
  Stack,
  Text,
  ThemeIcon,
  Title,
} from "@mantine/core";
import { IconLockAccess, IconShieldLock } from "@tabler/icons-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, type ReactNode } from "react";
import { getAuthStatus, getMe, signInWithTailscale } from "../api";

/**
 * The server needs a session cookie, or a development identity on loopback, so
 * the shell never renders data for an anonymous request.
 *
 * There is no sign-in form. If the request came through `tailscale serve`, the
 * server already knows who is asking and the shell trades that for a session.
 * Otherwise it says that the app is only reachable over Tailscale.
 */
export function AuthGate({ children }: { children: ReactNode }) {
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    retry: false,
    staleTime: 60_000,
    // A pending account becomes usable as soon as an administrator approves
    // it, so keep the session query warm until access is granted.
    refetchInterval: (query) =>
      query.state.data?.user.access === "pending" ? 5_000 : false,
    refetchIntervalInBackground: true,
  });

  if (me.isPending) {
    return (
      <Center h="100dvh" className="auth-screen">
        <Loader size="sm" />
      </Center>
    );
  }

  if (me.isError) {
    return <SignIn />;
  }

  // New sign-ups land on `pending` access, and revoked accounts keep a live
  // session on purpose so the shell can explain itself. Both would otherwise
  // render the workspace and fail every request with a 403.
  const access = me.data.user.access;
  if (access === "pending" || access === "disabled") {
    return <AccessScreen access={access} user={me.data.user} />;
  }

  return <>{children}</>;
}

function AccessScreen({
  access,
  user,
}: {
  access: "pending" | "disabled";
  user: { displayName: string; username: string };
}) {
  const pending = access === "pending";
  return (
    <Center h="100dvh" px="md" className="auth-screen">
      <Stack align="center" gap="sm" maw={430}>
        <ThemeIcon size={38} radius="xl" variant="light" color="gray">
          <IconLockAccess size={18} stroke={1.5} />
        </ThemeIcon>
        <Title order={2} size="h4">
          {pending ? "Waiting for approval" : "Access revoked"}
        </Title>
        <Text size="sm" c="dimmed" ta="center">
          {pending
            ? "Your account is recognized, but an administrator has not approved it yet. You will get access as soon as they do."
            : "An administrator removed your access to this workspace."}
        </Text>
        <Text size="sm">
          {user.displayName}{" "}
          <Text component="span" size="sm" c="dimmed">
            @{user.username}
          </Text>
        </Text>
        {pending ? (
          <Text size="xs" c="dimmed" ta="center">
            This page checks again automatically.
          </Text>
        ) : null}
      </Stack>
    </Center>
  );
}

function SignIn() {
  const queryClient = useQueryClient();
  const status = useQuery({
    queryKey: ["auth-status"],
    queryFn: getAuthStatus,
    retry: false,
    staleTime: 60_000,
  });
  const onTailnet = status.data?.status.tailscale === true;
  const identity = status.data?.status.identity;
  const reason = new URLSearchParams(window.location.search).get("auth_error");

  // One exchange per mount. On success the session query is refreshed; on
  // failure the explanation below is shown instead of retrying.
  const exchange = useMutation({
    mutationFn: signInWithTailscale,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["me"] });
      void queryClient.invalidateQueries({ queryKey: ["auth-status"] });
    },
  });
  useEffect(() => {
    if (onTailnet && exchange.isIdle) exchange.mutate();
  }, [onTailnet, exchange]);

  return (
    <Center h="100dvh" px="md" className="auth-screen">
      <Stack align="center" gap="sm" maw={430}>
        <ThemeIcon size={38} radius="xl" variant="light" color="gray">
          <IconShieldLock size={18} stroke={1.5} />
        </ThemeIcon>
        <Title order={2} size="h4">
          {onTailnet ? "Signing in…" : "Open this through Tailscale"}
        </Title>
        {onTailnet ? (
          <Text size="sm" c="dimmed" ta="center">
            {identity ? `Recognized as ${identity}.` : "Recognized on the tailnet."}
            {exchange.isPending ? " Creating your session…" : null}
          </Text>
        ) : (
          <Text size="sm" c="dimmed" ta="center">
            Threadbase listens on the machine&apos;s loopback port and is
            reachable only through <Code>tailscale serve</Code>. Open the
            tailnet URL for this machine — the address comes from{" "}
            <Code>tailscale status</Code> — and you are signed in as your
            Tailscale identity.
          </Text>
        )}

        {reason ? (
          <Alert color="red" variant="light" w="100%" p="xs">
            <Text size="xs">{reason}</Text>
          </Alert>
        ) : null}

        {exchange.isError ? (
          <Alert color="red" variant="light" w="100%" p="xs">
            <Text size="xs">
              {exchange.error instanceof Error
                ? exchange.error.message
                : "Could not sign in."}
            </Text>
          </Alert>
        ) : null}

        {status.isPending || exchange.isPending ? <Loader size="xs" /> : null}

        {status.isSuccess && !onTailnet ? (
          <Stack align="center" gap={4}>
            <Text size="xs" c="dimmed" ta="center">
              Developing on this machine? Set <Code>THREADBASE_DEV_IDENTITY</Code>{" "}
              and start the server directly; the shell signs itself in over
              loopback.
            </Text>
            <Group mt={4}>
              <Button
                variant="default"
                onClick={() => {
                  void status.refetch();
                }}
              >
                Try again
              </Button>
            </Group>
          </Stack>
        ) : null}
      </Stack>
    </Center>
  );
}
