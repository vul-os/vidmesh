/**
 * The gateway's own operator identity, used to sign compliance records
 * (`notice.takedown` / `notice.counter`) as the gateway itself rather than
 * as any one user — API.md's compliance endpoints are explicit that these
 * are "signed records" produced by the gateway. Reuses custody.ts's
 * at-rest encryption (same "not hardware-grade" disclosure applies) but
 * is a singleton row, not a per-user one.
 */
import { Keypair, createRecord, identity as kernelIdentity, type RecordInit } from "@vidmesh/kernel";
import type { Db } from "./db.ts";
import type { Config } from "./config.ts";
import { deriveWrapKey, encryptSecret, decryptSecret } from "./custody.ts";
import { processRecord, type IngestDeps } from "./ingest.ts";

export class GatewayIdentityService {
  private identityId: string | undefined;

  constructor(
    private readonly db: Db,
    private readonly config: Config,
    private readonly publish: (record: Uint8Array) => void,
    private readonly ingest: IngestDeps,
  ) {}

  /** Create the gateway's identity on first use; idempotent thereafter. */
  async ensure(): Promise<string> {
    if (this.identityId) return this.identityId;
    const row = this.db.prepare("SELECT identity_id FROM gateway_identity WHERE id = 1").get() as
      | { identity_id: string }
      | undefined;
    if (row) {
      this.identityId = row.identity_id;
      return row.identity_id;
    }
    const keypair = await Keypair.generate();
    const { identityId, record: genesis } = await kernelIdentity.genesis(keypair, {
      contestWindow: this.config.custody.contestWindowSeconds,
    });
    const wrapKey = deriveWrapKey(this.config.custody.secret, identityId);
    const secretKeyEnc = encryptSecret(keypair.secret, wrapKey);
    this.db
      .prepare("INSERT INTO gateway_identity (id, identity_id, secret_key_enc, created_at) VALUES (1, ?, ?, ?)")
      .run(identityId, secretKeyEnc, Date.now());
    const indexed = await processRecord(this.ingest, genesis);
    if (!indexed.stored) {
      throw new Error(`failed to index the gateway's own genesis record: ${indexed.reason}`);
    }
    this.publish(genesis);
    this.identityId = identityId;
    return identityId;
  }

  async signRecord(init: RecordInit): Promise<Uint8Array> {
    const identityId = await this.ensure();
    const row = this.db.prepare("SELECT secret_key_enc FROM gateway_identity WHERE id = 1").get() as {
      secret_key_enc: string;
    };
    const wrapKey = deriveWrapKey(this.config.custody.secret, identityId);
    const secret = decryptSecret(row.secret_key_enc, wrapKey);
    const keypair = await Keypair.fromSecret(secret);
    return createRecord(keypair, identityId, init);
  }
}
