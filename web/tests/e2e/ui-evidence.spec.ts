import {
  expect,
  test,
  type APIRequestContext,
  type Browser,
  type Page,
} from "@playwright/test";
import path from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Opt-in visual evidence for the UI polish audit.
 *
 *   E2E_UI_EVIDENCE=1 pnpm --dir web exec playwright test tests/e2e/ui-evidence.spec.ts
 *
 * Captures the 9-route × 1440/768 × dark/light matrix plus empty, loading,
 * error, comment, confirm, new-project, approvals and review-drawer states.
 * The rich e2e screenshots remain the job of `E2E_SCREENSHOTS=1`.
 */

const BASE = "http://127.0.0.1:7790";
const OUT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../docs/screenshots/mifb",
  process.env.E2E_UI_EVIDENCE_DIR ?? "after",
);
const ADMIN = {
  login: "ada@example.com",
  username: "ada",
  displayName: "Ada Lovelace",
};

function headers() {
  return {
    origin: BASE,
    "x-requested-with": "threadbase",
    "content-type": "application/json",
    "tailscale-user-login": ADMIN.login,
    "tailscale-user-name": ADMIN.displayName,
  };
}

async function seed(api: APIRequestContext) {
  const auth = await api.get(`${BASE}/api/auth/me`, { headers: headers() });
  expect(auth.status()).toBe(200);
  const projectsResponse = await api.get(`${BASE}/api/projects`);
  const projects = (await projectsResponse.json()).projects as Array<{
    id: string;
  }>;
  let projectId = projects[0]?.id;
  if (!projectId) {
    const created = await api.post(`${BASE}/api/projects`, {
      headers: headers(),
      data: { path: "/tmp/threadbase-e2e/repo", name: null },
    });
    projectId = (await created.json()).project.id;
  }
  const evidence = await api.post(`${BASE}/api/projects/${projectId}/threads`, {
    headers: headers(),
    data: {
      title: "Evidence thread",
      agent: "codex",
      model: null,
      effort: null,
      baseBranch: "main",
      initialPrompt: null,
    },
  });
  expect(evidence.status()).toBe(200);
  const threadId = (await evidence.json()).thread.id;

  const empty = await api.post(`${BASE}/api/projects/${projectId}/threads`, {
    headers: headers(),
    data: {
      title: "Empty thread",
      agent: "claude",
      model: null,
      effort: null,
      baseBranch: "main",
      initialPrompt: null,
    },
  });
  expect(empty.status()).toBe(200);
  const emptyThreadId = (await empty.json()).thread.id;

  return { projectId, threadId, emptyThreadId };
}

async function themedContext(
  browser: Browser,
  theme: "dark" | "light",
  width: number,
) {
  const context = await browser.newContext({
    extraHTTPHeaders: {
      "tailscale-user-login": ADMIN.login,
      "tailscale-user-name": ADMIN.displayName,
    },
    viewport: { width, height: 900 },
    reducedMotion: "reduce",
  });
  await context.addInitScript((value) => {
    try {
      localStorage.setItem("threadbase.theme", value);
    } catch {
      // Ignore opaque origins.
    }
  }, theme);
  return context;
}

async function shot(page: Page, name: string) {
  await page.screenshot({ path: path.join(OUT, `${name}.png`), fullPage: true });
}

async function matrix(
  browser: Browser,
  theme: "dark" | "light",
  width: number,
  ids: { projectId: string; threadId: string; emptyThreadId: string },
) {
  const context = await themedContext(browser, theme, width);
  const page = await context.newPage();
  const routes = [
    { name: "workspace", path: "/", wait: "Projects" },
    { name: "project", path: `/p/${ids.projectId}`, wait: "Threads" },
    {
      name: "thread",
      path: `/p/${ids.projectId}/t/${ids.threadId}`,
      wait: "Evidence thread",
    },
    { name: "inbox", path: "/inbox", wait: "Inbox" },
    { name: "settings", path: "/settings", wait: "Settings" },
    {
      name: "compare",
      path: `/p/${ids.projectId}/compare`,
      wait: "Compare threads",
    },
    { name: "invite", path: "/invite/not-a-real-token", wait: "Invite" },
    { name: "notfound", path: "/route-that-does-not-exist", wait: "Page not found" },
  ];
  for (const route of routes) {
    await page.goto(route.path);
    await expect(page.getByText(route.wait, { exact: false }).first()).toBeVisible({
      timeout: 20_000,
    });
    await page.waitForTimeout(300);
    await shot(page, `${route.name}-${width}-${theme}`);
  }
  await context.close();

}

async function loginShots(browser: Browser) {
  for (const theme of ["dark", "light"] as const) {
    for (const width of [1440, 768]) {
      const loginContext = await browser.newContext({
        viewport: { width, height: 900 },
        reducedMotion: "reduce",
      });
      await loginContext.addInitScript((value) => {
        try {
          localStorage.setItem("threadbase.theme", value);
        } catch {
          // Ignore opaque origins.
        }
      }, theme);
      const login = await loginContext.newPage();
      await login.goto("/login");
      await expect(
        login.getByRole("heading", { name: "Sign in with Tailscale" }),
      ).toBeVisible();
      await login.waitForTimeout(300);
      await shot(login, `login-${width}-${theme}`);
      await loginContext.close();
    }
  }
}

async function stateShots(
  browser: Browser,
  ids: { projectId: string; threadId: string; emptyThreadId: string },
) {
  const context = await themedContext(browser, "dark", 1440);
  const page = await context.newPage();

  // Empty thread.
  await page.goto(`/p/${ids.projectId}/t/${ids.emptyThreadId}`);
  await expect(page.getByText("No activity yet")).toBeVisible({ timeout: 20_000 });
  await shot(page, "thread-empty-1440-dark");

  // Thread error.
  await page.route("**/api/threads/*/events*", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: "probe" }),
    }),
  );
  await page.goto(`/p/${ids.projectId}/t/${ids.threadId}`);
  await expect(page.getByText("Cannot load thread activity")).toBeVisible({
    timeout: 20_000,
  });
  await shot(page, "thread-error-1440-dark");
  await page.unroute("**/api/threads/*/events*");

  // Workspace loading and error.
  await page.route("**/api/projects", async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 2000));
    await route.continue().catch(() => undefined);
  });
  await page.goto("/");
  await page.waitForTimeout(250);
  await shot(page, "workspace-loading-1440-dark");
  await page.unroute("**/api/projects");

  await page.route("**/api/projects", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: "probe" }),
    }),
  );
  await page.goto("/");
  await expect(page.getByText("Cannot load projects")).toBeVisible({
    timeout: 20_000,
  });
  await shot(page, "workspace-error-1440-dark");
  await page.unroute("**/api/projects");

  // New project dialog.
  await page.goto("/");
  await page.getByRole("button", { name: "Add project" }).click();
  await expect(page.getByText("Add project", { exact: true }).last()).toBeVisible();
  await shot(page, "new-project-1440-dark");
  await page.keyboard.press("Escape");

  // Thread message for comment/dialog states.
  await page.goto(`/p/${ids.projectId}/t/${ids.threadId}`);
  await page.reload();
  await expect(page.getByPlaceholder("Message")).toBeVisible({ timeout: 20_000 });
  await page.getByPlaceholder("Message").fill("stream");
  const send = page.getByRole("button", { name: "Send", exact: true });
  await expect(send).toBeEnabled({ timeout: 20_000 });
  await send.click();
  await expect(page.getByText("Hello", { exact: false }).first()).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByRole("button", { name: "Stop", exact: true })).toBeHidden({
    timeout: 20_000,
  });

  // Comment dialog.
  await page.getByLabel("Comment on message").first().click();
  await expect(page.getByText("Add comment", { exact: true })).toBeVisible();
  await shot(page, "comment-dialog-1440-dark");
  await page.keyboard.press("Escape");

  // Approvals drawer. No thread comment exists yet, so the fixture agent
  // produces the permission request instead of addressing review comments.
  await page.getByPlaceholder("Message").fill("permission");
  const permissionSend = page.getByRole("button", { name: "Send", exact: true });
  await expect(permissionSend).toBeEnabled({ timeout: 20_000 });
  await permissionSend.click();
  await expect(page.getByRole("button", { name: "Allow once" }).first()).toBeVisible({
    timeout: 30_000,
  });
  await page.getByLabel("Approvals").click();
  await expect(page.getByRole("complementary", { name: "Right drawer" })).toBeVisible();
  await shot(page, "approvals-drawer-1440-dark");
  await page.keyboard.press("Shift+Escape");

  // Confirm dialog via comment deletion on the empty thread.
  await context.request.post(`${BASE}/api/threads/${ids.emptyThreadId}/comments`, {
    headers: headers(),
    data: {
      body: "Evidence comment",
      anchor: { kind: "thread" },
      parentId: null,
      send: false,
    },
  });
  await page.goto(`/p/${ids.projectId}/t/${ids.emptyThreadId}`);
  await page.getByLabel("Show right drawer").click();
  await expect(page.getByRole("complementary", { name: "Right drawer" })).toBeVisible();
  await page
    .getByRole("complementary", { name: "Right drawer" })
    .getByRole("tab", { name: "Comments" })
    .click();
  await page.getByLabel("Delete comment").first().click();
  await expect(page.getByText("Delete comment?", { exact: false })).toBeVisible();
  await shot(page, "confirm-dialog-1440-dark");
  await page.keyboard.press("Escape");

  await context.close();

  // 768 light versions of the empty/error/comment/confirm states.
  const light = await themedContext(browser, "light", 768);
  const lightPage = await light.newPage();
  await lightPage.goto(`/p/${ids.projectId}/t/${ids.emptyThreadId}`);
  await expect(lightPage.getByText("No activity yet")).toBeVisible({ timeout: 20_000 });
  await shot(lightPage, "thread-empty-768-light");

  await lightPage.route("**/api/threads/*/events*", (route) =>
    route.fulfill({
      status: 500,
      contentType: "application/json",
      body: JSON.stringify({ error: "probe" }),
    }),
  );
  await lightPage.goto(`/p/${ids.projectId}/t/${ids.threadId}`);
  await expect(lightPage.getByText("Cannot load thread activity")).toBeVisible({
    timeout: 20_000,
  });
  await shot(lightPage, "thread-error-768-light");
  await lightPage.unroute("**/api/threads/*/events*");

  await lightPage.goto(`/p/${ids.projectId}/t/${ids.threadId}`);
  await lightPage.reload();
  await expect(lightPage.getByPlaceholder("Message")).toBeVisible({ timeout: 20_000 });
  await lightPage.getByPlaceholder("Message").fill("stream");
  const lightSend = lightPage.getByRole("button", { name: "Send", exact: true });
  await expect(lightSend).toBeEnabled({ timeout: 20_000 });
  await lightSend.click();
  await expect(lightPage.getByText("Hello", { exact: false }).first()).toBeVisible({
    timeout: 30_000,
  });
  await expect(lightPage.getByRole("button", { name: "Stop", exact: true })).toBeHidden({
    timeout: 20_000,
  });
  await lightPage.getByLabel("Comment on message").first().click();
  await expect(lightPage.getByText("Add comment", { exact: true })).toBeVisible();
  await shot(lightPage, "comment-dialog-768-light");
  await lightPage.keyboard.press("Escape");

  await light.request.post(`${BASE}/api/threads/${ids.emptyThreadId}/comments`, {
    headers: headers(),
    data: {
      body: "Evidence comment 768",
      anchor: { kind: "thread" },
      parentId: null,
      send: false,
    },
  });
  await lightPage.goto(`/p/${ids.projectId}/t/${ids.emptyThreadId}`);
  await lightPage.getByLabel("Show right drawer").click();
  await expect(
    lightPage.getByRole("complementary", { name: "Right drawer" }),
  ).toBeVisible();
  await lightPage
    .getByRole("complementary", { name: "Right drawer" })
    .getByRole("tab", { name: "Comments" })
    .click();
  await lightPage.getByLabel("Delete comment").first().click();
  await expect(lightPage.getByText("Delete comment?", { exact: false })).toBeVisible();
  await shot(lightPage, "confirm-dialog-768-light");

  await light.close();
}

test("UI evidence matrix and states", async ({ browser, playwright }) => {
  test.skip(process.env.E2E_UI_EVIDENCE !== "1", "evidence capture is opt-in");
  const api = await playwright.request.newContext({
    baseURL: BASE,
    extraHTTPHeaders: {
      "tailscale-user-login": ADMIN.login,
      "tailscale-user-name": ADMIN.displayName,
    },
  });
  const ids = await seed(api);
  if (process.env.E2E_UI_STATES_ONLY === "1") {
    await stateShots(browser, ids);
    await api.dispose();
    return;
  }
  for (const theme of ["dark", "light"] as const) {
    for (const width of [1440, 768]) {
      await matrix(browser, theme, width, ids);
    }
  }
  await loginShots(browser);
  await stateShots(browser, ids);
  await api.dispose();
});
