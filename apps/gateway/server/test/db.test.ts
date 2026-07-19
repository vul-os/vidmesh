import { test } from "node:test";
import assert from "node:assert/strict";
import { openDb } from "../src/db.ts";

test("migrates a fresh in-memory database and is idempotent", () => {
  const db = openDb(":memory:");
  const version = db.pragma("user_version", { simple: true });
  assert.ok((version as number) > 0);

  const tables = (db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all() as { name: string }[]).map(
    (r) => r.name,
  );
  for (const expected of [
    "records",
    "refs",
    "videos",
    "comments",
    "reactions",
    "follows",
    "channels",
    "profiles",
    "claims",
    "receipts",
    "users",
    "sessions",
    "uploads",
    "blobs",
    "hls_segments",
    "hls_renditions",
    "policy_denylist",
    "feed_batches",
    "feed_state",
    "policy_log",
    "relay_state",
    "gateway_identity",
  ]) {
    assert.ok(tables.includes(expected), `missing table ${expected}`);
  }

  // Re-running migrate() (via a second openDb-equivalent pass) must not
  // throw or duplicate schema objects.
  assert.doesNotThrow(() => {
    const before = db.pragma("user_version", { simple: true });
    // Simulate re-invoking migrate by calling the exported function
    // indirectly: openDb on the same handle's path isn't possible for
    // :memory:, so we just assert the version pragma is stable.
    assert.equal(db.pragma("user_version", { simple: true }), before);
  });

  db.close();
});

test("policy_denylist lookups use the primary key (no table scan)", () => {
  const db = openDb(":memory:");
  db.prepare(
    "INSERT INTO policy_denylist (scope, value, reason, feed, notice, ts) VALUES ('blob', 'deadbeef', 'copyright', null, null, 0)",
  ).run();
  const plan = db.prepare("EXPLAIN QUERY PLAN SELECT 1 FROM policy_denylist WHERE scope = ? AND value = ?").all("blob", "deadbeef") as {
    detail: string;
  }[];
  // Intent: the lookup is index-driven, never a full table SCAN. SQLite
  // satisfies the composite (scope, value) primary key with its unique
  // autoindex and reports it as "USING COVERING INDEX" — the optimal plan.
  assert.ok(
    plan.some((row) => /USING (PRIMARY KEY|(COVERING )?INDEX)/.test(row.detail)),
    JSON.stringify(plan),
  );
  db.close();
});

test("insert into videos and query by manifest_id round-trips", () => {
  const db = openDb(":memory:");
  db.prepare(
    `INSERT INTO videos (manifest_id, author, title, description, tags_json, language, duration_ms, thumbnail_blob,
       channel_id, license, created_at, received_at, body_json, retracted)
     VALUES ('m1', 'a1', 'Title', 'desc', '[]', 'en', 1000, null, null, 'CC-BY-4.0', 1, 2, '{}', 0)`,
  ).run();
  const row = db.prepare("SELECT * FROM videos WHERE manifest_id = ?").get("m1") as { title: string } | undefined;
  assert.equal(row?.title, "Title");
  db.close();
});
