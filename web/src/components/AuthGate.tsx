import {
  Alert,
  Box,
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
import { IconBrandGithub, IconShieldLock } from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { getAuthStatus, getMe } from "../api";

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

  return <>{children}</>;
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
          <Box mt={4}>
            <Button
              leftSection={<IconBrandGithub size={16} stroke={1.5} />}
              onClick={() => {
                window.location.href = loginUrl;
              }}
            >
              Continue with GitHub
            </Button>
          </Box>
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
