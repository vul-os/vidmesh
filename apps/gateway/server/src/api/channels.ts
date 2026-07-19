/**
 * `GET /api/channels/{identityId}` and `.../videos` (API.md).
 */
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { invalid, notFound } from "../errors.ts";
import { encodeCursor, decodeCursor, videoRowToSummary } from "./view-helpers.ts";
import type { Channel } from "../types.ts";

const ListQuerySchema = z.object({
  limit: z.coerce.number().int().min(1).max(100).default(20),
  cursor: z.string().optional(),
});

export function registerChannelRoutes(app: FastifyInstance, deps: AppDeps): void {
  const { db } = deps;

  app.get("/api/channels/:identityId", async (request): Promise<Channel> => {
    const { identityId } = request.params as { identityId: string };

    const profileRow = db.prepare("SELECT name, about, avatar_blob FROM profiles WHERE identity_id = ?").get(identityId) as
      | { name: string; about: string | null; avatar_blob: string | null }
      | undefined;
    const channelRows = db
      .prepare("SELECT record_id, title, description, avatar_blob, banner_blob FROM channels WHERE author = ? AND retracted = 0")
      .all(identityId) as { record_id: string; title: string; description: string; avatar_blob: string | null; banner_blob: string | null }[];
    const videoRows = db
      .prepare(
        `SELECT * FROM videos WHERE author = ? AND retracted = 0
         AND NOT EXISTS (SELECT 1 FROM policy_denylist pd WHERE pd.scope = 'record' AND pd.value = manifest_id)
         ORDER BY received_at DESC LIMIT 20`,
      )
      .all(identityId) as never[];

    if (!profileRow && channelRows.length === 0 && videoRows.length === 0) {
      throw notFound("identity has no known activity on this gateway");
    }

    return {
      identityId,
      profile: profileRow
        ? { name: profileRow.name, about: profileRow.about ?? undefined, avatarUrl: profileRow.avatar_blob ? `/media/thumb/${profileRow.avatar_blob}` : undefined }
        : null,
      channels: channelRows.map((c) => ({
        id: c.record_id,
        title: c.title,
        description: c.description || undefined,
        avatarUrl: c.avatar_blob ? `/media/thumb/${c.avatar_blob}` : undefined,
        bannerUrl: c.banner_blob ? `/media/thumb/${c.banner_blob}` : undefined,
      })),
      videos: videoRows.map((r) => videoRowToSummary(db, r as never)),
    };
  });

  app.get("/api/channels/:identityId/videos", async (request) => {
    const { identityId } = request.params as { identityId: string };
    const query = ListQuerySchema.safeParse(request.query);
    if (!query.success) throw invalid(query.error.message);
    const { limit, cursor } = query.data;
    const since = decodeCursor(cursor);

    const conditions = [
      "author = ?",
      "retracted = 0",
      "NOT EXISTS (SELECT 1 FROM policy_denylist pd WHERE pd.scope = 'record' AND pd.value = manifest_id)",
    ];
    const params: (string | number)[] = [identityId];
    if (since !== undefined) {
      conditions.push("received_at < ?");
      params.push(since);
    }
    params.push(limit + 1);
    const rows = db
      .prepare(`SELECT * FROM videos WHERE ${conditions.join(" AND ")} ORDER BY received_at DESC LIMIT ?`)
      .all(...params) as { received_at: number }[];

    const page = rows.slice(0, limit);
    const next = rows.length > limit ? encodeCursor(page[page.length - 1].received_at) : null;
    return { items: page.map((r) => videoRowToSummary(db, r as never)), next };
  });
}
