/**
 * Compliance toolkit endpoints (API.md, spec 005-claims.md §4, spec
 * 003-kinds-registry.md §6.5-6.6). Notices are signed by the gateway's
 * own operator identity (gateway-identity.ts), not the submitter — the
 * submitter is asserting a legal claim *to* the gateway, and the gateway
 * is the one making the resulting record (and the local de-index
 * decision) its own.
 */
import type { FastifyInstance } from "fastify";
import { z } from "zod";
import type { AppDeps } from "../app-deps.ts";
import { invalid, notFound } from "../errors.ts";
import { processRecord } from "../ingest.ts";

const ClaimantSchema = z.object({
  name: z.string().min(1),
  contact: z.string().min(1),
  on_behalf_of: z.string().optional(),
});
const SubjectSchema = z.object({ type: z.union([z.literal(0), z.literal(1)]), hash: z.string().regex(/^[0-9a-f]{64}$/i) });

const NoticeSchema = z.object({
  regime: z.string().min(1),
  claimant: ClaimantSchema,
  statement: z.string().min(1),
  work: z.string().min(1),
  signatureName: z.string().min(1),
  subjects: z.array(SubjectSchema).min(1),
});

const CounterSchema = z.object({
  regime: z.string().min(1),
  claimant: ClaimantSchema,
  statement: z.string().min(1),
  signatureName: z.string().min(1),
  noticeId: z.string().regex(/^[0-9a-f]{64}$/i),
});

export function registerComplianceRoutes(app: FastifyInstance, deps: AppDeps): void {
  const { db } = deps;

  app.post("/api/compliance/notice", async (request) => {
    const parsed = NoticeSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);
    const { regime, claimant, statement, work, signatureName, subjects } = parsed.data;

    const record = await deps.gatewayIdentity.signRecord({
      kind: 64,
      refs: subjects.map((s) => ({ type: s.type, hash: s.hash })),
      body: { regime, claimant, statement, work, signature_name: signatureName },
    });
    const result = await processRecord(deps.ingest, record);
    if (!result.stored || !result.recordId) throw invalid(`notice rejected: ${result.reason}`);
    deps.relays.publish(record);

    deps.policy.denylistForNotice(
      subjects.map((s) => ({ scope: s.type === 1 ? ("blob" as const) : ("record" as const), value: s.hash })),
      result.recordId,
    );

    return { noticeId: result.recordId };
  });

  app.post("/api/compliance/counter", async (request) => {
    const parsed = CounterSchema.safeParse(request.body);
    if (!parsed.success) throw invalid(parsed.error.message);
    const { regime, claimant, statement, signatureName, noticeId } = parsed.data;

    const record = await deps.gatewayIdentity.signRecord({
      kind: 65,
      refs: [{ type: 0, hash: noticeId }],
      body: { regime, claimant, statement, signature_name: signatureName },
    });
    const result = await processRecord(deps.ingest, record);
    if (!result.stored || !result.recordId) throw invalid(`counter-notice rejected: ${result.reason}`);
    deps.relays.publish(record);
    // Reinstatement after a counter-notice is a manual/legal-review step
    // in every regime this toolkit targets (e.g. DMCA's 10-14 business
    // day window) — never automatic. See legal/DMCA.md.
    return { counterId: result.recordId };
  });

  app.get("/api/compliance/notices/:id", async (request) => {
    const { id } = request.params as { id: string };
    const row = db.prepare("SELECT json, kind FROM records WHERE id = ?").get(id) as { json: string; kind: number } | undefined;
    if (!row || (row.kind !== 64 && row.kind !== 65)) throw notFound("notice not found");
    return { record: JSON.parse(row.json), id, kind: row.kind };
  });
}
