/**
 * The selection pipeline (build plan §9, spec 009-gateway.md §§1-3):
 * envelope verify → policy check → kind dispatch → store. This is the
 * single entry point for every record the gateway ever indexes, whether
 * it arrived from a relay (relay.ts), was just signed on a user's behalf
 * (custody.ts via the API routes), or was self-published by the upload
 * pipeline (upload.ts) — one code path, one set of rules, no shortcuts
 * for "our own" content.
 *
 * `supersede` and `retract` need the *target* record's kind and author to
 * validate (spec 003 §§4.2-4.3 explicitly say this is NOT checked by the
 * kernel's single-record kind validation — see crates/evermesh-kernel/src/
 * kinds/content.rs doc comments on `Supersede::parse` / `Retract::parse`),
 * so this file is where that cross-record check lives.
 */
import { verifyRecord, deriveId, validateKind, recordToJson } from "@evermesh/kernel";
import type { Db } from "./db.ts";
import type { PolicyEngine, FeedTakedownBody } from "./policy.ts";
import type { CsamMatcher } from "./csam.ts";
import { extractEnvelope } from "./envelope.ts";
import { blobExists, readBlob, toWebStream, blobSize } from "./blobstore.ts";
import {
  indexManifest,
  indexComment,
  indexReaction,
  indexFollow,
  indexChannel,
  indexProfile,
  indexClaim,
  indexReceipt,
  retractByKind,
  commentSubjectOf,
  SUPERSEDE_HANDLERS,
} from "./ingest-kinds.ts";

const KIND_ROTATION = 1;
const KIND_PROFILE = 2;
const KIND_MANIFEST = 16;
const KIND_SUPERSEDE = 17;
const KIND_RETRACT = 18;
const KIND_COMMENT = 32;
const KIND_REACTION = 33;
const KIND_FOLLOW = 34;
const KIND_CHANNEL = 36;
const KIND_CLAIM_AUTHOR = 48;
const KIND_CLAIM_LICENSE = 49;
const KIND_CLAIM_TRANSFER = 50;
const KIND_CLAIM_DISPUTE = 51;
const KIND_NOTICE_TAKEDOWN = 64;
const KIND_NOTICE_COUNTER = 65;
const KIND_FEED_TAKEDOWN = 66;
const KIND_RECEIPT = 81;

export interface IngestDeps {
  db: Db;
  policy: PolicyEngine;
  csam: CsamMatcher;
  blobDir: string;
  log?: (msg: string) => void;
}

export interface IngestResult {
  stored: boolean;
  recordId?: string;
  reason?: string;
}

interface RecordMeta {
  kind: number;
  author: string;
}

function getRecordMeta(db: Db, id: string): RecordMeta | undefined {
  return db.prepare("SELECT kind, author FROM records WHERE id = ?").get(id) as RecordMeta | undefined;
}

function storeRecordRow(
  db: Db,
  id: string,
  kind: number,
  author: string,
  createdAt: number,
  receivedAt: number,
  cbor: Uint8Array,
  json: Record<string, unknown>,
): void {
  db.prepare(
    `INSERT INTO records (id, kind, author, created_at, received_at, cbor, json)
     VALUES (@id, @kind, @author, @createdAt, @receivedAt, @cbor, @json)
     ON CONFLICT(id) DO NOTHING`,
  ).run({ id, kind, author, createdAt, receivedAt, cbor: Buffer.from(cbor), json: JSON.stringify(json) });

  const refs = (json["4"] as [number, string][] | undefined) ?? [];
  const insertRef = db.prepare("INSERT INTO refs (record_id, ref_type, hash, position) VALUES (?, ?, ?, ?)");
  refs.forEach(([type, hashHexPrefixed], i) => {
    const hash = hashHexPrefixed.startsWith("hex:") ? hashHexPrefixed.slice(4) : hashHexPrefixed;
    insertRef.run(id, type, hash, i);
  });
}

/** Every blob referenced by a manifest body, for the CSAM index-time gate. */
function manifestBlobIds(body: Record<string, unknown>): string[] {
  const ids: string[] = [];
  const original = body.original as { blob?: string } | undefined;
  if (original?.blob) ids.push(unhexLocal(original.blob));
  const renditions = (body.renditions as { blob?: string }[] | undefined) ?? [];
  for (const r of renditions) if (r.blob) ids.push(unhexLocal(r.blob));
  const captions = (body.captions as { blob?: string }[] | undefined) ?? [];
  for (const c of captions) if (c.blob) ids.push(unhexLocal(c.blob));
  if (typeof body.thumbnail === "string") ids.push(unhexLocal(body.thumbnail));
  return ids;
}

function unhexLocal(s: string): string {
  return s.startsWith("hex:") ? s.slice(4) : s;
}

/** Body (key "5") of a feed.takedown record is JSON-interchange text-keyed
 * with hex-prefixed hash/notice fields inside `add`/`remove` tuples. */
function parseFeedBody(body: Record<string, unknown>): FeedTakedownBody {
  const add = ((body.add as unknown[] | undefined) ?? []) as [number, string, string, string?][];
  const remove = ((body.remove as unknown[] | undefined) ?? []) as [number, string][];
  return {
    feed: String(body.feed ?? ""),
    seq: Number(body.seq ?? 0),
    add: add.map(([t, h, r, n]) => [t, unhexLocal(h), r, n ? unhexLocal(n) : undefined]),
    remove: remove.map(([t, h]) => [t, unhexLocal(h)]),
  };
}

/**
 * CSAM index-time gate (CSAM.md "Integration points" #2). Only checks
 * blobs the gateway already has locally pinned — a manifest ingested from
 * a relay typically references blobs it hasn't fetched yet, so there is
 * nothing to hash here yet. That gap is closed at serve time: media.ts
 * runs the same check against the persisted `blobs.csam_checked` cache
 * before ever streaming bytes out, so "before any blob is served" (the
 * CsamMatcher contract) still holds even though this ingest-time pass is
 * necessarily best-effort for content the gateway didn't originate.
 */
async function manifestPassesCsamGate(deps: IngestDeps, body: Record<string, unknown>): Promise<boolean> {
  for (const blobId of manifestBlobIds(body)) {
    if (!blobExists(deps.blobDir, blobId)) continue;
    const size = blobSize(deps.blobDir, blobId);
    const verdict = await deps.csam.checkBlob(toWebStream(readBlob(deps.blobDir, blobId)), { size, blobId });
    if (verdict.match) {
      deps.policy.log("csam_match", "blob", blobId, "index-time CSAM match", {
        listId: verdict.listId,
        reportingChannel: deps.csam.reportingChannel(),
      });
      return false;
    }
  }
  return true;
}

/** Main entry point: verify → policy → dispatch → store. */
export async function processRecord(deps: IngestDeps, bytes: Uint8Array): Promise<IngestResult> {
  try {
    await verifyRecord(bytes);
  } catch (err) {
    return { stored: false, reason: `envelope_invalid: ${(err as Error).message}` };
  }

  const recordId = await deriveId(bytes);
  if (getRecordMeta(deps.db, recordId)) {
    return { stored: true, recordId }; // dedup by id (spec 006 §4)
  }

  try {
    await validateKind(bytes);
  } catch (err) {
    deps.policy.log("kind_invalid", "record", recordId, (err as Error).message);
    return { stored: false, recordId, reason: `kind_invalid: ${(err as Error).message}` };
  }

  const json = await recordToJson(bytes);
  const { kind, authorId, createdAt, refs, body } = extractEnvelope(json);
  const receivedAt = Date.now();

  const decision = deps.policy.checkRecord(kind, authorId, refs);
  if (!decision.allowed) {
    return { stored: false, recordId, reason: decision.reason };
  }

  if (kind === KIND_MANIFEST) {
    if (!(await manifestPassesCsamGate(deps, body))) {
      return { stored: false, recordId, reason: "csam_match" };
    }
  }

  if (kind === KIND_SUPERSEDE || kind === KIND_RETRACT) {
    const targetId = refs[0]?.hash;
    if (!targetId) return { stored: false, recordId, reason: "missing target ref" };
    const target = getRecordMeta(deps.db, targetId);
    if (!target) {
      // Partition tolerance means this can legitimately arrive before its
      // target; we don't queue it for later replay in v1 (documented
      // limitation — see README "what's not implemented").
      return { stored: false, recordId, reason: "target_unknown" };
    }
    if (target.author !== authorId) {
      deps.policy.log("kind_invalid", "record", recordId, "supersede/retract author != target author");
      return { stored: false, recordId, reason: "author_mismatch" };
    }
    if (target.kind === KIND_ROTATION) {
      return { stored: false, recordId, reason: "target_is_rotation" };
    }
    if (kind === KIND_SUPERSEDE) {
      const targetKind = Number(body.target_kind);
      if (targetKind !== target.kind) {
        return { stored: false, recordId, reason: "target_kind_mismatch" };
      }
      const replacement = (body.body as Record<string, unknown>) ?? {};
      const handler = SUPERSEDE_HANDLERS[target.kind];
      if (handler) handler(deps.db, targetId, authorId, refs, replacement, createdAt, receivedAt);
    } else {
      retractByKind(deps.db, target.kind, targetId);
    }
    storeRecordRow(deps.db, recordId, kind, authorId, createdAt, receivedAt, bytes, json);
    return { stored: true, recordId };
  }

  switch (kind) {
    case KIND_MANIFEST:
      indexManifest(deps.db, recordId, authorId, refs, body, createdAt, receivedAt);
      break;
    case KIND_PROFILE:
      indexProfile(deps.db, recordId, authorId, refs, body, createdAt, receivedAt);
      break;
    case KIND_COMMENT: {
      const parentId = refs[1]?.hash;
      if (parentId) {
        const parentSubject = commentSubjectOf(deps.db, parentId);
        if (parentSubject && parentSubject !== refs[0]?.hash) {
          return { stored: false, recordId, reason: "comment/parent-subject-mismatch" };
        }
      }
      indexComment(deps.db, recordId, authorId, refs, body, createdAt, receivedAt);
      break;
    }
    case KIND_REACTION:
      indexReaction(deps.db, recordId, authorId, refs, body, createdAt, receivedAt);
      break;
    case KIND_FOLLOW:
      indexFollow(deps.db, recordId, authorId, refs, body, createdAt, receivedAt);
      break;
    case KIND_CHANNEL:
      indexChannel(deps.db, recordId, authorId, refs, body, createdAt, receivedAt);
      break;
    case KIND_CLAIM_AUTHOR:
    case KIND_CLAIM_LICENSE:
    case KIND_CLAIM_TRANSFER:
    case KIND_CLAIM_DISPUTE:
    case KIND_NOTICE_TAKEDOWN:
    case KIND_NOTICE_COUNTER:
      indexClaim(deps.db, recordId, kind, authorId, refs, body, createdAt, receivedAt);
      break;
    case KIND_RECEIPT:
      indexReceipt(deps.db, recordId, authorId, refs, body, createdAt, receivedAt);
      break;
    case KIND_FEED_TAKEDOWN:
      deps.policy.applyFeedBatch(recordId, authorId, parseFeedBody(body));
      break;
    default:
      // Envelope+kind valid, policy allows it, but no product surface
      // consumes this kind (e.g. rotation, delegate, mirror, similarity,
      // endorse.gateway, attest, anchor, keygrant, playlist, live.*).
      // Still stored generically so /api/records/{id} can serve it.
      break;
  }

  storeRecordRow(deps.db, recordId, kind, authorId, createdAt, receivedAt, bytes, json);
  return { stored: true, recordId };
}
