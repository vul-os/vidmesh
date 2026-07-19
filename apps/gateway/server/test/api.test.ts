import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { FastifyInstance } from "fastify";
import { openDb, type Db } from "../src/db.ts";
import { parseConfig, type Config } from "../src/config.ts";
import { PolicyEngine } from "../src/policy.ts";
import { StubMatcher } from "../src/csam.ts";
import { CustodyService } from "../src/custody.ts";
import { GatewayIdentityService } from "../src/gateway-identity.ts";
import { RelayManager } from "../src/relay.ts";
import { processRecord, type IngestDeps } from "../src/ingest.ts";
import { buildServer } from "../src/server.ts";
import type { AppDeps } from "../src/app-deps.ts";
import { kernelTest } from "./kernel-available.ts";

function writePolicyFile(dir: string): string {
  const path = join(dir, "policy.json");
  writeFileSync(
    path,
    JSON.stringify({
      name: "test gateway",
      description: "a test gateway",
      moderationPolicyHtml: "<p>test</p>",
      denyIdentities: [],
      denyBlobHashes: [],
      denyRecordIds: [],
      denyKinds: [],
      geoBlocks: [],
      feeds: [],
    }),
  );
  return path;
}

/**
 * Builds a full app the same way main.ts does, but with an empty relay
 * list (RelayManager becomes a pure no-op — this is the "relay mocked"
 * requirement) and no ffmpegPath (the "no ffmpeg" requirement).
 */
async function buildTestApp(): Promise<{ app: FastifyInstance; db: Db; config: Config }> {
  const dir = mkdtempSync(join(tmpdir(), "vidmesh-api-test-"));
  const config = parseConfig({
    dbPath: ":memory:",
    blobDir: join(dir, "blobs"),
    policyFilePath: writePolicyFile(dir),
    sessionSecret: "s".repeat(32),
    custody: { secret: "c".repeat(32) },
    relays: [],
  });
  const db = openDb(config.dbPath);
  const policy = new PolicyEngine(db, config.policyFilePath);
  const csam = new StubMatcher();
  const ingest: IngestDeps = { db, policy, csam, blobDir: config.blobDir, log: () => {} };
  const relays = new RelayManager(db, [], async (record) => {
    await processRecord(ingest, record);
  });
  const custody = new CustodyService(db, config, (record) => relays.publish(record), ingest);
  const gatewayIdentity = new GatewayIdentityService(db, config, (record) => relays.publish(record), ingest);
  const deps: AppDeps = { db, config, policy, csam, custody, gatewayIdentity, relays, ingest, log: () => {} };
  const app = await buildServer(config, deps);
  return { app, db, config };
}

function extractSessionCookie(setCookieHeader: string | string[] | undefined): string {
  const header = Array.isArray(setCookieHeader) ? setCookieHeader[0] : setCookieHeader;
  if (!header) throw new Error("no set-cookie header");
  return header.split(";")[0];
}

await kernelTest("GET /api/info and /api/policy work with no auth", async () => {
  const { app } = await buildTestApp();
  const info = await app.inject({ method: "GET", url: "/api/info" });
  assert.equal(info.statusCode, 200);
  assert.equal(info.json().uploadEnabled, true);

  const policyRes = await app.inject({ method: "GET", url: "/api/policy" });
  assert.equal(policyRes.statusCode, 200);
  assert.equal(policyRes.json().name, "test gateway");
});

await kernelTest("register -> login -> me -> logout flow", async () => {
  const { app } = await buildTestApp();

  const register = await app.inject({
    method: "POST",
    url: "/api/auth/register",
    payload: { handle: "alice", password: "hunter2hunter2" },
  });
  assert.equal(register.statusCode, 201);
  const cookie = extractSessionCookie(register.headers["set-cookie"]);

  const me = await app.inject({ method: "GET", url: "/api/me", headers: { cookie } });
  assert.equal(me.statusCode, 200);
  assert.equal(me.json().handle, "alice");
  assert.equal(me.json().exportAvailable, true);

  const meNoAuth = await app.inject({ method: "GET", url: "/api/me" });
  assert.equal(meNoAuth.statusCode, 401);
  assert.equal(meNoAuth.json().error.code, "unauthorized");

  const logout = await app.inject({ method: "POST", url: "/api/auth/logout", headers: { cookie } });
  assert.equal(logout.statusCode, 200);
  const meAfterLogout = await app.inject({ method: "GET", url: "/api/me", headers: { cookie } });
  assert.equal(meAfterLogout.statusCode, 401);
});

await kernelTest("duplicate registration returns a conflict error envelope", async () => {
  const { app } = await buildTestApp();
  await app.inject({ method: "POST", url: "/api/auth/register", payload: { handle: "bob", password: "hunter2hunter2" } });
  const second = await app.inject({ method: "POST", url: "/api/auth/register", payload: { handle: "bob", password: "hunter2hunter2" } });
  assert.equal(second.statusCode, 409);
  assert.equal(second.json().error.code, "conflict");
});

await kernelTest("comment flow: post a comment and read it back threaded", async () => {
  const { app, db } = await buildTestApp();
  const register = await app.inject({
    method: "POST",
    url: "/api/auth/register",
    payload: { handle: "carol", password: "hunter2hunter2" },
  });
  const cookie = extractSessionCookie(register.headers["set-cookie"]);

  // Insert a minimal video + backing record directly (isolates the
  // comment endpoint from the full upload pipeline).
  const manifestId = "aa".repeat(32);
  db.prepare(
    "INSERT INTO records (id, kind, author, created_at, received_at, cbor, json) VALUES (?, 16, ?, 0, 0, X'00', '{}')",
  ).run(manifestId, "bb".repeat(32));
  db.prepare(
    `INSERT INTO videos (manifest_id, author, title, description, tags_json, language, duration_ms, thumbnail_blob,
       channel_id, license, created_at, received_at, body_json, retracted)
     VALUES (?, ?, 'A Video', '', '[]', null, 0, null, null, 'CC0-1.0', 0, 0, '{}', 0)`,
  ).run(manifestId, "bb".repeat(32));

  const post = await app.inject({
    method: "POST",
    url: `/api/videos/${manifestId}/comments`,
    headers: { cookie },
    payload: { text: "great video" },
  });
  assert.equal(post.statusCode, 200);
  assert.equal(post.json().text, "great video");

  const list = await app.inject({ method: "GET", url: `/api/videos/${manifestId}/comments` });
  assert.equal(list.statusCode, 200);
  assert.equal(list.json().items.length, 1);
  assert.equal(list.json().items[0].text, "great video");
});

await kernelTest("export requires a valid session and re-confirms the password", async () => {
  const { app } = await buildTestApp();
  const register = await app.inject({
    method: "POST",
    url: "/api/auth/register",
    payload: { handle: "dave", password: "correct-horse-battery" },
  });
  const cookie = extractSessionCookie(register.headers["set-cookie"]);

  const wrongPassword = await app.inject({
    method: "POST",
    url: "/api/me/export",
    headers: { cookie },
    payload: { password: "wrong" },
  });
  assert.equal(wrongPassword.statusCode, 401);

  const ok = await app.inject({
    method: "POST",
    url: "/api/me/export",
    headers: { cookie },
    payload: { password: "correct-horse-battery" },
  });
  assert.equal(ok.statusCode, 200);
  assert.ok(ok.json().identity.identityId);
  assert.equal(ok.json().secretKeys.length, 1);
});

function buildMultipart(fields: Record<string, string>, file: Buffer): { body: Buffer; contentType: string } {
  const boundary = `----vidmeshtest${Math.random().toString(16).slice(2)}`;
  const parts: Buffer[] = [];
  for (const [k, v] of Object.entries(fields)) {
    parts.push(Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="${k}"\r\n\r\n${v}\r\n`));
  }
  parts.push(
    Buffer.from(`--${boundary}\r\nContent-Disposition: form-data; name="file"; filename="video.bin"\r\nContent-Type: application/octet-stream\r\n\r\n`),
  );
  parts.push(file);
  parts.push(Buffer.from(`\r\n--${boundary}--\r\n`));
  return { body: Buffer.concat(parts), contentType: `multipart/form-data; boundary=${boundary}` };
}

await kernelTest("upload without ffmpeg degrades to original-only and still publishes", async () => {
  const { app } = await buildTestApp();
  const register = await app.inject({
    method: "POST",
    url: "/api/auth/register",
    payload: { handle: "erin", password: "hunter2hunter2" },
  });
  const cookie = extractSessionCookie(register.headers["set-cookie"]);

  const { body, contentType } = buildMultipart(
    { title: "My Video", license: "CC0-1.0" },
    Buffer.from("not really a video, just bytes"),
  );
  const upload = await app.inject({
    method: "POST",
    url: "/api/upload",
    headers: { cookie, "content-type": contentType },
    payload: body,
  });
  assert.equal(upload.statusCode, 200);
  const { uploadId } = upload.json();
  assert.ok(uploadId);

  let status: { status: string; manifestId?: string; error?: string } = { status: "processing" };
  for (let i = 0; i < 200 && status.status === "processing"; i++) {
    await new Promise((resolve) => setTimeout(resolve, 20));
    const poll = await app.inject({ method: "GET", url: `/api/upload/${uploadId}`, headers: { cookie } });
    status = poll.json();
  }
  assert.equal(status.status, "published", JSON.stringify(status));
  assert.ok(status.manifestId);

  const video = await app.inject({ method: "GET", url: `/api/videos/${status.manifestId}` });
  assert.equal(video.statusCode, 200);
  assert.equal(video.json().title, "My Video");
  assert.equal(video.json().playback.hlsUrl, null); // no ffmpeg -> no HLS renditions
  assert.ok(video.json().playback.mp4Url); // original is always servable
});
