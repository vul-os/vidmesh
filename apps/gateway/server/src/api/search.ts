/**
 * `GET /api/search?q&limit` — title/description/tags search (API.md).
 * v1 implementation is a plain SQL LIKE scan (no FTS5 dependency); fine
 * at reference-gateway scale, called out in README as a place a
 * competing gateway would differentiate on product (spec 009 §6).
 */
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { invalid } from "../errors.ts";
import { videoRowToSummary } from "./view-helpers.ts";

const SearchQuerySchema = z.object({
  q: z.string().min(1),
  limit: z.coerce.number().int().min(1).max(100).default(20),
});

export function registerSearchRoutes(app: FastifyInstance, deps: AppDeps): void {
  const { db } = deps;

  app.get("/api/search", async (request) => {
    const query = SearchQuerySchema.safeParse(request.query);
    if (!query.success) throw invalid(query.error.message);
    const { q, limit } = query.data;
    const like = `%${q.replace(/[%_]/g, (c) => `\\${c}`)}%`;

    const rows = db
      .prepare(
        `SELECT * FROM videos
         WHERE retracted = 0
           AND NOT EXISTS (SELECT 1 FROM policy_denylist pd WHERE pd.scope = 'record' AND pd.value = manifest_id)
           AND (title LIKE ? ESCAPE '\\' OR description LIKE ? ESCAPE '\\' OR tags_json LIKE ? ESCAPE '\\')
         ORDER BY received_at DESC LIMIT ?`,
      )
      .all(like, like, like, limit) as never[];

    return { items: rows.map((r) => videoRowToSummary(db, r as never)) };
  });
}
