import { chromium } from "@playwright/test";
const url = "http://127.0.0.1:5199/p/01M70PND37HV17X598BHKMYB4Y/t/01M788PWN5EAMR8QHHDQ12KY0Q";
const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome-stable", args: ["--disable-gpu"] });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });
await context.addCookies([{ name: "threadbase_session", value: "preview-token", domain: "127.0.0.1", path: "/" }]);
const page = await context.newPage();
await page.goto(url, { waitUntil: "domcontentloaded" });
await page.waitForSelector(".message-block", { timeout: 25000 });
await page.waitForTimeout(2500);
// Find the injected markdown message.
let info = null;
for (let i = 0; i < 40 && !info; i += 1) {
  info = await page.evaluate(() => {
    const block = [...document.querySelectorAll(".message-agent")].find((b) => b.textContent.includes("MARKDOWN-CHECK"));
    if (!block) return null;
    block.scrollIntoView({ block: "start" });
    const prose = block.querySelector(".prose");
    const p = [...prose.querySelectorAll("p")].find((el) => el.textContent.includes("第一行"));
    const pre = prose.querySelector("pre");
    const code = pre?.querySelector("code");
    const table = prose.querySelector("table");
    const th = table?.querySelector("th");
    return {
      br: p?.querySelectorAll("br").length ?? 0,
      pHeight: p ? Math.round(p.getBoundingClientRect().height) : null,
      pre: pre && {
        padding: getComputedStyle(pre).padding,
        background: getComputedStyle(pre).backgroundColor,
        border: getComputedStyle(pre).borderTopWidth,
        overflowX: getComputedStyle(pre).overflowX,
      },
      code: code && { border: getComputedStyle(code).borderTopWidth, padding: getComputedStyle(code).padding },
      table: table && {
        width: Math.round(table.getBoundingClientRect().width),
        thPadding: th ? getComputedStyle(th).padding : null,
        thBackground: th ? getComputedStyle(th).backgroundColor : null,
      },
      h2: (() => { const h = prose.querySelector("h2"); return h ? { marginTop: getComputedStyle(h).marginTop, text: h.textContent } : null; })(),
    };
  });
  if (!info) {
    await page.evaluate(() => { document.querySelector(".thread-scroll").scrollTop -= 900; });
    await page.waitForTimeout(300);
  }
}
console.log("markdown", JSON.stringify(info, null, 2));
await page.screenshot({ path: "/tmp/markdown-check.png" });

// Multi-line prompt through the shared draft.
const editor = page.locator(".composer-input .ProseMirror");
await editor.click();
await page.keyboard.press("Control+A");
await page.keyboard.press("Backspace");
await page.keyboard.type("line one");
await page.keyboard.press("Enter");
await page.keyboard.type("line two");
await page.keyboard.press("Shift+Enter");
await page.keyboard.type("line three");
await page.waitForTimeout(400);
const draftText = await page.evaluate(() => document.querySelector(".composer-input .ProseMirror").textContent);
await page.getByRole("button", { name: "Send", exact: true }).click();
await page.waitForTimeout(3000);
const bubble = await page.evaluate(() => {
  const block = [...document.querySelectorAll(".message-user")].find((b) => (b.textContent ?? "").includes("line one"));
  if (!block) return null;
  const p = block.querySelector(".prose p");
  return { text: p?.textContent, height: p ? Math.round(p.getBoundingClientRect().height) : null };
});
console.log("draftText", JSON.stringify(draftText), "bubble", JSON.stringify(bubble));
await page.screenshot({ path: "/tmp/newline-check.png" });
await browser.close();
