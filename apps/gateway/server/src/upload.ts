/**
 * Upload pipeline (build plan §9): temp file → CSAM check → hash → store
 * → probe → (if ffmpeg) transcode + thumbnail + HLS package + sign
 * derivations → build + sign manifest → publish → mark published.
 *
 * Runs asynchronously after the HTTP request that accepted the multipart
 * body returns `{ uploadId }`; progress is polled via GET
 * /api/upload/{uploadId} (API.md), backed by the `uploads` table.
 */
import { createReadStream, existsSync, statSync, unlinkSync } from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { hashBlobStream } from "@vidmesh/kernel";
import type { Db } from "./db.ts";
import type { Config } from "./config.ts";
import type { PolicyEngine } from "./policy.ts";
import type { CsamMatcher } from "./csam.ts";
import type { CustodyService } from "./custody.ts";
import type { RelayManager } from "./relay.ts";
import { blobPath, commitBlob, recordBlob, markBlobChecked, toWebStream } from "./blobstore.ts";
import { probe, defaultFfprobePath, generateThumbnail, transcodeRendition, packageHls, RENDITION_TARGETS } from "./transcode.ts";
import { processRecord, type IngestDeps } from "./ingest.ts";

function hexField(hex: string): string {
  return `hex:${hex}`;
}

export interface UploadFormFields {
  title: string;
  description?: string;
  tags?: string[];
  channelId?: string;
  license: string;
}

export interface UploadDeps {
  db: Db;
  config: Config;
  csam: CsamMatcher;
  policy: PolicyEngine;
  custody: CustodyService;
  relays: RelayManager;
  log: (msg: string) => void;
}

export interface UploadRow {
  id: string;
  user_id: number;
  status: "processing" | "published" | "failed";
  progress: number;
  manifest_id: string | null;
  error: string | null;
  created_at: number;
  updated_at: number;
}

export function createUploadRow(db: Db, userId: number): string {
  const id = randomUUID();
  db.prepare(
    `INSERT INTO uploads (id, user_id, status, progress, created_at, updated_at) VALUES (?, ?, 'processing', 0, ?, ?)`,
  ).run(id, userId, Date.now(), Date.now());
  return id;
}

export function getUploadRow(db: Db, id: string): UploadRow | undefined {
  return db.prepare("SELECT * FROM uploads WHERE id = ?").get(id) as UploadRow | undefined;
}

function updateUpload(
  db: Db,
  id: string,
  patch: { status?: string; progress?: number; manifestId?: string; error?: string },
): void {
  const current = getUploadRow(db, id);
  if (!current) return;
  db.prepare(
    `UPDATE uploads SET status = @status, progress = @progress, manifest_id = @manifestId, error = @error, updated_at = @updatedAt WHERE id = @id`,
  ).run({
    id,
    status: patch.status ?? current.status,
    progress: patch.progress ?? current.progress,
    manifestId: patch.manifestId ?? current.manifest_id,
    error: patch.error ?? current.error,
    updatedAt: Date.now(),
  });
}

async function checkCsam(deps: UploadDeps, path: string, label: string): Promise<boolean> {
  const size = statSync(path).size;
  const verdict = await deps.csam.checkBlob(toWebStream(createReadStream(path)), { size, blobId: label });
  if (verdict.match) {
    deps.policy.log("csam_match", "blob", label, "upload-time CSAM match", {
      listId: verdict.listId,
      reportingChannel: deps.csam.reportingChannel(),
    });
    return false;
  }
  return true;
}

interface StoredBlob {
  id: string;
  size: number;
  chunkRoot: string | null;
  path: string;
}

async function hashAndStore(db: Db, blobDir: string, tempPath: string, mime: string | null): Promise<StoredBlob> {
  const summary = await hashBlobStream(toWebStream(createReadStream(tempPath)));
  const finalPath = commitBlob(blobDir, summary.id, tempPath);
  recordBlob(db, summary.id, summary.size, finalPath, mime);
  markBlobChecked(db, summary.id, false); // just cleared above by the caller
  return { id: summary.id, size: summary.size, chunkRoot: summary.chunkRoot, path: finalPath };
}

interface PendingRendition {
  name: string;
  height: number;
  width: number;
  bandwidth: number;
  codec: string;
  segments: { blobId: string; durationMs: number; isInit: boolean }[];
}

/** Runs the full pipeline; never throws — failures are recorded on the row. */
export async function runUploadPipeline(
  deps: UploadDeps,
  uploadId: string,
  userId: number,
  tempPath: string,
  fields: UploadFormFields,
): Promise<void> {
  try {
    await runPipelineInner(deps, uploadId, userId, tempPath, fields);
  } catch (err) {
    updateUpload(deps.db, uploadId, { status: "failed", error: (err as Error).message });
    if (existsSync(tempPath)) {
      try {
        unlinkSync(tempPath);
      } catch {
        /* best-effort cleanup */
      }
    }
  }
}

async function runPipelineInner(
  deps: UploadDeps,
  uploadId: string,
  userId: number,
  tempPath: string,
  fields: UploadFormFields,
): Promise<void> {
  const size = statSync(tempPath).size;
  if (size > deps.config.uploadMaxBytes) {
    throw new Error(`file exceeds uploadMaxBytes (${deps.config.uploadMaxBytes})`);
  }

  if (!(await checkCsam(deps, tempPath, `upload:${uploadId}:original`))) {
    unlinkSync(tempPath);
    updateUpload(deps.db, uploadId, { status: "failed", error: "rejected: matched a known CSAM hash list" });
    return;
  }
  updateUpload(deps.db, uploadId, { progress: 10 });

  const original = await hashAndStore(deps.db, deps.config.blobDir, tempPath, null);
  updateUpload(deps.db, uploadId, { progress: 25 });

  let probed = { codec: "unknown", width: 0, height: 0, durationMs: 0 };
  const ffmpegPath = deps.config.ffmpegPath;
  const ffprobePath = ffmpegPath ? defaultFfprobePath(ffmpegPath, deps.config.ffprobePath) : undefined;
  if (ffprobePath) {
    try {
      probed = await probe(ffprobePath, original.path);
    } catch (err) {
      deps.log(`ffprobe failed, continuing without metadata: ${(err as Error).message}`);
    }
  }
  updateUpload(deps.db, uploadId, { progress: 35 });

  const originalBody: Record<string, unknown> = {
    blob: hexField(original.id),
    size: original.size,
    codec: probed.codec,
    duration: probed.durationMs,
    width: probed.width,
    height: probed.height,
  };
  if (original.chunkRoot) originalBody.chunk_root = hexField(original.chunkRoot);

  let thumbnailBlobId: string | undefined;
  const renditionsBody: Record<string, unknown>[] = [];
  const pendingHls: PendingRendition[] = [];

  if (ffmpegPath && ffprobePath) {
    thumbnailBlobId = await tryGenerateThumbnail(deps, uploadId, ffmpegPath, original.path);
    updateUpload(deps.db, uploadId, { progress: 50 });

    let step = 0;
    for (const target of RENDITION_TARGETS) {
      step++;
      if (probed.height && probed.height < target.height) continue; // never upscale
      try {
        const outPath = join(deps.config.blobDir, "tmp", `${uploadId}-${target.name}.mp4`);
        const transcoded = await transcodeRendition(ffmpegPath, ffprobePath, original.path, outPath, target.height);
        if (!(await checkCsam(deps, outPath, `upload:${uploadId}:${target.name}`))) {
          unlinkSync(outPath);
          continue;
        }
        const stored = await hashAndStore(deps.db, deps.config.blobDir, outPath, "video/mp4");

        const derivation = await deps.custody.signDerivationFor(userId, {
          originalBlobId: original.id,
          renditionBlobId: stored.id,
          codec: transcoded.codec,
          width: transcoded.width,
          height: transcoded.height,
          bitrate: transcoded.bitrate,
        });

        renditionsBody.push({
          blob: hexField(stored.id),
          size: stored.size,
          ...(stored.chunkRoot ? { chunk_root: hexField(stored.chunkRoot) } : {}),
          codec: transcoded.codec,
          duration: transcoded.durationMs,
          width: transcoded.width,
          height: transcoded.height,
          bitrate: transcoded.bitrate,
          produced_by: [hexField(derivation.identityId), hexField(derivation.publicKeyHex)],
          derivation_sig: hexField(derivation.signatureHex),
        });

        const hlsDir = join(deps.config.blobDir, "hls", stored.id);
        const segments = await packageHls(ffmpegPath, stored.path, hlsDir);
        const storedSegments: PendingRendition["segments"] = [];
        for (const seg of segments) {
          const segStored = await hashAndStore(deps.db, deps.config.blobDir, seg.path, seg.isInit ? "video/mp4" : "video/iso.segment");
          storedSegments.push({ blobId: segStored.id, durationMs: seg.durationMs, isInit: seg.isInit });
        }
        pendingHls.push({
          name: target.name,
          height: transcoded.height,
          width: transcoded.width,
          bandwidth: transcoded.bitrate,
          codec: transcoded.codec,
          segments: storedSegments,
        });
      } catch (err) {
        deps.log(`rendition ${target.name} failed, skipping: ${(err as Error).message}`);
      }
      updateUpload(deps.db, uploadId, { progress: 50 + step * 10 });
    }
  }

  const manifestBody: Record<string, unknown> = {
    title: fields.title,
    description: fields.description ?? "",
    tags: fields.tags ?? [],
    original: originalBody,
    renditions: renditionsBody,
    captions: [],
    license: fields.license,
    sponsorship: [],
    payment: [],
  };
  if (thumbnailBlobId) manifestBody.thumbnail = hexField(thumbnailBlobId);
  if (deps.config.publicBaseUrl) {
    manifestBody.hints = [[1, `${deps.config.publicBaseUrl}/media/blob/`]];
  }
  const refs = fields.channelId ? [{ type: 0 as const, hash: fields.channelId }] : [];

  updateUpload(deps.db, uploadId, { progress: 90 });
  const manifestBytes = await deps.custody.signRecord(userId, { kind: 16, refs, body: manifestBody });

  const ingestDeps: IngestDeps = { db: deps.db, policy: deps.policy, csam: deps.csam, blobDir: deps.config.blobDir, log: deps.log };
  const result = await processRecord(ingestDeps, manifestBytes);
  if (!result.stored || !result.recordId) {
    throw new Error(`failed to index the manifest we just signed: ${result.reason}`);
  }
  const manifestId = result.recordId;

  for (const p of pendingHls) {
    deps.db
      .prepare(
        `INSERT INTO hls_renditions (manifest_id, rendition, height, width, bandwidth, codec) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(manifest_id, rendition) DO NOTHING`,
      )
      .run(manifestId, p.name, p.height, p.width, p.bandwidth, p.codec);
    p.segments.forEach((seg, seq) => {
      deps.db
        .prepare(
          `INSERT INTO hls_segments (manifest_id, rendition, seq, blob_id, duration_ms, is_init) VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(manifest_id, rendition, seq) DO NOTHING`,
        )
        .run(manifestId, p.name, seq, seg.blobId, seg.durationMs, seg.isInit ? 1 : 0);
    });
  }

  deps.relays.publish(manifestBytes);
  updateUpload(deps.db, uploadId, { status: "published", manifestId, progress: 100 });
}

async function tryGenerateThumbnail(
  deps: UploadDeps,
  uploadId: string,
  ffmpegPath: string,
  originalPath: string,
): Promise<string | undefined> {
  try {
    const thumbTemp = join(deps.config.blobDir, "tmp", `${uploadId}-thumb.jpg`);
    await generateThumbnail(ffmpegPath, originalPath, thumbTemp);
    if (!(await checkCsam(deps, thumbTemp, `upload:${uploadId}:thumb`))) {
      unlinkSync(thumbTemp);
      return undefined;
    }
    const stored = await hashAndStore(deps.db, deps.config.blobDir, thumbTemp, "image/jpeg");
    return stored.id;
  } catch (err) {
    deps.log(`thumbnail generation failed, continuing without one: ${(err as Error).message}`);
    return undefined;
  }
}

export function blobTempPath(blobDir: string, name: string): string {
  return join(blobDir, "tmp", name);
}

// Re-exported for callers that need the on-disk path of an already-stored
// blob (e.g. tests asserting content-addressing).
export { blobPath };
