/**
 * Content-addressed blob storage on disk: `<blobDir>/<ab>/<cd>/<hex>` (the
 * first two hex-byte pairs of the blob id as two levels of fan-out
 * directories, to keep any single directory from growing unbounded).
 * Shared by upload.ts (writes), media.ts (reads/serves), and ingest.ts
 * (existence checks for the CSAM index-time gate).
 */
import { createReadStream, existsSync, mkdirSync, renameSync, statSync } from "node:fs";
import { Readable } from "node:stream";
import { join } from "node:path";
import type { Db } from "./db.ts";

export function blobPath(blobDir: string, blobId: string): string {
  return join(blobDir, blobId.slice(0, 2), blobId.slice(2, 4), blobId);
}

export function blobExists(blobDir: string, blobId: string): boolean {
  return existsSync(blobPath(blobDir, blobId));
}

/** Move a completed temp file into the content-addressed store. */
export function commitBlob(blobDir: string, blobId: string, tempPath: string): string {
  const dest = blobPath(blobDir, blobId);
  mkdirSync(join(blobDir, blobId.slice(0, 2), blobId.slice(2, 4)), { recursive: true });
  if (!existsSync(dest)) renameSync(tempPath, dest);
  return dest;
}

export function blobSize(blobDir: string, blobId: string): number {
  return statSync(blobPath(blobDir, blobId)).size;
}

/** A fresh Node Readable for the blob's full bytes (no range applied). */
export function readBlob(blobDir: string, blobId: string, range?: { start: number; end: number }): Readable {
  const path = blobPath(blobDir, blobId);
  return range ? createReadStream(path, { start: range.start, end: range.end }) : createReadStream(path);
}

/** WHATWG stream view, for APIs (kernel hashing, CsamMatcher) that want one. */
export function toWebStream(readable: Readable): ReadableStream<Uint8Array> {
  return Readable.toWeb(readable) as ReadableStream<Uint8Array>;
}

export function recordBlob(db: Db, blobId: string, size: number, path: string, mime: string | null): void {
  db.prepare(
    `INSERT INTO blobs (id, size, path, mime, csam_checked, csam_match, created_at)
     VALUES (@id, @size, @path, @mime, 0, 0, @createdAt)
     ON CONFLICT(id) DO NOTHING`,
  ).run({ id: blobId, size, path, mime, createdAt: Date.now() });
}

export function getBlobRow(
  db: Db,
  blobId: string,
): { id: string; size: number; path: string; mime: string | null; csam_checked: number; csam_match: number } | undefined {
  return db.prepare("SELECT * FROM blobs WHERE id = ?").get(blobId) as never;
}

export function markBlobChecked(db: Db, blobId: string, match: boolean): void {
  db.prepare("UPDATE blobs SET csam_checked = 1, csam_match = ? WHERE id = ?").run(match ? 1 : 0, blobId);
}
