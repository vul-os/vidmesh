/**
 * Minimal, self-contained canonical CBOR (RFC 8949) codec.
 *
 * Scope is deliberately narrow: this exists to encode/decode the relay
 * websocket frames of spec 006-relay.md §1 (arrays, text, byte strings,
 * unsigned integers, maps with text keys, bool, null) — NOT to replace
 * the kernel's record codec (records are built/verified by @vidmesh/kernel,
 * which does its own canonical CBOR internally). We never encode or decode
 * negative integers or floats because no relay frame or gateway-internal
 * use needs them (spec 001 §2 bans floats in kernel structures entirely).
 *
 * Canonical form follows spec 001 §2 / RFC 8949 §4.2.1:
 *   - definite lengths only
 *   - shortest-form integer/length encoding
 *   - map keys unique, sorted by bytewise-lexicographic order of their
 *     own canonical encoding (not by decoded string value — see
 *     `compareBytes`)
 *   - no floats, no tags
 */

/** Decoded/encodable value. Integers are `bigint` (unsigned only). */
export type CborValue =
  | bigint
  | Uint8Array
  | string
  | CborValue[]
  | Map<string, CborValue>
  | boolean
  | null;

const MT_UINT = 0;
const MT_BYTES = 2;
const MT_TEXT = 3;
const MT_ARRAY = 4;
const MT_MAP = 5;
const MT_SIMPLE = 7;

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

function encodeHead(majorType: number, n: bigint): number[] {
  const mt = majorType << 5;
  if (n < 0n) throw new Error("cbor: negative length/value not supported");
  if (n < 24n) return [mt | Number(n)];
  if (n <= 0xffn) return [mt | 24, Number(n)];
  if (n <= 0xffffn) {
    const v = Number(n);
    return [mt | 25, (v >> 8) & 0xff, v & 0xff];
  }
  if (n <= 0xffffffffn) {
    const v = Number(n);
    return [mt | 26, (v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff];
  }
  const out = [mt | 27];
  for (let shift = 56n; shift >= 0n; shift -= 8n) {
    out.push(Number((n >> shift) & 0xffn));
  }
  return out;
}

function encodeInto(value: CborValue, out: number[]): void {
  if (typeof value === "bigint") {
    out.push(...encodeHead(MT_UINT, value));
    return;
  }
  if (value === null) {
    out.push((MT_SIMPLE << 5) | 22);
    return;
  }
  if (typeof value === "boolean") {
    out.push((MT_SIMPLE << 5) | (value ? 21 : 20));
    return;
  }
  if (typeof value === "string") {
    const bytes = new TextEncoder().encode(value);
    out.push(...encodeHead(MT_TEXT, BigInt(bytes.length)));
    for (const b of bytes) out.push(b);
    return;
  }
  if (value instanceof Uint8Array) {
    out.push(...encodeHead(MT_BYTES, BigInt(value.length)));
    for (const b of value) out.push(b);
    return;
  }
  if (Array.isArray(value)) {
    out.push(...encodeHead(MT_ARRAY, BigInt(value.length)));
    for (const item of value) encodeInto(item, out);
    return;
  }
  if (value instanceof Map) {
    const entries = Array.from(value.entries()).map(([k, v]) => {
      const kBytes: number[] = [];
      encodeInto(k, kBytes);
      const vBytes: number[] = [];
      encodeInto(v, vBytes);
      return { kBytes, vBytes };
    });
    entries.sort((a, b) => compareBytes(a.kBytes, b.kBytes));
    out.push(...encodeHead(MT_MAP, BigInt(entries.length)));
    for (const e of entries) {
      out.push(...e.kBytes);
      out.push(...e.vBytes);
    }
    return;
  }
  throw new Error("cbor: unsupported value type");
}

/** Bytewise lexicographic comparison, per spec 001 §2 rule 3. */
function compareBytes(a: number[], b: number[]): number {
  const len = Math.min(a.length, b.length);
  for (let i = 0; i < len; i++) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return a.length - b.length;
}

/** Encode a value to canonical CBOR bytes. */
export function encodeCbor(value: CborValue): Uint8Array {
  const out: number[] = [];
  encodeInto(value, out);
  return Uint8Array.from(out);
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

class Decoder {
  pos = 0;
  private readonly bytes: Uint8Array;
  constructor(bytes: Uint8Array) {
    this.bytes = bytes;
  }

  private byte(): number {
    if (this.pos >= this.bytes.length) throw new Error("cbor: unexpected end of input");
    return this.bytes[this.pos++];
  }

  private readLength(additional: number): bigint {
    if (additional < 24) return BigInt(additional);
    if (additional === 24) return BigInt(this.byte());
    if (additional === 25) {
      const hi = this.byte();
      const lo = this.byte();
      return BigInt((hi << 8) | lo);
    }
    if (additional === 26) {
      let v = 0n;
      for (let i = 0; i < 4; i++) v = (v << 8n) | BigInt(this.byte());
      return v;
    }
    if (additional === 27) {
      let v = 0n;
      for (let i = 0; i < 8; i++) v = (v << 8n) | BigInt(this.byte());
      return v;
    }
    throw new Error("cbor: indefinite-length items are not supported (non-canonical)");
  }

  decode(): CborValue {
    const head = this.byte();
    const majorType = head >> 5;
    const additional = head & 0x1f;

    switch (majorType) {
      case MT_UINT:
        return this.readLength(additional);
      case 1:
        throw new Error("cbor: negative integers are not supported");
      case MT_BYTES: {
        const len = Number(this.readLength(additional));
        const bytes = this.bytes.slice(this.pos, this.pos + len);
        if (bytes.length !== len) throw new Error("cbor: truncated byte string");
        this.pos += len;
        return bytes;
      }
      case MT_TEXT: {
        const len = Number(this.readLength(additional));
        const bytes = this.bytes.slice(this.pos, this.pos + len);
        if (bytes.length !== len) throw new Error("cbor: truncated text string");
        this.pos += len;
        return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
      }
      case MT_ARRAY: {
        const len = Number(this.readLength(additional));
        const arr: CborValue[] = [];
        for (let i = 0; i < len; i++) arr.push(this.decode());
        return arr;
      }
      case MT_MAP: {
        const len = Number(this.readLength(additional));
        const map = new Map<string, CborValue>();
        for (let i = 0; i < len; i++) {
          const key = this.decode();
          const val = this.decode();
          if (typeof key !== "string") {
            throw new Error("cbor: only text-string map keys are supported");
          }
          map.set(key, val);
        }
        return map;
      }
      case MT_SIMPLE: {
        if (additional === 20) return false;
        if (additional === 21) return true;
        if (additional === 22) return null;
        throw new Error(`cbor: unsupported simple value ${additional}`);
      }
      case 6:
        throw new Error("cbor: tags are not permitted (spec 001 §2 rule 5)");
      default:
        throw new Error(`cbor: unsupported major type ${majorType}`);
    }
  }
}

/** Decode exactly one CBOR item; throws if trailing bytes remain. */
export function decodeCbor(bytes: Uint8Array): CborValue {
  const d = new Decoder(bytes);
  const value = d.decode();
  if (d.pos !== bytes.length) {
    throw new Error("cbor: trailing bytes after top-level item");
  }
  return value;
}

/** Decode one CBOR item, returning how many bytes it consumed (for framing). */
export function decodeCborPrefix(bytes: Uint8Array): { value: CborValue; length: number } {
  const d = new Decoder(bytes);
  const value = d.decode();
  return { value, length: d.pos };
}
