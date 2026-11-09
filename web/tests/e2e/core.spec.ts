import { expect, test, type BrowserContext, type APIRequestContext } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

const BASE = "http://127.0.0.1:7790";
const REPO = "/tmp/threadbase-e2e/repo";
const ADMIN = {
  username: "ada",
  password: "correct horse battery",
  displayName: "Ada Lovelace",
};
const BOB = {
  username: "bob",
  password: "correct horse battery",
  displayName: "Bob Stone",
};

function csrf() {
  return {
    origin: BASE,
    "x-requested-with": "threadbase",
    "content-type": "application/json",
  };
}

async function setupAdmin(request: APIRequestContext) {
  const response = await request.post(`${BASE}/api/auth/setup`, {
    headers: csrf(),
    data: ADMIN,
  });
  expect(response.status()).toBe(200);
  return request.storageState();
}

async function createProject(request: APIRequestContext) {
  const response = await request.post(`${BASE}/api/projects`, {
    headers: csrf(),
    data: { path: REPO, name: null },
  });
  expect(response.status()).toBe(200);
  return (await response.json()).project;
}

async function createThread(request: APIRequestContext, projectId: string) {
  const response = await request.post(`${BASE}/api/projects/${projectId}/threads`, {
    headers: csrf(),
    data: {
      title: "Fix the login redirect",
      agent: "codex",
      model: null,
      effort: null,
      baseBranch: "main",
      initialPrompt: null,
    },
  });
  expect(response.status()).toBe(200);
  return (await response.json()).thread;
}

async function createBob(request: APIRequestContext) {
  const response = await request.post(`${BASE}/api/auth/users`, {
    headers: csrf(),
    data: { ...BOB, isAdmin: false },
  });
  expect(response.status()).toBe(200);
}

async function shareWorkspace(request: APIRequestContext, threadId: string) {
  const response = await request.patch(`${BASE}/api/threads/${threadId}`, {
    headers: csrf(),
    data: {
      title: null,
      pinned: null,
      archived: null,
      shareLevel: "workspace",
    },
  });
  expect(response.status()).toBe(200);
}

async function loginAsBob(context: BrowserContext) {
  const page = await context.newPage();
  await page.goto("/login");
  await page.getByLabel("Username").fill(BOB.username);
  await page.getByLabel("Password").fill(BOB.password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/$/);
  return page;
}

test("setup, create thread, stream, approve, replay and read-only viewer", async ({
  browser,
  playwright,
}) => {
  const api = await playwright.request.newContext({ baseURL: BASE });
  const storage = await setupAdmin(api);
  const project = await createProject(api);
  const thread = await createThread(api, project.id);
  await createBob(api);
  await shareWorkspace(api, thread.id);

  const adminContext = await browser.newContext({
    storageState: storage,
    viewport: { width: 1440, height: 900 },
  });
  const admin = await adminContext.newPage();
  const adminErrors: string[] = [];
  admin.on("console", (message) => {
    if (message.type() === "error") {
      adminErrors.push(
        `${message.text()} (${message.location().url || "unknown"})`,
      );
    }
  });
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await expect(admin.getByText("Fix the login redirect").first()).toBeVisible();
  await expect(admin.getByPlaceholder("Message")).toBeVisible();


  const composer = admin.getByPlaceholder("Message");
  await composer.fill("stream");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible({ timeout: 15_000 });
  await expect(admin.getByText("Run tests", { exact: false }).first()).toBeVisible();
  await expect(admin.getByText("Full parameters")).toBeHidden();

  await composer.fill("permission");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("Allow once", { exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(admin.getByText("Full parameters")).toBeVisible();
  await admin.getByRole("button", { name: "Allow once" }).click();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible();
  await expect(admin.getByText("Allow once", { exact: true })).toBeHidden({ timeout: 10_000 });
  await composer.fill("permission");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("Allow once", { exact: true }).first()).toBeVisible({
    timeout: 15_000,
  });
  await admin.getByLabel("Approvals").click();
  const drawer = admin.getByRole("complementary", { name: "Right drawer" });
  await expect(drawer).toBeVisible();
  await drawer.getByRole("button", { name: "Allow once" }).click();
  await expect(drawer.getByText("No approvals waiting")).toBeVisible({
    timeout: 10_000,
  });
  await admin.keyboard.press("Shift+Escape");

  await admin.reload();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible({ timeout: 10_000 });
  await admin.screenshot({
    path: "test-results/screenshots/thread-1440-dark.png",
    fullPage: true,
  });
  await admin.getByRole("button", { name: "Switch to light theme" }).click();
  await admin.screenshot({
    path: "test-results/screenshots/thread-1440-light.png",
    fullPage: true,
  });
  await admin.getByRole("button", { name: "Switch to dark theme" }).click();

  const bobContext = await browser.newContext({
    viewport: { width: 768, height: 900 },
  });
  const bob = await loginAsBob(bobContext);
  const bobErrors: string[] = [];
  bob.on("console", (message) => {
    if (message.type() === "error") {
      bobErrors.push(message.text());
    }
  });
  await bob.goto(`/p/${project.id}/t/${thread.id}`);
  await expect(bob.getByText("Hello", { exact: false })).toBeVisible({ timeout: 10_000 });
  await expect(bob.getByText("read-only", { exact: false })).toBeVisible();
  await expect(bob.getByPlaceholder("Message")).toHaveCount(0);
  await bob.getByRole("button", { name: "Request access" }).click();
  await expect(bob.getByText("Access request sent")).toBeVisible({ timeout: 5_000 });
  await bob.screenshot({ path: "test-results/screenshots/thread-768-readonly.png", fullPage: true });
  await bob.evaluate(() => localStorage.setItem("threadbase.theme", "light"));
  await bob.reload();
  await expect(bob.getByText("Hello", { exact: false })).toBeVisible();
  await bob.screenshot({ path: "test-results/screenshots/thread-768-light.png", fullPage: true });
  await bob.evaluate(() => localStorage.setItem("threadbase.theme", "dark"));
  await bob.reload();

  const addEditor = await api.post(`${BASE}/api/threads/${thread.id}/members`, {
    headers: csrf(),
    data: { username: BOB.username, role: "editor" },
  });
  expect(addEditor.status()).toBe(200);
  await bob.reload();
  const bobComposer = bob.getByPlaceholder("Message");
  await expect(bobComposer).toBeVisible({ timeout: 10_000 });

  await composer.fill("shared draft from ada");
  await expect(bob.getByPlaceholder("Edit draft…").first()).toHaveValue(
    "shared draft from ada",
    { timeout: 10_000 },
  );
  await bob
    .getByPlaceholder("Edit draft…")
    .first()
    .fill("shared draft from ada + bob");
  await expect(composer).toHaveValue("shared draft from ada + bob", {
    timeout: 10_000,
  });
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("shared draft from ada + bob", { exact: false })).toBeVisible({
    timeout: 15_000,
  });
  await expect(bobComposer).toHaveValue("", { timeout: 10_000 });

  const second = await createThread(api, project.id);
  await shareWorkspace(api, second.id);
  await expect(admin.getByTitle("Follow Bob Stone")).toBeVisible({
    timeout: 10_000,
  });
  await admin.getByTitle("Follow Bob Stone").click();
  await expect(admin.getByText("Following Bob Stone · stop")).toBeVisible();
  await bob.goto(`/p/${project.id}/t/${second.id}`);
  await expect(admin).toHaveURL(new RegExp(second.id), { timeout: 10_000 });
  await admin.getByText("Following Bob Stone · stop").click();
  await expect(admin.getByText("Following Bob Stone · stop")).toBeHidden();

  const threadInfo = await api.get(`${BASE}/api/threads/${thread.id}`);
  const worktree = (await threadInfo.json()).thread.worktreePath as string;
  fs.writeFileSync(path.join(worktree, "README.md"), "one\nthree\n");
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible();
  await admin.getByLabel("Show right drawer").click();
  const drawerAgain = admin.getByRole("complementary", { name: "Right drawer" });
  await expect(drawerAgain.getByText("README.md").first()).toBeVisible({
    timeout: 10_000,
  });
  await drawerAgain.getByText("README.md").first().click();
  await expect(drawerAgain.getByText("three", { exact: false }).first()).toBeVisible({
    timeout: 10_000,
  });
  await admin.screenshot({
    path: "test-results/screenshots/changes-1440-dark.png",
    fullPage: true,
  });
  await drawerAgain.getByText("three", { exact: false }).first().click();
  await expect(drawerAgain.getByText("Comment on README.md:2")).toBeVisible();
  await drawerAgain
    .getByPlaceholder("Leave a review comment…")
    .fill("Please explain this line");
  await drawerAgain.getByRole("button", { name: "Add pending comment" }).click();
  await expect(drawerAgain.getByText("Please explain this line")).toBeVisible();
  await drawerAgain
    .getByRole("checkbox", { name: "Mark README.md as seen" })
    .click();
  await expect(drawerAgain.getByText("1/1")).toBeVisible({ timeout: 10_000 });
  fs.writeFileSync(path.join(worktree, "README.md"), "one\nfour\n");
  await admin.reload();
  await admin.getByLabel("Show right drawer").click();
  await expect(
    admin.getByRole("complementary", { name: "Right drawer" }).getByText("0/1"),
  ).toBeVisible({ timeout: 10_000 });

  await composer.fill("stream");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await admin.getByRole("tab", { name: "Comments" }).click();
  await expect(admin.getByText("Please explain this line").first()).toBeVisible();
  await expect(admin.getByText("delivered").first()).toBeVisible({
    timeout: 15_000,
  });

  // Project picker: one field, live directory suggestions, no free-form name.
  await admin.getByLabel("New project").click();
  const pathInput = admin.getByPlaceholder("/home/you/code");
  await expect(pathInput).toHaveValue(/.+/, { timeout: 10_000 });
  await pathInput.fill(REPO);
  await expect(admin.getByRole("button", { name: "Use repo" })).toBeVisible({
    timeout: 10_000,
  });
  await admin.getByRole("button", { name: "Cancel" }).click();

  // New thread is direct: no title dialog, no extra step.
  await admin.getByRole("button", { name: "New Thread" }).click();
  await expect(admin).toHaveURL(/\/t\/[^/]+$/, { timeout: 15_000 });
  await expect(admin.getByPlaceholder("Message")).toBeVisible({
    timeout: 10_000,
  });
  await admin.keyboard.press("Shift+Escape");
  await admin.getByPlaceholder("Message").fill("Draft in progress");
  await admin.waitForTimeout(300);
  await admin.screenshot({
    path: "test-results/screenshots/composer-1440-dark.png",
    fullPage: true,
  });
  await admin.getByPlaceholder("Message").fill("");

  // Subagent dispatch card, live running summary and the full picker.
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await admin.getByPlaceholder("Message").fill("subagents");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  const subagentCard = admin
    .locator("main")
    .getByText("Test investigator", { exact: true })
    .first();
  await expect(subagentCard).toBeVisible({ timeout: 15_000 });
  await subagentCard.scrollIntoViewIfNeeded();
  await subagentCard.click();
  await expect(admin.getByText("Nested helper", { exact: true }).first()).toBeVisible();
  await admin.screenshot({
    path: "test-results/screenshots/subagents-1440-dark.png",
    fullPage: true,
  });
  await admin.keyboard.press("Control+j");
  await expect(admin.getByRole("dialog")).toBeVisible({ timeout: 5_000 });
  await admin.screenshot({
    path: "test-results/screenshots/subagent-picker-1440-dark.png",
    fullPage: true,
  });
  await admin.keyboard.press("Escape");

  // Deny with an explicit reason: the decision request carries the note.
  await admin.getByPlaceholder("Message").fill("permission");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByRole("button", { name: "Allow once", exact: true })).toBeVisible({
    timeout: 15_000,
  });
  await admin.getByRole("button", { name: /^(Deny|Reject)$/ }).click();
  await admin.getByPlaceholder("Why deny?").fill("This command modifies the wrong worktree");
  await admin.getByRole("button", { name: "Deny with reason" }).click();
  await expect(admin.getByText("This command modifies the wrong worktree")).toBeHidden({
    timeout: 10_000,
  });

  // Share dialog.
  await admin.keyboard.press("Control+Shift+S");
  await expect(admin.getByText("Share thread", { exact: true })).toBeVisible({
    timeout: 5_000,
  });
  await admin.screenshot({
    path: "test-results/screenshots/share-1440-dark.png",
    fullPage: true,
  });
  await admin.keyboard.press("Escape");

  // Thread lifecycle: rename, fork, delete from the thread action menu.
  await admin.getByRole("button", { name: "Thread actions" }).click();
  await admin.getByRole("menuitem", { name: "Rename" }).click();
  await admin.getByRole("dialog").locator("input").fill("Renamed thread");
  await admin.getByRole("button", { name: "Save", exact: true }).click();
  await expect(admin.getByText("Renamed thread", { exact: true }).first()).toBeVisible({
    timeout: 10_000,
  });
  await admin.getByRole("button", { name: "Thread actions" }).click();
  await admin.getByRole("menuitem", { name: "Fork thread" }).click();
  await expect(admin).toHaveURL(/\/t\/[^/]+$/, { timeout: 15_000 });
  await expect(admin.getByText("Renamed thread (fork)", { exact: true }).first()).toBeVisible({
    timeout: 10_000,
  });
  const forkedUrl = admin.url();
  await admin.getByRole("button", { name: "Thread actions" }).click();
  await admin.getByRole("menuitem", { name: "Delete thread…" }).click();
  await expect(admin.getByText("Delete thread?", { exact: true })).toBeVisible();
  await admin.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(admin).toHaveURL(new RegExp(`/p/${project.id}$`), { timeout: 15_000 });
  expect(forkedUrl).not.toBe(admin.url());

  // Project and new-thread options, then inbox evidence.
  await admin.screenshot({
    path: "test-results/screenshots/project-1440-dark.png",
    fullPage: true,
  });
  await admin.getByRole("button", { name: "New thread options" }).click();
  await expect(admin.getByText("Base branch", { exact: true })).toBeVisible();
  await admin.screenshot({
    path: "test-results/screenshots/new-thread-menu-1440-dark.png",
    fullPage: true,
  });
  await admin.getByLabel("Agent").selectOption("claude");
  await admin.getByLabel("Model").fill("sonnet");
  await admin.getByLabel("Base branch").fill("main");
  await admin.getByRole("button", { name: "Create", exact: true }).click();
  await expect(admin).toHaveURL(/\/t\/[^/]+$/, { timeout: 15_000 });
  await expect(admin.getByPlaceholder("Message")).toBeVisible({ timeout: 10_000 });

  await api.post(`${BASE}/api/threads/${thread.id}/comments`, {
    headers: csrf(),
    data: {
      body: "@bob please review the redirect fix",
      anchor: { kind: "thread" },
      parentId: null,
      send: false,
    },
  });
  await admin.goto("/inbox");
  await admin.screenshot({
    path: "test-results/screenshots/inbox-1440-dark.png",
    fullPage: true,
  });

  expect(adminErrors, adminErrors.join("\n")).toEqual([]);
  expect(bobErrors, bobErrors.join("\n")).toEqual([]);
  await adminContext.close();
  await bobContext.close();
  await api.dispose();
});
