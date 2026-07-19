/**
 * The dependency bag threaded through every API route module. Everything
 * is constructed once in main.ts and passed down — no module reaches for
 * a global singleton, which is what keeps api/*.ts testable with fastify
 * .inject() and a stubbed relay/ffmpeg.
 */
import type { Db } from "./db.ts";
import type { Config } from "./config.ts";
import type { PolicyEngine } from "./policy.ts";
import type { CsamMatcher } from "./csam.ts";
import type { CustodyService } from "./custody.ts";
import type { GatewayIdentityService } from "./gateway-identity.ts";
import type { RelayManager } from "./relay.ts";
import type { IngestDeps } from "./ingest.ts";

export interface AppDeps {
  db: Db;
  config: Config;
  policy: PolicyEngine;
  csam: CsamMatcher;
  custody: CustodyService;
  gatewayIdentity: GatewayIdentityService;
  relays: RelayManager;
  ingest: IngestDeps;
  log: (msg: string) => void;
}
