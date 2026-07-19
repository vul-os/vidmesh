/**
 * The policy engine (spec 009-gateway.md §1, §3). Local policy is
 * "absolute and instant": allow/deny by identity, blob hash, or kind is
 * gateway configuration, never a protocol action, and every decision is
 * cheap — static rules live in memory as Sets (reloaded whole on SIGHUP,
 * never touched per-request), and the one dynamic source (feed.takedown
 * de-indexing) is a primary-key lookup in `policy_denylist`.
 *
 * Every denial and every feed-driven de-index is logged to `policy_log`
 * with a timestamp and reason (spec 009 §1: "every gateway... SHOULD log
 * every selection action locally for its own audit" — we make it MUST,
 * it's nearly free).
 */
import { readFileSync } from "node:fs";
import { z } from "zod";
import type { Db } from "./db.ts";

const PolicyFileSchema = z.object({
  name: z.string().min(1),
  description: z.string().default(""),
  moderationPolicyHtml: z.string().default("<p>No moderation policy has been published.</p>"),
  denyIdentities: z.array(z.string()).default([]),
  denyBlobHashes: z.array(z.string()).default([]),
  denyRecordIds: z.array(z.string()).default([]),
  denyKinds: z.array(z.number().int()).default([]),
  geoBlocks: z
    .array(z.object({ hash: z.string(), countries: z.array(z.string()) }))
    .default([]),
  feeds: z.array(z.object({ feed: z.string(), publisher: z.string() })).default([]),
});

export type PolicyFile = z.infer<typeof PolicyFileSchema>;

export interface Ref {
  type: 0 | 1;
  hash: string;
}

export interface PolicyDecision {
  allowed: boolean;
  reason?: string;
}

export interface GeoBlockEntry {
  hash: string;
  countries: string[];
}

/** Feed batch entry per spec 003 §6.7: `[ref_type, hash, reason, notice?]`. */
export interface FeedTakedownBody {
  feed: string;
  seq: number;
  add: [number, string, string, string?][];
  remove: [number, string][];
}

export class PolicyEngine {
  private file!: PolicyFile;
  private denyIdentities = new Set<string>();
  private denyBlobHashes = new Set<string>();
  private denyRecordIds = new Set<string>();
  private denyKinds = new Set<number>();
  private geoBlocks = new Map<string, string[]>();

  private readonly checkDenylistStmt;
  private readonly insertDenylistStmt;
  private readonly deleteDenylistStmt;
  private readonly logStmt;
  private readonly batchSeenStmt;
  private readonly recordBatchStmt;
  private readonly upsertFeedStateStmt;

  constructor(
    private readonly db: Db,
    private readonly filePath: string,
  ) {
    this.checkDenylistStmt = db.prepare(
      "SELECT 1 FROM policy_denylist WHERE scope = ? AND value = ?",
    );
    this.insertDenylistStmt = db.prepare(
      `INSERT INTO policy_denylist (scope, value, reason, feed, notice, ts)
       VALUES (@scope, @value, @reason, @feed, @notice, @ts)
       ON CONFLICT(scope, value) DO UPDATE SET reason=excluded.reason, feed=excluded.feed,
         notice=excluded.notice, ts=excluded.ts`,
    );
    this.deleteDenylistStmt = db.prepare(
      "DELETE FROM policy_denylist WHERE scope = ? AND value = ?",
    );
    this.logStmt = db.prepare(
      `INSERT INTO policy_log (ts, action, subject_type, subject, reason, detail)
       VALUES (@ts, @action, @subjectType, @subject, @reason, @detail)`,
    );
    this.batchSeenStmt = db.prepare(
      "SELECT 1 FROM feed_batches WHERE feed = ? AND seq = ?",
    );
    this.recordBatchStmt = db.prepare(
      `INSERT INTO feed_batches (feed, seq, publisher, record_id, applied_at)
       VALUES (@feed, @seq, @publisher, @recordId, @appliedAt)`,
    );
    this.upsertFeedStateStmt = db.prepare(
      `INSERT INTO feed_state (feed, publisher, last_seq) VALUES (@feed, @publisher, @seq)
       ON CONFLICT(feed) DO UPDATE SET publisher = excluded.publisher,
         last_seq = MAX(last_seq, excluded.last_seq)`,
    );
    this.reload();
  }

  /** Re-read the policy file. Wired to SIGHUP by main.ts. */
  reload(): void {
    const raw = JSON.parse(readFileSync(this.filePath, "utf-8"));
    this.file = PolicyFileSchema.parse(raw);
    this.denyIdentities = new Set(this.file.denyIdentities);
    this.denyBlobHashes = new Set(this.file.denyBlobHashes);
    this.denyRecordIds = new Set(this.file.denyRecordIds);
    this.denyKinds = new Set(this.file.denyKinds);
    this.geoBlocks = new Map(this.file.geoBlocks.map((g) => [g.hash, g.countries]));
  }

  get subscribedFeeds(): { feed: string; publisher: string }[] {
    return this.file.feeds;
  }

  get name(): string {
    return this.file.name;
  }

  get description(): string {
    return this.file.description;
  }

  get moderationPolicyHtml(): string {
    return this.file.moderationPolicyHtml;
  }

  geoCountriesFor(hash: string): string[] {
    return this.geoBlocks.get(hash) ?? [];
  }

  /** Check whether a hash (blob or record id) is currently denylisted. */
  private hashDenied(scope: "record" | "blob", hash: string): string | undefined {
    if (scope === "blob" && this.denyBlobHashes.has(hash)) return "denylisted blob hash";
    if (scope === "record" && this.denyRecordIds.has(hash)) return "denylisted record id";
    const row = this.checkDenylistStmt.get(scope, hash);
    if (row) return "de-indexed by a subscribed compliance feed";
    return undefined;
  }

  /** Cheap, indexed decision for whether to index/select an incoming record. */
  checkRecord(kind: number, authorId: string, refs: Ref[]): PolicyDecision {
    if (this.denyKinds.has(kind)) {
      return this.deny("kind", String(kind), "kind is denylisted");
    }
    if (this.denyIdentities.has(authorId)) {
      return this.deny("identity", authorId, "author identity is denylisted");
    }
    for (const ref of refs) {
      const scope = ref.type === 1 ? "blob" : "record";
      const reason = this.hashDenied(scope, ref.hash);
      if (reason) return this.deny(scope, ref.hash, reason);
    }
    return { allowed: true };
  }

  /** Standalone check used by media.ts before ever serving a blob by hash. */
  checkBlobHash(blobId: string): PolicyDecision {
    const reason = this.hashDenied("blob", blobId);
    if (reason) return this.deny("blob", blobId, reason);
    return { allowed: true };
  }

  private deny(subjectType: string, subject: string, reason: string): PolicyDecision {
    this.log("deny", subjectType, subject, reason);
    return { allowed: false, reason };
  }

  log(action: string, subjectType: string, subject: string, reason?: string, detail?: unknown): void {
    this.logStmt.run({
      ts: Date.now(),
      action,
      subjectType,
      subject,
      reason: reason ?? null,
      detail: detail !== undefined ? JSON.stringify(detail) : null,
    });
  }

  /**
   * Apply a `feed.takedown` batch (spec 003 §6.7, spec 009 §3). Only
   * applied when (feed, publisher) matches a configured subscription —
   * subscribing is the policy decision, and de-indexing is automatic
   * once subscribed. Idempotent per (feed, seq): a re-delivered batch
   * (relay resend, multi-relay overlap) is a no-op. Batches MAY arrive
   * out of (feed, seq) order or with gaps (partition posture); each
   * batch is applied independently as received rather than buffered
   * waiting for missing sequence numbers.
   */
  applyFeedBatch(recordId: string, publisher: string, body: FeedTakedownBody): void {
    if (!this.subscribedFeeds.some((f) => f.feed === body.feed && f.publisher === publisher)) {
      return; // not a feed this gateway subscribes to
    }
    if (this.batchSeenStmt.get(body.feed, body.seq)) return; // already applied

    const txn = this.db.transaction(() => {
      for (const [refType, hash, reason, notice] of body.add) {
        const scope = refType === 1 ? "blob" : "record";
        this.insertDenylistStmt.run({
          scope,
          value: hash,
          reason: reason ?? "other",
          feed: body.feed,
          notice: notice ?? null,
          ts: Date.now(),
        });
      }
      for (const [refType, hash] of body.remove) {
        const scope = refType === 1 ? "blob" : "record";
        this.deleteDenylistStmt.run(scope, hash);
      }
      this.recordBatchStmt.run({
        feed: body.feed,
        seq: body.seq,
        publisher,
        recordId,
        appliedAt: Date.now(),
      });
      this.upsertFeedStateStmt.run({ feed: body.feed, publisher, seq: body.seq });
    });
    txn();

    this.log("feed_takedown_apply", "feed", body.feed, "matching subscribed feed batch", {
      recordId,
      seq: body.seq,
      added: body.add.length,
      removed: body.remove.length,
    });
  }

  /**
   * Apply local policy directly (spec 009 §1: "local policy is absolute
   * and instant") for a compliance notice submitted through
   * /api/compliance/notice, as distinct from a subscribed feed batch.
   */
  denylistForNotice(entries: { scope: "record" | "blob"; value: string }[], noticeRecordId: string): void {
    const txn = this.db.transaction(() => {
      for (const e of entries) {
        this.insertDenylistStmt.run({
          scope: e.scope,
          value: e.value,
          reason: "notice",
          feed: null,
          notice: noticeRecordId,
          ts: Date.now(),
        });
      }
    });
    txn();
    this.log("notice_takedown_apply", "notice", noticeRecordId, "direct compliance notice intake", { entries });
  }

  /** Last applied batch sequence for a subscribed feed, or undefined if none applied yet. */
  lastSeqFor(feed: string): number | undefined {
    const row = this.db.prepare("SELECT last_seq FROM feed_state WHERE feed = ?").get(feed) as
      | { last_seq: number }
      | undefined;
    return row?.last_seq;
  }

  stats(): { videos: number; deindexed: number; policyLogEntries: number } {
    const videos = (this.db.prepare("SELECT COUNT(*) AS n FROM videos WHERE retracted = 0").get() as {
      n: number;
    }).n;
    const deindexed = (this.db.prepare("SELECT COUNT(*) AS n FROM policy_denylist").get() as {
      n: number;
    }).n;
    const policyLogEntries = (this.db.prepare("SELECT COUNT(*) AS n FROM policy_log").get() as {
      n: number;
    }).n;
    return { videos, deindexed, policyLogEntries };
  }
}
