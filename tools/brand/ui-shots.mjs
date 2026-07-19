#!/usr/bin/env node
/**
 * Screenshots of the uniform reference UI for the README.
 *
 *   pnpm --filter @vidmesh/gateway-web build
 *   node tools/brand/ui-shots.mjs
 *
 * Serves the built frontend and answers the gateway REST API with fixed
 * sample responses, because **no vidmesh gateway is deployed** — see the
 * status table in README.md. These are pictures of the real interface
 * with a stubbed backend, and the README says so; they are not pictures
 * of a running network, and must never be captioned as if they were.
 *
 * Writes apps/site/screenshots/ui-{dark,light}.png.
 */
import { chromium } from "playwright";
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const dist = path.join(repo, "apps", "gateway", "web", "dist");
const shots = path.join(repo, "apps", "site", "screenshots");

if (!fs.existsSync(dist)) {
  console.error("build the frontend first: pnpm --filter @vidmesh/gateway-web build");
  process.exit(1);
}

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".wasm": "application/wasm",
};

const hours = (n) => Math.floor(Date.now() / 1000) - n * 3600;
const VIDEOS = [
  ["Recovering a channel after a gateway shutdown", "kestrel.tv", 11 * 60_000 + 42_000, hours(5)],
  ["Chunk trees, explained with a whiteboard", "ana@substrate.dev", 8 * 60_000 + 15_000, hours(19)],
  ["Field recording: the coast road, 4am", "mor.audio", 26 * 60_000, hours(30)],
  ["Why we stopped running our own CDN", "kestrel.tv", 17 * 60_000 + 4_000, hours(54)],
  ["Reading the spec: identity rotation", "ana@substrate.dev", 33 * 60_000 + 20_000, hours(72)],
  ["A relay on a solar panel, six months in", "quiet.works", 6 * 60_000 + 58_000, hours(96)],
  ["Offline bundles across a border", "mor.audio", 14 * 60_000 + 11_000, hours(120)],
  ["Conformance vectors from scratch", "quiet.works", 21 * 60_000 + 39_000, hours(150)],
].map(([title, name, durationMs, createdAt], i) => ({
  id: "vm1" + String(i).padStart(3, "0") + "9f3ab2c7d1e",
  title,
  author: { identityId: "vmid" + i, name },
  thumbnailUrl: null,
  durationMs,
  createdAt,
}));

const ROUTES = {
  "/api/info": { gateway: { name: "demo.gateway", about: "" }, relays: [], uploadEnabled: true },
  "/api/videos": { items: VIDEOS, nextCursor: null },
};

const server = http
  .createServer((req, res) => {
    const url = req.url.split("?")[0];
    if (url in ROUTES) {
      res.writeHead(200, { "content-type": "application/json" });
      return res.end(JSON.stringify(ROUTES[url]));
    }
    if (url.startsWith("/api/")) {
      res.writeHead(401, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: { code: "unauthenticated", message: "no session" } }));
    }
    let file = path.join(dist, url);
    if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) file = path.join(dist, "index.html");
    res.writeHead(200, { "content-type": TYPES[path.extname(file)] ?? "application/octet-stream" });
    fs.createReadStream(file).pipe(res);
  })
  .listen(0);
await new Promise((r) => server.once("listening", r));
const base = `http://localhost:${server.address().port}`;

fs.mkdirSync(shots, { recursive: true });
const browser = await chromium.launch();
for (const scheme of ["dark", "light"]) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 }, colorScheme: scheme });
  await page.goto(base + "/", { waitUntil: "networkidle" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForSelector("h3");
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(shots, `ui-${scheme}.png`) });
  console.log("wrote apps/site/screenshots/ui-" + scheme + ".png");
  await page.close();
}
await browser.close();
server.close();
