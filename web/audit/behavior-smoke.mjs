/**
 * Reproducible behavior checks for the audit findings that are not visual:
 * real identity, live online indicators, default baseline, panel reset across
 * threads, Review/Share form reset, and no unreachable share-link control.
 *
 * Run from web/: `pnpm smoke:behavior` while API 7800 + Vite 5173 are up.
 * Writes audit/behavior-smoke.json and exits 1 on any failed assertion.
 */
import { chromium } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const baseUrl = process.env.THREADBASE_WEB ?? "http://127.0.0.1:5173";
const chrome = process.env.CHROME_PATH ?? "/usr/bin/google-chrome-stable";
const WIRE = "01M75KYCF0JDZ5E11C9BAZ44BZ";
const OTHER = "01M75MKEATRSHM2WTGTCQMR7F3";
const checks = [];
const errors = [];

function check(name, pass, detail) {
  checks.push({ name, pass: Boolean(pass), detail });
}

async function patchSettings(page, body) {
  return page.evaluate(async (value) => {
    const response = await fetch("/api/settings", {
      method: "PATCH",
      headers: {
        "content-type": "application/json",
        "x-requested-with": "threadbase",
      },
      body: JSON.stringify(value),
    });
    if (!response.ok) throw new Error(`settings ${response.status}`);
    return response.json();
  }, body);
}

async function openThread(page, name) {
  await page.getByRole("button", { name: new RegExp(name) }).first().click();
  await page.waitForTimeout(1300);
}

async function openModal(page, name) {
  await page.getByRole("button", { name, exact: true }).first().click();
  await page.getByRole("dialog").waitFor();
  await page.waitForTimeout(500);
}

async function closeModal(page) {
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
}

async function main() {
  const browser = await chromium.launch({
    headless: true,
    executablePath: chrome,
    args: ["--disable-gpu"],
  });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(error.message));
  try {
    await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await page.waitForTimeout(1400);
    await page.getByRole("button", { name: /tb-picker-repo/ }).first().click();
    await page.waitForTimeout(500);
    await openThread(page, "Wire the real thread");

    const footer = await page.locator(".sidebar-footer").innerText();
    check(
      "real account in sidebar footer",
      footer.includes("ada@example.com") && footer.includes("@ada") && !footer.includes("Ada Lovelace"),
      footer.replaceAll("\n", " / "),
    );
    const composerAvatar = await page
      .locator(".composer-row .mantine-Avatar-root")
      .first()
      .innerText();
    check(
      "real account in composer",
      composerAvatar.trim() !== "AL",
      JSON.stringify(composerAvatar),
    );

    await page.waitForTimeout(1200);
    const indicators = await page
      .locator(".header .mantine-Indicator-indicator")
      .count();
    check("live online indicator in title bar", indicators > 0, `${indicators}`);

    const inbox = await page.evaluate(async () => {
      const response = await fetch("/api/inbox?filter=unread");
      return response.json();
    });
    const inboxDot = await page.locator(".inbox-button .icon-dot").count();
    check(
      "inbox dot reflects unread state",
      inbox.unread === 0 ? inboxDot === 0 : inboxDot > 0,
      `unread=${inbox.unread} dots=${inboxDot}`,
    );

    if (inbox.unread > 0) {
      await page.evaluate(async () => {
        await fetch("/api/inbox/read", {
          method: "POST",
          headers: {
            "content-type": "application/json",
            "x-requested-with": "threadbase",
          },
          body: JSON.stringify({ threadId: null, all: true }),
        });
      });
      await page.reload({ waitUntil: "domcontentloaded" });
      await page.waitForTimeout(1400);
      const cleared = await page.evaluate(async () => {
        const response = await fetch("/api/inbox?filter=unread");
        return response.json();
      });
      const clearedDot = await page.locator(".inbox-button .icon-dot").count();
      check(
        "inbox dot clears after mark all read",
        cleared.unread === 0 && clearedDot === 0,
        `unread=${cleared.unread} dots=${clearedDot}`,
      );
    }

    const cancelledRow = page
      .locator(".subagent-row")
      .filter({ hasText: "Test investigator" });
    const cancelledCancel = await cancelledRow
      .getByRole("button", { name: "Cancel" })
      .count();
    check(
      "terminal subagent has no Cancel action",
      cancelledCancel === 0,
      `${cancelledCancel}`,
    );

    // Baseline: backend applies default_baseline to the bundle, and the UI
    // renders that label in the Changes header.
    await patchSettings(page, { defaultBaseline: "last_turn" });
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForTimeout(1200);
    await page.getByRole("button", { name: /tb-picker-repo/ }).first().click();
    await page.waitForTimeout(500);
    await openThread(page, "Wire the real thread");
    const asideAfterBaseline = await page.locator(".aside").innerText();
    check(
      "default diff baseline applied",
      asideAfterBaseline.includes("Since Last turn"),
      asideAfterBaseline.split("\n").slice(0, 4).join(" / "),
    );

    // Switching threads must not leave the previous worktree's file selected.
    await page.getByRole("button", { name: /Review the auth changes/ }).first().click();
    await page.waitForTimeout(1500);
    const otherAside = await page.locator(".aside").innerText();
    check(
      "changes panel resets across threads",
      !otherAside.includes("docs/presence.md") &&
        !otherAside.toLowerCase().includes("cannot load"),
      otherAside.split("\n").slice(0, 4).join(" / "),
    );
    await openThread(page, "Wire the real thread");

    await openModal(page, "Reviews");
    const reviewNote = page.getByLabel("Review note");
    await reviewNote.fill("residue check");
    await closeModal(page);
    await openModal(page, "Reviews");
    const reviewValue = await page.getByLabel("Review note").inputValue();
    check("review form resets on reopen", reviewValue === "", reviewValue);
    await closeModal(page);

    await openModal(page, "Share");
    const email = page.getByPlaceholder("name@example.com");
    await email.fill("residue@example.com");
    const copyLink = await page.getByText("Copy link", { exact: true }).count();
    check("no unreachable share-link control", copyLink === 0, `${copyLink}`);
    await closeModal(page);
    await openModal(page, "Share");
    const emailValue = await page.getByPlaceholder("name@example.com").inputValue();
    check("share form resets on reopen", emailValue === "", emailValue);
    await closeModal(page);

    await patchSettings(page, { defaultBaseline: "base_branch" });
  } finally {
    await browser.close();
  }

  const result = {
    checkedAt: new Date().toISOString(),
    baseUrl,
    threadIds: { wire: WIRE, other: OTHER },
    checks,
    consoleErrors: errors,
  };
  fs.writeFileSync(
    path.join(here, "behavior-smoke.json"),
    `${JSON.stringify(result, null, 2)}\n`,
  );
  console.log(JSON.stringify(result, null, 2));
  if (errors.length || checks.some((item) => !item.pass)) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
