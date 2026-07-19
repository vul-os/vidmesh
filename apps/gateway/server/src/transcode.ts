/**
 * ffmpeg/ffprobe invocation via child_process. Every function here is only
 * ever called when `config.ffmpegPath` is set — the upload pipeline
 * (upload.ts) checks that once and degrades to original-only playback
 * when it's absent, per the build plan's "feature-degrades gracefully"
 * requirement. Nothing in this file is exercised by the test suite (no
 * ffmpeg in CI/dev sandboxes); it's structured so every operation is one
 * narrow, independently-callable function.
 */
import { execFile } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export interface ProbeResult {
  codec: string;
  width: number;
  height: number;
  durationMs: number;
}

export interface RenditionTarget {
  name: string;
  height: number;
}

export const RENDITION_TARGETS: RenditionTarget[] = [
  { name: "720p", height: 720 },
  { name: "480p", height: 480 },
];

/**
 * Derive the ffprobe path alongside ffmpeg if not configured explicitly:
 * replace the last occurrence of "ffmpeg" in the binary's basename with
 * "ffprobe" (handles "ffmpeg", "ffmpeg.exe", "ffmpeg7", etc.); falls back
 * to the literal name "ffprobe" in the same directory if "ffmpeg" isn't
 * part of the configured binary's name at all.
 */
export function defaultFfprobePath(ffmpegPath: string, configured?: string): string {
  if (configured) return configured;
  const dir = dirname(ffmpegPath);
  const base = basename(ffmpegPath);
  const probeBase = base.includes("ffmpeg") ? base.replace(/ffmpeg/g, "ffprobe") : "ffprobe";
  return join(dir, probeBase);
}

export async function probe(ffprobePath: string, filePath: string): Promise<ProbeResult> {
  const { stdout } = await execFileAsync(ffprobePath, [
    "-v",
    "error",
    "-select_streams",
    "v:0",
    "-show_entries",
    "stream=codec_name,width,height:format=duration",
    "-of",
    "json",
    filePath,
  ]);
  const parsed = JSON.parse(stdout) as {
    streams?: { codec_name?: string; width?: number; height?: number }[];
    format?: { duration?: string };
  };
  const stream = parsed.streams?.[0];
  return {
    codec: stream?.codec_name ?? "unknown",
    width: stream?.width ?? 0,
    height: stream?.height ?? 0,
    durationMs: Math.round(parseFloat(parsed.format?.duration ?? "0") * 1000),
  };
}

/** Transcode to a whole-file MP4 at the target height; returns the probed result. */
export async function transcodeRendition(
  ffmpegPath: string,
  ffprobePath: string,
  srcPath: string,
  outPath: string,
  targetHeight: number,
): Promise<ProbeResult & { bitrate: number }> {
  mkdirSync(dirname(outPath), { recursive: true });
  await execFileAsync(ffmpegPath, [
    "-y",
    "-i",
    srcPath,
    "-vf",
    `scale=-2:${targetHeight}`,
    "-c:v",
    "libx264",
    "-c:a",
    "aac",
    "-movflags",
    "+faststart",
    outPath,
  ]);
  const result = await probe(ffprobePath, outPath);
  const sizeBytes = readFileSync(outPath).length;
  const bitrate =
    result.durationMs > 0 ? Math.round((sizeBytes * 8) / (result.durationMs / 1000)) : 0;
  return { ...result, bitrate };
}

export async function generateThumbnail(ffmpegPath: string, srcPath: string, outPath: string): Promise<void> {
  mkdirSync(dirname(outPath), { recursive: true });
  await execFileAsync(ffmpegPath, [
    "-y",
    "-i",
    srcPath,
    "-ss",
    "1",
    "-frames:v",
    "1",
    "-f",
    "image2",
    outPath,
  ]);
}

export interface HlsSegment {
  path: string;
  durationMs: number;
  isInit: boolean;
}

/**
 * Package a rendition into fMP4 HLS segments. The playlist ffmpeg writes
 * is discarded — media.ts regenerates playlists from `hls_segments` rows
 * on every request — but its `#EXTINF` durations are parsed out here so
 * we know each segment's actual duration (see README "HLS packaging" for
 * why segment lists live in our own tables rather than the signed
 * manifest, which only ever names one whole-file blob per rendition).
 */
export async function packageHls(ffmpegPath: string, renditionPath: string, outDir: string): Promise<HlsSegment[]> {
  mkdirSync(outDir, { recursive: true });
  const initName = "init.mp4";
  const segPattern = "seg_%03d.m4s";
  const playlistPath = join(outDir, "playlist.m3u8");
  await execFileAsync(ffmpegPath, [
    "-y",
    "-i",
    renditionPath,
    "-c",
    "copy",
    "-f",
    "hls",
    "-hls_time",
    "6",
    "-hls_segment_type",
    "fmp4",
    "-hls_fmp4_init_filename",
    initName,
    "-hls_segment_filename",
    join(outDir, segPattern),
    playlistPath,
  ]);

  const playlistText = readFileSync(playlistPath, "utf-8");
  const durations: number[] = [];
  for (const line of playlistText.split("\n")) {
    const m = /^#EXTINF:([0-9.]+),?/.exec(line.trim());
    if (m) durations.push(Math.round(parseFloat(m[1]) * 1000));
  }

  const segmentFiles = readdirSync(outDir)
    .filter((f) => f.startsWith("seg_") && f.endsWith(".m4s"))
    .sort();

  const segments: HlsSegment[] = [{ path: join(outDir, initName), durationMs: 0, isInit: true }];
  segmentFiles.forEach((file, i) => {
    segments.push({ path: join(outDir, file), durationMs: durations[i] ?? 6000, isInit: false });
  });
  return segments;
}
