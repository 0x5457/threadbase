/**
 * Mobile navigation benchmark: route switches and the nav drawer.
 *
 * The phone build is the slowest target, so this drives a 390x844 touch
 * context under CPU throttling and reports long animation frames for each
 * interaction. It is read-only: it clicks through the app, it does not send
 * messages or change thread state.
 *
 *   node audit/perf-mobile.mjs --url http://127.0.0.1:7777 --cpu 6
 *   THREADBASE_SESSION=<cookie> node audit/perf-mobile.mjs --url https://...
 *
 * With THREADBASE_DEV_IDENTITY set on a local server, no session is needed.
 * Otherwise pass the browser's `threadbase_session` cookie with --session.
 */
import { chromium } from "@playwright/test";

const args = new Map();
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index].replace(/^--/, ""), process.argv[index + 1]);
}
const base = (args.get("url") ?? process.env.THREADBASE_URL ?? "http://127.0.0.1:7777").replace(/\/$/, "");
const cpu = Number(args.get("cpu") ?? process.env.PERF_CPU ?? 6);
const runs = Number(args.get("runs") ?? process.env.PERF_RUNS ?? 3);
const session = args.get("session") ?? process.env.THREADBASE_SESSION;

const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH || undefined,
  args: ["--disable-gpu"],
});

const results = [];

for (let run = 0; run < runs; run += 1) {
  const context = await browser.newContext({
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
    isMobile: true,
    hasTouch: true,
  });
  if (session) {
    await context.addCookies([
      {
        name: "threadbase_session",
        value: session,
        domain: new URL(base).hostname,
        path: "/",
      },
    ]);
  }
  const page = await context.newPage();
  const cdp = await context.newCDPSession(page);
  if (cpu > 1) await cdp.send("Emulation.setCPUThrottlingRate", { rate: cpu });

  await page.addInitScript(() => {
    window.__laf = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        window.__laf.push({
          duration: Math.round(entry.duration),
          blocking: Math.round(entry.blockingDuration),
          scripts: entry.scripts
            .slice()
            .sort((a, b) => b.duration - a.duration)
            .slice(0, 3)
            .map((script) => ({
              duration: Math.round(script.duration),
              fn: script.sourceFunctionName || "",
              url: (script.sourceURL || "").split("/").pop(),
            })),
        });
      }
    }).observe({ type: "long-animation-frame", buffered: true });
  });

  await page.goto(`${base}/inbox`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);

  const mark = () => page.evaluate(() => window.__laf.length);
  const summary = async (start) => {
    const frames = await page.evaluate((index) => window.__laf.slice(index), start);
    return {
      count: frames.length,
      total: frames.reduce((sum, entry) => sum + entry.duration, 0),
      blocking: frames.reduce((sum, entry) => sum + entry.blocking, 0),
      max: Math.max(0, ...frames.map((entry) => entry.duration)),
      top: frames
        .flatMap((entry) => entry.scripts)
        .sort((a, b) => b.duration - a.duration)
        .slice(0, 5),
    };
  };
  const tap = async (selector) => page.locator(selector).first().tap();
  const settle = (ms = 800) => page.waitForTimeout(ms);
  const step = async (name, action, wait = 800) => {
    const start = await mark();
    await page.evaluate(() => {
      window.__stepStart = performance.now();
    });
    await action();
    // Time until the first frame after the interaction paints is the number
    // the user feels when they tap a route; the two-frame figure adds the
    // drawer/panel transition's next frame on top.
    const firstFrame = await page.evaluate(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => resolve(performance.now() - window.__stepStart)),
        ),
    );
    await page.evaluate(
      () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
    );
    const twoFrames = await page.evaluate(() => performance.now() - window.__stepStart);
    await settle(wait);
    return {
      name,
      firstFrame: Math.round(firstFrame),
      twoFrames: Math.round(twoFrames),
      url: new URL(page.url()).pathname,
      laf: await summary(start),
    };
  };

  const steps = [];
  const threadsTab = '.mobile-tab[aria-label="Threads and projects"]';

  // The sidebar is warmed after idle, so the first open is mostly layout and
  // the drawer transition; later toggles are pure chrome.
  steps.push(await step("drawer:first-open", () => tap(threadsTab)));
  steps.push(await step("drawer:close", () => tap(threadsTab)));
  steps.push(await step("drawer:reopen", () => tap(threadsTab)));

  // Expand a project so the drawer carries a real thread list, then leave it
  // expanded: that is the common state after browsing.
  steps.push(
    await step("drawer:expand-project", () =>
      page.locator(".side-project-head > .side-nav").first().tap(),
    ),
  );
  steps.push(await step("drawer:close-with-tree", () => tap(threadsTab)));

  // Open a thread from the drawer.
  steps.push(await step("drawer:open-thread-1", () => tap(threadsTab)));
  steps.push(
    await step(
      "route:inbox-to-thread",
      () => page.locator(".side-project .mantine-NavLink-children .side-nav").first().tap(),
      1200,
    ),
  );

  // Toggle the drawer over a mounted transcript; the transcript is the heavy
  // part of the screen and must not re-render for a shell state change.
  steps.push(await step("thread:drawer-open", () => tap(threadsTab)));
  steps.push(await step("thread:drawer-close", () => tap(threadsTab)));
  steps.push(await step("thread:drawer-open-2", () => tap(threadsTab)));
  steps.push(await step("thread:drawer-close-2", () => tap(threadsTab)));

  // Back to Inbox and Settings, the two other top-level routes.
  steps.push(await step("route:thread-to-inbox", () => tap('.mobile-tab[aria-label^="Inbox"]')));
  steps.push(await step("route:inbox-to-settings", () => tap('.mobile-tab[aria-label="Settings"]'), 1200));
  steps.push(await step("route:settings-to-inbox", () => tap('.mobile-tab[aria-label^="Inbox"]')));

  results.push({ run, steps });
  await context.close();
}

const compact = results.map((result) => ({
  run: result.run,
  steps: result.steps.map((step) => ({
    name: step.name,
    url: step.url,
    firstFrame: step.firstFrame,
    twoFrames: step.twoFrames,
    lafCount: step.laf.count,
    lafTotal: step.laf.total,
    blocking: step.laf.blocking,
    max: step.laf.max,
    top: step.laf.top.map((script) => `${script.duration}ms ${script.fn}@${script.url}`),
  })),
}));
console.log(JSON.stringify(compact, null, 2));
await browser.close();
