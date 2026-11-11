import { Group, Loader, Stack, Text } from "@mantine/core";
import type { ReactNode } from "react";
import { Navigate, Outlet, useLocation } from "react-router-dom";
import { useMe } from "@/api/queries";
import { ErrorState, LogoMark } from "@/components/app";
import { ApiError } from "@/lib/api";

export function RequireAuth() {
  const location = useLocation();
  const me = useMe();

  if (me.isLoading) {
    return <FullPageLoading label="Checking session…" />;
  }

  if (me.isError) {
    if (me.error instanceof ApiError && me.error.status === 401) {
      return (
        <Navigate
          to="/login"
          replace
          state={{ from: `${location.pathname}${location.search}` }}
        />
      );
    }
    return (
      <FullPageShell>
        <ErrorState
          error={me.error}
          title="Cannot open Threadbase"
          description="The local service did not answer the session check. Start the Threadbase server and retry."
          onRetry={() => void me.refetch()}
        />
      </FullPageShell>
    );
  }

  return <Outlet />;
}

function FullPageShell({ children }: { children: ReactNode }) {
  return (
    <Stack
      align="center"
      justify="center"
      h="100%"
      maw={448}
      mx="auto"
      p="md"
    >
      {children}
    </Stack>
  );
}

function FullPageLoading({ label }: { label: string }) {
  return (
    <Stack align="center" justify="center" h="100%" gap="xs">
      <LogoMark size={24} />
      <Group gap="xs">
        <Loader size="xs" color="blue" />
        <Text size="xs" c="dimmed">
          {label}
        </Text>
      </Group>
    </Stack>
  );
}
