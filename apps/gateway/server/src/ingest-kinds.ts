/**
 * Per-kind table indexers, used both for a record's first arrival and for
 * `supersede` replacement bodies (the target's stable record id is reused
 * as the primary key so refs into the original id keep resolving — spec
 * 003 §4.2: "the original id remains the stable reference for refs").
 *
 * Records are handed in as JSON-interchange values (spec 001 §11): byte
 * fields are `"hex:<hex>"` strings. `unhex` strips the prefix.
 */
import type { Db } from "./db.ts";
import type { Ref } from "./policy.ts";

export function unhex(v: unknown): string {
  if (typeof v !== "string") return "";
  return v.startsWith("hex:") ? v.slice(4) : v;
}

function str(v: unknown, fallback = ""): string {
  return typeof v === "string" ? v : fallback;
}

function num(v: unknown, fallback = 0): number {
  return typeof v === "number" ? v : fallback;
}

function strArray(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
}

function bodyMap(v: unknown): Record<string, unknown> {
  return v && typeof v === "object" ? (v as Record<string, unknown>) : {};
}

/** `refs[0]` of type 0, if present — the common "channel" / "subject" slot. */
function firstRecordRef(refs: Ref[], index = 0): string | null {
  const ref = refs[index];
  return ref && ref.type === 0 ? ref.hash : null;
}

// ---------------------------------------------------------------------------
// manifest (16)
// ---------------------------------------------------------------------------

export function indexManifest(
  db: Db,
  recordId: string,
  authorId: string,
  refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): void {
  const original = bodyMap(body.original);
  const channelId = firstRecordRef(refs);
  db.prepare(
    `INSERT INTO videos (manifest_id, author, title, description, tags_json, language,
       duration_ms, thumbnail_blob, channel_id, license, created_at, received_at, body_json, retracted)
     VALUES (@id, @author, @title, @description, @tags, @language,
       @duration, @thumb, @channel, @license, @createdAt, @receivedAt, @bodyJson, 0)
     ON CONFLICT(manifest_id) DO UPDATE SET
       author=excluded.author, title=excluded.title, description=excluded.description,
       tags_json=excluded.tags_json, language=excluded.language, duration_ms=excluded.duration_ms,
       thumbnail_blob=excluded.thumbnail_blob, channel_id=excluded.channel_id, license=excluded.license,
       body_json=excluded.body_json, retracted=0`,
  ).run({
    id: recordId,
    author: authorId,
    title: str(body.title, "untitled"),
    description: str(body.description),
    tags: JSON.stringify(strArray(body.tags)),
    language: typeof body.language === "string" ? body.language : null,
    duration: num(original.duration),
    thumb: body.thumbnail ? unhex(body.thumbnail) : null,
    channel: channelId,
    license: str(body.license, "all-rights-reserved"),
    createdAt,
    receivedAt,
    bodyJson: JSON.stringify(body),
  });
}

// ---------------------------------------------------------------------------
// comment (32)
// ---------------------------------------------------------------------------

export function indexComment(
  db: Db,
  recordId: string,
  authorId: string,
  refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): { manifestId: string | null } {
  const manifestId = firstRecordRef(refs, 0);
  const parent = firstRecordRef(refs, 1);
  db.prepare(
    `INSERT INTO comments (record_id, manifest_id, author, text, media_json, parent, created_at, received_at, retracted)
     VALUES (@id, @manifestId, @author, @text, @media, @parent, @createdAt, @receivedAt, 0)
     ON CONFLICT(record_id) DO UPDATE SET text=excluded.text, media_json=excluded.media_json, retracted=0`,
  ).run({
    id: recordId,
    manifestId,
    author: authorId,
    text: str(body.text),
    media: JSON.stringify(strArray(body.media).map(unhex)),
    parent,
    createdAt,
    receivedAt,
  });
  return { manifestId };
}

/** Defense-in-depth check for spec 003 §5.1: a parent must share the subject. */
export function commentSubjectOf(db: Db, commentRecordId: string): string | null {
  const row = db.prepare("SELECT manifest_id FROM comments WHERE record_id = ?").get(commentRecordId) as
    | { manifest_id: string }
    | undefined;
  return row?.manifest_id ?? null;
}

// ---------------------------------------------------------------------------
// reaction (33) — an identity's later reaction to the same target supersedes
// its earlier one for counting purposes (spec 003 §5.2). "Later" is
// resolved by relay-local receive order (received_at), never the
// untrusted author-claimed created_at (spec 001 §10).
// ---------------------------------------------------------------------------

export function indexReaction(
  db: Db,
  recordId: string,
  authorId: string,
  refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): void {
  const targetId = firstRecordRef(refs, 0);
  if (!targetId) return;
  db.prepare(
    `INSERT INTO reactions (target_id, author, record_id, reaction, created_at, received_at)
     VALUES (@target, @author, @id, @reaction, @createdAt, @receivedAt)
     ON CONFLICT(target_id, author) DO UPDATE SET
       record_id=excluded.record_id, reaction=excluded.reaction,
       created_at=excluded.created_at, received_at=excluded.received_at
     WHERE excluded.received_at >= reactions.received_at`,
  ).run({ target: targetId, author: authorId, id: recordId, reaction: str(body.reaction), createdAt, receivedAt });
}

// ---------------------------------------------------------------------------
// follow (34)
// ---------------------------------------------------------------------------

export function indexFollow(
  db: Db,
  recordId: string,
  authorId: string,
  refs: Ref[],
  _body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): void {
  const target = firstRecordRef(refs, 0);
  if (!target) return;
  db.prepare(
    `INSERT INTO follows (record_id, author, target, created_at, received_at, retracted)
     VALUES (@id, @author, @target, @createdAt, @receivedAt, 0)
     ON CONFLICT(record_id) DO UPDATE SET retracted=0`,
  ).run({ id: recordId, author: authorId, target, createdAt, receivedAt });
}

// ---------------------------------------------------------------------------
// channel (36)
// ---------------------------------------------------------------------------

export function indexChannel(
  db: Db,
  recordId: string,
  authorId: string,
  _refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): void {
  db.prepare(
    `INSERT INTO channels (record_id, author, title, description, avatar_blob, banner_blob, created_at, received_at, retracted)
     VALUES (@id, @author, @title, @description, @avatar, @banner, @createdAt, @receivedAt, 0)
     ON CONFLICT(record_id) DO UPDATE SET
       title=excluded.title, description=excluded.description,
       avatar_blob=excluded.avatar_blob, banner_blob=excluded.banner_blob, retracted=0`,
  ).run({
    id: recordId,
    author: authorId,
    title: str(body.title, "untitled channel"),
    description: str(body.description),
    avatar: body.avatar ? unhex(body.avatar) : null,
    banner: body.banner ? unhex(body.banner) : null,
    createdAt,
    receivedAt,
  });
}

// ---------------------------------------------------------------------------
// profile (2) — spec 002 §6: latest-wins by rotation-chain order of the
// signing key, then supersession; created_at is explicitly NOT the rule.
// kernel-ts exposes only the *resolved* chain state (IdentityState), not an
// ordered list ranking historical keys, so exact chain-order comparison
// isn't computable from this API surface alone (see README "kernel-ts API
// gaps"). We approximate with last-received-wins, which is monotonic
// within one gateway process and coincides with chain order in the
// overwhelmingly common case (no rotation between two profile posts).
// ---------------------------------------------------------------------------

export function indexProfile(
  db: Db,
  recordId: string,
  authorId: string,
  _refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): void {
  const existing = db.prepare("SELECT received_at FROM profiles WHERE identity_id = ?").get(authorId) as
    | { received_at: number }
    | undefined;
  if (existing && existing.received_at > receivedAt) return; // a newer profile is already indexed
  db.prepare(
    `INSERT INTO profiles (identity_id, record_id, name, about, avatar_blob, payment_json, relays_json, created_at, received_at)
     VALUES (@identityId, @recordId, @name, @about, @avatar, @payment, @relays, @createdAt, @receivedAt)
     ON CONFLICT(identity_id) DO UPDATE SET
       record_id=excluded.record_id, name=excluded.name, about=excluded.about, avatar_blob=excluded.avatar_blob,
       payment_json=excluded.payment_json, relays_json=excluded.relays_json,
       created_at=excluded.created_at, received_at=excluded.received_at`,
  ).run({
    identityId: authorId,
    recordId,
    name: str(body.name, "anonymous"),
    about: typeof body.about === "string" ? body.about : null,
    avatar: body.avatar ? unhex(body.avatar) : null,
    payment: JSON.stringify(body.payment ?? []),
    relays: JSON.stringify(strArray(body.relays)),
    createdAt,
    receivedAt,
  });
}

// ---------------------------------------------------------------------------
// claims + notices (48-51, 64-65)
// ---------------------------------------------------------------------------

export function resolveClaimSubject(db: Db, kind: number, refs: Ref[]): { targetRecordId: string; subjectManifestId: string | null } {
  if (kind === 48 || kind === 49 || kind === 50) {
    const manifestId = firstRecordRef(refs, 0) ?? "";
    return { targetRecordId: manifestId, subjectManifestId: manifestId || null };
  }
  if (kind === 51 || kind === 65) {
    // dispute / counter-notice: refs[0] is the disputed claim/notice, not
    // the manifest directly — resolve one hop via the already-indexed row.
    const targetRecordId = firstRecordRef(refs, 0) ?? "";
    const row = db.prepare("SELECT subject_manifest_id FROM claims WHERE record_id = ?").get(targetRecordId) as
      | { subject_manifest_id: string | null }
      | undefined;
    return { targetRecordId, subjectManifestId: row?.subject_manifest_id ?? null };
  }
  if (kind === 64) {
    // notice.takedown: refs are one-or-more [0, manifest] or [1, blob], in
    // any order. Prefer a manifest ref for target_record_id too (it's the
    // more useful "what does this point at" value); fall back to the
    // first ref only when every subject is a bare blob.
    const manifestRef = refs.find((r) => r.type === 0);
    const targetRecordId = manifestRef?.hash ?? refs[0]?.hash ?? "";
    return { targetRecordId, subjectManifestId: manifestRef?.hash ?? null };
  }
  return { targetRecordId: firstRecordRef(refs, 0) ?? "", subjectManifestId: null };
}

export function indexClaim(
  db: Db,
  recordId: string,
  kind: number,
  authorId: string,
  refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): void {
  const { targetRecordId, subjectManifestId } = resolveClaimSubject(db, kind, refs);
  db.prepare(
    `INSERT INTO claims (record_id, kind, author, subject_manifest_id, target_record_id, body_json, created_at, received_at, retracted)
     VALUES (@id, @kind, @author, @subject, @target, @bodyJson, @createdAt, @receivedAt, 0)
     ON CONFLICT(record_id) DO UPDATE SET body_json=excluded.body_json, retracted=0`,
  ).run({
    id: recordId,
    kind,
    author: authorId,
    subject: subjectManifestId,
    target: targetRecordId,
    bodyJson: JSON.stringify(body),
    createdAt,
    receivedAt,
  });
}

// ---------------------------------------------------------------------------
// receipt (81)
// ---------------------------------------------------------------------------

export function indexReceipt(
  db: Db,
  recordId: string,
  authorId: string,
  refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
): void {
  const manifestId = firstRecordRef(refs, 0);
  if (!manifestId) return;
  db.prepare(
    `INSERT INTO receipts (record_id, manifest_id, payer, amount, currency, rail, payee, proof, message, created_at, received_at)
     VALUES (@id, @manifestId, @payer, @amount, @currency, @rail, @payee, @proof, @message, @createdAt, @receivedAt)
     ON CONFLICT(record_id) DO UPDATE SET amount=excluded.amount`,
  ).run({
    id: recordId,
    manifestId,
    payer: authorId,
    amount: num(body.amount),
    currency: str(body.currency),
    rail: num(body.rail),
    payee: unhex(body.payee),
    proof: typeof body.proof === "string" ? body.proof : null,
    message: typeof body.message === "string" ? body.message : null,
    createdAt,
    receivedAt,
  });
}

// ---------------------------------------------------------------------------
// retract dispatch (18) — marks the target row in its own kind's table.
// ---------------------------------------------------------------------------

const RETRACTABLE: Record<number, { table: string; keyColumn: string }> = {
  16: { table: "videos", keyColumn: "manifest_id" },
  32: { table: "comments", keyColumn: "record_id" },
  34: { table: "follows", keyColumn: "record_id" },
  36: { table: "channels", keyColumn: "record_id" },
  48: { table: "claims", keyColumn: "record_id" },
  49: { table: "claims", keyColumn: "record_id" },
  50: { table: "claims", keyColumn: "record_id" },
  51: { table: "claims", keyColumn: "record_id" },
};

export function retractByKind(db: Db, kind: number, targetId: string): void {
  if (kind === 33) {
    // Reactions have no `retracted` flag (they're superseded by a newer
    // reaction from the same author, not usually withdrawn) — but the
    // kind registry doesn't forbid retracting one, so honor it by
    // removing the row outright rather than silently ignoring it.
    db.prepare("DELETE FROM reactions WHERE record_id = ?").run(targetId);
    return;
  }
  const target = RETRACTABLE[kind];
  if (!target) return; // no per-kind table to hide
  db.prepare(`UPDATE ${target.table} SET retracted = 1 WHERE ${target.keyColumn} = ?`).run(targetId);
}

export type IndexFn = (
  db: Db,
  recordId: string,
  authorId: string,
  refs: Ref[],
  body: Record<string, unknown>,
  createdAt: number,
  receivedAt: number,
) => void;

/** Kinds that `supersede` (17) knows how to replace, per spec 003 §4.2. */
export const SUPERSEDE_HANDLERS: Record<number, IndexFn> = {
  16: indexManifest,
  32: (db, id, author, refs, body, c, r) => void indexComment(db, id, author, refs, body, c, r),
  34: indexFollow,
  36: indexChannel,
  2: indexProfile,
  48: (db, id, author, refs, body, c, r) => indexClaim(db, id, 48, author, refs, body, c, r),
  49: (db, id, author, refs, body, c, r) => indexClaim(db, id, 49, author, refs, body, c, r),
  50: (db, id, author, refs, body, c, r) => indexClaim(db, id, 50, author, refs, body, c, r),
};
