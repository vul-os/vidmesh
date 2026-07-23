import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openDb } from "../src/db.ts";
import { PolicyEngine, type PolicyFile } from "../src/policy.ts";

function writePolicyFile(policy: Partial<PolicyFile>): string {
  const dir = mkdtempSync(join(tmpdir(), "evermesh-policy-"));
  const path = join(dir, "policy.json");
  writeFileSync(
    path,
    JSON.stringify({
      name: "test gateway",
      description: "",
      moderationPolicyHtml: "<p>ok</p>",
      denyIdentities: [],
      denyBlobHashes: [],
      denyRecordIds: [],
      denyKinds: [],
      geoBlocks: [],
      feeds: [],
      ...policy,
    }),
  );
  return path;
}

test("allows records with no matching deny rule", () => {
  const db = openDb(":memory:");
  const engine = new PolicyEngine(db, writePolicyFile({}));
  const decision = engine.checkRecord(32, "author1", [{ type: 0, hash: "target1" }]);
  assert.equal(decision.allowed, true);
});

test("denies by kind, identity, and hash — each is an indexed/O(1) lookup", () => {
  const db = openDb(":memory:");
  const path = writePolicyFile({ denyKinds: [66], denyIdentities: ["baduser"], denyBlobHashes: ["deadbeef"] });
  const engine = new PolicyEngine(db, path);

  assert.equal(engine.checkRecord(66, "anyone", []).allowed, false);
  assert.equal(engine.checkRecord(32, "baduser", []).allowed, false);
  assert.equal(engine.checkRecord(16, "ok", [{ type: 1, hash: "deadbeef" }]).allowed, false);
  assert.equal(engine.checkRecord(16, "ok", [{ type: 1, hash: "safe-hash" }]).allowed, true);
});

test("every denial is logged with a timestamp and reason", () => {
  const db = openDb(":memory:");
  const engine = new PolicyEngine(db, writePolicyFile({ denyKinds: [66] }));
  engine.checkRecord(66, "someone", []);
  const rows = db.prepare("SELECT * FROM policy_log").all() as { action: string; reason: string; ts: number }[];
  assert.equal(rows.length, 1);
  assert.equal(rows[0].action, "deny");
  assert.ok(rows[0].reason.length > 0);
  assert.ok(rows[0].ts > 0);
});

test("reload() picks up an edited policy file (SIGHUP hot-reload)", () => {
  const db = openDb(":memory:");
  const path = writePolicyFile({ denyIdentities: [] });
  const engine = new PolicyEngine(db, path);
  assert.equal(engine.checkRecord(32, "someone", []).allowed, true);

  writeFileSync(
    path,
    JSON.stringify({
      name: "test gateway",
      description: "",
      moderationPolicyHtml: "<p>ok</p>",
      denyIdentities: ["someone"],
      denyBlobHashes: [],
      denyRecordIds: [],
      denyKinds: [],
      geoBlocks: [],
      feeds: [],
    }),
  );
  engine.reload();
  assert.equal(engine.checkRecord(32, "someone", []).allowed, false);
});

test("feed.takedown batch de-indexes only for a subscribed (feed, publisher) pair", () => {
  const db = openDb(":memory:");
  const engine = new PolicyEngine(db, writePolicyFile({ feeds: [{ feed: "acme/us", publisher: "acme-id" }] }));

  // Not subscribed: ignored.
  engine.applyFeedBatch("rec1", "other-publisher", {
    feed: "acme/us",
    seq: 1,
    add: [[1, "hash1", "copyright"]],
    remove: [],
  });
  assert.equal(engine.checkBlobHash("hash1").allowed, true);

  // Subscribed: applied.
  engine.applyFeedBatch("rec2", "acme-id", {
    feed: "acme/us",
    seq: 1,
    add: [[1, "hash1", "copyright"]],
    remove: [],
  });
  assert.equal(engine.checkBlobHash("hash1").allowed, false);

  // Idempotent replay of the same (feed, seq) batch.
  engine.applyFeedBatch("rec2-dup", "acme-id", {
    feed: "acme/us",
    seq: 1,
    add: [[1, "hash1", "copyright"]],
    remove: [],
  });
  const batches = db.prepare("SELECT COUNT(*) AS n FROM feed_batches WHERE feed = ? AND seq = ?").get("acme/us", 1) as {
    n: number;
  };
  assert.equal(batches.n, 1);

  // A later batch removing the entry reinstates it.
  engine.applyFeedBatch("rec3", "acme-id", {
    feed: "acme/us",
    seq: 2,
    add: [],
    remove: [[1, "hash1"]],
  });
  assert.equal(engine.checkBlobHash("hash1").allowed, true);
  assert.equal(engine.lastSeqFor("acme/us"), 2);
});

test("stats() reflects live counts", () => {
  const db = openDb(":memory:");
  const engine = new PolicyEngine(db, writePolicyFile({}));
  const before = engine.stats();
  db.prepare(
    `INSERT INTO videos (manifest_id, author, title, description, tags_json, language, duration_ms, thumbnail_blob,
       channel_id, license, created_at, received_at, body_json, retracted)
     VALUES ('m1','a1','t','','[]',null,0,null,null,'CC-BY-4.0',0,0,'{}',0)`,
  ).run();
  const after = engine.stats();
  assert.equal(after.videos, before.videos + 1);
});
