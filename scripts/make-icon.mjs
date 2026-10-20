// Rend design/app-icon.svg en PNG 1024 px (source de `tauri icon`).
import { readFileSync } from "node:fs";
import { chromium } from "playwright";

const svg = readFileSync("design/app-icon.svg", "utf8");
const browser = await chromium.launch({ executablePath: process.env.LUMEN_CHROMIUM || undefined });
const page = await browser.newPage({ viewport: { width: 1024, height: 1024 } });
await page.setContent(`<html><body style="margin:0;background:transparent">${svg}</body></html>`);
await page.locator("svg").screenshot({ path: "design/app-icon-1024.png", omitBackground: true });
await browser.close();
console.log("design/app-icon-1024.png");
