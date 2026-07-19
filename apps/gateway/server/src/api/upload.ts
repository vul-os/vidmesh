/**
 * `POST /api/upload` (multipart) and `GET /api/upload/{uploadId}` (API.md).
 * The route's only job is to land the file on disk and validate the
 * accompanying fields; the actual pipeline (CSAM → hash → transcode →
 * sign → publish) is upload.ts, kicked off in the background so the
 * response returns `{ uploadId }` immediately.
 */
import { createWriteStream } from "node:fs";
import { mkdir } from "node:fs/promises";
import { pipeline } from "node:stream/promises";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { ApiError, invalid, notFound } from "../errors.ts";
import { requireUserId } from "../session.ts";
import { createUploadRow, getUploadRow, runUploadPipeline, type UploadFormFields } from "../upload.ts";

const FieldsSchema = z.object({
  title: z.string().min(1).max(512),
  description: z.string().max(16384).optional(),
  tags: z.string().optional(), // comma-separated on the wire
  channelId: z.string().regex(/^[0-9a-f]{64}$/i).optional(),
  license: z.string().min(1),
});

export function registerUploadRoutes(app: FastifyInstance, deps: AppDeps): void {
  const { db, config } = deps;

  app.post("/api/upload", async (request) => {
    const userId = requireUserId(request, db);
    if (!config.uploadEnabled) throw new ApiError("upload_failed", "uploads are disabled on this gateway");

    const tmpDir = join(config.blobDir, "tmp");
    await mkdir(tmpDir, { recursive: true });
    const tempPath = join(tmpDir, randomUUID());

    const rawFields: Record<string, string> = {};
    let sawFile = false;

    for await (const part of request.parts()) {
      if (part.type === "file") {
        sawFile = true;
        try {
          await pipeline(part.file, createWriteStream(tempPath));
        } catch (err) {
          throw new ApiError("upload_failed", `failed to receive file: ${(err as Error).message}`);
        }
        if (part.file.truncated) {
          throw new ApiError("upload_failed", `file exceeds uploadMaxBytes (${config.uploadMaxBytes})`);
        }
      } else {
        rawFields[part.fieldname] = String(part.value);
      }
    }
    if (!sawFile) throw invalid("multipart request must include a file part");

    const parsed = FieldsSchema.safeParse(rawFields);
    if (!parsed.success) throw invalid(parsed.error.message);
    const fields: UploadFormFields = {
      title: parsed.data.title,
      description: parsed.data.description,
      tags: parsed.data.tags ? parsed.data.tags.split(",").map((t) => t.trim()).filter(Boolean) : undefined,
      channelId: parsed.data.channelId,
      license: parsed.data.license,
    };

    const uploadId = createUploadRow(db, userId);
    void runUploadPipeline(
      { db, config, csam: deps.csam, policy: deps.policy, custody: deps.custody, relays: deps.relays, log: deps.log },
      uploadId,
      userId,
      tempPath,
      fields,
    );
    return { uploadId };
  });

  app.get("/api/upload/:uploadId", async (request) => {
    const userId = requireUserId(request, db);
    const { uploadId } = request.params as { uploadId: string };
    const row = getUploadRow(db, uploadId);
    if (!row || row.user_id !== userId) throw notFound("upload not found");
    return {
      status: row.status,
      manifestId: row.manifest_id ?? undefined,
      progress: row.progress,
      error: row.error ?? undefined,
    };
  });
}
