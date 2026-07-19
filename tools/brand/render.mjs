#!/usr/bin/env node
/**
 * Renders the raster brand exports from their vector/HTML sources.
 *
 *   node tools/brand/render.mjs
 *
 * Writes:
 *   assets/og-image.png                    <- tools/brand/og-card.html  (1200x630)
 *   apps/site/assets/apple-touch-icon.png  <- assets/favicon.svg        (180x180)
 *   apps/site/assets/og-image.png          <- copy of the card
 *
 * Everything is served over a throwaway local HTTP server first: file://
 * pages cannot load the sibling stylesheets/fonts under Chromium's
 * same-origin rules, and the card must have the real vendored faces
 * loaded before it is captured.
 */
import { chromium } from "playwright";
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const TYPES = {
  ".html": "text/html",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
};

const server = http
  .createServer((req, res) => {
    const rel = decodeURIComponent(req.url.split("?")[0]);
    const file = path.join(repo, rel);
    if (!file.startsWith(repo) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      res.writeHead(404);
      return res.end("not found");
    }
    res.writeHead(200, { "content-type": TYPES[path.extname(file)] ?? "application/octet-stream" });
    fs.createReadStream(file).pipe(res);
  })
  .listen(0);
await new Promise((r) => server.once("listening", r));
const base = `http://127.0.0.1:${server.address().port}`;

const browser = await chromium.launch();

async function shoot(url, width, height, out) {
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
  await page.goto(url, { waitUntil: "load" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(250);
  await page.screenshot({ path: path.join(repo, out) });
  await page.close();
  console.log("wrote", out);
}

await shoot(`${base}/tools/brand/og-card.html`, 1200, 630, "assets/og-image.png");
await shoot(`${base}/assets/favicon.svg`, 180, 180, "apps/site/assets/apple-touch-icon.png");

fs.copyFileSync(
  path.join(repo, "assets/og-image.png"),
  path.join(repo, "apps/site/assets/og-image.png"),
);
console.log("wrote apps/site/assets/og-image.png");

await browser.close();
server.close();
