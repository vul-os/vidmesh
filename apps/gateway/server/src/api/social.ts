/**
 * `POST /api/videos/{id}/comments`, `/reactions`, `POST/DELETE /api/follow`
 * (API.md) — each signs and publishes a record as the caller's custodied
 * identity, then indexes it locally so it shows up immediately.
 */
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { invalid, notFound } from "../errors.ts";
import { requireUserId } from "../session.ts";
import { processRecord } from "../ingest.ts";
import { commentRowToView } from "./view-helpers.ts";

const CommentSchema = z.object({
  text: z.string().min(1).max(8192),
  parent: z.string().regex(/^[0-9a-f]{64}$/i).optional(),
});
const ReactionSchema = z.object({ reaction: z.string().min(1).max(32) });
const FollowSchema = z.object({ identityId: z.string().regex(/^[0-9a-f]{64}$/i) });

export function registerSocialRoutes(app: FastifyInstance, deps: AppDeps): void {
  const { db } = deps;

  app.post("/api/videos/:manifestId/comments", async (request) => {
    const userId = requireUserId(request, db);
    const { manifestId } = request.params as { manifestId: string };
    const parsed = CommentSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);
    const { text, parent } = parsed.data;

    const refs = [{ type: 0 as const, hash: manifestId }, ...(parent ? [{ type: 0 as const, hash: parent }] : [])];
    const record = await deps.custody.signRecord(userId, { kind: 32, refs, body: { text } });
    const result = await processRecord(deps.ingest, record);
    if (!result.stored || !result.recordId) throw invalid(`comment rejected: ${result.reason}`);
    deps.relays.publish(record);

    const recordRow = db.prepare("SELECT json FROM records WHERE id = ?").get(result.recordId) as { json: string } | undefined;
    const commentRow = db.prepare("SELECT * FROM comments WHERE record_id = ?").get(result.recordId) as never | undefined;
    if (!recordRow || !commentRow) {
      // Should be unreachable: processRecord just reported this comment as
      // stored. Fail loudly rather than throw a raw TypeError below.
      throw invalid("comment was indexed but could not be re-read");
    }
    return commentRowToView(db, commentRow, JSON.parse(recordRow.json));
  });

  app.post("/api/videos/:manifestId/reactions", async (request) => {
    const userId = requireUserId(request, db);
    const { manifestId } = request.params as { manifestId: string };
    const parsed = ReactionSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);

    const record = await deps.custody.signRecord(userId, {
      kind: 33,
      refs: [{ type: 0, hash: manifestId }],
      body: { reaction: parsed.data.reaction },
    });
    const result = await processRecord(deps.ingest, record);
    if (!result.stored) throw invalid(`reaction rejected: ${result.reason}`);
    deps.relays.publish(record);
    return { ok: true, recordId: result.recordId };
  });

  app.post("/api/follow", async (request) => {
    const userId = requireUserId(request, db);
    const parsed = FollowSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);

    const record = await deps.custody.signRecord(userId, {
      kind: 34,
      refs: [{ type: 0, hash: parsed.data.identityId }],
      body: {},
    });
    const result = await processRecord(deps.ingest, record);
    if (!result.stored) throw invalid(`follow rejected: ${result.reason}`);
    deps.relays.publish(record);
    return { ok: true, recordId: result.recordId };
  });

  app.delete("/api/follow/:identityId", async (request) => {
    const userId = requireUserId(request, db);
    const { identityId } = request.params as { identityId: string };
    const user = deps.custody.getUserById(userId);
    if (!user) throw notFound("user not found");

    const existing = db
      .prepare("SELECT record_id FROM follows WHERE author = ? AND target = ? AND retracted = 0 ORDER BY received_at DESC LIMIT 1")
      .get(user.identity_id, identityId) as { record_id: string } | undefined;
    if (!existing) return { ok: true }; // idempotent: already not following

    const record = await deps.custody.signRecord(userId, {
      kind: 18,
      refs: [{ type: 0, hash: existing.record_id }],
      body: { reason: "unfollow" },
    });
    const result = await processRecord(deps.ingest, record);
    if (!result.stored) throw invalid(`unfollow rejected: ${result.reason}`);
    deps.relays.publish(record);
    return { ok: true };
  });
}
