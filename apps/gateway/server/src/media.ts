/**
 * Blob serving with Range support, plus HLS playlist/segment routes and
 * thumbnails (API.md "Blob/media serving"). Master/media playlists are
 * generated on every request from `hls_segments`/`hls_renditions` — the
 * signed manifest only ever names one whole-file blob per rendition
 * (spec 004 §3), so segment lists are a gateway-serving-layer artifact,
 * not substrate state (see README "HLS packaging").
 *
 * Every blob read here runs through the CSAM gate first (CSAM.md: "MUST
 * be called before any blob is served"), with the verdict cached in
 * `blobs.csam_checked` so a blob already cleared at upload/ingest time
 * isn't re-hashed on every view.
 */
import type { FastifyInstance, FastifyReply, FastifyRequest } from "fastify";
import { statSync } from "node:fs";
import type { Db } from "./db.ts";
import type { Config } from "./config.ts";
import type { PolicyEngine } from "./policy.ts";
import type { CsamMatcher } from "./csam.ts";
import { blobPath, getBlobRow, markBlobChecked, readBlob, toWebStream } from "./blobstore.ts";
import { notFound } from "./errors.ts";

export interface MediaDeps {
  db: Db;
  config: Config;
  policy: PolicyEngine;
  csam: CsamMatcher;
}

async function ensureServable(deps: MediaDeps, blobId: string): Promise<{ size: number; mime: string | null } | null> {
  const decision = deps.policy.checkBlobHash(blobId);
  if (!decision.allowed) return null;

  const row = getBlobRow(deps.db, blobId);
  if (!row) return null;

  if (!row.csam_checked) {
    const size = statSync(blobPath(deps.config.blobDir, blobId)).size;
    const verdict = await deps.csam.checkBlob(toWebStream(readBlob(deps.config.blobDir, blobId)), { size, blobId });
    markBlobChecked(deps.db, blobId, verdict.match);
    if (verdict.match) {
      deps.policy.log("csam_match", "blob", blobId, "serve-time CSAM match", {
        listId: verdict.listId,
        reportingChannel: deps.csam.reportingChannel(),
      });
      return null;
    }
    return { size: row.size, mime: row.mime };
  }
  if (row.csam_match) return null;
  return { size: row.size, mime: row.mime };
}

function parseRange(header: string | undefined, size: number): { start: number; end: number } | null {
  if (!header?.startsWith("bytes=")) return null;
  const [startStr, endStr] = header.slice(6).split("-");

  // Suffix form "bytes=-500" (RFC 9110 §14.1.2): last N bytes of the
  // representation — startStr is empty, not "0".
  if (startStr === "" && endStr) {
    const suffixLength = parseInt(endStr, 10);
    if (Number.isNaN(suffixLength) || suffixLength <= 0) return null;
    const start = Math.max(0, size - suffixLength);
    return { start, end: size - 1 };
  }

  const start = startStr ? parseInt(startStr, 10) : 0;
  const end = endStr ? parseInt(endStr, 10) : size - 1;
  if (Number.isNaN(start) || Number.isNaN(end) || start > end || end >= size) return null;
  return { start, end };
}

async function serveBlob(deps: MediaDeps, request: FastifyRequest, reply: FastifyReply, blobId: string, mimeOverride?: string) {
  const info = await ensureServable(deps, blobId);
  if (!info) throw notFound();

  const size = info.size;
  const range = parseRange(request.headers.range, size);
  const mime = mimeOverride ?? info.mime ?? "application/octet-stream";

  reply.header("accept-ranges", "bytes");
  reply.header("content-type", mime);
  if (range) {
    reply.code(206);
    reply.header("content-range", `bytes ${range.start}-${range.end}/${size}`);
    reply.header("content-length", String(range.end - range.start + 1));
    return reply.send(readBlob(deps.config.blobDir, blobId, range));
  }
  reply.header("content-length", String(size));
  return reply.send(readBlob(deps.config.blobDir, blobId));
}

function getInitSegment(db: Db, manifestId: string, rendition: string): string | undefined {
  const row = db
    .prepare("SELECT blob_id FROM hls_segments WHERE manifest_id = ? AND rendition = ? AND is_init = 1")
    .get(manifestId, rendition) as { blob_id: string } | undefined;
  return row?.blob_id;
}

function listMediaSegments(db: Db, manifestId: string, rendition: string): { blob_id: string; duration_ms: number }[] {
  return db
    .prepare("SELECT blob_id, duration_ms FROM hls_segments WHERE manifest_id = ? AND rendition = ? AND is_init = 0 ORDER BY seq")
    .all(manifestId, rendition) as { blob_id: string; duration_ms: number }[];
}

export function registerMediaRoutes(app: FastifyInstance, deps: MediaDeps): void {
  app.get("/media/blob/:blobId", async (request, reply) => {
    const { blobId } = request.params as { blobId: string };
    return serveBlob(deps, request, reply, blobId);
  });

  app.get("/media/thumb/:blobId", async (request, reply) => {
    const { blobId } = request.params as { blobId: string };
    return serveBlob(deps, request, reply, blobId, "image/jpeg");
  });

  app.get("/media/hls/:manifestId/master.m3u8", async (request, reply) => {
    const { manifestId } = request.params as { manifestId: string };
    const renditions = deps.db
      .prepare("SELECT rendition, height, width, bandwidth, codec FROM hls_renditions WHERE manifest_id = ?")
      .all(manifestId) as { rendition: string; height: number; width: number; bandwidth: number; codec: string }[];
    if (renditions.length === 0) throw notFound("no HLS renditions for this manifest");

    const lines = ["#EXTM3U", "#EXT-X-VERSION:7"];
    for (const r of renditions) {
      const resolution = r.width > 0 ? `,RESOLUTION=${r.width}x${r.height}` : "";
      lines.push(`#EXT-X-STREAM-INF:BANDWIDTH=${r.bandwidth}${resolution},CODECS="${r.codec}"`);
      lines.push(`${r.rendition}/index.m3u8`);
    }
    reply.header("content-type", "application/vnd.apple.mpegurl");
    return reply.send(lines.join("\n") + "\n");
  });

  app.get("/media/hls/:manifestId/:rendition/index.m3u8", async (request, reply) => {
    const { manifestId, rendition } = request.params as { manifestId: string; rendition: string };
    const init = getInitSegment(deps.db, manifestId, rendition);
    const segments = listMediaSegments(deps.db, manifestId, rendition);
    if (!init || segments.length === 0) throw notFound("rendition not found");

    const targetDuration = Math.ceil(Math.max(...segments.map((s) => s.duration_ms), 1000) / 1000);
    const lines = [
      "#EXTM3U",
      "#EXT-X-VERSION:7",
      `#EXT-X-TARGETDURATION:${targetDuration}`,
      "#EXT-X-PLAYLIST-TYPE:VOD",
      `#EXT-X-MAP:URI="init.m4s"`,
    ];
    segments.forEach((seg, i) => {
      lines.push(`#EXTINF:${(seg.duration_ms / 1000).toFixed(3)},`);
      lines.push(`${String(i).padStart(3, "0")}.m4s`);
    });
    lines.push("#EXT-X-ENDLIST");

    reply.header("content-type", "application/vnd.apple.mpegurl");
    return reply.send(lines.join("\n") + "\n");
  });

  app.get("/media/hls/:manifestId/:rendition/:segment", async (request, reply) => {
    const { manifestId, rendition, segment } = request.params as { manifestId: string; rendition: string; segment: string };
    const name = segment.endsWith(".m4s") ? segment.slice(0, -4) : segment;

    if (name === "init") {
      const blobId = getInitSegment(deps.db, manifestId, rendition);
      if (!blobId) throw notFound();
      return serveBlob(deps, request, reply, blobId, "video/mp4");
    }
    const index = parseInt(name, 10);
    const segments = listMediaSegments(deps.db, manifestId, rendition);
    const target = segments[index];
    if (Number.isNaN(index) || !target) throw notFound();
    return serveBlob(deps, request, reply, target.blob_id, "video/iso.segment");
  });
}
