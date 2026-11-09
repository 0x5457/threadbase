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
      adminErrors.push(message.text());
    }
  });
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await expect(admin.getByText("Fix the login redirect").first()).toBeVisible();
  await expect(admin.getByText("Agent is running", { exact: false }).or(admin.getByText("Shared draft", { exact: false }))).toBeVisible();
  await admin.screenshot({ path: "test-results/screenshots/thread-1440.png", fullPage: true });
  await admin.getByRole("button", { name: "Switch to light theme" }).click();
  await admin.screenshot({ path: "test-results/screenshots/thread-1440-light.png", fullPage: true });
  await admin.getByRole("button", { name: "Switch to dark theme" }).click();

  const composer = admin.getByPlaceholder("Write a message…");
  await composer.fill("stream");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible({ timeout: 15_000 });
  await expect(admin.getByText("Run tests", { exact: false }).first()).toBeVisible();

  await composer.fill("permission");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("Allow once", { exact: true })).toBeVisible({ timeout: 15_000 });
  await admin.getByRole("button", { name: "Allow once" }).click();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible();
  await expect(admin.getByText("Allow once", { exact: true })).toBeHidden({ timeout: 10_000 });
  await admin.screenshot({ path: "test-results/screenshots/thread-approved.png", fullPage: true });

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
  await expect(bob.getByPlaceholder("Write a message…")).toHaveCount(0);
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
  const bobComposer = bob.getByPlaceholder("Write a message…");
  await expect(bobComposer).toBeVisible({ timeout: 10_000 });

  await composer.fill("shared draft from ada");
  await expect(bob.getByPlaceholder("Edit this shared draft…").first()).toHaveValue(
    "shared draft from ada",
    { timeout: 10_000 },
  );
  await bob
    .getByPlaceholder("Edit this shared draft…")
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

  expect(adminErrors, adminErrors.join("\n")).toEqual([]);
  expect(bobErrors, bobErrors.join("\n")).toEqual([]);
  await adminContext.close();
  await bobContext.close();
  await api.dispose();
});
