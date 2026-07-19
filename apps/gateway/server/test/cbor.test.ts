import { test } from "node:test";
import assert from "node:assert/strict";
import { encodeCbor, decodeCbor, decodeCborPrefix } from "../src/cbor.ts";

function hex(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString("hex");
}

test("canonical uint encoding matches RFC 8949 appendix A", () => {
  assert.equal(hex(encodeCbor(0n)), "00");
  assert.equal(hex(encodeCbor(1n)), "01");
  assert.equal(hex(encodeCbor(10n)), "0a");
  assert.equal(hex(encodeCbor(23n)), "17");
  assert.equal(hex(encodeCbor(24n)), "1818");
  assert.equal(hex(encodeCbor(25n)), "1819");
  assert.equal(hex(encodeCbor(100n)), "1864");
  assert.equal(hex(encodeCbor(1000n)), "1903e8");
  assert.equal(hex(encodeCbor(1000000n)), "1a000f4240");
  assert.equal(hex(encodeCbor(1000000000000n)), "1b000000e8d4a51000");
});

test("text and byte strings", () => {
  assert.equal(hex(encodeCbor("IETF")), "6449455446");
  assert.equal(hex(encodeCbor("")), "60");
  assert.equal(hex(encodeCbor(new Uint8Array([1, 2, 3, 4]))), "4401020304");
  assert.equal(hex(encodeCbor(new Uint8Array())), "40");
});

test("arrays", () => {
  assert.equal(hex(encodeCbor([])), "80");
  assert.equal(hex(encodeCbor([1n, 2n, 3n])), "83010203");
  assert.equal(
    hex(encodeCbor([1n, [2n, 3n], [4n, 5n]])),
    "8301820203820405",
  );
});

test("bool and null", () => {
  assert.equal(hex(encodeCbor(false)), "f4");
  assert.equal(hex(encodeCbor(true)), "f5");
  assert.equal(hex(encodeCbor(null)), "f6");
});

test("map keys are sorted by bytewise-lexicographic order of their encoding", () => {
  // "b" (1-byte string, head 0x61) sorts before "aa" (2-byte string, head
  // 0x62) because the encoded length is part of the head byte — this is
  // NOT plain alphabetic string order (spec 001 §2 rule 3).
  const map = new Map<string, bigint>([
    ["aa", 1n],
    ["b", 2n],
  ]);
  const encoded = encodeCbor(map);
  assert.equal(hex(encoded), "a2" + "6162" + "02" + "626161" + "01");
});

test("round-trip through decode for every supported shape", () => {
  const map = new Map<string, unknown>([
    ["kinds", [16n, 32n]],
    ["since", 42n],
  ]) as Map<string, import("../src/cbor.ts").CborValue>;
  const value: import("../src/cbor.ts").CborValue = [
    "REQ",
    "sub-1",
    map,
    new Uint8Array([0xde, 0xad, 0xbe, 0xef]),
    true,
    null,
  ];
  const encoded = encodeCbor(value);
  const decoded = decodeCbor(encoded);
  assert.deepEqual(decoded, value);
});

test("rejects trailing bytes at the top level", () => {
  const encoded = encodeCbor(1n);
  const withTrailing = new Uint8Array([...encoded, 0x00]);
  assert.throws(() => decodeCbor(withTrailing));
  const prefix = decodeCborPrefix(withTrailing);
  assert.equal(prefix.value, 1n);
  assert.equal(prefix.length, 1);
});

test("rejects indefinite length and tags (non-canonical)", () => {
  assert.throws(() => decodeCbor(new Uint8Array([0x9f, 0x01, 0xff]))); // indefinite array
  assert.throws(() => decodeCbor(new Uint8Array([0xc0, 0x00]))); // tag 0
});

test("rejects negative integers (out of scope by design)", () => {
  assert.throws(() => decodeCbor(new Uint8Array([0x20]))); // -1
});
