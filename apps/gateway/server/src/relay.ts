/**
 * Websocket relay client (spec 006-relay.md §§1-3, §8). One client per
 * configured relay: subscribes from its persisted `since` cursor, verifies
 * every incoming record with the kernel before handing it to ingest,
 * publishes outgoing records and tracks their OK, and reconnects with
 * exponential backoff. A relay connection carries no serving obligations
 * of its own — all selection happens in ingest.ts/policy.ts downstream.
 */
import { randomUUID } from "node:crypto";
import { WebSocket } from "ws";
import { verifyRecord, deriveId } from "@vidmesh/kernel";
import { encodeClientFrame, decodeRelayFrame, type RelayFrame } from "./relay-frames.ts";
import type Database from "better-sqlite3";
import type { Db } from "./db.ts";

const BACKOFF_INITIAL_MS = 1000;
const BACKOFF_MAX_MS = 30_000;
const PUB_TIMEOUT_MS = 10_000;

export type IngestFn = (record: Uint8Array, source: { relay: string }) => Promise<void>;

interface PendingPub {
  resolve: (accepted: boolean, reason: string) => void;
  timeout: NodeJS.Timeout;
}

/** One reconnecting client for a single relay's `/sync` websocket. */
export class RelayClient {
  private ws: WebSocket | undefined;
  private backoffMs = BACKOFF_INITIAL_MS;
  private closed = false;
  private readonly subId = randomUUID();
  private readonly pending = new Map<string, PendingPub>();
  private readonly outboxWhileDisconnected: Uint8Array[] = [];
  private readonly getSinceStmt: Database.Statement;
  private readonly upsertSinceStmt: Database.Statement;

  constructor(
    readonly url: string,
    private readonly db: Db,
    private readonly ingest: IngestFn,
    private readonly log: (msg: string) => void = () => {},
  ) {
    this.getSinceStmt = db.prepare("SELECT since_seq FROM relay_state WHERE url = ?");
    this.upsertSinceStmt = db.prepare(
      `INSERT INTO relay_state (url, since_seq) VALUES (?, ?)
       ON CONFLICT(url) DO UPDATE SET since_seq = excluded.since_seq`,
    );
    if (!this.getSinceStmt.get(url)) this.upsertSinceStmt.run(url, 0);
  }

  start(): void {
    this.closed = false;
    this.connect();
  }

  stop(): void {
    this.closed = true;
    this.ws?.close();
  }

  private since(): number {
    const row = this.getSinceStmt.get(this.url) as { since_seq: number } | undefined;
    return row?.since_seq ?? 0;
  }

  private setSince(seq: number): void {
    const current = this.since();
    if (seq > current) this.upsertSinceStmt.run(this.url, seq);
  }

  private connect(): void {
    if (this.closed) return;
    let ws: WebSocket;
    try {
      ws = new WebSocket(this.url);
    } catch (err) {
      this.scheduleReconnect(err);
      return;
    }
    this.ws = ws;
    ws.binaryType = "nodebuffer";

    ws.on("open", () => {
      this.backoffMs = BACKOFF_INITIAL_MS;
      this.send(encodeClientFrame({ type: "REQ", subId: this.subId, filter: { since: this.since() } }));
      for (const record of this.outboxWhileDisconnected.splice(0)) {
        this.publish(record).catch(() => {});
      }
    });

    ws.on("message", (data, isBinary) => {
      if (!isBinary) return; // text frames MUST be ignored (spec 006 §1)
      this.handleFrame(data as Buffer).catch((err) =>
        this.log(`relay ${this.url}: frame handling error: ${(err as Error).message}`),
      );
    });

    ws.on("close", () => this.scheduleReconnect(undefined));
    ws.on("error", (err) => this.log(`relay ${this.url}: socket error: ${err.message}`));
  }

  private scheduleReconnect(err: unknown): void {
    if (err) this.log(`relay ${this.url}: connect failed: ${(err as Error).message}`);
    if (this.closed) return;
    const delay = this.backoffMs;
    this.backoffMs = Math.min(this.backoffMs * 2, BACKOFF_MAX_MS);
    setTimeout(() => this.connect(), delay);
  }

  private send(bytes: Uint8Array): void {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(bytes);
  }

  private async handleFrame(data: Buffer): Promise<void> {
    let frame: RelayFrame;
    try {
      frame = decodeRelayFrame(new Uint8Array(data));
    } catch {
      return; // unknown/malformed frame: ignore (spec 006 §1)
    }
    switch (frame.type) {
      case "REC": {
        try {
          await verifyRecord(frame.record);
          await this.ingest(frame.record, { relay: this.url });
        } catch (err) {
          this.log(`relay ${this.url}: rejected record: ${(err as Error).message}`);
        }
        this.setSince(Number(frame.seq));
        return;
      }
      case "EOSE":
        return; // stored backfill complete; subsequent RECs are live
      case "OK": {
        const idHex = Buffer.from(frame.id).toString("hex");
        const pending = this.pending.get(idHex);
        if (pending) {
          clearTimeout(pending.timeout);
          this.pending.delete(idHex);
          pending.resolve(frame.accepted, frame.reason);
        }
        return;
      }
      case "CLOSED":
        return;
    }
  }

  /** Publish a record; resolves once this relay answers OK or the socket is down/times out. */
  async publish(record: Uint8Array): Promise<{ accepted: boolean; reason: string }> {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      this.outboxWhileDisconnected.push(record);
      return { accepted: false, reason: "relay not connected; queued for retry" };
    }
    const idHex = await deriveId(record);
    return new Promise((resolve) => {
      const timeout = setTimeout(() => {
        this.pending.delete(idHex);
        resolve({ accepted: false, reason: "timeout waiting for OK" });
      }, PUB_TIMEOUT_MS);
      this.pending.set(idHex, {
        resolve: (accepted, reason) => resolve({ accepted, reason }),
        timeout,
      });
      this.send(encodeClientFrame({ type: "PUB", record, nonce: null }));
    });
  }
}

/** Owns one RelayClient per configured relay URL. */
export class RelayManager {
  private readonly clients: RelayClient[];

  constructor(
    db: Db,
    relayUrls: string[],
    ingest: IngestFn,
    log: (msg: string) => void = () => {},
  ) {
    this.clients = relayUrls.map((url) => new RelayClient(url, db, ingest, log));
  }

  start(): void {
    for (const client of this.clients) client.start();
  }

  stop(): void {
    for (const client of this.clients) client.stop();
  }

  /** Fire-and-forget publish to every configured relay. */
  publish(record: Uint8Array): void {
    for (const client of this.clients) {
      client.publish(record).catch(() => {});
    }
  }

  /** Publish and wait for every relay's answer (used where callers care). */
  async publishAndWait(record: Uint8Array): Promise<{ url: string; accepted: boolean; reason: string }[]> {
    return Promise.all(
      this.clients.map(async (c) => ({ url: c.url, ...(await c.publish(record)) })),
    );
  }
}
