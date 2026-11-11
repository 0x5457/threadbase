import { Button, Stack } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useEffect } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { api, ApiError } from "@/lib/api";
import { EmptyState } from "@/components/app";
import { useMutation } from "@tanstack/react-query";

/**
 * Map the server's error to a human sentence. Invite links expire and are
 * single-use, so 403/404 read as "invalid or expired" rather than echoing
 * `ApiError: origin is not allowed` into the page.
 */
function inviteErrorText(error: unknown): string {
  if (error instanceof ApiError && (error.status === 403 || error.status === 404)) {
    return "This invite link is invalid or has expired. Ask a workspace admin for a new one.";
  }
  if (error instanceof ApiError && error.status === 401) {
    return "Sign in first, then reopen this invite link.";
  }
  if (error instanceof ApiError) {
    return error.message;
  }
  return "The invite could not be accepted. Try again or ask a workspace admin for a new link.";
}

export function InvitePage() {
  const { token } = useParams();
  const navigate = useNavigate();
  const accept = useMutation({
    mutationFn: () =>
      api(`/api/invites/${encodeURIComponent(token ?? "")}/accept`, {
        method: "POST",
      }),
    onSuccess: () => {
      notifications.show({
        message: "Invite accepted",
        color: "green",
      });
      navigate("/", { replace: true });
    },
  });

  useEffect(() => {
    if (token) {
      accept.mutate();
    }
    // The mutation object is intentionally not a dependency; it would re-run on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  return (
    <Stack align="center" justify="center" h="100%" p="lg">
      {accept.isError ? (
        <EmptyState
          title="Invite could not be accepted"
          description={inviteErrorText(accept.error)}
          action={
            <Button
              size="xs"
              variant="default"
              onClick={() => navigate("/")}
            >
              Back to workspace
            </Button>
          }
        />
      ) : (
        <EmptyState
          title="Joining thread…"
          description="Checking the invite and adding you to the thread."
        />
      )}
    </Stack>
  );
}
