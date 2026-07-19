/**
 * Row → API-response-shape conversions shared by videos.ts, channels.ts,
 * and search.ts, plus the opaque cursor codec (API.md: "cursor pagination:
 * opaque string, next: null at the end"). Pagination and freshness both
 * key off `received_at` (relay-local receive order), never the
 * author-claimed `created_at` (spec 001 §10: untrusted, no ordering
 * guarantee) — see also policy.ts's post-hoc denylist check, applied here
 * so a `feed.takedown` batch that lands after a manifest was indexed still
 * hides it (moderation is instant, not just at ingest time).
 */
import type { Db } from "../db.ts";
import { unhex } from "../ingest-kinds.ts";
import type { Comment, ClaimView, ReceiptView, VideoSummary, Video, AuthorRef } from "../types.ts";

export function encodeCursor(receivedAt: number): string {
  return Buffer.from(String(receivedAt), "utf-8").toString("base64url");
}

export function decodeCursor(cursor: string | undefined): number | undefined {
  if (!cursor) return undefined;
  const n = Number(Buffer.from(cursor, "base64url").toString("utf-8"));
  return Number.isFinite(n) ? n : undefined;
}

/** Cheap, indexed check: is this record id currently policy-denylisted? */
export function isRecordDenylisted(db: Db, recordId: string): boolean {
  return !!db.prepare("SELECT 1 FROM policy_denylist WHERE scope = 'record' AND value = ?").get(recordId);
}

interface VideoRow {
  manifest_id: string;
  author: string;
  title: string;
  description: string;
  tags_json: string;
  language: string | null;
  duration_ms: number;
  thumbnail_blob: string | null;
  channel_id: string | null;
  license: string;
  created_at: number;
  received_at: number;
  body_json: string;
}

interface ProfileRow {
  name: string;
  avatar_blob: string | null;
}

function authorRef(db: Db, identityId: string): AuthorRef {
  const p = db.prepare("SELECT name, avatar_blob FROM profiles WHERE identity_id = ?").get(identityId) as
    | ProfileRow
    | undefined;
  return {
    identityId,
    name: p?.name ?? "anonymous",
    avatarUrl: p?.avatar_blob ? `/media/thumb/${p.avatar_blob}` : undefined,
  };
}

export function videoRowToSummary(db: Db, row: VideoRow): VideoSummary {
  return {
    id: row.manifest_id,
    title: row.title,
    author: authorRef(db, row.author),
    thumbnailUrl: row.thumbnail_blob ? `/media/thumb/${row.thumbnail_blob}` : null,
    durationMs: row.duration_ms,
    createdAt: row.created_at,
    channelId: row.channel_id ?? undefined,
  };
}

export function videoRowToDetail(db: Db, row: VideoRow, recordJson: Record<string, unknown>): Video {
  const body = JSON.parse(row.body_json) as Record<string, unknown>;
  const original = (body.original as { blob?: string } | undefined) ?? {};
  const mp4Url = original.blob ? `/media/blob/${unhex(original.blob)}` : null;

  const hlsRenditions = db
    .prepare("SELECT rendition, height FROM hls_renditions WHERE manifest_id = ?")
    .all(row.manifest_id) as { rendition: string; height: number }[];
  const hlsUrl = hlsRenditions.length > 0 ? `/media/hls/${row.manifest_id}/master.m3u8` : null;

  const captions = ((body.captions as { blob?: string; language?: string }[] | undefined) ?? []).map((c) => ({
    language: c.language ?? "",
    url: c.blob ? `/media/blob/${unhex(c.blob)}` : "",
  }));

  const commentCount = (
    db.prepare("SELECT COUNT(*) AS n FROM comments WHERE manifest_id = ? AND retracted = 0").get(row.manifest_id) as {
      n: number;
    }
  ).n;
  const reactionRows = db
    .prepare("SELECT reaction, COUNT(*) AS n FROM reactions WHERE target_id = ? GROUP BY reaction")
    .all(row.manifest_id) as { reaction: string; n: number }[];
  const reactions: Record<string, number> = {};
  for (const r of reactionRows) reactions[r.reaction] = r.n;

  return {
    ...videoRowToSummary(db, row),
    description: row.description,
    tags: JSON.parse(row.tags_json) as string[],
    language: row.language ?? undefined,
    record: recordJson,
    recordCborUrl: `/api/records/${row.manifest_id}/cbor`,
    playback: {
      hlsUrl,
      mp4Url,
      renditions: hlsRenditions.map((r) => ({
        height: r.height,
        hlsUrl: `/media/hls/${row.manifest_id}/${r.rendition}/index.m3u8`,
      })),
    },
    captions,
    license: row.license,
    payment: ((body.payment as [number, string][] | undefined) ?? []),
    sponsorship: ((body.sponsorship as { startMs: number; endMs: number; label: string }[] | undefined) ?? []),
    counts: { comments: commentCount, reactions },
  };
}

interface CommentRow {
  record_id: string;
  author: string;
  text: string;
  parent: string | null;
  created_at: number;
}

export function commentRowToView(db: Db, row: CommentRow, recordJson: Record<string, unknown>): Comment {
  return {
    id: row.record_id,
    author: { identityId: row.author, name: authorRef(db, row.author).name },
    text: row.text,
    createdAt: row.created_at,
    parent: row.parent,
    record: recordJson,
  };
}

const CLAIM_KIND_NAMES: Record<number, string> = {
  48: "claim.author",
  49: "claim.license",
  50: "claim.transfer",
  51: "claim.dispute",
  64: "notice.takedown",
  65: "notice.counter",
};

interface ClaimRow {
  record_id: string;
  kind: number;
  author: string;
  target_record_id: string;
  body_json: string;
  created_at: number;
}

export function claimRowToView(row: ClaimRow): ClaimView {
  return {
    id: row.record_id,
    kind: row.kind,
    kindName: CLAIM_KIND_NAMES[row.kind] ?? String(row.kind),
    author: row.author,
    createdAt: row.created_at,
    body: JSON.parse(row.body_json) as Record<string, unknown>,
    targetRecordId: row.target_record_id,
  };
}

interface ReceiptRow {
  record_id: string;
  payer: string;
  amount: number;
  currency: string;
  rail: number;
  payee: string;
  proof: string | null;
  message: string | null;
  created_at: number;
}

export function receiptRowToView(row: ReceiptRow): ReceiptView {
  return {
    id: row.record_id,
    author: row.payer,
    createdAt: row.created_at,
    amount: row.amount,
    currency: row.currency,
    rail: row.rail,
    payee: row.payee,
    message: row.message ?? undefined,
    proof: row.proof ?? undefined,
  };
}
