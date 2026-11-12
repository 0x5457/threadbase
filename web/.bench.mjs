import { chromium } from "@playwright/test";
const url = process.env.URL;
const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome-stable", args: ["--disable-gpu"] });
const context = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, hasTouch: true, isMobile: true });
await context.addCookies([{ name: "threadbase_session", value: "preview-token", domain: "127.0.0.1", path: "/" }]);
const page = await context.newPage();
const cdp = await context.newCDPSession(page);
await cdp.send("Emulation.setCPUThrottlingRate", { rate: 6 });
await page.addInitScript(() => {
  window.__long = [];
  new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) window.__long.push(Math.round(entry.duration));
  }).observe({ type: "longtask", buffered: true });
});
await page.goto(url, { waitUntil: "domcontentloaded" });
await page.waitForTimeout(4500);
const toggle = async () => {
  const before = await page.evaluate(() => window.__long.length);
  const ms = await page.evaluate(async () => {
    const start = performance.now();
    const burger = document.querySelector(".burger-slot button");
    if (!burger) return -1;
    burger.click();
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    return performance.now() - start;
  });
  await page.waitForTimeout(800);
  const after = await page.evaluate((index) => window.__long.slice(index), before);
  return { frames: Math.round(ms), long: after };
};
const out = [];
for (let i = 0; i < 4; i += 1) out.push(await toggle());
console.log(JSON.stringify(out));
await browser.close();
