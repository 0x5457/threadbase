/**
 * Thread-open benchmark.
 *
 * Opening a thread is the app's heaviest first paint: the transcript mounts
 * markdown, code highlighting and (for edited files) the Pierre diff stack.
 * This script measures that paint the way a mid-range phone experiences it —
 * 6x CPU throttle and a 390x844 viewport — and reports long-animation-frame
 * data, so a change can be judged by numbers instead of feel.
 *
 *   THREADBASE_SESSION=<cookie> node audit/perf-thread-open.mjs \
 *     --url http://127.0.0.1:7777/p/<project>/t/<thread> [--cpu 6] [--runs 3]
 *
 * The session cookie is the browser cookie, not the token hash:
 * open devtools → Application → Cookies → `threadbase_session`.
 * Nothing is written to the server; the script only reads.
 */
import { chromium } from "@playwright/test";

const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index].replace(/^--/, ""), process.argv[index + 1]);
}

const url = args.get("url") ?? process.env.THREADBASE_URL;
const session = args.get("session") ?? process.env.THREADBASE_SESSION;
const cpu = Number(args.get("cpu") ?? process.env.PERF_CPU ?? 6);
const runs = Number(args.get("runs") ?? process.env.PERF_RUNS ?? 3);
const mobile = (args.get("mobile") ?? "1") !== "0";

if (!url) {
  console.error("Missing --url <thread url>");
  process.exit(2);
}

const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH || undefined,
  args: ["--disable-gpu"],
});

const origin = new URL(url).origin;
const results = [];

for (let run = 0; run < runs; run += 1) {
  const context = await browser.newContext(
    mobile
      ? {
          viewport: { width: 390, height: 844 },
          deviceScaleFactor: 2,
          isMobile: true,
          hasTouch: true,
        }
      : { viewport: { width: 1440, height: 900 } },
  );
  if (session) {
    await context.addCookies([
      {
        name: "threadbase_session",
        value: session,
        domain: new URL(url).hostname,
        path: "/",
      },
    ]);
  }
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  if (cpu > 1) await cdp.send("Emulation.setCPUThrottlingRate", { rate: cpu });

  await page.addInitScript(() => {
    window.__lafs = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        window.__lafs.push({
          duration: Math.round(entry.duration),
          blocking: Math.round(entry.blockingDuration),
          scripts: entry.scripts.map((script) => ({
            duration: Math.round(script.duration),
            url: (script.sourceURL || "").split("/").pop(),
          })),
        });
      }
    }).observe({ type: "long-animation-frame" });
  });

  const started = Date.now();
  await page.goto(url, { waitUntil: "load" });
  await page.waitForTimeout(Number(args.get("wait") ?? 5_000));

  const measured = await page.evaluate(() => {
    const paint = Object.fromEntries(
      performance
        .getEntriesByType("paint")
        .map((entry) => [entry.name, Math.round(entry.startTime)]),
    );
    return {
      firstContentfulPaint: paint["first-contentful-paint"] ?? 0,
      frames: window.__lafs,
    };
  });

  const scripts = new Map();
  for (const frame of measured.frames) {
    for (const script of frame.scripts) {
      scripts.set(script.url, (scripts.get(script.url) ?? 0) + script.duration);
    }
  }

  results.push({
    wall: Date.now() - started,
    fcp: measured.firstContentfulPaint,
    blocking: measured.frames.reduce((sum, frame) => sum + frame.blocking, 0),
    longFrames: measured.frames.length,
    worstFrame: Math.max(0, ...measured.frames.map((frame) => frame.duration)),
    // The tail is where the transcript render shows up; total script time is
    // the honest measure of "how much work did this paint cost".
    scripts: [...scripts.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 6)
      .map(([file, ms]) => `${ms}ms ${file}`),
  });
  await context.close();
}

const median = (pick) => {
  const values = results.map(pick).sort((a, b) => a - b);
  return values[Math.floor(values.length / 2)] ?? 0;
};

console.log(
  JSON.stringify(
    {
      url,
      origin,
      cpu,
      runs: results,
      median: {
        fcp: median((r) => r.fcp),
        blocking: median((r) => r.blocking),
        longFrames: median((r) => r.longFrames),
        worstFrame: median((r) => r.worstFrame),
      },
    },
    null,
    2,
  ),
);

await browser.close();
