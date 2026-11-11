import { expect, test, type BrowserContext, type APIRequestContext, type Page } from "@playwright/test";
import fs from "node:fs";
import { execSync } from "node:child_process";
import path from "node:path";

const BASE = "http://127.0.0.1:7790";
const REPO = "/tmp/threadbase-e2e/repo";

/**
 * Screenshot evidence is opt-in: `E2E_SCREENSHOTS=1 pnpm exec playwright test`.
 * Full-page captures plus their animation-settle waits were the single largest
 * cost in this suite, so the default run skips both.
 */
const SCREENSHOTS = process.env.E2E_SCREENSHOTS === "1";

async function shot(page: Page, name: string): Promise<void> {
  if (!SCREENSHOTS) {
    return;
  }
  await page.screenshot({
    path: `test-results/screenshots/${name}.png`,
    fullPage: true,
  });
}
// Sign-in is performed by `tailscale serve`; the server trusts these headers
// because the test connects over loopback.
const ADMIN = {
  login: "ada@example.com",
  username: "ada",
  displayName: "Ada Lovelace",
};
const BOB = {
  login: "bob@example.com",
  username: "bob",
  displayName: "Bob Stone",
};

function identityHeaders(who: { login: string; displayName: string }) {
  return {
    "tailscale-user-login": who.login,
    "tailscale-user-name": who.displayName,
  };
}

function csrf(who: { login: string; displayName: string } = ADMIN) {
  return {
    origin: BASE,
    "x-requested-with": "threadbase",
    "content-type": "application/json",
    ...identityHeaders(who),
  };
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

// The first identity to authenticate becomes the administrator.
async function authenticateAdmin(request: APIRequestContext) {
  const response = await request.get(`${BASE}/api/auth/me`, {
    headers: csrf(),
  });
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body.user.username).toBe(ADMIN.username);
  expect(body.user.isAdmin).toBe(true);
}

// Provisioning a second identity keeps it non-admin.
async function createBob(request: APIRequestContext) {
  const response = await request.get(`${BASE}/api/auth/me`, {
    headers: csrf(BOB),
  });
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body.user.username).toBe(BOB.username);
  expect(body.user.isAdmin).toBe(false);
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

async function openAs(
  context: BrowserContext,
  path: string,
) {
  const page = await context.newPage();
  await page.goto(path);
  await expect(page).not.toHaveURL(/\/login$/);
  return page;
}

test("setup, create thread, stream, approve, replay and read-only viewer", async ({
  browser,
  playwright,
}) => {
  const api = await playwright.request.newContext({
    baseURL: BASE,
    extraHTTPHeaders: identityHeaders(ADMIN),
  });
  await authenticateAdmin(api);
  const project = await createProject(api);
  const thread = await createThread(api, project.id);
  await createBob(api);
  await shareWorkspace(api, thread.id);

  const adminContext = await browser.newContext({
    extraHTTPHeaders: identityHeaders(ADMIN),
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
  await admin.getByRole("button", { name: "Thread actions" }).focus();
  await admin.keyboard.press("Enter");
  await expect(admin.getByRole("menuitem", { name: "Rename" })).toBeVisible();
  await expect(admin.locator("[data-autofocus]")).toBeFocused();
  await admin.keyboard.press("Escape");
  await expect(admin.getByRole("menuitem", { name: "Rename" })).toBeHidden();


  const composer = admin.getByPlaceholder("Message");
  await composer.fill("stream");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible({ timeout: 15_000 });
  await expect(admin.getByText("Run tests", { exact: false }).first()).toBeVisible();
  await expect(admin.getByText("Full parameters")).toBeHidden();

  await composer.fill("permission");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  const allowOnce = admin.getByRole("button", { name: "Allow once", exact: true });
  await expect(allowOnce).toBeVisible({ timeout: 15_000 });
  // The allow path is primary; rejecting must not look like the default action.
  await expect(allowOnce).toHaveAttribute("data-variant", "filled");
  const reject = admin.getByRole("button", { name: /^(Reject|Deny)$/ });
  if (await reject.count()) {
    await expect(reject.first()).not.toHaveAttribute("data-variant", "filled");
  }
  await expect(admin.getByText("Full parameters")).toBeVisible();
  // UI and backend projection must agree on the pending approval count.
  const bundleResponse = await api.get(`/api/threads/${thread.id}/bundle`);
  const bundleBody = await bundleResponse.json();
  expect(bundleBody.bundle.thread.pendingApprovals).toBe(1);
  await expect(admin.getByText("1 waiting").first()).toBeVisible();

  // Hold the decision response open long enough to observe the pending state;
  // every decision surface must disable while the request is in flight.
  await admin.route("**/approvals/*/decision", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 2500));
    await route.continue();
  });
  const allowOnceButton = admin.getByRole("button", { name: "Allow once" });
  await allowOnceButton.click();
  await expect(allowOnceButton).toBeDisabled();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible();
  await expect(admin.getByText("Allow once", { exact: true })).toBeHidden({ timeout: 10_000 });
  await admin.unroute("**/approvals/*/decision");
  await composer.fill("permission");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByText("Allow once", { exact: true }).first()).toBeVisible({
    timeout: 15_000,
  });
  await admin.getByLabel("Approvals").click();
  const drawer = admin.getByRole("complementary", { name: "Right drawer" });
  await expect(drawer).toBeVisible();
  // Opening the drawer moves focus to its active tab instead of leaving it
  // behind in the transcript.
  await expect(drawer.getByRole("tab", { name: "Approvals" })).toBeFocused();
  await drawer.getByRole("button", { name: "Allow once" }).click();
  await expect(drawer.getByText("No approvals waiting")).toBeVisible({
    timeout: 10_000,
  });
  await admin.keyboard.press("Shift+Escape");
  // Closing the drawer returns focus to the button that opened it.
  await expect(admin.getByLabel("Approvals")).toBeFocused();

  await admin.reload();
  await expect(admin.getByText("Hello", { exact: false })).toBeVisible({ timeout: 10_000 });
  await shot(admin, "thread-1440-dark");
  await admin.getByRole("button", { name: "Switch to light theme" }).click();
  await shot(admin, "thread-1440-light");
  await admin.getByRole("button", { name: "Switch to dark theme" }).click();

  const bobContext = await browser.newContext({
    extraHTTPHeaders: identityHeaders(BOB),
    viewport: { width: 768, height: 900 },
  });
  const bob = await openAs(bobContext, "/");
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
  await shot(bob, "thread-768-readonly");
  await bob.evaluate(() => localStorage.setItem("threadbase.theme", "light"));
  await bob.reload();
  await expect(bob.getByText("Hello", { exact: false })).toBeVisible();
  await shot(bob, "thread-768-light");
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
  await expect(admin.getByText("shared draft from ada + bob", { exact: false }).first()).toBeVisible({
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
  await shot(admin, "changes-1440-dark");

  // Keyboard path: one tabbable line per hunk, C/Enter/Space open comments.
  const diffLines = drawerAgain.locator("[data-diff-line]");
  await expect(drawerAgain.locator('[data-diff-line][tabindex="0"]')).toHaveCount(1);
  const firstDiffLine = diffLines.first();
  for (const key of ["c", "Enter", " "]) {
    await firstDiffLine.focus();
    await admin.keyboard.press(key);
    await expect(drawerAgain.getByText(/Comment on README\.md:/)).toBeVisible();
    await drawerAgain.getByRole("button", { name: "Cancel" }).click();
  }

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

  // Queue: while a slow turn runs, queue a follow-up, cancel the turn, then
  // continue the superseded item. The worker sends it and a new tool card lands.
  await expect
    .poll(
      async () => {
        const response = await api.get(`/api/threads/${thread.id}/bundle`);
        const body = await response.json();
        return body.bundle.thread.status as string;
      },
      { timeout: 15_000 },
    )
    .toBe("idle");
  await composer.fill("slow");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect
    .poll(
      async () => {
        const response = await api.get(`/api/threads/${thread.id}/bundle`);
        const body = await response.json();
        return body.bundle.thread.status as string;
      },
      { timeout: 10_000 },
    )
    .toBe("running");
  await expect(
    admin.getByRole("button", { name: "Stop", exact: true }),
  ).toBeVisible({ timeout: 10_000 });
  await composer.fill("stream");
  await admin.getByRole("button", { name: "More send options" }).click();
  await admin
    .getByRole("menuitem", { name: "Queue after the current turn" })
    .click();
  await expect(admin.getByText("Queued at position 1")).toBeVisible({
    timeout: 10_000,
  });
  const queuePanel = admin.getByTestId("queue-panel");
  await expect(queuePanel).toBeVisible({ timeout: 10_000 });
  const helloMessages = admin.getByText("Hello", { exact: false });
  const helloBefore = await helloMessages.count();
  await admin.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(queuePanel.getByText("previous turn cancelled")).toBeVisible({
    timeout: 10_000,
  });
  await queuePanel.getByRole("button", { name: "continue" }).click();
  await expect
    .poll(() => helloMessages.count(), { timeout: 15_000 })
    .toBeGreaterThan(helloBefore);
  await expect(queuePanel).toBeHidden({ timeout: 15_000 });

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
  await admin
    .getByLabel("Workspace sidebar")
    .getByRole("button", { name: "New Thread" })
    .first()
    .click();
  await expect(admin).toHaveURL(/\/t\/[^/]+$/, { timeout: 15_000 });
  await expect(admin.getByPlaceholder("Message")).toBeVisible({
    timeout: 10_000,
  });
  await admin.keyboard.press("Shift+Escape");
  await admin.getByPlaceholder("Message").fill("Draft in progress");
  await shot(admin, "composer-1440-dark");
  await admin.getByPlaceholder("Message").fill("");

  // Subagent dispatch card, live running summary and the full picker.
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await admin.getByPlaceholder("Message").fill("subagents");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  const subagentCard = admin
    .locator("main")
    .getByText("Test investigator", { exact: true })
    .first();
  await expect(subagentCard).toBeVisible({ timeout: 30_000 });
  await subagentCard.scrollIntoViewIfNeeded();
  await subagentCard.click();
  await expect(admin.getByText("Nested helper", { exact: true }).first()).toBeVisible({
    timeout: 15_000,
  });
  await shot(admin, "subagents-1440-dark");
  await admin.keyboard.press("Control+j");
  await expect(admin.getByRole("dialog")).toBeVisible({ timeout: 5_000 });
  await shot(admin, "subagent-picker-1440-dark");
  await admin.keyboard.press("Escape");

  // Deny with an explicit reason: the decision request carries the note.
  await admin.getByPlaceholder("Message").fill("permission");
  await admin.getByRole("button", { name: "Send", exact: true }).click();
  await expect(admin.getByRole("button", { name: "Allow once", exact: true })).toBeVisible({
    timeout: 15_000,
  });
  await admin.getByRole("button", { name: /^(Deny|Reject)$/ }).click();
  await admin.getByPlaceholder("Why deny?").fill("This command modifies the wrong worktree");
  // A failed decision must keep the typed reason so it can be retried.
  // The deliberate 500 below is the only console error we allow in this flow.
  const consoleErrorsBeforeFailure = adminErrors.length;
  await admin.route("**/approvals/*/decision", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: "boom" }),
    }),
  );
  await admin.getByRole("button", { name: "Deny with reason" }).click();
  await expect(admin.getByPlaceholder("Why deny?")).toHaveValue(
    "This command modifies the wrong worktree",
  );
  await expect(
    admin.getByRole("alert").filter({ hasText: /boom|approval|failed/i }).first(),
  ).toBeVisible({ timeout: 10_000 });
  adminErrors.splice(consoleErrorsBeforeFailure);
  await admin.unroute("**/approvals/*/decision");
  await admin.getByRole("button", { name: "Deny with reason" }).click();
  await expect(admin.getByText("This command modifies the wrong worktree")).toBeHidden({
    timeout: 10_000,
  });

  // Share dialog.
  await admin.keyboard.press("Control+Shift+S");
  await expect(admin.getByText("Share thread", { exact: true })).toBeVisible({
    timeout: 5_000,
  });
  await shot(admin, "share-1440-dark");
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
  await shot(admin, "project-1440-dark");
  await admin.getByRole("button", { name: "New thread options" }).click();
  await expect(admin.getByText("Base branch", { exact: true })).toBeVisible();
  await shot(admin, "new-thread-menu-1440-dark");
  await admin.getByRole("combobox", { name: "Agent" }).click();
  await admin.getByRole("option", { name: "Claude Code" }).click();
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
  await expect(admin.getByText("Inbox", { exact: false }).first()).toBeVisible();
  await shot(admin, "inbox-1440-dark");
  await admin.goto("/");
  await expect(
    admin.getByRole("heading", { name: "Projects", exact: true }),
  ).toBeVisible();
  await shot(admin, "workspace-1440-dark");
  await admin.goto("/settings");
  await expect(admin.getByText("Settings", { exact: true }).first()).toBeVisible();
  await shot(admin, "settings-1440-dark");
  await admin.evaluate(() =>
    localStorage.setItem("threadbase.theme", "light"),
  );
  await admin.reload();
  await expect(admin.getByRole("heading", { name: "Settings" })).toBeVisible();
  await shot(admin, "settings-1440-light");
  await admin.goto("/");
  await expect(
    admin.getByRole("heading", { name: "Projects", exact: true }),
  ).toBeVisible();
  await shot(admin, "workspace-1440-light");

  const narrowContext = await browser.newContext({
    extraHTTPHeaders: identityHeaders(ADMIN),
    viewport: { width: 768, height: 900 },
  });
  const narrow = await narrowContext.newPage();
  await narrow.goto("/");
  await expect(narrow.getByRole("heading", { name: "Projects", exact: true })).toBeVisible();
  await shot(narrow, "workspace-768-dark");
  await narrow.goto("/settings");
  await expect(narrow.getByRole("heading", { name: "Settings" })).toBeVisible();
  await shot(narrow, "settings-768-dark");
  await narrow.evaluate(() => localStorage.setItem("threadbase.theme", "light"));
  await narrow.reload();
  await shot(narrow, "settings-768-light");
  await narrow.goto("/");
  await shot(narrow, "workspace-768-light");
  await narrowContext.close();

  // Narrow-window visual matrix for the remaining review surfaces.
  await admin.setViewportSize({ width: 768, height: 900 });
  await admin.evaluate(() => localStorage.setItem("threadbase.theme", "dark"));
  await admin.goto(`/p/${project.id}`);
  await expect(admin.getByRole("heading", { name: "repo" })).toBeVisible();
  await shot(admin, "project-768-dark");
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await expect(admin.getByPlaceholder("Message")).toBeVisible();
  await shot(admin, "composer-768-dark");
  await admin.getByLabel("Show right drawer").click();
  const narrowDrawer = admin.getByRole("complementary", { name: "Right drawer" });
  await narrowDrawer.getByRole("tab", { name: "Files" }).click();
  await expect(narrowDrawer.getByText("README.md")).toBeVisible();
  await shot(admin, "files-768-dark");
  await narrowDrawer.getByRole("tab", { name: "Output" }).click();
  await expect(narrowDrawer.getByText("Run tests").first()).toBeVisible();
  await shot(admin, "output-768-dark");
  await narrowDrawer.getByRole("tab", { name: "Changes" }).click();
  await expect(narrowDrawer.getByText("README.md")).toBeVisible();
  await shot(admin, "changes-768-dark");
  await admin.keyboard.press("Shift+Escape");
  await admin.getByRole("button", { name: "Share", exact: true }).click();
  await expect(admin.getByText("Share thread", { exact: true })).toBeVisible();
  await shot(admin, "share-768-dark");
  await admin.keyboard.press("Escape");

  await admin.evaluate(() => localStorage.setItem("threadbase.theme", "light"));
  await admin.goto(`/p/${project.id}`);
  await shot(admin, "project-768-light");
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await admin.getByLabel("Show right drawer").click();
  const lightDrawer = admin.getByRole("complementary", { name: "Right drawer" });
  await lightDrawer.getByRole("tab", { name: "Changes" }).click();
  await shot(admin, "changes-768-light");
  await admin.keyboard.press("Shift+Escape");
  await admin.goto("/inbox");
  await shot(admin, "inbox-768-light");

  await admin.setViewportSize({ width: 1440, height: 900 });
  await admin.evaluate(() => localStorage.setItem("threadbase.theme", "dark"));
  await admin.goto(`/p/${project.id}/compare`);
  await expect(admin.getByRole("heading", { name: "Compare threads" })).toBeVisible();
  await shot(admin, "compare-1440-dark");
  await admin.goto(`/p/${project.id}/t/${thread.id}/subagents/compare`);
  await expect(admin.getByRole("heading", { name: "Compare subagents" })).toBeVisible();
  await shot(admin, "subagent-compare-1440-dark");

  // 768/light coverage for the comparison and share surfaces.
  await admin.setViewportSize({ width: 768, height: 900 });
  await admin.evaluate(() => localStorage.setItem("threadbase.theme", "light"));
  await admin.goto(`/p/${project.id}/compare`);
  await shot(admin, "compare-768-light");
  await admin.goto(`/p/${project.id}/t/${thread.id}/subagents/compare`);
  await shot(admin, "subagent-compare-768-light");
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await admin.getByRole("button", { name: "Share", exact: true }).click();
  await expect(admin.getByText("Share thread", { exact: true })).toBeVisible();
  await shot(admin, "share-768-light");
  await admin.keyboard.press("Escape");
  await shot(admin, "composer-768-light");
  await admin.evaluate(() => localStorage.setItem("threadbase.theme", "dark"));
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  const compactSubagent = admin
    .locator("main")
    .getByText("Test investigator", { exact: true })
    .first();
  await compactSubagent.scrollIntoViewIfNeeded();
  await shot(admin, "subagents-768-dark");
  await admin.goto("/inbox");
  await admin.reload();
  await shot(admin, "inbox-768-dark");

  // Phone layout: form fields stay at 16px (iOS Safari zooms the page when a
  // smaller field is focused), nothing overflows horizontally, and the review
  // drawer uses the full width instead of the desktop 420px panel.
  const phoneContext = await browser.newContext({
    extraHTTPHeaders: identityHeaders(ADMIN),
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  });
  const phone = await phoneContext.newPage();
  await phone.goto(`/p/${project.id}/t/${thread.id}`);
  const phoneComposer = phone.getByPlaceholder("Message");
  await expect(phoneComposer).toBeVisible();
  await phoneComposer.focus();
  const phoneMetrics = await phone.evaluate(() => {
    const field = document.querySelector<HTMLTextAreaElement>(
      'textarea[placeholder="Message"]',
    );
    return {
      width: window.innerWidth,
      scrollWidth: Math.max(
        document.documentElement.scrollWidth,
        document.body.scrollWidth,
      ),
      fieldFontSize: field ? getComputedStyle(field).fontSize : "",
    };
  });
  expect(phoneMetrics.scrollWidth).toBe(phoneMetrics.width);
  expect(phoneMetrics.fieldFontSize).toBe("16px");
  // The shell follows the visual viewport, so the software keyboard cannot
  // cover the composer.
  await phone.setViewportSize({ width: 390, height: 500 });
  await expect
    .poll(() =>
      phone.evaluate(() =>
        getComputedStyle(document.documentElement)
          .getPropertyValue("--tb-app-height")
          .trim(),
      ),
    )
    .toBe("500px");
  await phone.setViewportSize({ width: 390, height: 844 });
  await phone.getByRole("button", { name: "Show right drawer" }).click();
  await expect(phone.getByRole("dialog")).toHaveCSS("width", "390px");
  await shot(phone, "thread-390-dark");
  await phoneContext.close();

  // Live reconnect state and a real divergence caused by rewriting the project branch.
  await admin.setViewportSize({ width: 1440, height: 900 });
  await admin.goto(`/p/${project.id}/t/${thread.id}`);

  const offlineContext = await browser.newContext({
    extraHTTPHeaders: identityHeaders(ADMIN),
    viewport: { width: 1440, height: 900 },
  });
  const offline = await offlineContext.newPage();
  await offline.goto(`/p/${project.id}/t/${thread.id}`);
  await expect(offline.getByPlaceholder("Message")).toBeVisible();
  await offlineContext.setOffline(true);
  await expect(offline.getByText("Reconnecting…").first()).toBeVisible({ timeout: 10_000 });
  await shot(offline, "reconnecting-1440-dark");
  await offlineContext.setOffline(false);
  await expect(offline.getByText("Reconnecting…").first()).toBeHidden({ timeout: 15_000 });
  await expect(offline.getByText("Hello", { exact: false }).first()).toBeVisible({
    timeout: 15_000,
  });
  await expect(offline.getByText("Run tests", { exact: false }).first()).toBeVisible({
    timeout: 15_000,
  });
  await offlineContext.close();

  execSync(`git -C ${REPO} checkout --orphan diverged`, { stdio: "ignore" });
  execSync(`git -C ${REPO} rm -rf .`, { stdio: "ignore" });
  fs.writeFileSync(`${REPO}/README.md`, "diverged\n");
  execSync(`git -C ${REPO} add -A && git -C ${REPO} commit -m diverged`, { stdio: "ignore" });
  execSync(`git -C ${REPO} branch -M main`, { stdio: "ignore" });
  await admin.reload();
  await expect(
    admin.getByText("This thread has diverged from the project branch."),
  ).toBeVisible({ timeout: 10_000 });
  await shot(admin, "diverged-1440-dark");

  // Archive is a real thread action, not only a code path.
  await admin.goto(`/p/${project.id}/t/${thread.id}`);
  await admin.getByRole("button", { name: "Archive thread" }).click();
  await expect(admin).toHaveURL(new RegExp(`/p/${project.id}$`), { timeout: 15_000 });

  const loginContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
  });
  const login = await loginContext.newPage();
  await login.goto("/login");
  await expect(
    login.getByRole("heading", { name: "Sign in with Tailscale" }),
  ).toBeVisible();
  await expect(
    login.getByText("Open this workspace through Tailscale"),
  ).toBeVisible();
  await shot(login, "login-1440-dark");
  await loginContext.close();

  expect(adminErrors, adminErrors.join("\n")).toEqual([]);
  expect(bobErrors, bobErrors.join("\n")).toEqual([]);
  await adminContext.close();
  await bobContext.close();
  await api.dispose();
});
