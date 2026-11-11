import { expect, test, type APIRequestContext, type Browser } from "@playwright/test";
import { execSync } from "node:child_process";
import fs from "node:fs";

const BASE = "http://127.0.0.1:7790";
const ADMIN = {
  login: "ada@example.com",
  displayName: "Ada Lovelace",
};
const REPO = "/tmp/threadbase-e2e/repo-interactions";
const REPO2 = "/tmp/threadbase-e2e/repo-interactions-2";

function makeRepo(path: string) {
  fs.rmSync(path, { recursive: true, force: true });
  fs.mkdirSync(path, { recursive: true });
  fs.mkdirSync(`${path}/child`, { recursive: true });
  fs.writeFileSync(`${path}/child/note.txt`, "child\n");
  execSync("git init -b main", { cwd: path });
  execSync('git config user.email "e2e@example.com"', { cwd: path });
  execSync('git config user.name "E2E"', { cwd: path });
  fs.writeFileSync(`${path}/README.md`, "one\n");
  execSync("git add -A", { cwd: path });
  execSync('git commit -m initial', { cwd: path });
}

function identity() {
  return {
    "tailscale-user-login": ADMIN.login,
    "tailscale-user-name": ADMIN.displayName,
  };
}

function csrf() {
  return {
    origin: BASE,
    "x-requested-with": "threadbase",
    "content-type": "application/json",
    ...identity(),
  };
}

async function api(request: APIRequestContext, url: string, init = {}) {
  const response = await request.fetch(`${BASE}${url}`, {
    ...init,
    headers: csrf(),
  });
  expect(response.status()).toBe(200);
  return response.json();
}

async function createThread(request: APIRequestContext, projectId: string) {
  const body = await api(request, `/api/projects/${projectId}/threads`, {
    method: "POST",
    data: {
      title: "Interaction checks",
      agent: "codex",
      model: null,
      effort: null,
      baseBranch: "main",
      initialPrompt: null,
    },
  });
  return body.thread as { id: string; projectId: string };
}

async function openPage(browser: Browser, path: string) {
  const context = await browser.newContext({
    extraHTTPHeaders: identity(),
    viewport: { width: 1440, height: 900 },
    colorScheme: "dark",
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  await page.goto(`${BASE}${path}`);
  return { context, page };
}

test.describe.configure({ mode: "serial" });

let projectId = "";
let threadId = "";

test.beforeAll(async ({ playwright }) => {
  makeRepo(REPO);
  makeRepo(REPO2);
  const request = await playwright.request.newContext({
    baseURL: BASE,
    extraHTTPHeaders: identity(),
  });
  await api(request, "/api/auth/me");
  const project = await api(request, "/api/projects", {
    method: "POST",
    data: { path: REPO, name: null },
  });
  projectId = project.project.id;
  const thread = await createThread(request, projectId);
  threadId = thread.id;
  await request.dispose();
});

test("keyboard: roving tree, visible focus, shortcuts stay out of the composer", async ({
  browser,
}) => {
  const { context, page } = await openPage(
    browser,
    `/p/${projectId}/t/${threadId}`,
  );
  const composer = page.getByPlaceholder("Message");
  await expect(composer).toBeVisible({ timeout: 20_000 });

  await composer.fill("draft text");
  await page.keyboard.press("Control+j");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(composer).toHaveValue("draft text");

  await page.keyboard.press("Control+Alt+m");
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await page.keyboard.press("Control+k");
  const palette = page.getByRole("dialog");
  await expect(palette).toBeVisible();
  await page.keyboard.type("Interaction");
  await expect(palette.getByText("Interaction checks").first()).toBeVisible({
    timeout: 10_000,
  });
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toBeHidden({ timeout: 10_000 });
  await composer.fill("");

  // One Tab stop for the whole tree; every other row is reachable by arrows.
  await expect(page.locator('[data-tree-row][tabindex="0"]')).toHaveCount(1);
  const row = page.locator("[data-tree-row]").first();
  await row.focus();
  const outline = await row.evaluate(
    (element) => getComputedStyle(element).outlineStyle,
  );
  expect(outline).not.toBe("none");
  const durations = await row.evaluate((element) =>
    getComputedStyle(element)
      .transitionDuration.split(",")
      .map((value) => value.trim()),
  );
  expect(durations.every((value) => value === "0s")).toBe(true);

  await context.close();
});

test("new project: Enter adds the typed repository instead of walking up", async ({
  browser,
}) => {
  const { context, page } = await openPage(browser, "/");
  await page.getByLabel("New project").click();
  const input = page.getByPlaceholder("/home/you/code");
  await expect(input).toHaveValue(/.+/, { timeout: 10_000 });
  await input.fill(REPO2);
  await expect(page.getByRole("button", { name: "Use repo-interactions-2" })).toBeVisible({
    timeout: 10_000,
  });

  // Arrow keys walk the directory list and ArrowRight enters the active entry.
  const dialog = page.getByRole("dialog", { name: "Add project" });
  const childRow = dialog.getByRole("button", { name: "child" });
  for (let step = 0; step < 6; step += 1) {
    if ((await childRow.getAttribute("data-dir-active")) === "true") {
      break;
    }
    await input.press("ArrowDown");
  }
  await expect(childRow).toHaveAttribute("data-dir-active", "true");
  await input.press("ArrowRight");
  await expect(input).toHaveValue(/\/child$/);
  await input.fill(REPO2);
  await expect(page.getByRole("button", { name: "Use repo-interactions-2" })).toBeVisible({
    timeout: 10_000,
  });

  // An open modal owns the keyboard: global shortcuts must not leak through
  // when focus sits on a dialog button rather than a text field.
  await dialog.getByRole("button", { name: "Cancel" }).focus();
  await page.keyboard.press("Control+j");
  await expect(page.getByRole("dialog")).toHaveCount(1);
  await input.focus();

  await input.press("Enter");
  await expect(page.getByRole("dialog")).toBeHidden({ timeout: 10_000 });
  await expect(page).toHaveURL(/\/p\/[^/]+$/, { timeout: 10_000 });

  await context.close();
});

test("comments: delete asks for confirmation and keeps the comment on cancel", async ({
  browser,
}) => {
  const { context, page } = await openPage(
    browser,
    `/p/${projectId}/t/${threadId}`,
  );
  await page.getByLabel("Show right drawer").click();
  const drawer = page.getByRole("complementary", { name: "Right drawer" });
  await drawer.getByRole("tab", { name: "Comments" }).click();
  const body = drawer.getByPlaceholder("Leave a comment for the thread…");
  await body.fill("Needs a second look");
  await drawer.getByRole("button", { name: "Save pending" }).click();
  await expect(drawer.getByText("Needs a second look")).toBeVisible({
    timeout: 10_000,
  });

  await drawer.getByRole("button", { name: "Delete comment" }).click();
  const confirm = page.getByRole("dialog");
  await expect(confirm.getByText("Delete comment?", { exact: true })).toBeVisible();
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await expect(drawer.getByText("Needs a second look")).toBeVisible();

  await drawer.getByRole("button", { name: "Delete comment" }).click();
  await confirm.getByRole("button", { name: "Delete" }).click();
  await expect(drawer.getByText("Needs a second look")).toBeHidden({
    timeout: 10_000,
  });

  await context.close();
});

test("settings: leaving with unsaved changes asks before discarding", async ({
  browser,
}) => {
  const { context, page } = await openPage(browser, "/settings");
  const agent = page.getByLabel("Default agent");
  await expect(agent).toBeVisible({ timeout: 20_000 });
  await agent.fill("claude");
  await expect(page.getByText("Unsaved changes")).toBeVisible();

  const inbox = page
    .getByLabel("Workspace sidebar")
    .getByRole("button", { name: /^Inbox/ });
  await inbox.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Discard unsaved settings?")).toBeVisible();
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(page).toHaveURL(/\/settings$/);

  await inbox.click();
  await expect(dialog.getByText("Discard unsaved settings?")).toBeVisible();
  await dialog.getByRole("button", { name: "Discard changes" }).click();
  await expect(page).toHaveURL(/\/inbox$/);

  await context.close();
});

test("transcript: follows new events and offers jump-to-latest", async ({
  browser,
}) => {
  const { context, page } = await openPage(
    browser,
    `/p/${projectId}/t/${threadId}`,
  );
  await page.setViewportSize({ width: 1440, height: 240 });
  const composer = page.getByPlaceholder("Message");
  await expect(composer).toBeVisible({ timeout: 20_000 });

  for (let index = 0; index < 2; index += 1) {
    await expect(composer).toBeVisible();
    await composer.fill("stream");
    await page.getByRole("button", { name: "Send", exact: true }).click();
    await expect(
      page.getByText("Hello", { exact: false }).nth(index),
    ).toBeVisible({ timeout: 30_000 });
    await expect(page.getByRole("button", { name: "Stop" })).toBeHidden({
      timeout: 20_000,
    });
  }

  const viewport = page.locator("#timeline-scroll .mantine-ScrollArea-viewport");
  await expect
    .poll(() =>
      viewport.evaluate(
        (element) =>
          element.scrollHeight - element.scrollTop - element.clientHeight,
      ),
    )
    .toBeLessThan(24);

  await viewport.evaluate((element) => {
    element.scrollTop = 0;
    element.dispatchEvent(new Event("scroll"));
  });
  const jump = page.getByRole("button", { name: "Jump to latest" });
  await expect(jump).toBeVisible();
  await jump.click();
  await expect
    .poll(() =>
      viewport.evaluate(
        (element) =>
          element.scrollHeight - element.scrollTop - element.clientHeight,
      ),
    )
    .toBeLessThan(24);

  await context.close();
});
