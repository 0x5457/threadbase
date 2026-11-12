import { chromium } from "@playwright/test";
import fs from "node:fs";
const url = "http://127.0.0.1:5199/p/01M70PND37HV17X598BHKMYB4Y/t/01M788PWN5EAMR8QHHDQ12KY0Q";
const png = fs.readFileSync("/tmp/paste.png").toString("base64");
const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome-stable", args: ["--disable-gpu"] });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });
await context.addCookies([{ name: "threadbase_session", value: "preview-token", domain: "127.0.0.1", path: "/" }]);
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
await page.goto(url, { waitUntil: "domcontentloaded" });
await page.waitForSelector(".ProseMirror", { timeout: 25000 });
await page.waitForTimeout(2000);
await page.evaluate((base64) => {
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  const file = new File([bytes], "paste.png", { type: "image/png" });
  const data = new DataTransfer();
  data.items.add(file);
  const editor = document.querySelector(".composer-input .ProseMirror") ?? document.querySelector(".ProseMirror");
  editor.focus();
  editor.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
}, png);
await page.waitForSelector(".composer-image img");
await page.waitForFunction(() => !document.querySelector(".composer-image-busy"), null, { timeout: 15000 });
await page.waitForTimeout(500);
await page.getByRole("button", { name: "Send", exact: true }).click();
// The new user message with an attachment appears in the timeline.
await page.waitForSelector(".message-images img", { timeout: 20000 });
await page.waitForTimeout(1500);
const result = await page.evaluate(() => {
  const imgs = [...document.querySelectorAll(".message-images img")];
  const last = imgs[imgs.length - 1];
  return {
    count: imgs.length,
    src: last?.getAttribute("src"),
    width: last?.naturalWidth ?? 0,
    chipsLeft: document.querySelectorAll(".composer-image").length,
  };
});
await page.screenshot({ path: "/tmp/sent-image.png" });
console.log("result", result, "errors", errors.slice(0, 5));
await browser.close();
