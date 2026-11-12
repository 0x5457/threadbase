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
import {
  IconBrandGithub,
  IconClock,
  IconLogout,
  IconShieldLock,
  IconUserOff,
} from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { getAuthStatus, getMe, logout, type User } from "../api";

/**
 * The server accepts either a GitHub session cookie or an explicitly
 * configured development identity, so the shell never renders data for an
 * anonymous caller. The sign-in screen explains which of the two is available
 * and surfaces whatever reason the OAuth callback redirected back with.
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

  if (me.data.user.access === "pending") {
    return <AccessPending user={me.data.user} />;
  }

  if (me.data.user.access === "disabled") {
    return <AccessRevoked user={me.data.user} />;
  }

  return <>{children}</>;
}

function signOut() {
  void logout().then(() => {
    window.location.reload();
  });
}

function AccessPending({ user }: { user: User }) {
  return (
    <Center h="100dvh" px="md" className="auth-screen">
      <Stack align="center" gap="sm" maw={430}>
        <ThemeIcon size={38} radius="xl" variant="light" color="yellow">
          <IconClock size={18} stroke={1.5} />
        </ThemeIcon>
        <Title order={2} size="h4">
          Waiting for approval
        </Title>
        <Text size="sm" c="dimmed" ta="center">
          An administrator must approve your account before you can open
          threads. You can leave this page open — it checks again automatically.
        </Text>
        <Text size="sm">
          {user.displayName}{" "}
          <Text component="span" size="sm" c="dimmed">
            @{user.username}
          </Text>
        </Text>
        <Button
          variant="default"
          leftSection={<IconLogout size={15} stroke={1.5} />}
          onClick={signOut}
        >
          Sign out
        </Button>
      </Stack>
    </Center>
  );
}

function AccessRevoked({ user }: { user: User }) {
  return (
    <Center h="100dvh" px="md" className="auth-screen">
      <Stack align="center" gap="sm" maw={430}>
        <ThemeIcon size={38} radius="xl" variant="light" color="red">
          <IconUserOff size={18} stroke={1.5} />
        </ThemeIcon>
        <Title order={2} size="h4">
          Access revoked
        </Title>
        <Text size="sm" c="dimmed" ta="center">
          Your access to this workspace has been revoked. Ask an administrator
          to restore it if you think this is a mistake.
        </Text>
        <Text size="sm">
          {user.displayName}{" "}
          <Text component="span" size="sm" c="dimmed">
            @{user.username}
          </Text>
        </Text>
        <Button
          variant="default"
          leftSection={<IconLogout size={15} stroke={1.5} />}
          onClick={signOut}
        >
          Sign out
        </Button>
      </Stack>
    </Center>
  );
}

function SignIn() {
  const status = useQuery({
    queryKey: ["auth-status"],
    queryFn: getAuthStatus,
    retry: false,
    staleTime: 60_000,
  });
  const reason = new URLSearchParams(window.location.search).get("auth_error");
  const github = status.data?.status.github === true;
  const loginUrl = status.data?.status.loginUrl;

  return (
    <Center h="100dvh" px="md" className="auth-screen">
      <Stack align="center" gap="sm" maw={430}>
        <ThemeIcon size={38} radius="xl" variant="light" color="gray">
          <IconShieldLock size={18} stroke={1.5} />
        </ThemeIcon>
        <Title order={2} size="h4">
          Sign in to this workspace
        </Title>
        <Text size="sm" c="dimmed" ta="center">
          Threadbase keeps a server-side session after you authorize with
          GitHub. No Tailscale client is required on the device you browse
          from.
        </Text>

        {reason ? (
          <Alert color="red" variant="light" w="100%" p="xs">
            <Text size="xs">{reason}</Text>
          </Alert>
        ) : null}

        {status.isPending ? <Loader size="xs" /> : null}

        {github && loginUrl ? (
          <Group mt={4}>
            <Button
              leftSection={<IconBrandGithub size={16} stroke={1.5} />}
              onClick={() => {
                window.location.href = loginUrl;
              }}
            >
              Continue with GitHub
            </Button>
          </Group>
        ) : null}

        {status.isSuccess && !github ? (
          <Stack align="center" gap={4}>
            <Text size="xs" c="dimmed" ta="center">
              GitHub sign-in is not configured on this server. Set{" "}
              <Code>THREADBASE_GITHUB_CLIENT_ID</Code>,{" "}
              <Code>THREADBASE_GITHUB_CLIENT_SECRET</Code> and{" "}
              <Code>THREADBASE_PUBLIC_ORIGIN</Code>, or use{" "}
              <Code>THREADBASE_DEV_IDENTITY</Code> while developing locally.
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
