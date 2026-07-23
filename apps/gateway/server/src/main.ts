/**
 * Entry point: load config, open+migrate SQLite, start relay clients,
 * start fastify, and wire graceful shutdown + SIGHUP policy reload.
 */
import { loadConfig } from "./config.ts";
import { openDb } from "./db.ts";
import { PolicyEngine } from "./policy.ts";
import { StubMatcher } from "./csam.ts";
import { CustodyService } from "./custody.ts";
import { GatewayIdentityService } from "./gateway-identity.ts";
import { RelayManager } from "./relay.ts";
import { processRecord, type IngestDeps } from "./ingest.ts";
import { buildServer } from "./server.ts";
import type { AppDeps } from "./app-deps.ts";

async function main(): Promise<void> {
  const config = loadConfig();
  const log = (msg: string) => console.log(`[gateway] ${msg}`);

  const db = openDb(config.dbPath);
  const policy = new PolicyEngine(db, config.policyFilePath);
  const csam = new StubMatcher();

  if (csam instanceof StubMatcher) {
    console.warn(
      "\n".repeat(1) +
        "############################################################\n" +
        "# WARNING: StubMatcher is the active CsamMatcher.\n" +
        "# It ALWAYS returns { match: false } and reports nowhere.\n" +
        "# Running this gateway with real user traffic on StubMatcher\n" +
        "# is NON-COMPLIANT (spec 009-gateway.md §4, apps/gateway/\n" +
        "# server/CSAM.md) and is NOT covered by the Evermesh trademark\n" +
        "# program. Do not deploy this configuration publicly.\n" +
        "############################################################\n",
    );
  }

  const ingest: IngestDeps = { db, policy, csam, blobDir: config.blobDir, log };

  const relays = new RelayManager(
    db,
    config.relays,
    async (record) => {
      const result = await processRecord(ingest, record);
      if (!result.stored) log(`relay record not indexed: ${result.reason ?? "unknown"}`);
    },
    log,
  );

  const custody = new CustodyService(db, config, (record) => relays.publish(record), ingest);
  const gatewayIdentity = new GatewayIdentityService(db, config, (record) => relays.publish(record), ingest);
  await gatewayIdentity.ensure(); // publish the gateway's own operator identity on first boot

  relays.start();

  const deps: AppDeps = { db, config, policy, csam, custody, gatewayIdentity, relays, ingest, log };
  const app = await buildServer(config, deps);

  await app.listen({ port: config.port, host: config.host });
  log(`listening on http://${config.host}:${config.port}`);

  process.on("SIGHUP", () => {
    try {
      policy.reload();
      log("policy reloaded (SIGHUP)");
    } catch (err) {
      log(`policy reload failed: ${(err as Error).message}`);
    }
  });

  const shutdown = async (signal: string) => {
    log(`received ${signal}, shutting down`);
    relays.stop();
    await app.close();
    db.close();
    process.exit(0);
  };
  process.on("SIGINT", () => void shutdown("SIGINT"));
  process.on("SIGTERM", () => void shutdown("SIGTERM"));
}

main().catch((err) => {
  console.error("gateway-server: fatal error during startup:", err);
  process.exit(1);
});
