/**
 * Reproducible browser console smoke for the real dev workspace.
 *
 * Requires:
 *   - API on 7800 and Vite on 5173 (see web/audit/final.md)
 *   - Chrome/Chromium (CHROME_PATH, default /usr/bin/google-chrome-stable)
 *
 * Run from web/: `pnpm smoke:console`
 * Writes audit/console-smoke.json and exits 1 if any console/page errors occur.
 */
import { chromium } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const baseUrl = process.env.THREADBASE_WEB ?? "http://127.0.0.1:5173";
const chrome =
  process.env.CHROME_PATH ?? "/usr/bin/google-chrome-stable";
const errors = [];
const steps = [];

function watch(page, label) {
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    errors.push({
      area: label,
      kind: "console",
      text: message.text(),
      location: message.location(),
    });
  });
  page.on("pageerror", (error) => {
    errors.push({
      area: label,
      kind: "pageerror",
      text: error.message,
      stack: error.stack,
    });
  });
}

async function step(label, action) {
  await action();
  steps.push(label);
}

async function closeModal(page) {
  await page.keyboard.press("Escape");
  await page.waitForTimeout(250);
}

async function openThread(page, phone) {
  if (phone) {
    await page.getByRole("button", { name: "Toggle navigation" }).click();
    await page.waitForTimeout(300);
  }
  await page
    .getByRole("button", { name: /tb-picker-repo/ })
    .first()
    .click();
  await page.waitForTimeout(600);
  await page
    .getByRole("button", { name: /Wire the real thread/ })
    .first()
    .click();
  await page.waitForTimeout(1200);
}

async function openThreadMenuAction(page, name) {
  await page.getByRole("button", { name: "Thread actions" }).click();
  await page.getByRole("menuitem", { name, exact: true }).click();
  await page.waitForTimeout(700);
}

async function main() {
  const browser = await chromium.launch({
    headless: true,
    executablePath: chrome,
    args: ["--disable-gpu"],
  });
  try {
    // Desktop 1440
    const desktop = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    watch(desktop, "desktop");
    await desktop.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await desktop.waitForTimeout(1800);
    await step("desktop inbox", async () => {
      await desktop.getByText("Inbox", { exact: true }).first().waitFor();
    });
    await step("desktop thread", () => openThread(desktop, false));
    await step("desktop changes", async () => {
      await desktop.getByRole("button", { name: "Files", exact: true }).click();
      await desktop.waitForTimeout(500);
      await desktop.getByRole("button", { name: "Changes", exact: true }).click();
      await desktop.waitForTimeout(400);
    });
    await step("desktop review", async () => {
      await desktop.getByRole("button", { name: "Reviews", exact: true }).click();
      await desktop.getByRole("dialog").waitFor();
      await desktop.waitForTimeout(600);
      await closeModal(desktop);
    });
    await step("desktop settings", async () => {
      await desktop.getByRole("button", { name: "Settings", exact: true }).first().click();
      await desktop.waitForTimeout(700);
    });
    await step("desktop preview", async () => {
      await desktop.getByText("Design sample", { exact: true }).click();
      await desktop.waitForTimeout(1200);
      const openChanges = desktop.getByRole("button", { name: "Open changes" });
      if (await openChanges.isVisible().catch(() => false)) {
        await openChanges.click();
        await desktop.waitForTimeout(600);
        await closeModal(desktop);
      }
    });

    // Phone 390
    const phone = await browser.newPage({ viewport: { width: 390, height: 844 } });
    watch(phone, "phone");
    await phone.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await phone.waitForTimeout(1600);
    await step("phone inbox", async () => {
      await phone.getByText("Inbox", { exact: true }).first().waitFor();
    });
    await step("phone thread", () => openThread(phone, true));
    await step("phone thread menu review", async () => {
      await openThreadMenuAction(phone, "Reviews");
      await phone.getByRole("dialog").waitFor();
      await phone.waitForTimeout(600);
      await closeModal(phone);
    });
    await step("phone changes drawer", async () => {
      await phone.getByRole("button", { name: "Open changes" }).click();
      await phone.waitForTimeout(500);
      await phone.getByRole("button", { name: "Files", exact: true }).click();
      await phone.waitForTimeout(500);
      await phone.getByRole("button", { name: "Changes", exact: true }).click();
      await phone.waitForTimeout(400);
      await phone.getByRole("button", { name: "Close changes" }).click();
    });
  } finally {
    await browser.close();
  }

  const result = {
    baseUrl,
    chrome,
    checkedAt: new Date().toISOString(),
    steps,
    errors,
  };
  fs.writeFileSync(
    path.join(here, "console-smoke.json"),
    `${JSON.stringify(result, null, 2)}\n`,
  );
  console.log(JSON.stringify(result, null, 2));
  if (errors.length) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
