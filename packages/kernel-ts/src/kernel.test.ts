// Cross-runtime kernel tests: the same operations the Rust unit tests
// perform, exercised through the WASM boundary under Node. Skips (with a
// loud message) when the WASM build is absent — run `pnpm build:wasm`
// first. Per project directive these are written ahead of being run.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";

const wasmBuilt = existsSync(new URL("../wasm/vidmesh_wasm_bg.wasm", import.meta.url));

const t = wasmBuilt
  ? test
  : (name: string, _fn: unknown) =>
      test(`${name} (SKIPPED: run pnpm build:wasm first)`, { skip: true }, () => {});

t("record create → verify → id round-trip", async () => {
  const kernel = await import("./index.ts");
  const kp = await kernel.Keypair.fromSecret(new Uint8Array(32).fill(7));
  const { identityId, record: genesis } = await kernel.identity.genesis(kp, {
    createdAt: 100,
  });
  await kernel.verifyRecord(genesis);

  const comment = await kernel.createRecord(kp, identityId, {
    kind: 32,
    createdAt: 200,
    refs: [{ type: 0, hash: await kernel.deriveId(genesis) }],
    body: { text: "hello from node" },
  });
  await kernel.verifyRecord(comment);
  await kernel.validateKind(comment);

  const json = await kernel.recordToJson(comment);
  const back = await kernel.recordFromJson(json);
  assert.deepEqual(back, comment);
});

t("tampered record fails verification", async () => {
  const kernel = await import("./index.ts");
  const kp = await kernel.Keypair.fromSecret(new Uint8Array(32).fill(8));
  const { identityId } = await kernel.identity.genesis(kp, { createdAt: 100 });
  const record = await kernel.createRecord(kp, identityId, {
    kind: 33,
    createdAt: 300,
    refs: [{ type: 0, hash: "ab".repeat(32) }],
    body: { reaction: "🔥" },
  });
  const tampered = record.slice();
  tampered[tampered.length - 1] ^= 0xff;
  await assert.rejects(kernel.verifyRecord(tampered));
});

t("identity chain: rotation advances state", async () => {
  const kernel = await import("./index.ts");
  const oldKp = await kernel.Keypair.fromSecret(new Uint8Array(32).fill(1));
  const newKp = await kernel.Keypair.fromSecret(new Uint8Array(32).fill(2));
  const { identityId, record: genesis } = await kernel.identity.genesis(oldKp, {
    createdAt: 100,
  });
  const rotation = await kernel.identity.rotate(oldKp, {
    identityId,
    prevRotationId: await kernel.deriveId(genesis),
    newKey: newKp.publicKey,
    createdAt: 200,
  });
  const state = await kernel.identity.verifyChain([genesis, rotation], 1000);
  assert.equal(state.identityId, identityId);
  assert.equal(state.signingKey, kernel.toHex(newKp.publicKey));
  assert.equal(state.depth, 1);
});

t("blob stream hashing matches whole-blob hashing", async () => {
  const kernel = await import("./index.ts");
  const data = new Uint8Array(1_048_576 + 123).fill(9);
  const wholeHash = await kernel.hashBlob(data);
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(data.slice(0, 1000));
      controller.enqueue(data.slice(1000));
      controller.close();
    },
  });
  const summary = await kernel.hashBlobStream(stream);
  assert.equal(summary.id, wholeHash);
  assert.equal(summary.size, data.length);
  assert.equal(summary.nChunks, 2);
  assert.notEqual(summary.chunkRoot, null);
});

t("hex helpers round-trip and reject junk", async () => {
  const kernel = await import("./index.ts");
  const bytes = new Uint8Array([0, 1, 254, 255]);
  assert.deepEqual(kernel.fromHex(kernel.toHex(bytes)), bytes);
  assert.throws(() => kernel.fromHex("zz"));
  assert.throws(() => kernel.fromHex("abc"));
});
