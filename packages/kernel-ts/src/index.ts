/**
 * @vidmesh/kernel — typed TypeScript API over the Vidmesh WASM kernel.
 *
 * Everything here delegates to `crates/vidmesh-wasm` (the same Rust
 * kernel that runs natively), so ids, signatures, and canonical bytes
 * are identical across Rust, Node, and browsers. Call {@link init} once
 * before anything else (all helpers await it internally, so explicit
 * init is optional but avoids a first-call latency spike).
 *
 * Conventions mirror the spec (001 §11): records are `Uint8Array`
 * canonical CBOR; hashes cross as lowercase hex strings; structured
 * bodies use the JSON interchange form, in which byte strings are
 * `"hex:<hex>"` strings.
 */

import initWasm, * as wasm from "../wasm/vidmesh_wasm.js";

let ready: Promise<void> | undefined;

/** Load and initialize the WASM kernel (idempotent). */
export function init(): Promise<void> {
  ready ??= load();
  return ready;
}

async function load(): Promise<void> {
  if (typeof process !== "undefined" && process.versions?.node) {
    const { readFile } = await import("node:fs/promises");
    const bytes = await readFile(new URL("../wasm/vidmesh_wasm_bg.wasm", import.meta.url));
    await initWasm({ module_or_path: bytes });
  } else {
    // Browser: wasm-pack's default relative fetch.
    await initWasm();
  }
}

/** A ref: 0 = record reference, 1 = blob reference (spec 001 §6). */
export interface Ref {
  type: 0 | 1;
  /** 64 lowercase hex chars. */
  hash: string;
}

/** Inputs for {@link createRecord}. */
export interface RecordInit {
  kind: number;
  /** Unix seconds; defaults to now. */
  createdAt?: number | bigint;
  refs?: Ref[];
  /**
   * The body map in JSON interchange form: plain object with string
   * keys; byte values as `"hex:<hex>"` strings; integers only.
   */
  body?: Record<string, unknown>;
}

/** Current state of an identity per spec 002 §4. */
export interface IdentityState {
  identityId: string;
  signingKey: string;
  keyAlg: number;
  recovery: [number, string][];
  contestWindow: number;
  head: string;
  depth: number;
}

/** Result of {@link hashBlobStream}. */
export interface BlobSummary {
  /** Blob id, lowercase hex (spec 001 §6). */
  id: string;
  size: number;
  nChunks: number;
  /** Chunk-tree root, or null for the empty blob (spec 001 §8). */
  chunkRoot: string | null;
}

const hexTable = Array.from({ length: 256 }, (_, i) => i.toString(16).padStart(2, "0"));

/** Bytes → lowercase hex. */
export function toHex(bytes: Uint8Array): string {
  let out = "";
  for (const b of bytes) out += hexTable[b];
  return out;
}

/** Lowercase/uppercase hex → bytes. Throws on malformed input. */
export function fromHex(hex: string): Uint8Array {
  if (hex.length % 2 !== 0 || /[^0-9a-fA-F]/.test(hex)) {
    throw new Error("malformed hex string");
  }
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** An Ed25519 keypair (thin wrapper over the WASM kernel's). */
export class Keypair {
  /** @internal */
  constructor(readonly inner: wasm.WasmKeypair) {}

  static async generate(): Promise<Keypair> {
    await init();
    return new Keypair(new wasm.WasmKeypair());
  }

  static async fromSecret(secret: Uint8Array): Promise<Keypair> {
    await init();
    return new Keypair(wasm.WasmKeypair.fromSecret(secret));
  }

  get publicKey(): Uint8Array {
    return this.inner.publicKey();
  }

  /** The 32 secret bytes. Handle with care. */
  get secret(): Uint8Array {
    return this.inner.secret();
  }
}

function refsJson(refs: Ref[]): string {
  return JSON.stringify(refs.map((r) => [r.type, `hex:${r.hash}`]));
}

/**
 * Build and sign a record; returns canonical CBOR bytes (spec 001 §§1–4).
 * `identityId` is the author's identity id (hex); use the all-zero id
 * only inside {@link genesis}.
 */
export async function createRecord(
  keypair: Keypair,
  identityId: string,
  init_: RecordInit,
): Promise<Uint8Array> {
  await init();
  const createdAt = init_.createdAt ?? Math.floor(Date.now() / 1000);
  return wasm.buildRecord(
    keypair.inner,
    fromHex(identityId),
    init_.kind,
    BigInt(createdAt),
    refsJson(init_.refs ?? []),
    JSON.stringify(init_.body ?? {}),
  );
}

/** Derive a record's id (hex). Validates envelope shape, not signature. */
export async function deriveId(record: Uint8Array): Promise<string> {
  await init();
  return toHex(wasm.record_id(record));
}

/** Full envelope verification (canonical form + signature). Throws on failure. */
export async function verifyRecord(record: Uint8Array): Promise<void> {
  await init();
  wasm.verify_record(record);
}

/** Kind-level validation for known kinds (spec 003); unknown kinds pass. */
export async function validateKind(record: Uint8Array): Promise<void> {
  await init();
  wasm.validate_kind(record);
}

/** Canonical bytes → JSON interchange object. */
export async function recordToJson(record: Uint8Array): Promise<Record<string, unknown>> {
  await init();
  return JSON.parse(wasm.record_to_json(record)) as Record<string, unknown>;
}

/** JSON interchange (object or string) → canonical bytes. */
export async function recordFromJson(
  json: string | Record<string, unknown>,
): Promise<Uint8Array> {
  await init();
  return wasm.record_from_json(typeof json === "string" ? json : JSON.stringify(json));
}

/** Identity chain helpers (spec 002). */
export const identity = {
  /** Create a new identity; returns its id and the genesis record to publish. */
  async genesis(
    keypair: Keypair,
    opts: {
      recovery?: [number, string][];
      contestWindow?: number;
      createdAt?: number | bigint;
    } = {},
  ): Promise<{ identityId: string; record: Uint8Array }> {
    await init();
    const record = wasm.genesisRecord(
      keypair.inner,
      JSON.stringify((opts.recovery ?? []).map(([alg, key]) => [alg, `hex:${key}`])),
      opts.contestWindow ?? 604_800,
      BigInt(opts.createdAt ?? Math.floor(Date.now() / 1000)),
    );
    return { identityId: toHex(wasm.record_id(record)), record };
  },

  /** Build a rotation record (spec 002 §3). */
  async rotate(
    signer: Keypair,
    opts: {
      identityId: string;
      prevRotationId: string;
      newKey: Uint8Array;
      newKeyAlg?: number;
      recovery?: [number, string][];
      contestWindow?: number;
      createdAt?: number | bigint;
    },
  ): Promise<Uint8Array> {
    await init();
    return wasm.rotateRecord(
      signer.inner,
      fromHex(opts.identityId),
      fromHex(opts.prevRotationId),
      opts.newKey,
      opts.newKeyAlg ?? 1,
      JSON.stringify((opts.recovery ?? []).map(([alg, key]) => [alg, `hex:${key}`])),
      opts.contestWindow ?? 604_800,
      BigInt(opts.createdAt ?? Math.floor(Date.now() / 1000)),
    );
  },

  /** Verify a rotation chain and return the current state (spec 002 §4). */
  async verifyChain(
    records: Uint8Array[],
    now: number | bigint = Math.floor(Date.now() / 1000),
  ): Promise<IdentityState> {
    await init();
    const arr = new Array<Uint8Array>();
    for (const r of records) arr.push(r);
    const raw = JSON.parse(wasm.verifyChain(arr, BigInt(now))) as {
      identity_id: string;
      signing_key: string;
      key_alg: number;
      recovery: [number, string][];
      contest_window: number;
      head: string;
      depth: number;
    };
    const stripHex = (s: string) => s.replace(/^hex:/, "");
    return {
      identityId: stripHex(raw.identity_id),
      signingKey: stripHex(raw.signing_key),
      keyAlg: raw.key_alg,
      recovery: raw.recovery.map(([alg, key]) => [alg, stripHex(key)]),
      contestWindow: raw.contest_window,
      head: stripHex(raw.head),
      depth: raw.depth,
    };
  },
};

/** BLAKE3-256 of an in-memory blob; returns hex (spec 001 §6). */
export async function hashBlob(bytes: Uint8Array): Promise<string> {
  await init();
  return toHex(wasm.hashBlob(bytes));
}

/**
 * Sign a rendition derivation statement (spec 004 §3.1). Statement
 * construction happens inside the kernel so all runtimes sign the same
 * bytes. Returns the signature for the manifest's `derivation_sig`.
 */
export async function signDerivation(
  keypair: Keypair,
  opts: {
    originalBlobId: string;
    renditionBlobId: string;
    codec: string;
    width: number;
    height: number;
    bitrate: number;
  },
): Promise<Uint8Array> {
  await init();
  return wasm.signDerivation(
    keypair.inner,
    fromHex(opts.originalBlobId),
    fromHex(opts.renditionBlobId),
    opts.codec,
    opts.width,
    opts.height,
    opts.bitrate,
  );
}

/**
 * Hash a WHATWG stream in one pass: flat blob id plus the 1 MiB chunk
 * tree (spec 001 §8), without buffering the blob.
 */
export async function hashBlobStream(
  stream: ReadableStream<Uint8Array>,
): Promise<BlobSummary> {
  await init();
  const hasher = new wasm.BlobHasher();
  const reader = stream.getReader();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value) hasher.update(value);
    }
  } finally {
    reader.releaseLock();
  }
  hasher.finalize();
  return {
    id: hasher.idHex,
    size: hasher.size,
    nChunks: hasher.nChunks,
    chunkRoot: hasher.chunkRootHex ?? null,
  };
}

/**
 * Verify one chunk against a chunk root (spec 001 §8). `proof` is the
 * prover's sibling path as an array of hex hashes. Throws on failure.
 */
export async function verifyChunk(opts: {
  root: string;
  nChunks: number;
  index: number;
  chunk: Uint8Array;
  proof: string[];
}): Promise<void> {
  await init();
  const proof = new Uint8Array(opts.proof.length * 32);
  opts.proof.forEach((h, i) => proof.set(fromHex(h), i * 32));
  wasm.verifyChunk(fromHex(opts.root), opts.nChunks, opts.index, opts.chunk, proof);
}
