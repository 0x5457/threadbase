import {
  expect,
  test,
  type APIRequestContext,
  type Page,
} from "@playwright/test";

/**
 * Opt-in visual/motion probe for the UI polish audit.
 *
 *   E2E_UI_PROBE=1 pnpm --dir web exec playwright test tests/e2e/ui-probe.spec.ts
 *
 * Verifies 10%-equivalent motion sampling, disclosure rotation, press scale,
 * hit-area expansion, touch tree chevrons and horizontal overflow.
 */

const BASE = "http://127.0.0.1:7790";
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
  const projects = await api.get(`${BASE}/api/projects`);
  const projectList = (await projects.json()).projects as Array<{ id: string }>;
  let projectId = projectList[0]?.id;
  if (!projectId) {
    const created = await api.post(`${BASE}/api/projects`, {
      headers: headers(),
      data: { path: "/tmp/threadbase-e2e/repo", name: null },
    });
    projectId = (await created.json()).project.id;
  }
  const created = await api.post(`${BASE}/api/projects/${projectId}/threads`, {
    headers: headers(),
    data: {
      title: "Probe thread",
      agent: "codex",
      model: null,
      effort: null,
      baseBranch: "main",
      initialPrompt: null,
    },
  });
  expect(created.status()).toBe(200);
  const threadId = (await created.json()).thread.id;
  return { projectId, threadId };
}


async function auditTargets(page: Page) {
  return page.evaluate(() => {
    const visible = (selector: string) => {
      const elements = Array.from(document.querySelectorAll<HTMLElement>(selector));
      return elements.find((element) => element.getBoundingClientRect().width > 0) ?? null;
    };
    const rect = (element: HTMLElement | null) => {
      if (!element) return null;
      const box = element.getBoundingClientRect();
      const after = getComputedStyle(element, "::after");
      const hitWidth = after.content === "none" ? 0 : parseFloat(after.width) || 0;
      const hitHeight = after.content === "none" ? 0 : parseFloat(after.height) || 0;
      const width = Math.max(box.width, hitWidth);
      const height = Math.max(box.height, hitHeight);
      return {
        width: Math.round(width),
        height: Math.round(height),
        hitWidth: Math.round(hitWidth),
        hitHeight: Math.round(hitHeight),
        left: box.left + box.width / 2 - width / 2,
        right: box.left + box.width / 2 + width / 2,
        top: box.top + box.height / 2 - height / 2,
        bottom: box.top + box.height / 2 + height / 2,
      };
    };
    const pair = (a: HTMLElement | null, b: HTMLElement | null) => {
      const one = rect(a);
      const two = rect(b);
      if (!one || !two) return null;
      return {
        overlap:
          one.left < two.right &&
          one.right > two.left &&
          one.top < two.bottom &&
          one.bottom > two.top,
        a: one,
        b: two,
      };
    };
    const send = Array.from(document.querySelectorAll<HTMLElement>("main button")).find(
      (element) => element.textContent?.trim() === "Send",
    ) ?? null;
    const more = visible('[aria-label="More send options"]');
    const theme = visible('aside [aria-label*="Switch to"]');
    const account = visible('aside [aria-label="Account menu"]');
    return {
      theme: rect(theme),
      chevron: rect(visible("[data-tree-chevron]")),
      mention: rect(visible('[aria-label="Mention a teammate"]')),
      search: rect(visible('header [aria-label="Search workspace"]')),
      pairs: {
        message: pair(
          visible('[aria-label="View comments"]'),
          visible('[aria-label="Comment on message"]'),
        ),
        titlebar: pair(
          visible('header [aria-label="Approvals"]'),
          visible('header [aria-label="Show right drawer"]'),
        ),
        footer: pair(theme, account),
        threadHeader: pair(
          visible('[aria-label="Archive thread"]'),
          visible('[aria-label="Thread actions"]'),
        ),
        composer: pair(send, more),
        sidebarSearch: pair(
          visible('header [aria-label="Hide sidebar"]'),
          visible('header [aria-label="Search workspace"]'),
        ),
      },
    };
  });
}

async function sendStream(page: Page) {
  await page.getByPlaceholder("Message").fill("stream");
  const send = page.getByRole("button", { name: "Send", exact: true });
  await expect(send).toBeEnabled({ timeout: 20_000 });
  await send.click();
  await expect(page.getByText("Hello", { exact: false }).first()).toBeVisible({
    timeout: 30_000,
  });
  await expect(
    page.getByRole("button", { name: "Stop", exact: true }),
  ).toBeHidden({ timeout: 20_000 });
}

test("motion, hit area and state probe", async ({ browser, playwright }) => {
  test.skip(process.env.E2E_UI_PROBE !== "1", "probe is opt-in");
  const api = await playwright.request.newContext({
    baseURL: BASE,
    extraHTTPHeaders: {
      "tailscale-user-login": ADMIN.login,
      "tailscale-user-name": ADMIN.displayName,
    },
  });
  const { projectId, threadId } = await seed(api);

  const context = await browser.newContext({
    extraHTTPHeaders: {
      "tailscale-user-login": ADMIN.login,
      "tailscale-user-name": ADMIN.displayName,
    },
    viewport: { width: 1440, height: 900 },
    reducedMotion: "no-preference",
  });
  const page = await context.newPage();
  await page.goto(`/p/${projectId}/t/${threadId}`);
  await expect(page.getByPlaceholder("Message")).toBeVisible({ timeout: 20_000 });
  await sendStream(page);
  const targets = await auditTargets(page);

  // Empty/disabled states.
  await expect(
    page.getByRole("button", { name: "Send", exact: true }),
  ).toBeDisabled();

  // 10% replay of the disclosure chevron (120ms → 1.2s).
  await page.addStyleTag({
    content:
      "summary [data-disclosure-chevron] { transition-duration: 1200ms !important; }",
  });
  const summary = page.locator("summary", { hasText: "Details" }).first();
  await summary.evaluate((element) => (element as HTMLElement).click());
  await page.waitForTimeout(120);
  const chevron = summary.locator("[data-disclosure-chevron]");
  const midRotation = await chevron.evaluate(
    (element) => getComputedStyle(element).transform,
  );
  await page.waitForTimeout(1400);
  const endRotation = await chevron.evaluate(
    (element) => getComputedStyle(element).transform,
  );

  const metrics = await page.evaluate(() => {
    const box = (selector: string) => {
      const element = document.querySelector<HTMLElement>(selector);
      if (!element) return null;
      const rect = element.getBoundingClientRect();
      return {
        width: Math.round(rect.width),
        height: Math.round(rect.height),
        hit: getComputedStyle(element, "::after").width,
      };
    };
    return {
      overflow1440: document.documentElement.scrollWidth - window.innerWidth,
      titlebarApprovals: box('header [aria-label="Approvals"]'),
      messageComment: box('main [aria-label="Comment on message"]'),
    };
  });

  await page.setViewportSize({ width: 768, height: 900 });
  const overflow768 = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );

  // Press scale on a filled button. A fixed probe button avoids depending on
  // page scroll/overlay geometry while exercising the same global CSS rule.
  await page.goto("/");
  const projectRow = page.locator('a[href^="/p/"]').first();
  await expect(projectRow).toBeVisible();
  const hoverBefore = await projectRow.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  await projectRow.hover();
  await page.waitForTimeout(60);
  const hoverAfter = await projectRow.evaluate(
    (element) => getComputedStyle(element).backgroundColor,
  );
  const addProject = page.getByRole("button", { name: "Add project" });
  const addMinHeight = await addProject.evaluate(
    (element) => getComputedStyle(element).minHeight,
  );
  const projectRowHeight = Math.round((await projectRow.boundingBox())?.height ?? 0);
  await addProject.click();
  const back = page.getByLabel("Parent directory");
  await expect(back).toBeVisible({ timeout: 20_000 });
  const backHit = await back.evaluate(
    (element) => getComputedStyle(element, "::after").width,
  );
  await page.keyboard.press("Escape");
  await page.goto(`/p/${projectId}`);
  const splitBox = await page.getByLabel("New thread options").boundingBox();
  const mainThreadBox = await page
    .getByRole("button", { name: "New thread", exact: true })
    .boundingBox();
  const splitOverlap =
    splitBox && mainThreadBox
      ? !(
          splitBox.x + splitBox.width <= mainThreadBox.x ||
          mainThreadBox.x + mainThreadBox.width <= splitBox.x ||
          splitBox.y + splitBox.height <= mainThreadBox.y ||
          mainThreadBox.y + mainThreadBox.height <= splitBox.y
        )
      : true;
  await page.goto("/settings");
  const themeButtonHeights = await page.evaluate(() =>
    Array.from(document.querySelectorAll<HTMLElement>("button"))
      .filter((element) => ["Dark", "Light"].includes(element.textContent?.trim() ?? ""))
      .map((element) => Math.round(element.getBoundingClientRect().height)),
  );
  await page.evaluate(() => {
    const button = document.createElement("button");
    button.id = "press-probe";
    button.className = "mantine-Button-root";
    button.dataset.variant = "filled";
    button.textContent = "Press probe";
    Object.assign(button.style, {
      position: "fixed",
      left: "10px",
      top: "10px",
      zIndex: "9999",
      minHeight: "40px",
    });
    document.body.appendChild(button);
  });
  const probeButton = page.locator("#press-probe");
  const probeBox = await probeButton.boundingBox();
  if (probeBox) {
    await page.mouse.move(
      probeBox.x + probeBox.width / 2,
      probeBox.y + probeBox.height / 2,
    );
    await page.mouse.down();
  }
  await page.waitForTimeout(220);
  const addPressScale = await probeButton.evaluate(
    (element) => getComputedStyle(element).scale,
  );
  await page.mouse.up();
  await context.close();

  // 10% replay of the theme icon crossfade (300ms → 3s).
  const loginContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: "no-preference",
  });
  const login = await loginContext.newPage();
  await login.goto("/login");
  expect(
    await login.evaluate(() =>
      matchMedia("(prefers-reduced-motion: reduce)").matches,
    ),
  ).toBe(false);
  await login.addStyleTag({
    content: ".tb-theme-icon { transition-duration: 3000ms !important; }",
  });
  const themeAnimations = await login.evaluate(() => {
    const icons = Array.from(
      document.querySelectorAll<HTMLElement>(".tb-theme-icon"),
    );
    if (icons.length !== 2) {
      throw new Error("expected two theme icons");
    }
    icons[0].dataset.active = "true";
    icons[1].dataset.active = "false";
    void document.body.offsetHeight;
    icons[0].dataset.active = "false";
    icons[1].dataset.active = "true";
    return document.getAnimations().length;
  });
  await login.waitForTimeout(300);
  const themeMid = await login.evaluate(() =>
    Array.from(document.querySelectorAll<HTMLElement>(".tb-theme-icon")).map(
      (element) => ({
        opacity: getComputedStyle(element).opacity,
        scale: getComputedStyle(element).scale,
        filter: getComputedStyle(element).filter,
      }),
    ),
  );
  await loginContext.close();

  // Touch tree chevron is 44px on a phone.
  const phoneContext = await browser.newContext({
    extraHTTPHeaders: {
      "tailscale-user-login": ADMIN.login,
      "tailscale-user-name": ADMIN.displayName,
    },
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
    reducedMotion: "no-preference",
  });
  const phone = await phoneContext.newPage();
  await phone.goto(`/p/${projectId}/t/${threadId}`);
  await expect(phone.getByPlaceholder("Message")).toBeVisible({ timeout: 20_000 });
  await phone.getByLabel("Show sidebar").click();
  const chevronBox = await phone
    .locator("[data-tree-chevron]")
    .first()
    .boundingBox();
  await phoneContext.close();

  // Reduced motion removes the transition but keeps both icon states.
  const reducedContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: "reduce",
  });
  const reduced = await reducedContext.newPage();
  await reduced.goto("/login");
  const reducedDuration = await reduced
    .locator(".tb-theme-icon")
    .first()
    .evaluate((element) => getComputedStyle(element).transitionDuration);
  await reducedContext.close();

  console.log(
    JSON.stringify(
      {
        midRotation,
        endRotation,
        themeAnimations,
        themeMid,
        metrics,
        overflow768,
        addPressScale,
        addMinHeight,
        hoverBefore,
        hoverAfter,
        projectRowHeight,
        splitButton: splitBox,
        splitOverlap,
        backHit,
        themeButtonHeights,
        targets,
        touchChevron: chevronBox,
        reducedDuration,
      },
      null,
      2,
    ),
  );

  expect(midRotation).not.toBe(endRotation);
  expect(endRotation).toContain("matrix(0, 1, -1, 0, 0, 0)");
  expect(themeAnimations).toBeGreaterThan(0);
  expect(themeMid.length).toBe(2);
  expect(
    themeMid.some(
      (icon) =>
        (icon.opacity !== "0" && icon.opacity !== "1") ||
        (icon.scale !== "0.25" && icon.scale !== "1"),
    ),
  ).toBe(true);
  expect(metrics.overflow1440).toBeLessThanOrEqual(0);
  expect(overflow768).toBeLessThanOrEqual(0);
  expect(addPressScale).toBe("0.96");
  expect(addMinHeight).toBe("40px");
  expect(hoverAfter).not.toBe(hoverBefore);
  expect(metrics.titlebarApprovals?.hit).toBe("40px");
  expect(metrics.messageComment?.hit).toBe("40px");
  expect(chevronBox?.width).toBe(44);
  expect(chevronBox?.height).toBe(44);
  expect(reducedDuration).toBe("0s");
  expect(targets.theme?.hitWidth).toBe(40);
  expect(targets.chevron?.width).toBe(40);
  expect(targets.chevron?.height).toBe(40);
  expect(targets.mention?.height).toBe(40);
  expect(targets.search?.height).toBe(40);
  expect(projectRowHeight).toBeGreaterThanOrEqual(40);
  expect(splitBox?.width).toBe(40);
  expect(splitBox?.height).toBe(40);
  expect(splitOverlap).toBe(false);
  expect(backHit).toBe("40px");
  expect(themeButtonHeights.every((height) => height >= 40)).toBe(true);
  for (const pair of Object.values(targets.pairs)) {
    expect(pair?.overlap).toBe(false);
  }
  await api.dispose();
});
