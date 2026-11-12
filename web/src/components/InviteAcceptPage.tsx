import {
  Button,
  Card,
  Center,
  Loader,
  Stack,
  Text,
  ThemeIcon,
} from "@mantine/core";
import { IconCheck, IconMail } from "@tabler/icons-react";
import { useState } from "react";
import { acceptInvite } from "../api";

export function InviteAcceptPage({ token }: { token: string }) {
  const [state, setState] = useState<"idle" | "busy" | "done" | "error">(
    "idle",
  );
  const [error, setError] = useState<string | null>(null);

  const accept = async () => {
    setState("busy");
    setError(null);
    try {
      await acceptInvite(token);
      setState("done");
      window.setTimeout(() => {
        window.location.href = "/";
      }, 700);
    } catch (cause) {
      setState("error");
      setError(cause instanceof Error ? cause.message : "Invite is invalid");
    }
  };

  return (
    <Center h="100dvh" px="md" className="auth-screen">
      <Card withBorder p="lg" maw={420} w="100%">
        <Stack align="center" gap="sm">
          <ThemeIcon size={38} radius="xl" variant="light" color="gray">
            <IconMail size={18} stroke={1.5} />
          </ThemeIcon>
          <Text size="md" fw={500}>
            Join this thread
          </Text>
          <Text size="sm" c="dimmed" ta="center">
            You were invited to a shared Threadbase thread. Accepting adds you
            as a member of the thread.
          </Text>
          {state === "done" ? (
            <Stack align="center" gap={4}>
              <IconCheck size={18} stroke={2} color="var(--add)" />
              <Text size="sm">Joined. Redirecting to the workspace…</Text>
            </Stack>
          ) : state === "busy" ? (
            <Loader size="sm" />
          ) : (
            <>
              {error ? (
                <Text size="sm" c="red" ta="center">
                  {error}
                </Text>
              ) : null}
              <Button color="dark" onClick={() => void accept()}>
                Accept invite
              </Button>
            </>
          )}
        </Stack>
      </Card>
    </Center>
  );
}
