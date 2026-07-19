/**
 * `GET /api/records/{recordId}` and `.../cbor` (API.md) — raw fetch of any
 * indexed record, in JSON interchange form or as the canonical CBOR bytes
 * the browser verifies client-side.
 */
import type { FastifyInstance } from "fastify";
import type { Db } from "../db.ts";
import { notFound } from "../errors.ts";

interface RecordRow {
  id: string;
  kind: number;
  json: string;
  cbor: Buffer;
}

export function registerRecordRoutes(app: FastifyInstance, db: Db): void {
  const getStmt = db.prepare("SELECT id, kind, json, cbor FROM records WHERE id = ?");

  app.get("/api/records/:recordId", async (request) => {
    const { recordId } = request.params as { recordId: string };
    const row = getStmt.get(recordId) as RecordRow | undefined;
    if (!row) throw notFound("record not found");
    return { record: JSON.parse(row.json), id: row.id, kind: row.kind };
  });

  app.get("/api/records/:recordId/cbor", async (request, reply) => {
    const { recordId } = request.params as { recordId: string };
    const row = getStmt.get(recordId) as RecordRow | undefined;
    if (!row) throw notFound("record not found");
    reply.header("content-type", "application/cbor");
    return reply.send(row.cbor);
  });
}
