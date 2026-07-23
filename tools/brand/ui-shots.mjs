#!/usr/bin/env node
/**
 * Screenshots of the uniform reference UI for the README.
 *
 *   pnpm --filter @evermesh/gateway-web build
 *   node tools/brand/ui-shots.mjs
 *
 * Serves the built frontend and answers the gateway REST API with fixed
 * sample responses, because **no evermesh gateway is deployed** — see the
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
  console.error("build the frontend first: pnpm --filter @evermesh/gateway-web build");
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

/*
 * Locally generated, self-contained thumbnail imagery for the stub API.
 *
 * A video-catalogue screenshot with zero imagery always reads as broken,
 * not minimal — every card was the same flat grey tile. These stand in
 * for real video frames: a deterministic hue pair, one of six distinct
 * compositions, and distinct luminance per item, so the grid reads as a
 * catalogue of different things. Plain inline SVG data: URIs — no
 * network fetch, no fonts, no third-party marks, no photos of real
 * people, nothing that could be mistaken for a captured frame of
 * anything real.
 */
function hashStr(str) {
  let h = 0;
  for (let i = 0; i < str.length; i++) h = (h * 31 + str.charCodeAt(i)) >>> 0;
  return h;
}

const SCENES = [
  // horizon: gradient sky, a sun, a hill silhouette
  (h1, h2, seed) => `
    <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="hsl(${h1} 70% 55%)"/>
      <stop offset="100%" stop-color="hsl(${h2} 60% 22%)"/>
    </linearGradient></defs>
    <rect width="320" height="180" fill="url(#g)"/>
    <circle cx="${60 + (seed % 200)}" cy="${50 + (seed % 40)}" r="26" fill="hsl(${h1} 85% 82%)" opacity="0.92"/>
    <path d="M0,140 Q80,${100 + (seed % 30)} 160,135 T320,120 V180 H0 Z" fill="hsl(${h2} 45% 14%)"/>`,
  // waveform: an audio bar-graph
  (h1, h2, seed) => {
    let bars = "";
    for (let i = 0; i < 26; i++) {
      const bh = 18 + ((seed * (i + 3) * 37) % 120);
      bars += `<rect x="${i * 12.4 + 6}" y="${92 - bh / 2}" width="7" height="${bh}" rx="3.5" fill="hsl(${(h1 + i * 5) % 360} 62% 56%)"/>`;
    }
    return `<rect width="320" height="180" fill="hsl(${h2} 35% 12%)"/>${bars}`;
  },
  // node mesh: a small connected graph (a nod to the substrate, not the wordmark)
  (h1, h2, seed) => {
    // Unsigned shift (`>>>`), not `>>`: seed can exceed 2^31, and a signed
    // shift on such a value yields a negative coordinate — every point
    // (and the lines/dots drawn at it) lands off the 0-180 viewBox, so
    // the "mesh" scene silently renders as a flat, empty rectangle. That
    // failure mode is worth naming because it's exactly the bug this
    // generator exists to fix (a card that looks like a broken image).
    const pts = Array.from({ length: 6 }, (_, i) => [20 + ((seed >>> i) % 280), 20 + ((seed >>> (i + 3)) % 140)]);
    const lines = pts.slice(1).map((p, i) => `<line x1="${pts[i][0]}" y1="${pts[i][1]}" x2="${p[0]}" y2="${p[1]}" stroke="hsl(${h1} 50% 58%)" stroke-width="1.5" opacity="0.75"/>`).join("");
    const dots = pts.map(([x, y], i) => `<circle cx="${x}" cy="${y}" r="${i === 0 ? 7 : 4.5}" fill="hsl(${h2} 70% 62%)"/>`).join("");
    return `<rect width="320" height="180" fill="hsl(${h1} 30% 13%)"/>${lines}${dots}`;
  },
  // contour: concentric rings, topographic-map style
  (h1, h2, seed) => {
    let rings = "";
    for (let r = 12; r < 230; r += 24) {
      rings += `<circle cx="${60 + (seed % 200)}" cy="90" r="${r}" fill="none" stroke="hsl(${(h1 + r) % 360} 55% 52%)" stroke-width="3" opacity="0.55"/>`;
    }
    return `<rect width="320" height="180" fill="hsl(${h2} 30% 14%)"/>${rings}`;
  },
  // grid + circuit: a dot lattice with one routed trace
  (h1, h2, seed) => {
    let cells = "";
    for (let x = 10; x < 320; x += 26) {
      for (let y = 10; y < 180; y += 26) {
        if ((x + y + seed) % 43 < 18) cells += `<circle cx="${x}" cy="${y}" r="2.4" fill="hsl(${h1} 70% 62%)"/>`;
      }
    }
    const trace = `<path d="M20,${20 + (seed % 60)} H150 V${100 + (seed % 40)} H300" stroke="hsl(${h2} 60% 56%)" stroke-width="2.5" fill="none" opacity="0.85"/>`;
    return `<rect width="320" height="180" fill="hsl(${h1} 26% 12%)"/>${cells}${trace}`;
  },
  // aurora: soft rotated bands
  (h1, h2, seed) => {
    let bands = "";
    for (let i = 0; i < 5; i++) {
      const y = i * 40 - 20 + (seed % 20);
      bands += `<rect x="-40" y="${y}" width="400" height="34" fill="hsl(${(h1 + i * 28) % 360} 60% 52%)" opacity="0.4" transform="rotate(${-18 + (seed % 10)} 160 90)"/>`;
    }
    return `<rect width="320" height="180" fill="hsl(${h2} 35% 10%)"/>${bands}`;
  },
];

function thumbFor(title, i) {
  const seed = hashStr(title + i);
  const h1 = seed % 360;
  const h2 = (h1 + 130 + ((seed >>> 4) % 100)) % 360;
  const scene = SCENES[i % SCENES.length](h1, h2, seed);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180">${scene}</svg>`;
  return "data:image/svg+xml;base64," + Buffer.from(svg).toString("base64");
}

const VIDEOS = [
  ["Recovering a channel after a gateway shutdown", "kestrel.tv", 11 * 60_000 + 42_000, hours(5)],
  ["Chunk trees, explained with a whiteboard", "ana@substrate.dev", 8 * 60_000 + 15_000, hours(19)],
  ["Field recording: the coast road, 4am", "mor.audio", 26 * 60_000, hours(30)],
  ["Why we stopped running our own CDN", "kestrel.tv", 17 * 60_000 + 4_000, hours(54)],
  ["Reading the spec: identity rotation", "ana@substrate.dev", 33 * 60_000 + 20_000, hours(72)],
  ["A relay on a solar panel, six months in", "quiet.works", 6 * 60_000 + 58_000, hours(96)],
  ["Offline bundles across a border", "mor.audio", 14 * 60_000 + 11_000, hours(120)],
  ["Conformance vectors from scratch", "quiet.works", 21 * 60_000 + 39_000, hours(150)],
  ["A gateway migration, live and mid-stream", "kestrel.tv", 9 * 60_000 + 30_000, hours(170)],
  ["Why blob proofs, not just hashes", "ana@substrate.dev", 12 * 60_000 + 5_000, hours(200)],
  ["Building a node from spare parts", "quiet.works", 19 * 60_000 + 47_000, hours(230)],
  ["Voice notes from the last outage", "mor.audio", 4 * 60_000 + 58_000, hours(260)],
].map(([title, name, durationMs, createdAt], i) => ({
  id: "vm1" + String(i).padStart(3, "0") + "9f3ab2c7d1e",
  title,
  author: { identityId: "vmid" + i, name },
  thumbnailUrl: thumbFor(title, i),
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
