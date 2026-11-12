/**
 * Capture the final UI matrix for the goal evidence:
 * 1440/820/390 x light/dark x inbox/thread/changes/files/review/
 * settings/preview.
 *
 * Requires API 7800 + Vite 5173 and the fixture thread data described in
 * web/audit/final.md.
 *
 * Run from web/: `pnpm capture:matrix`.
 */
import { chromium } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, "screenshots-final");
const baseUrl = process.env.THREADBASE_WEB ?? "http://127.0.0.1:5173";
const chrome = process.env.CHROME_PATH ?? "/usr/bin/google-chrome-stable";
const THREAD = process.env.THREAD_ID ?? "01M75KYCF0JDZ5E11C9BAZ44BZ";

const sizes = [
  { name: "1440", width: 1440, height: 900 },
  { name: "820", width: 820, height: 1180 },
  { name: "390", width: 390, height: 844 },
];
const themes = ["light", "dark"];

async function sidebarVisible(page) {
  const box = await page.locator(".sidebar").boundingBox().catch(() => null);
  return Boolean(box && box.x >= 0);
}

async function ensureNavOpen(page, phone) {
  if (phone && !(await sidebarVisible(page))) {
    await page.getByRole("button", { name: "Toggle navigation" }).click();
    await page.waitForTimeout(300);
  }
}

async function closeNav(page, phone) {
  if (phone && (await sidebarVisible(page))) {
    await page.getByRole("button", { name: "Toggle navigation" }).click();
    await page.waitForTimeout(300);
  }
}

async function openThread(page, phone) {
  if (phone) {
    await page.getByRole("button", { name: "Toggle navigation" }).click();
    await page.waitForTimeout(250);
  }
  await page.getByRole("button", { name: /tb-picker-repo/ }).first().click();
  await page.waitForTimeout(500);
  await page.getByRole("button", { name: /Wire the real thread/ }).first().click();
  await page.waitForTimeout(1300);
}

async function openThreadMenu(page, name) {
  await page.getByRole("button", { name: "Thread actions" }).first().click();
  await page.getByRole("menuitem", { name, exact: true }).click();
  await page.waitForTimeout(700);
}

async function openModal(page, compact, name) {
  if (compact) await openThreadMenu(page, name);
  else await page.getByRole("button", { name, exact: true }).first().click();
  await page.getByRole("dialog").waitFor();
  await page.waitForTimeout(600);
}

async function closeModal(page) {
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
}

async function openChanges(page, drawer) {
  if (drawer) {
    await page.getByRole("button", { name: "Open changes" }).click();
    await page.waitForTimeout(500);
  }
}

async function closeDrawer(page, drawer, kind) {
  if (!drawer) return;
  await page.getByRole("button", { name: `Close ${kind}` }).click();
  await page.waitForTimeout(300);
}

async function openFiles(page, phone) {
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await page.waitForTimeout(600);
  // Show a real added file rather than only the tree root.
  try {
    await page.getByRole("button", { name: "Open folder docs" }).click();
    await page.waitForTimeout(350);
    await page
      .getByRole("button", { name: "Open file docs/presence.md" })
      .click();
    await page.waitForTimeout(700);
  } catch {
    // Root tree is still valid evidence if the fixture file is absent.
  }
}

async function main() {
  fs.rmSync(out, { recursive: true, force: true });
  const browser = await chromium.launch({
    headless: true,
    executablePath: chrome,
    args: ["--disable-gpu"],
  });
  const consoles = [];
  try {
    for (const theme of themes) {
      for (const size of sizes) {
        const phone = size.width < 768;
        const compact = size.width < 992;
        const drawer = size.width < 1200;
        const dir = path.join(out, `${size.name}-${theme}`);
        fs.mkdirSync(dir, { recursive: true });
        const context = await browser.newContext({
          viewport: { width: size.width, height: size.height },
          deviceScaleFactor: 2,
        });
        await context.addInitScript(
          (value) => localStorage.setItem("threadbase.theme", value),
          theme,
        );
        const page = await context.newPage();
        page.on("console", (message) => {
          if (message.type() === "error") {
            consoles.push({ area: `${size.name}-${theme}`, text: message.text() });
          }
        });
        page.on("pageerror", (error) => {
          consoles.push({ area: `${size.name}-${theme}`, text: error.message });
        });
        const shot = async (name) => {
          await page.waitForTimeout(500);
          await page.screenshot({ path: path.join(dir, `${name}.png`) });
        };

        await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
        await page.waitForTimeout(1600);
        await shot("inbox");

        await openThread(page, phone);
        await shot("thread");

        await openChanges(page, drawer);
        await shot("changes");
        await openFiles(page, phone);
        await shot("files");
        await closeDrawer(page, drawer, "files");

        await openModal(page, compact, "Reviews");
        await shot("review");
        await closeModal(page);

        await ensureNavOpen(page, phone);
        await page.getByRole("button", { name: "Settings", exact: true }).first().click();
        await page.waitForTimeout(800);
        await closeNav(page, phone);
        await shot("settings");

        await ensureNavOpen(page, phone);
        await page
          .locator(".sidebar-scroll")
          .evaluate((element) => {
            element.scrollTop = element.scrollHeight;
          })
          .catch(() => undefined);
        await page.getByText("Design sample", { exact: true }).click();
        await page.waitForTimeout(1200);
        await shot("preview");
        await openChanges(page, drawer);
        await shot("preview-changes");
        await closeDrawer(page, drawer, "changes");

        await context.close();
      }
    }
  } finally {
    await browser.close();
  }
  const report = {
    capturedAt: new Date().toISOString(),
    baseUrl,
    sizes: sizes.map((size) => size.name),
    themes,
    surfaces: [
      "inbox",
      "thread",
      "changes",
      "files",
      "review",
      "settings",
      "preview",
      "preview-changes",
    ],
    consoles,
  };
  fs.writeFileSync(
    path.join(here, "capture-matrix.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  console.log(JSON.stringify(report, null, 2));
  if (consoles.length) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
