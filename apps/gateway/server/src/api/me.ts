/**
 * `GET /api/me`, `POST /api/me/export`, `PUT /api/me/profile` (API.md).
 * Export is the non-negotiable identity-export path (spec 009 §5) —
 * see custody.ts for the password re-confirmation and rate limiting.
 */
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { invalid, notFound } from "../errors.ts";
import { requireUserId } from "../session.ts";
import { processRecord } from "../ingest.ts";

const ExportSchema = z.object({ password: z.string().min(1) });
const ProfileSchema = z.object({
  name: z.string().min(1).max(256),
  about: z.string().max(16384).optional(),
  avatarBlobId: z.string().regex(/^[0-9a-f]{64}$/i).optional(),
});

export function registerMeRoutes(app: FastifyInstance, deps: AppDeps): void {
  const { db } = deps;

  app.get("/api/me", async (request) => {
    const userId = requireUserId(request, db);
    const user = deps.custody.getUserById(userId);
    if (!user) throw notFound("user not found");
    const profile = db.prepare("SELECT name, about, avatar_blob FROM profiles WHERE identity_id = ?").get(user.identity_id) as
      | { name: string; about: string | null; avatar_blob: string | null }
      | undefined;
    return {
      handle: user.handle,
      identityId: user.identity_id,
      profile: profile
        ? { name: profile.name, about: profile.about ?? undefined, avatarUrl: profile.avatar_blob ? `/media/thumb/${profile.avatar_blob}` : undefined }
        : null,
      exportAvailable: true,
    };
  });

  app.post("/api/me/export", async (request) => {
    const userId = requireUserId(request, db);
    const parsed = ExportSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);
    return deps.custody.exportIdentity(userId, parsed.data.password);
  });

  app.put("/api/me/profile", async (request) => {
    const userId = requireUserId(request, db);
    const parsed = ProfileSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);
    const { name, about, avatarBlobId } = parsed.data;

    const body: Record<string, unknown> = { name };
    if (about) body.about = about;
    if (avatarBlobId) body.avatar = `hex:${avatarBlobId}`;

    const record = await deps.custody.signRecord(userId, { kind: 2, refs: [], body });
    const result = await processRecord(deps.ingest, record);
    if (!result.stored) throw invalid(`profile rejected: ${result.reason}`);
    deps.relays.publish(record);
    return { ok: true, recordId: result.recordId };
  });
}
