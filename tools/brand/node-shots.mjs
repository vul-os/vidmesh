#!/usr/bin/env node
/**
 * A screenshot of the desktop (Tauri) node client's browse view, for the
 * README/site alongside `ui-shots.mjs`'s gateway-web screenshots.
 *
 *   pnpm --filter @evermesh/node-web build
 *   node tools/brand/node-shots.mjs
 *
 * `crates/evermesh-node/ui` (the node app's built frontend, gitignored —
 * see justfile) is a plain static bundle: everything it does crosses an
 * `invoke()` IPC boundary into Rust (`src/lib/tauri.ts`), which only
 * exists inside an actual Tauri webview. There is no Tauri runtime in
 * this environment, so this script does for that boundary what
 * `ui-shots.mjs` does for the gateway's REST API: it defines
 * `window.__TAURI_INTERNALS__.invoke` before the app's own scripts run
 * (the same mechanism `@tauri-apps/api/mocks`'s `mockIPC` uses, reduced
 * to the handful of commands `Browse.tsx` actually calls) and answers it
 * with fixed sample data. This is a picture of the real interface with a
 * stubbed native backend, not a picture of a built desktop binary — the
 * same honesty rule `ui-shots.mjs` states for the gateway screenshots
 * applies here.
 *
 * Writes apps/site/screenshots/ui-node-{dark,light}.png.
 */
import { chromium } from "playwright";
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const dist = path.join(repo, "crates", "evermesh-node", "ui");
const shots = path.join(repo, "apps", "site", "screenshots");

if (!fs.existsSync(dist)) {
  console.error("build the node frontend first: pnpm --filter @evermesh/node-web build");
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
};

const hours = (n) => Math.floor(Date.now() / 1000) - n * 3600;

/** Same deterministic-hue placeholder-art idea as ui-shots.mjs, kept
 *  intentionally smaller (one composition) — this is one grid among
 *  several screenshots, not the catalogue's main showcase. */
function hashStr(str) {
  let h = 0;
  for (let i = 0; i < str.length; i++) h = (h * 31 + str.charCodeAt(i)) >>> 0;
  return h;
}

function thumbFor(title) {
  const seed = hashStr(title);
  const h1 = seed % 360;
  const h2 = (h1 + 130 + ((seed >>> 4) % 100)) % 360;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180">
    <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="hsl(${h1} 65% 46%)"/>
      <stop offset="100%" stop-color="hsl(${h2} 55% 22%)"/>
    </linearGradient></defs>
    <rect width="320" height="180" fill="url(#g)"/>
  </svg>`;
  return "data:image/svg+xml;base64," + Buffer.from(svg).toString("base64");
}

const CATALOG = [
  ["A relay on a solar panel, six months in", "quiet.works", "video", 6 * 60_000 + 58_000, hours(4)],
  ["Field recording: the coast road, 4am", "mor.audio", "audio", 26 * 60_000, hours(9)],
  ["Reading the spec: identity rotation", "ana@substrate.dev", "video", 33 * 60_000 + 20_000, hours(20)],
  ["Voice notes from the last outage", "mor.audio", "audio", 4 * 60_000 + 58_000, hours(31)],
  ["Why we stopped running our own CDN", "kestrel.tv", "video", 17 * 60_000 + 4_000, hours(50)],
  ["Nine variations on a chunk tree", "mor.audio", "audio", 9 * 60_000 + 12_000, hours(70)],
].map(([title, name, mediaKind, durationMs, createdAt], i) => ({
  id: "vm1node" + String(i).padStart(3, "0") + "9f3ab2c7d1e",
  title,
  author: { identityId: "vmid" + i, name },
  thumbnailUrl: thumbFor(title),
  mediaKind,
  coverArtUrl: mediaKind === "audio" ? thumbFor(title) : undefined,
  durationMs,
  createdAt,
}));

const GATEWAY = "https://demo.gateway";

/** The handful of `invoke()` commands `Browse.tsx` (and the header's
 *  gateway picker) exercise. Every other command a route might call
 *  (`node_status`, pin/library commands, …) resolves to an inert default
 *  so a stray call elsewhere never rejects and shows an error state. */
function invokeHandlerSource() {
  return `
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.convertFileSrc = (path) => path;
    window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
      switch (cmd) {
        case "fetch_catalog":
          return { items: window.__EVERMESH_CATALOG__, next: null };
        case "node_status":
          return { version: "0.1.0", pinnedCount: 2, seeding: false, dbPath: "" };
        case "get_budgets":
          return { diskGb: 20, bandwidthMbps: 50 };
        case "list_pins":
        case "list_library":
          return [];
        case "is_blob_pinned":
          return false;
        case "local_media_path":
        case "get_cached_entry":
          return null;
        case "validate_gateway_url":
          return (args && args.gatewayUrl) || "";
        default:
          return null;
      }
    };
  `;
}

const server = http
  .createServer((req, res) => {
    const url = req.url.split("?")[0];
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

  // Seed the gateway allow-list before any app script runs, and hand the
  // canned catalog to the invoke stub the same way.
  await page.addInitScript(
    ({ gateway, catalog }) => {
      localStorage.setItem("evermesh-node:gateways", JSON.stringify([gateway]));
      localStorage.setItem("evermesh-node:current-gateway", gateway);
      window.__EVERMESH_CATALOG__ = catalog;
    },
    { gateway: GATEWAY, catalog: CATALOG },
  );
  await page.addInitScript(invokeHandlerSource());

  await page.goto(base + "/", { waitUntil: "networkidle" });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForSelector("h1");
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(shots, `ui-node-${scheme}.png`) });
  console.log(`wrote apps/site/screenshots/ui-node-${scheme}.png`);
  await page.close();
}

await browser.close();
server.close();
