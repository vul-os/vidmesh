/**
 * Public read API for videos (API.md): listing, detail, comments, claims,
 * receipts, and the OG share-card fields.
 */
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { notFound, policyDenied, invalid } from "../errors.ts";
import { computeOgFields } from "../og.ts";
import {
  encodeCursor,
  decodeCursor,
  isRecordDenylisted,
  videoRowToSummary,
  videoRowToDetail,
  commentRowToView,
  claimRowToView,
  receiptRowToView,
} from "./view-helpers.ts";

const ListQuerySchema = z.object({
  limit: z.coerce.number().int().min(1).max(100).default(20),
  cursor: z.string().optional(),
  channel: z.string().optional(),
  author: z.string().optional(),
});

export function registerVideoRoutes(app: FastifyInstance, deps: AppDeps): void {
  const { db } = deps;

  app.get("/api/videos", async (request) => {
    const query = ListQuerySchema.safeParse(request.query);
    if (!query.success) throw invalid(query.error.message);
    const { limit, cursor, channel, author } = query.data;
    const since = decodeCursor(cursor);

    const conditions = ["v.retracted = 0", "NOT EXISTS (SELECT 1 FROM policy_denylist pd WHERE pd.scope = 'record' AND pd.value = v.manifest_id)"];
    const params: (string | number)[] = [];
    if (channel) {
      conditions.push("v.channel_id = ?");
      params.push(channel);
    }
    if (author) {
      conditions.push("v.author = ?");
      params.push(author);
    }
    if (since !== undefined) {
      conditions.push("v.received_at < ?");
      params.push(since);
    }
    const sql = `SELECT * FROM videos v WHERE ${conditions.join(" AND ")} ORDER BY v.received_at DESC LIMIT ?`;
    params.push(limit + 1);
    const rows = db.prepare(sql).all(...params) as { manifest_id: string; received_at: number }[];

    const page = rows.slice(0, limit);
    const next = rows.length > limit ? encodeCursor(page[page.length - 1].received_at) : null;
    return { items: page.map((r) => videoRowToSummary(db, r as never)), next };
  });

  app.get("/api/videos/:manifestId", async (request) => {
    const { manifestId } = request.params as { manifestId: string };
    const row = db.prepare("SELECT * FROM videos WHERE manifest_id = ?").get(manifestId) as
      | { manifest_id: string; retracted: number }
      | undefined;
    if (!row || row.retracted) throw notFound("video not found");
    if (isRecordDenylisted(db, manifestId)) throw policyDenied();

    const recordRow = db.prepare("SELECT json FROM records WHERE id = ?").get(manifestId) as { json: string } | undefined;
    if (!recordRow) throw notFound("video not found");
    return videoRowToDetail(db, row as never, JSON.parse(recordRow.json));
  });

  app.get("/api/videos/:manifestId/comments", async (request) => {
    const { manifestId } = request.params as { manifestId: string };
    if (isRecordDenylisted(db, manifestId)) throw policyDenied();
    const rows = db
      .prepare(
        `SELECT * FROM comments WHERE manifest_id = ? AND retracted = 0
         AND NOT EXISTS (SELECT 1 FROM policy_denylist pd WHERE pd.scope = 'record' AND pd.value = record_id)
         ORDER BY received_at ASC`,
      )
      .all(manifestId) as { record_id: string }[];
    const items = [];
    for (const r of rows) {
      const recordRow = db.prepare("SELECT json FROM records WHERE id = ?").get(r.record_id) as { json: string } | undefined;
      if (!recordRow) continue; // shouldn't happen (comment rows are always written with their record) — skip defensively
      items.push(commentRowToView(db, r as never, JSON.parse(recordRow.json)));
    }
    return { items };
  });

  app.get("/api/videos/:manifestId/claims", async (request) => {
    const { manifestId } = request.params as { manifestId: string };
    if (isRecordDenylisted(db, manifestId)) throw policyDenied();
    const rows = db
      .prepare(
        `SELECT * FROM claims WHERE subject_manifest_id = ? AND retracted = 0
         AND NOT EXISTS (SELECT 1 FROM policy_denylist pd WHERE pd.scope = 'record' AND pd.value = record_id)
         ORDER BY received_at ASC`,
      )
      .all(manifestId) as never[];
    return { items: rows.map((r) => claimRowToView(r as never)) };
  });

  app.get("/api/videos/:manifestId/receipts", async (request) => {
    const { manifestId } = request.params as { manifestId: string };
    if (isRecordDenylisted(db, manifestId)) throw policyDenied();
    const rows = db
      .prepare(
        `SELECT * FROM receipts WHERE manifest_id = ?
         AND NOT EXISTS (SELECT 1 FROM policy_denylist pd WHERE pd.scope = 'record' AND pd.value = record_id)
         ORDER BY received_at ASC`,
      )
      .all(manifestId) as never[];
    return { items: rows.map((r) => receiptRowToView(r as never)) };
  });

  app.get("/api/videos/:manifestId/og", async (request) => {
    const { manifestId } = request.params as { manifestId: string };
    const row = db.prepare("SELECT * FROM videos WHERE manifest_id = ? AND retracted = 0").get(manifestId) as
      | { manifest_id: string; title: string; description: string; thumbnail_blob: string | null }
      | undefined;
    if (!row || isRecordDenylisted(db, manifestId)) throw notFound("video not found");
    return computeOgFields(deps.config, {
      manifestId: row.manifest_id,
      title: row.title,
      description: row.description,
      thumbnailBlob: row.thumbnail_blob,
    });
  });
}
