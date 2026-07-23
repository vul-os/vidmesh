import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { openDb, type Db } from "../src/db.ts";
import { PolicyEngine } from "../src/policy.ts";
import { StubMatcher } from "../src/csam.ts";
import { processRecord, type IngestDeps } from "../src/ingest.ts";
import { kernelTest } from "./kernel-available.ts";

function permissivePolicy(db: Db): PolicyEngine {
  const dir = mkdtempSync(join(tmpdir(), "evermesh-ingest-"));
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

function deps(): IngestDeps {
  const db = openDb(":memory:");
  return { db, policy: permissivePolicy(db), csam: new StubMatcher(), blobDir: mkdtempSync(join(tmpdir(), "evermesh-blobs-")) };
}

function fakeBlobHex(): string {
  return `hex:${"ab".repeat(32)}`;
}

await kernelTest("indexes a manifest into the videos table", async () => {
  const { Keypair, identity, createRecord } = await import("@evermesh/kernel");
  const d = deps();
  const kp = await Keypair.generate();
  const { identityId } = await identity.genesis(kp);

  const manifest = await createRecord(kp, identityId, {
    kind: 16,
    refs: [],
    body: {
      title: "Test Video",
      original: { blob: fakeBlobHex(), size: 100, codec: "av01.0.08M.08", duration: 1000, width: 100, height: 100 },
      license: "CC-BY-4.0",
    },
  });

  const result = await processRecord(d, manifest);
  assert.equal(result.stored, true);
  const row = d.db.prepare("SELECT title, license FROM videos WHERE manifest_id = ?").get(result.recordId) as
    | { title: string; license: string }
    | undefined;
  assert.equal(row?.title, "Test Video");
  assert.equal(row?.license, "CC-BY-4.0");
});

await kernelTest("comment threading: valid reply indexes, cross-subject reply is rejected", async () => {
  const { Keypair, identity, createRecord } = await import("@evermesh/kernel");
  const d = deps();
  const kp = await Keypair.generate();
  const { identityId } = await identity.genesis(kp);

  const manifestA = await createRecord(kp, identityId, {
    kind: 16,
    body: { title: "A", original: { blob: fakeBlobHex(), size: 1, codec: "x", duration: 1, width: 1, height: 1 }, license: "CC0-1.0" },
  });
  const manifestAId = (await processRecord(d, manifestA)).recordId!;
  const manifestB = await createRecord(kp, identityId, {
    kind: 16,
    body: { title: "B", original: { blob: fakeBlobHex(), size: 1, codec: "x", duration: 1, width: 1, height: 1 }, license: "CC0-1.0" },
  });
  const manifestBId = (await processRecord(d, manifestB)).recordId!;

  const c1 = await createRecord(kp, identityId, { kind: 32, refs: [{ type: 0, hash: manifestAId }], body: { text: "first" } });
  const c1Id = (await processRecord(d, c1)).recordId!;

  const mismatchedReply = await createRecord(kp, identityId, {
    kind: 32,
    refs: [{ type: 0, hash: manifestBId }, { type: 0, hash: c1Id }],
    body: { text: "wrong subject" },
  });
  const mismatchResult = await processRecord(d, mismatchedReply);
  assert.equal(mismatchResult.stored, false);
  assert.equal(mismatchResult.reason, "comment/parent-subject-mismatch");

  const validReply = await createRecord(kp, identityId, {
    kind: 32,
    refs: [{ type: 0, hash: manifestAId }, { type: 0, hash: c1Id }],
    body: { text: "agreed" },
  });
  const validResult = await processRecord(d, validReply);
  assert.equal(validResult.stored, true);
  const row = d.db.prepare("SELECT parent FROM comments WHERE record_id = ?").get(validResult.recordId) as { parent: string };
  assert.equal(row.parent, c1Id);
});

await kernelTest("a later reaction supersedes an earlier one for counting", async () => {
  const { Keypair, identity, createRecord } = await import("@evermesh/kernel");
  const d = deps();
  const kp = await Keypair.generate();
  const { identityId } = await identity.genesis(kp);
  const target = "cc".repeat(32);

  const first = await createRecord(kp, identityId, { kind: 33, refs: [{ type: 0, hash: target }], body: { reaction: "👍" } });
  await processRecord(d, first);
  const second = await createRecord(kp, identityId, { kind: 33, refs: [{ type: 0, hash: target }], body: { reaction: "🔥" } });
  await processRecord(d, second);

  const row = d.db.prepare("SELECT reaction FROM reactions WHERE target_id = ? AND author = ?").get(target, identityId) as {
    reaction: string;
  };
  assert.equal(row.reaction, "🔥");
  const count = d.db.prepare("SELECT COUNT(*) AS n FROM reactions WHERE target_id = ?").get(target) as { n: number };
  assert.equal(count.n, 1); // one row per (target, author): the earlier reaction doesn't linger
});

await kernelTest("follow then unfollow via retract", async () => {
  const { Keypair, identity, createRecord } = await import("@evermesh/kernel");
  const d = deps();
  const kp = await Keypair.generate();
  const { identityId } = await identity.genesis(kp);
  const followedIdentity = "dd".repeat(32);

  const follow = await createRecord(kp, identityId, { kind: 34, refs: [{ type: 0, hash: followedIdentity }], body: {} });
  const followResult = await processRecord(d, follow);
  assert.equal(followResult.stored, true);
  assert.equal(
    (d.db.prepare("SELECT retracted FROM follows WHERE record_id = ?").get(followResult.recordId) as { retracted: number })
      .retracted,
    0,
  );

  const retract = await createRecord(kp, identityId, {
    kind: 18,
    refs: [{ type: 0, hash: followResult.recordId! }],
    body: { reason: "unfollow" },
  });
  const retractResult = await processRecord(d, retract);
  assert.equal(retractResult.stored, true);
  assert.equal(
    (d.db.prepare("SELECT retracted FROM follows WHERE record_id = ?").get(followResult.recordId) as { retracted: number })
      .retracted,
    1,
  );
});

await kernelTest("supersede replaces the body but keeps the original id as the key", async () => {
  const { Keypair, identity, createRecord } = await import("@evermesh/kernel");
  const d = deps();
  const kp = await Keypair.generate();
  const { identityId } = await identity.genesis(kp);
  const subject = "ee".repeat(32);

  const comment = await createRecord(kp, identityId, { kind: 32, refs: [{ type: 0, hash: subject }], body: { text: "typo" } });
  const commentId = (await processRecord(d, comment)).recordId!;

  const supersede = await createRecord(kp, identityId, {
    kind: 17,
    refs: [{ type: 0, hash: commentId }],
    body: { target_kind: 32, body: { text: "fixed" } },
  });
  const supersedeResult = await processRecord(d, supersede);
  assert.equal(supersedeResult.stored, true);

  const row = d.db.prepare("SELECT record_id, text FROM comments WHERE record_id = ?").get(commentId) as {
    record_id: string;
    text: string;
  };
  assert.equal(row.record_id, commentId); // stable reference for refs
  assert.equal(row.text, "fixed");
});

await kernelTest("retract by a different author is rejected", async () => {
  const { Keypair, identity, createRecord } = await import("@evermesh/kernel");
  const d = deps();
  const authorKp = await Keypair.generate();
  const { identityId: authorId } = await identity.genesis(authorKp);
  const attackerKp = await Keypair.generate();
  const { identityId: attackerId } = await identity.genesis(attackerKp);
  const subject = "ff".repeat(32);

  const comment = await createRecord(authorKp, authorId, { kind: 32, refs: [{ type: 0, hash: subject }], body: { text: "mine" } });
  const commentId = (await processRecord(d, comment)).recordId!;

  const forgedRetract = await createRecord(attackerKp, attackerId, {
    kind: 18,
    refs: [{ type: 0, hash: commentId }],
    body: {},
  });
  const result = await processRecord(d, forgedRetract);
  assert.equal(result.stored, false);
  assert.equal(result.reason, "author_mismatch");
  assert.equal(
    (d.db.prepare("SELECT retracted FROM comments WHERE record_id = ?").get(commentId) as { retracted: number }).retracted,
    0,
  );
});

await kernelTest("a record is only ever indexed once (dedup by id)", async () => {
  const { Keypair, identity, createRecord } = await import("@evermesh/kernel");
  const d = deps();
  const kp = await Keypair.generate();
  const { identityId } = await identity.genesis(kp);
  const record = await createRecord(kp, identityId, { kind: 34, refs: [{ type: 0, hash: "aa".repeat(32) }], body: {} });

  const first = await processRecord(d, record);
  const second = await processRecord(d, record);
  assert.equal(first.stored, true);
  assert.equal(second.stored, true);
  const count = d.db.prepare("SELECT COUNT(*) AS n FROM records WHERE id = ?").get(first.recordId) as { n: number };
  assert.equal(count.n, 1);
});
