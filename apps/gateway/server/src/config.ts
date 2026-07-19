/**
 * Config: JSON file + environment overrides, zod-validated (build plan §9).
 *
 * Load order: read the JSON file at `path` (defaults to
 * `GATEWAY_CONFIG` env var or `config.json` in the cwd), then apply a
 * fixed set of env-var overrides, then validate. The result is a frozen,
 * immutable object passed by dependency injection — nothing in this
 * module reaches for a global singleton.
 */
import { readFileSync } from "node:fs";
import { z } from "zod";

const ConfigSchema = z.object({
  port: z.number().int().positive().default(8080),
  host: z.string().default("0.0.0.0"),
  dbPath: z.string().min(1),
  blobDir: z.string().min(1),
  relays: z.array(z.string().url()).default([]),
  policyFilePath: z.string().min(1),
  sessionSecret: z.string().min(32, "sessionSecret must be at least 32 characters"),
  custody: z
    .object({
      // Derives the AES key that wraps custodial secret keys. Kept
      // separate from sessionSecret so rotating cookie signing doesn't
      // also invalidate custodied key material. See custody.ts for the
      // "not hardware-grade" disclosure this backs.
      secret: z.string().min(32, "custody.secret must be at least 32 characters"),
      contestWindowSeconds: z.number().int().positive().default(604_800),
    })
    .default({ secret: "", contestWindowSeconds: 604_800 }),
  ffmpegPath: z.string().optional(),
  ffprobePath: z.string().optional(),
  uploadMaxBytes: z.number().int().positive().default(4 * 1024 * 1024 * 1024),
  gatewayName: z.string().default("vidmesh-reference-gateway"),
  gatewayDescription: z.string().default("A reference Vidmesh gateway."),
  publicBaseUrl: z.string().url().optional(),
  uploadEnabled: z.boolean().default(true),
});

export type Config = z.infer<typeof ConfigSchema>;

/** Raw (pre-validation) config shape, for callers constructing in tests. */
export type ConfigInput = z.input<typeof ConfigSchema>;

function applyEnvOverrides(raw: Record<string, unknown>): Record<string, unknown> {
  const out = { ...raw };
  const env = process.env;
  if (env.GATEWAY_PORT) out.port = Number(env.GATEWAY_PORT);
  if (env.GATEWAY_HOST) out.host = env.GATEWAY_HOST;
  if (env.GATEWAY_DB_PATH) out.dbPath = env.GATEWAY_DB_PATH;
  if (env.GATEWAY_BLOB_DIR) out.blobDir = env.GATEWAY_BLOB_DIR;
  if (env.GATEWAY_RELAYS) out.relays = env.GATEWAY_RELAYS.split(",").map((s) => s.trim());
  if (env.GATEWAY_POLICY_FILE) out.policyFilePath = env.GATEWAY_POLICY_FILE;
  if (env.GATEWAY_SESSION_SECRET) out.sessionSecret = env.GATEWAY_SESSION_SECRET;
  if (env.GATEWAY_CUSTODY_SECRET) {
    out.custody = { ...(out.custody as object), secret: env.GATEWAY_CUSTODY_SECRET };
  }
  if (env.GATEWAY_FFMPEG_PATH) out.ffmpegPath = env.GATEWAY_FFMPEG_PATH;
  if (env.GATEWAY_FFPROBE_PATH) out.ffprobePath = env.GATEWAY_FFPROBE_PATH;
  if (env.GATEWAY_UPLOAD_MAX_BYTES) out.uploadMaxBytes = Number(env.GATEWAY_UPLOAD_MAX_BYTES);
  if (env.GATEWAY_PUBLIC_BASE_URL) out.publicBaseUrl = env.GATEWAY_PUBLIC_BASE_URL;
  if (env.GATEWAY_NAME) out.gatewayName = env.GATEWAY_NAME;
  if (env.GATEWAY_UPLOAD_ENABLED) out.uploadEnabled = env.GATEWAY_UPLOAD_ENABLED !== "false";
  return out;
}

/** Load config from a JSON file plus env overrides, validating with zod. */
export function loadConfig(path?: string): Config {
  const configPath = path ?? process.env.GATEWAY_CONFIG ?? "config.json";
  const raw = JSON.parse(readFileSync(configPath, "utf-8")) as Record<string, unknown>;
  return parseConfig(applyEnvOverrides(raw));
}

/**
 * Validate an in-memory config object (used directly by tests). Zod
 * already enforces `custody.secret`'s 32-character minimum — including
 * when `custody` is omitted entirely, since `.default(...)` values are
 * re-validated against the inner schema — so there is no separate
 * manual check needed here.
 */
export function parseConfig(raw: unknown): Config {
  return Object.freeze(ConfigSchema.parse(raw));
}
