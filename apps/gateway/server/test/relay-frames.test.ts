import { test } from "node:test";
import assert from "node:assert/strict";
import { encodeClientFrame, decodeClientFrame, decodeRelayFrame } from "../src/relay-frames.ts";
import { encodeCbor } from "../src/cbor.ts";

const HASH_A = "aa".repeat(32);
const HASH_B = "bb".repeat(32);

test("REQ frame round-trips with a full filter", () => {
  const frame = encodeClientFrame({
    type: "REQ",
    subId: "sub-1",
    filter: { kinds: [16, 32], authors: [HASH_A], refs: [HASH_B], ids: [HASH_A], since: 42, limit: 10 },
  });
  const decoded = decodeClientFrame(frame);
  assert.deepEqual(decoded, {
    type: "REQ",
    subId: "sub-1",
    filter: { kinds: [16, 32], authors: [HASH_A], refs: [HASH_B], ids: [HASH_A], since: 42, limit: 10 },
  });
});

test("REQ frame with an empty filter matches everything", () => {
  const frame = encodeClientFrame({ type: "REQ", subId: "s", filter: {} });
  const decoded = decodeClientFrame(frame);
  assert.deepEqual(decoded, { type: "REQ", subId: "s", filter: {} });
});

test("CLOSE frame round-trips", () => {
  const frame = encodeClientFrame({ type: "CLOSE", subId: "sub-1" });
  assert.deepEqual(decodeClientFrame(frame), { type: "CLOSE", subId: "sub-1" });
});

test("PUB frame round-trips with and without a PoW nonce", () => {
  const record = new Uint8Array([1, 2, 3]);
  const withNonce = encodeClientFrame({ type: "PUB", record, nonce: 7n });
  assert.deepEqual(decodeClientFrame(withNonce), { type: "PUB", record, nonce: 7n });

  const withoutNonce = encodeClientFrame({ type: "PUB", record, nonce: null });
  assert.deepEqual(decodeClientFrame(withoutNonce), { type: "PUB", record, nonce: null });
});

test("REC frame decodes", () => {
  const record = new Uint8Array([9, 9, 9]);
  const bytes = encodeCbor(["REC", "sub-1", 5n, record]);
  assert.deepEqual(decodeRelayFrame(bytes), { type: "REC", subId: "sub-1", seq: 5n, record });
});

test("EOSE frame decodes", () => {
  const bytes = encodeCbor(["EOSE", "sub-1"]);
  assert.deepEqual(decodeRelayFrame(bytes), { type: "EOSE", subId: "sub-1" });
});

test("OK frame decodes accepted and rejected", () => {
  const id = new Uint8Array(32).fill(0xab);
  const accepted = encodeCbor(["OK", id, true, ""]);
  assert.deepEqual(decodeRelayFrame(accepted), { type: "OK", id, accepted: true, reason: "" });

  const rejected = encodeCbor(["OK", id, false, "pow"]);
  assert.deepEqual(decodeRelayFrame(rejected), { type: "OK", id, accepted: false, reason: "pow" });
});

test("CLOSED frame decodes", () => {
  const bytes = encodeCbor(["CLOSED", "sub-1", "over-broad filter"]);
  assert.deepEqual(decodeRelayFrame(bytes), { type: "CLOSED", subId: "sub-1", reason: "over-broad filter" });
});

test("unknown frame tags are rejected, not silently coerced", () => {
  const bytes = encodeCbor(["NOPE", "x"]);
  assert.throws(() => decodeRelayFrame(bytes));
});
