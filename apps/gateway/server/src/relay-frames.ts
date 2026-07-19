/**
 * Typed encode/decode for the spec 006-relay.md §1 websocket frames, built
 * on the minimal codec in cbor.ts. All frames are binary WebSocket messages
 * containing one canonically encoded CBOR array whose first element is a
 * text tag.
 */
import { encodeCbor, decodeCbor, type CborValue } from "./cbor.ts";

export interface Filter {
  kinds?: number[];
  authors?: string[]; // hex identity ids
  refs?: string[]; // hex hashes
  ids?: string[]; // hex record ids
  since?: number;
  limit?: number;
}

export type ClientFrame =
  | { type: "REQ"; subId: string; filter: Filter }
  | { type: "CLOSE"; subId: string }
  | { type: "PUB"; record: Uint8Array; nonce: bigint | null };

export type RelayFrame =
  | { type: "REC"; subId: string; seq: bigint; record: Uint8Array }
  | { type: "EOSE"; subId: string }
  | { type: "OK"; id: Uint8Array; accepted: boolean; reason: string }
  | { type: "CLOSED"; subId: string; reason: string };

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function bytesToHex(bytes: Uint8Array): string {
  let out = "";
  for (const b of bytes) out += b.toString(16).padStart(2, "0");
  return out;
}

function encodeFilter(filter: Filter): Map<string, CborValue> {
  const map = new Map<string, CborValue>();
  if (filter.kinds) map.set("kinds", filter.kinds.map((k) => BigInt(k)));
  if (filter.authors) map.set("authors", filter.authors.map(hexToBytes));
  if (filter.refs) map.set("refs", filter.refs.map(hexToBytes));
  if (filter.ids) map.set("ids", filter.ids.map(hexToBytes));
  if (filter.since !== undefined) map.set("since", BigInt(filter.since));
  if (filter.limit !== undefined) map.set("limit", BigInt(filter.limit));
  return map;
}

function decodeFilter(value: CborValue): Filter {
  if (!(value instanceof Map)) throw new Error("relay: filter must be a map");
  const filter: Filter = {};
  const kinds = value.get("kinds");
  if (kinds !== undefined) {
    if (!Array.isArray(kinds)) throw new Error("relay: filter.kinds must be an array");
    filter.kinds = kinds.map((k) => Number(asUint(k)));
  }
  const authors = value.get("authors");
  if (authors !== undefined) {
    if (!Array.isArray(authors)) throw new Error("relay: filter.authors must be an array");
    filter.authors = authors.map((a) => bytesToHex(asBytes(a)));
  }
  const refs = value.get("refs");
  if (refs !== undefined) {
    if (!Array.isArray(refs)) throw new Error("relay: filter.refs must be an array");
    filter.refs = refs.map((r) => bytesToHex(asBytes(r)));
  }
  const ids = value.get("ids");
  if (ids !== undefined) {
    if (!Array.isArray(ids)) throw new Error("relay: filter.ids must be an array");
    filter.ids = ids.map((i) => bytesToHex(asBytes(i)));
  }
  const since = value.get("since");
  if (since !== undefined) filter.since = Number(asUint(since));
  const limit = value.get("limit");
  if (limit !== undefined) filter.limit = Number(asUint(limit));
  return filter;
}

function asUint(v: CborValue): bigint {
  if (typeof v !== "bigint") throw new Error("relay: expected unsigned integer");
  return v;
}

function asBytes(v: CborValue): Uint8Array {
  if (!(v instanceof Uint8Array)) throw new Error("relay: expected byte string");
  return v;
}

function asText(v: CborValue): string {
  if (typeof v !== "string") throw new Error("relay: expected text string");
  return v;
}

/** Encode a client→relay frame (spec 006 §1). */
export function encodeClientFrame(frame: ClientFrame): Uint8Array {
  switch (frame.type) {
    case "REQ":
      return encodeCbor(["REQ", frame.subId, encodeFilter(frame.filter)]);
    case "CLOSE":
      return encodeCbor(["CLOSE", frame.subId]);
    case "PUB":
      return encodeCbor(["PUB", frame.record, frame.nonce === null ? null : frame.nonce]);
  }
}

/** Decode a relay→client frame (spec 006 §1). Throws on malformed frames. */
export function decodeRelayFrame(bytes: Uint8Array): RelayFrame {
  const value = decodeCbor(bytes);
  if (!Array.isArray(value) || value.length === 0 || typeof value[0] !== "string") {
    throw new Error("relay: frame must be an array starting with a text tag");
  }
  const tag = value[0];
  switch (tag) {
    case "REC":
      return { type: "REC", subId: asText(value[1]), seq: asUint(value[2]), record: asBytes(value[3]) };
    case "EOSE":
      return { type: "EOSE", subId: asText(value[1]) };
    case "OK":
      return {
        type: "OK",
        id: asBytes(value[1]),
        accepted: value[2] === true,
        reason: value.length > 3 ? asText(value[3]) : "",
      };
    case "CLOSED":
      return { type: "CLOSED", subId: asText(value[1]), reason: value.length > 2 ? asText(value[2]) : "" };
    default:
      throw new Error(`relay: unknown frame tag ${tag}`);
  }
}

/** Decode a client→relay frame. Exposed for tests and relay-side reuse. */
export function decodeClientFrame(bytes: Uint8Array): ClientFrame {
  const value = decodeCbor(bytes);
  if (!Array.isArray(value) || value.length === 0 || typeof value[0] !== "string") {
    throw new Error("relay: frame must be an array starting with a text tag");
  }
  const tag = value[0];
  switch (tag) {
    case "REQ":
      return { type: "REQ", subId: asText(value[1]), filter: decodeFilter(value[2]) };
    case "CLOSE":
      return { type: "CLOSE", subId: asText(value[1]) };
    case "PUB":
      return { type: "PUB", record: asBytes(value[1]), nonce: value[2] === null ? null : asUint(value[2]) };
    default:
      throw new Error(`relay: unknown frame tag ${tag}`);
  }
}
