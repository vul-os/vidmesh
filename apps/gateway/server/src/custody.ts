/**
 * v1 custodial identities (spec 009-gateway.md §5, spec 002-identity.md §7).
 *
 * The gateway signs on behalf of its own users: register() creates a
 * Keypair and a genesis identity, encrypts the secret key at rest, and
 * publishes the genesis record. Custody here is deliberately simple and
 * explicitly NOT hardware-grade: the wrapping key is derived from a
 * single server-side secret (config `custody.secret`) via HKDF, not a
 * hardware enclave, threshold scheme, or per-user passphrase. That is an
 * acceptable v1 story only because the non-negotiable part of the
 * bargain — export — is real: POST /api/me/export (see exportIdentity
 * below) always returns the genesis record and the raw secret key, so a
 * user can rotate away from this gateway's custody at any time using
 * nothing but the kernel (spec 009 §5: "leaving is a rotation requiring
 * nothing from the gateway").
 */
import { createCipheriv, createDecipheriv, hkdfSync, randomBytes } from "node:crypto";
import argon2 from "argon2";
import {
  Keypair,
  createRecord,
  identity as kernelIdentity,
  signDerivation,
  recordToJson,
  toHex,
  fromHex,
  type RecordInit,
} from "@evermesh/kernel";
import type { Db } from "./db.ts";
import type { Config } from "./config.ts";
import { ApiError } from "./errors.ts";
import { processRecord, type IngestDeps } from "./ingest.ts";

const HKDF_INFO = Buffer.from("evermesh:custody:v1", "utf-8");
const EXPORT_MAX_ATTEMPTS_PER_HOUR = 5;
const EXPORT_WINDOW_MS = 60 * 60 * 1000;

export function deriveWrapKey(custodySecret: string, identityId: string): Buffer {
  const salt = fromHex(identityId);
  const bits = hkdfSync("sha256", Buffer.from(custodySecret, "utf-8"), salt, HKDF_INFO, 32);
  return Buffer.from(bits);
}

/** AES-256-GCM encrypt; wire format `iv.tag.ciphertext`, each base64url. */
export function encryptSecret(secret: Uint8Array, key: Buffer): string {
  const iv = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", key, iv);
  const ciphertext = Buffer.concat([cipher.update(secret), cipher.final()]);
  const tag = cipher.getAuthTag();
  return [iv, tag, ciphertext].map((b) => b.toString("base64url")).join(".");
}

export function decryptSecret(enc: string, key: Buffer): Uint8Array {
  const [ivB64, tagB64, ctB64] = enc.split(".");
  if (!ivB64 || !tagB64 || !ctB64) throw new Error("custody: malformed encrypted secret");
  const iv = Buffer.from(ivB64, "base64url");
  const tag = Buffer.from(tagB64, "base64url");
  const ciphertext = Buffer.from(ctB64, "base64url");
  const decipher = createDecipheriv("aes-256-gcm", key, iv);
  decipher.setAuthTag(tag);
  const plaintext = Buffer.concat([decipher.update(ciphertext), decipher.final()]);
  return new Uint8Array(plaintext);
}

export interface UserRow {
  id: number;
  handle: string;
  pw_hash: string;
  identity_id: string;
  secret_key_enc: string;
  created_at: number;
}

export class CustodyService {
  private readonly getUserByHandleStmt;
  private readonly getUserByIdStmt;
  private readonly insertUserStmt;
  private readonly insertExportAttemptStmt;
  private readonly countExportAttemptsStmt;
  private readonly getRecordCborStmt;

  constructor(
    private readonly db: Db,
    private readonly config: Config,
    /** Publish a freshly signed record to all configured relays (fire-and-forget). */
    private readonly publish: (record: Uint8Array) => void,
    /** Used to index a new user's genesis record locally and immediately (not just publish it out). */
    private readonly ingest: IngestDeps,
  ) {
    this.getUserByHandleStmt = db.prepare("SELECT * FROM users WHERE handle = ?");
    this.getUserByIdStmt = db.prepare("SELECT * FROM users WHERE id = ?");
    this.insertUserStmt = db.prepare(
      `INSERT INTO users (handle, pw_hash, identity_id, secret_key_enc, created_at)
       VALUES (@handle, @pwHash, @identityId, @secretKeyEnc, @createdAt)`,
    );
    this.insertExportAttemptStmt = db.prepare("INSERT INTO export_attempts (user_id, ts) VALUES (?, ?)");
    this.countExportAttemptsStmt = db.prepare(
      "SELECT COUNT(*) AS n FROM export_attempts WHERE user_id = ? AND ts > ?",
    );
    this.getRecordCborStmt = db.prepare("SELECT cbor FROM records WHERE id = ?");
  }

  getUserByHandle(handle: string): UserRow | undefined {
    return this.getUserByHandleStmt.get(handle) as UserRow | undefined;
  }

  getUserById(userId: number): UserRow | undefined {
    return this.getUserByIdStmt.get(userId) as UserRow | undefined;
  }

  /** Create a custodied identity + account; publishes the genesis record. */
  async register(handle: string, password: string): Promise<{ userId: number; identityId: string; genesis: Uint8Array }> {
    if (this.getUserByHandle(handle)) {
      throw new ApiError("conflict", "handle already registered", 409);
    }
    const keypair = await Keypair.generate();
    const { identityId, record: genesis } = await kernelIdentity.genesis(keypair, {
      contestWindow: this.config.custody.contestWindowSeconds,
    });
    const pwHash = await argon2.hash(password);
    const wrapKey = deriveWrapKey(this.config.custody.secret, identityId);
    const secretKeyEnc = encryptSecret(keypair.secret, wrapKey);

    const info = this.insertUserStmt.run({
      handle,
      pwHash,
      identityId,
      secretKeyEnc,
      createdAt: Date.now(),
    });

    // Index locally first (so export/reads see it immediately), then
    // publish out — mirrors the upload pipeline's own local-index-then-
    // publish order.
    const indexed = await processRecord(this.ingest, genesis);
    if (!indexed.stored) {
      throw new Error(`failed to index the genesis record we just created: ${indexed.reason}`);
    }
    this.publish(genesis);

    return { userId: Number(info.lastInsertRowid), identityId, genesis };
  }

  async verifyPassword(userId: number, password: string): Promise<boolean> {
    const user = this.getUserById(userId);
    if (!user) return false;
    return argon2.verify(user.pw_hash, password);
  }

  private async keypairFor(user: UserRow): Promise<Keypair> {
    const wrapKey = deriveWrapKey(this.config.custody.secret, user.identity_id);
    const secret = decryptSecret(user.secret_key_enc, wrapKey);
    return Keypair.fromSecret(secret);
  }

  /** Sign-on-behalf: build and sign a record as the user's custodied identity. */
  async signRecord(userId: number, init: RecordInit): Promise<Uint8Array> {
    const user = this.getUserById(userId);
    if (!user) throw new ApiError("unauthorized", "unknown user");
    const keypair = await this.keypairFor(user);
    return createRecord(keypair, user.identity_id, init);
  }

  /**
   * Sign a rendition derivation statement on behalf of a custodied
   * uploader (spec 004-manifest.md §3.1). Kept as one isolated function
   * per the build plan, even though `@evermesh/kernel` already exposes
   * `signDerivation` (verified against crates/evermesh-wasm/src/lib.rs:
   * it builds the exact `[original, rendition, codec, width, height,
   * bitrate]` statement and signs `"evermesh:derivation:v1" ||
   * BLAKE3-256(stmt)` — no gap to work around here).
   */
  async signDerivationFor(
    userId: number,
    opts: { originalBlobId: string; renditionBlobId: string; codec: string; width: number; height: number; bitrate: number },
  ): Promise<{ identityId: string; publicKeyHex: string; signatureHex: string }> {
    const user = this.getUserById(userId);
    if (!user) throw new ApiError("unauthorized", "unknown user");
    const keypair = await this.keypairFor(user);
    const sig = await signDerivation(keypair, opts);
    return { identityId: user.identity_id, publicKeyHex: toHex(keypair.publicKey), signatureHex: toHex(sig) };
  }

  /**
   * The non-negotiable export path (spec 009 §5). Password re-confirmed;
   * rate-limited via an indexed, append-only attempts log so the check
   * stays cheap even under repeated calls.
   */
  async exportIdentity(
    userId: number,
    password: string,
  ): Promise<{ identity: { identityId: string; genesis: Record<string, unknown> }; secretKeys: { alg: number; secretHex: string }[] }> {
    const user = this.getUserById(userId);
    if (!user) throw new ApiError("unauthorized", "unknown user");

    const windowStart = Date.now() - EXPORT_WINDOW_MS;
    const attempts = (this.countExportAttemptsStmt.get(userId, windowStart) as { n: number }).n;
    if (attempts >= EXPORT_MAX_ATTEMPTS_PER_HOUR) {
      throw new ApiError("rate_limited", "too many export attempts; try again later");
    }
    this.insertExportAttemptStmt.run(userId, Date.now());

    const ok = await argon2.verify(user.pw_hash, password);
    if (!ok) throw new ApiError("unauthorized", "incorrect password");

    const row = this.getRecordCborStmt.get(user.identity_id) as { cbor: Buffer } | undefined;
    if (!row) throw new ApiError("not_found", "genesis record missing from index");
    const genesisJson = await recordToJson(new Uint8Array(row.cbor));

    const keypair = await this.keypairFor(user);
    return {
      identity: { identityId: user.identity_id, genesis: genesisJson },
      secretKeys: [{ alg: 1, secretHex: toHex(keypair.secret) }],
    };
  }
}
