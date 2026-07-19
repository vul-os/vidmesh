import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openDb, type Db } from "../src/db.ts";
import { CustodyService } from "../src/custody.ts";
import { PolicyEngine } from "../src/policy.ts";
import { StubMatcher } from "../src/csam.ts";
import type { IngestDeps } from "../src/ingest.ts";
import { parseConfig, type Config } from "../src/config.ts";
import { kernelTest } from "./kernel-available.ts";

function testConfig(overrides: Partial<Config> = {}): Config {
  return parseConfig({
    dbPath: ":memory:",
    blobDir: "/tmp/vidmesh-test-blobs",
    policyFilePath: "/dev/null",
    sessionSecret: "a".repeat(32),
    custody: { secret: "b".repeat(32), contestWindowSeconds: 604_800 },
    ...overrides,
  });
}

function permissivePolicy(db: Db): PolicyEngine {
  const dir = mkdtempSync(join(tmpdir(), "vidmesh-custody-test-"));
  const path = join(dir, "policy.json");
  writeFileSync(
    path,
    JSON.stringify({
      name: "test",
      description: "",
      moderationPolicyHtml: "",
      denyIdentities: [],
      denyBlobHashes: [],
      denyRecordIds: [],
      denyKinds: [],
      geoBlocks: [],
      feeds: [],
    }),
  );
  return new PolicyEngine(db, path);
}

/** Builds a CustodyService wired to a real (permissive) ingest pipeline. */
function buildCustody(publish: (record: Uint8Array) => void = () => {}): { custody: CustodyService; db: Db; ingest: IngestDeps } {
  const db = openDb(":memory:");
  const ingest: IngestDeps = { db, policy: permissivePolicy(db), csam: new StubMatcher(), blobDir: "/tmp" };
  const custody = new CustodyService(db, testConfig(), publish, ingest);
  return { custody, db, ingest };
}

await kernelTest("register creates an account, indexes + publishes genesis, and encrypts the secret at rest", async () => {
  const published: Uint8Array[] = [];
  const { custody, db } = buildCustody((record) => void published.push(record));

  const { userId, identityId } = await custody.register("alice", "hunter2hunter2");
  assert.ok(userId > 0);
  assert.equal(identityId.length, 64);
  assert.equal(published.length, 1);

  const indexed = db.prepare("SELECT kind FROM records WHERE id = ?").get(identityId) as { kind: number } | undefined;
  assert.equal(indexed?.kind, 1); // genesis (rotation) is indexed locally, not just published out

  const user = custody.getUserByHandle("alice");
  assert.ok(user);
  assert.equal(user!.identity_id, identityId);
  assert.notEqual(user!.secret_key_enc, "");
  assert.ok(!user!.secret_key_enc.includes(identityId)); // ciphertext shouldn't leak the identity id as substring
});

await kernelTest("duplicate handle registration is rejected", async () => {
  const { custody } = buildCustody();
  await custody.register("bob", "hunter2hunter2");
  await assert.rejects(() => custody.register("bob", "different-password"));
});

await kernelTest("signRecord signs as the custodied identity and verifies", async () => {
  const { verifyRecord } = await import("@vidmesh/kernel");
  const { custody } = buildCustody();
  const { userId } = await custody.register("carol", "hunter2hunter2");

  const record = await custody.signRecord(userId, { kind: 34, refs: [{ type: 0, hash: "aa".repeat(32) }], body: {} });
  await assert.doesNotReject(() => verifyRecord(record));
});

await kernelTest("export requires the correct password and returns the genesis + secret key", async () => {
  const { custody } = buildCustody();
  const { userId, identityId } = await custody.register("dave", "correct-horse-battery");

  await assert.rejects(() => custody.exportIdentity(userId, "wrong-password"));

  const exported = await custody.exportIdentity(userId, "correct-horse-battery");
  assert.equal(exported.identity.identityId, identityId);
  assert.equal(exported.secretKeys.length, 1);
  assert.equal(exported.secretKeys[0].secretHex.length, 64);
});

await kernelTest("export is rate-limited", async () => {
  const { custody } = buildCustody();
  const { userId } = await custody.register("erin", "correct-horse-battery");

  for (let i = 0; i < 5; i++) {
    await assert.rejects(() => custody.exportIdentity(userId, "wrong")); // consumes rate-limit budget too
  }
  await assert.rejects(
    () => custody.exportIdentity(userId, "correct-horse-battery"),
    /rate|too many/i,
  );
});
