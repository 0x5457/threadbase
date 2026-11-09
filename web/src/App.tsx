import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { api } from "./lib/api";

type Health = {
  ok: boolean;
  version: string;
  needsSetup: boolean;
};

function useHealth() {
  return useQuery({
    queryKey: ["health"],
    queryFn: () => api<Health>("/api/health"),
    retry: 0,
  });
}

function ThemeToggle() {
  const [theme, setTheme] = useState(
    () => localStorage.getItem("threadbase.theme") ?? "dark",
  );

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("threadbase.theme", theme);
  }, [theme]);

  return (
    <button
      type="button"
      className="rounded-md border border-border px-3 py-1.5 text-muted transition-colors hover:bg-hover hover:text-fg"
      onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
    >
      {theme === "dark" ? "Light" : "Dark"}
    </button>
  );
}

export default function App() {
  const health = useHealth();

  return (
    <div className="relative flex min-h-full flex-col overflow-hidden bg-canvas">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_50%_-20%,rgba(138,164,255,0.10),transparent_55%)]"
      />
      <header className="relative flex h-12 items-center justify-between border-b border-border px-5">
        <div className="flex items-center gap-2.5">
          <LogoMark />
          <span className="text-[13px] font-medium tracking-tight">
            Threadbase
          </span>
        </div>
        <ThemeToggle />
      </header>
      <main className="relative flex flex-1 items-center justify-center p-8">
        <section className="w-full max-w-[560px]">
          <div className="mb-5 flex items-center gap-3">
            <div className="flex size-10 items-center justify-center rounded-xl border border-border bg-surface">
              <LogoMark size={22} />
            </div>
            <div>
              <h1 className="text-[22px] font-semibold tracking-[-0.02em]">
                Shared coding agent threads
              </h1>
              <p className="text-muted">
                One local workspace. Every teammate on the same thread.
              </p>
            </div>
          </div>
          <div className="rounded-xl border border-border bg-panel">
            <div className="flex items-center justify-between border-b border-border px-4 py-3">
              <span className="text-muted">Local service</span>
              <StatusPill
                state={
                  health.isLoading
                    ? "loading"
                    : health.isError
                      ? "error"
                      : "ready"
                }
                label={
                  health.isLoading
                    ? "Connecting"
                    : health.isError
                      ? "Unavailable"
                      : `v${health.data?.version ?? ""}`
                }
              />
            </div>
            <div className="space-y-2 px-4 py-4 text-muted">
              <p>
                The workspace is being prepared. Projects, threads and agents
                will appear here.
              </p>
              {health.isError ? (
                <p className="text-danger">
                  Cannot reach the Threadbase service. Start it with{" "}
                  <code className="rounded bg-raised px-1.5 py-0.5 text-fg">
                    threadbase
                  </code>
                  .
                </p>
              ) : null}
            </div>
          </div>
        </section>
      </main>
    </div>
  );
}

function StatusPill({
  state,
  label,
}: {
  state: "loading" | "ready" | "error";
  label: string;
}) {
  const color =
    state === "ready"
      ? "bg-success"
      : state === "error"
        ? "bg-danger"
        : "bg-warning";
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-surface px-2 py-0.5 text-[11px] text-muted">
      <span className={`size-1.5 rounded-full ${color}`} />
      {label}
    </span>
  );
}

export function LogoMark({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden
    >
      <rect x="3" y="3" width="18" height="18" rx="5" stroke="currentColor" />
      <path
        d="M7.5 9.25h9M7.5 12h5.5M7.5 14.75h7"
        stroke="currentColor"
        strokeLinecap="round"
      />
    </svg>
  );
}
