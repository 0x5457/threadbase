import { Alert, Button, Code, Stack, Text, Title } from "@mantine/core";
import { RotateCw, ShieldCheck, ShieldX } from "lucide-react";
import { useEffect, type ReactNode } from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { useAuthStatus, useMe } from "@/api/queries";
import { LogoMark } from "@/components/app";
import { ThemeToggle } from "@/components/shell/ThemeToggle";

interface LoginLocationState {
  from?: string;
}

/**
 * Sign-in is performed by `tailscale serve`, which forwards the tailnet
 * identity of the caller. This page only explains what to do when that
 * identity is missing or was rejected.
 */
export function LoginPage() {
  const navigate = useNavigate();
  const location = useLocation();
  const status = useAuthStatus();
  const me = useMe();
  const from = (location.state as LoginLocationState | null)?.from ?? "/";
  const signedIn = Boolean(me.data);

  useEffect(() => {
    if (signedIn) {
      navigate(from, { replace: true });
    }
  }, [from, navigate, signedIn]);

  if (me.data) {
    return <Navigate to={from} replace />;
  }

  const auth = status.data?.status;
  const behindTailscale = auth?.tailscaleHeader ?? false;
  const devIdentity = auth?.devIdentity ?? false;

  return (
    <Page>
      <Stack gap={6} mb="lg">
        <Title order={1} fz={22} fw={600} style={{ letterSpacing: "-0.02em" }}>
          Sign in with Tailscale
        </Title>
        <Text size="xs" c="dimmed">
          Threadbase has no passwords. Access is granted by your tailnet
          identity.
        </Text>
      </Stack>

      {status.isError ? (
        <Alert
          color="red"
          variant="light"
          icon={<ShieldX size={14} />}
          mb="md"
        >
          <Text size="xs">
            Cannot reach the Threadbase service. Start it on the host machine
            and retry.
          </Text>
        </Alert>
      ) : behindTailscale ? (
        <Alert
          color="red"
          variant="light"
          icon={<ShieldX size={14} />}
          title="Identity not accepted"
          mb="md"
        >
          <Stack gap={6}>
            <Text size="xs">
              Tailscale forwarded this request, but the server did not accept
              the identity headers. Check that the service is reached through
              <Code>tailscale serve</Code> on the host, not a raw port forward.
            </Text>
          </Stack>
        </Alert>
      ) : devIdentity ? (
        <Alert
          color="yellow"
          variant="light"
          icon={<ShieldCheck size={14} />}
          title="Development identity enabled"
          mb="md"
        >
          <Text size="xs">
            Requests from this machine use the configured development identity.
            Reload after starting the server, or open the workspace through
            Tailscale.
          </Text>
        </Alert>
      ) : (
        <Alert
          color="blue"
          variant="light"
          icon={<ShieldCheck size={14} />}
          title="Open this workspace through Tailscale"
          mb="md"
        >
          <Stack gap={8}>
            <Text size="xs">
              This service only accepts requests proxied by Tailscale on the
              host machine. Open the tailnet address instead of a local port.
            </Text>
            <Code block>
              tailscale serve --bg 443 http://127.0.0.1:7777
            </Code>
            <Text size="xs" c="dimmed">
              Then browse to{" "}
              <Code>https://&lt;machine&gt;.&lt;tailnet&gt;.ts.net</Code>.
            </Text>
          </Stack>
        </Alert>
      )}

      {auth?.needsAdmin ? (
        <Alert color="grape" variant="light" mb="md">
          <Text size="xs">
            No administrator yet. The first tailnet identity that signs in
            becomes the administrator of this instance.
          </Text>
        </Alert>
      ) : null}

      <Button
        fullWidth
        variant="filled"
        leftSection={<RotateCw size={14} />}
        loading={status.isFetching || me.isFetching}
        onClick={() => {
          void status.refetch();
          void me.refetch();
        }}
      >
        Retry sign-in
      </Button>

      <Text size="xs" c="dimmed" mt="lg" ta="center">
        Signed in automatically while your tailnet session is valid.
      </Text>
    </Page>
  );
}

function Page({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        minHeight: "100dvh",
        alignItems: "center",
        justifyContent: "center",
        overflowY: "auto",
        padding: "max(24px, env(safe-area-inset-top)) 16px",
      }}
    >
      <div
        style={{
          position: "absolute",
          right: "max(12px, env(safe-area-inset-right))",
          top: "max(12px, env(safe-area-inset-top))",
          zIndex: 10,
        }}
      >
        <ThemeToggle />
      </div>
      <div
        style={{
          position: "relative",
          width: "100%",
          maxWidth: 420,
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 10,
            marginBottom: 28,
          }}
        >
          <LogoMark size={22} />
          <Text size="md" fw={600} style={{ letterSpacing: "-0.01em" }}>
            Threadbase
          </Text>
        </div>
        {children}
      </div>
    </div>
  );
}
