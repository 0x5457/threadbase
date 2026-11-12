import { chromium } from "@playwright/test";
const url = "http://127.0.0.1:5199/p/01M70PND37HV17X598BHKMYB4Y/t/01M788PWN5EAMR8QHHDQ12KY0Q";
const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome-stable", args: ["--disable-gpu"] });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });
await context.addCookies([{ name: "threadbase_session", value: "preview-token", domain: "127.0.0.1", path: "/" }]);
const page = await context.newPage();
page.on("pageerror", (e) => console.log("pageerror", e.message));
await page.goto(url, { waitUntil: "domcontentloaded" });
await page.waitForSelector(".message-block", { timeout: 25000 });
await page.waitForTimeout(2600);
let info = null;
for (let i = 0; i < 30 && !info; i += 1) {
  info = await page.evaluate(() => {
    const block = [...document.querySelectorAll(".message-agent")].find((b) => b.textContent.includes("MARKDOWN-CHECK"));
    if (!block) return null;
    block.scrollIntoView({ block: "center" });
    const prose = block.querySelector(".prose");
    const p = [...prose.querySelectorAll("p")].find((el) => el.textContent.includes("第一行"));
    const pre = prose.querySelector("pre");
    const code = pre?.querySelector("code");
    const table = prose.querySelector("table");
    const th = table?.querySelector("th");
    const h2 = prose.querySelector("h2");
    return {
      br: p?.querySelectorAll("br").length ?? 0,
      pHeight: p ? Math.round(p.getBoundingClientRect().height) : null,
      pre: pre && {
        padding: getComputedStyle(pre).padding,
        background: getComputedStyle(pre).backgroundColor,
        borderWidth: getComputedStyle(pre).borderTopWidth,
        overflowX: getComputedStyle(pre).overflowX,
      },
      codeInsidePre: code && { border: getComputedStyle(code).borderTopWidth, padding: getComputedStyle(code).padding },
      table: table && { width: Math.round(table.getBoundingClientRect().width), thPadding: th && getComputedStyle(th).padding, thBackground: th && getComputedStyle(th).backgroundColor },
      headingMargin: h2 && getComputedStyle(h2).marginTop,
    };
  });
  if (!info) {
    await page.evaluate(() => { document.querySelector(".thread-scroll").scrollTop -= 900; });
    await page.waitForTimeout(250);
  }
}
console.log("markdown", JSON.stringify(info, null, 2));
await page.screenshot({ path: "/tmp/md-check.png" });

// Multi-line prompt through the shared draft: Enter makes a paragraph, Shift+Enter a hard break.
const editor = page.locator(".composer-input .ProseMirror");
await editor.click();
await page.keyboard.press("Control+A");
await page.keyboard.press("Backspace");
await page.keyboard.type("line one");
await page.keyboard.press("Enter");
await page.keyboard.type("line two");
await page.keyboard.press("Shift+Enter");
await page.keyboard.type("line three");
await page.waitForTimeout(500);
const editorHtml = await page.evaluate(() => document.querySelector(".composer-input .ProseMirror").innerHTML);
await page.getByRole("button", { name: "Send", exact: true }).click();
await page.waitForTimeout(3500);
const bubble = await page.evaluate(() => {
  const block = [...document.querySelectorAll(".message-user")].find((b) => (b.textContent ?? "").includes("line one"));
  if (!block) return null;
  const p = block.querySelector(".prose p");
  return {
    text: p?.textContent,
    height: p ? Math.round(p.getBoundingClientRect().height) : null,
    lineHeight: p ? getComputedStyle(p).lineHeight : null,
  };
});
console.log("editorHtml", JSON.stringify(editorHtml));
console.log("bubble", JSON.stringify(bubble));
await page.screenshot({ path: "/tmp/newline-check.png" });
await browser.close();
