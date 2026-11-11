import { defineConfig, devices } from "@playwright/test";

/**
 * Concurrency ceiling. Each worker boots its own browser context, so raising
 * this only costs memory. Note that specs which mutate the shared server
 * instance (core.spec.ts creates the admin identity, a project and threads)
 * must keep their state isolated per file — see the note on `fullyParallel`.
 */
const WORKERS = Number(process.env.E2E_WORKERS ?? 32);

export default defineConfig({
  testDir: "./tests/e2e",
  // Tests inside one file share the server-side fixtures (admin identity,
  // project, repo). Parallelising them would race on that state, so keep
  // file-level serialisation while letting separate spec files run in
  // parallel across `WORKERS`.
  fullyParallel: false,
  workers: WORKERS,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: "http://127.0.0.1:7790",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
    colorScheme: "dark",
    // Mantine honours this and zeroes its transition durations, so screenshots
    // and assertions do not have to sleep for animations to settle.
    reducedMotion: "reduce",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        launchOptions: {
          executablePath:
            process.env.E2E_CHROME ?? "/usr/bin/google-chrome-stable",
        },
      },
    },
  ],
  webServer: {
    command: "bash ../scripts/e2e-server.sh",
    url: "http://127.0.0.1:7790/api/health",
    // Always start hermetic: the script wipes /tmp/threadbase-e2e, and the
    // cached cargo/vite builds make a cold start cheap (~4s).
    reuseExistingServer: false,
    timeout: 300_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
