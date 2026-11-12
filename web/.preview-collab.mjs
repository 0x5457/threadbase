import { chromium } from "@playwright/test";
const url = "http://127.0.0.1:5199/p/01M70PND37HV17X598BHKMYB4Y/t/01M788PWN5EAMR8QHHDQ12KY0Q";
const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome-stable", args: ["--disable-gpu"] });

async function open(token, extra = "") {
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });
  await context.addCookies([{ name: "threadbase_session", value: token, domain: "127.0.0.1", path: "/" }]);
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  await page.goto(url, { waitUntil: "domcontentloaded" });
  await page.waitForSelector(".ProseMirror", { timeout: 25000 });
  await page.waitForTimeout(2500);
  return { context, page, errors };
}

const ada = await open("preview-token");
const bob = await open("preview-bob");

// Ada types; Bob's editor must receive the same text through the CRDT.
await ada.page.evaluate(() => {
  const el = document.querySelector(".composer-input .ProseMirror");
  el.focus();
  document.execCommand("insertText", false, "hello from ada");
});
await bob.page.waitForFunction(
  () => document.querySelector(".composer-input .ProseMirror")?.textContent.includes("hello from ada"),
  null,
  { timeout: 10000 },
);
// Bob types at the end; Ada must see it, and Bob's caret must appear for Ada.
await bob.page.evaluate(() => {
  const el = document.querySelector(".composer-input .ProseMirror");
  el.focus();
  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const selection = window.getSelection();
  selection.removeAllRanges();
  selection.addRange(range);
  document.execCommand("insertText", false, " and bob");
});
await ada.page.waitForFunction(
  () => document.querySelector(".composer-input .ProseMirror")?.textContent.includes("and bob"),
  null,
  { timeout: 10000 },
);
await ada.page.waitForTimeout(800);

const adaText = await ada.page.evaluate(() => document.querySelector(".composer-input .ProseMirror").textContent);
const bobText = await bob.page.evaluate(() => document.querySelector(".composer-input .ProseMirror").textContent);
const caret = await bob.page.evaluate(() => document.querySelectorAll("[class*='collaboration-carets__caret']").length);
console.log("ada:", JSON.stringify(adaText), "bob:", JSON.stringify(bobText), "bob sees carets:", caret);

// Send from Ada: the server consumes the shared draft, both editors must clear.
await ada.page.getByRole("button", { name: "Send", exact: true }).click();
await ada.page.waitForTimeout(2500);
const cleared = {
  ada: await ada.page.evaluate(() => document.querySelector(".composer-input .ProseMirror")?.textContent),
  bob: await bob.page.evaluate(() => document.querySelector(".composer-input .ProseMirror")?.textContent),
};
console.log("after send:", JSON.stringify(cleared), "errors:", [...ada.errors, ...bob.errors].slice(0, 5));
await ada.page.screenshot({ path: "/tmp/collab.png" });
await ada.context.close();
await bob.context.close();
await browser.close();
