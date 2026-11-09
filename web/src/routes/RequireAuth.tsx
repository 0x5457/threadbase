import type { ReactNode } from "react";
import { Navigate, Outlet, useLocation } from "react-router-dom";
import { useMe } from "@/api/queries";
import { ErrorState, LogoMark } from "@/components/ui";
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
    <div className="flex min-h-full items-center justify-center bg-canvas p-6">
      <div className="w-full max-w-md rounded-xl border border-border bg-panel p-6">
        {children}
      </div>
    </div>
  );
}

function FullPageLoading({ label }: { label: string }) {
  return (
    <div className="flex min-h-full flex-col items-center justify-center gap-3 bg-canvas text-muted">
      <span className="flex size-10 items-center justify-center rounded-xl border border-border bg-panel">
        <LogoMark size={22} />
      </span>
      <p className="flex items-center gap-2 text-[12px]">
        <span className="size-1.5 animate-pulse rounded-full bg-accent" />
        {label}
      </p>
    </div>
  );
}
