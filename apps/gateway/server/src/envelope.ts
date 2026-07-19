/**
 * Field extraction for the kernel's JSON interchange form (spec 001 §11).
 *
 * The envelope itself is a CBOR map with INTEGER keys 1-7 (spec 001 §1);
 * `to_json` (crates/vidmesh-kernel/src/codec.rs) maps integer map keys to
 * decimal-string keys generically, so `recordToJson()` returns an object
 * keyed `"1".."7"`, NOT named fields like `kind`/`author`. Only kind
 * *bodies* (key "5") use text keys, per spec 003 §2 — that part reads
 * naturally as `body.title`, `body.original`, etc. This module is the one
 * place that knows the "1".."7" mapping so the rest of the codebase never
 * has to.
 */
import { unhex } from "./ingest-kinds.ts";
import type { Ref } from "./policy.ts";

export interface EnvelopeFields {
  kind: number;
  authorId: string;
  signingKey: string;
  createdAt: number;
  refs: Ref[];
  body: Record<string, unknown>;
}

export function extractEnvelope(json: Record<string, unknown>): EnvelopeFields {
  const kind = Number(json["1"]);
  const authorTuple = json["2"] as [string, string] | undefined;
  const createdAt = Number(json["3"]);
  const refsRaw = (json["4"] as [number, string][] | undefined) ?? [];
  const body = (json["5"] as Record<string, unknown> | undefined) ?? {};
  return {
    kind,
    authorId: authorTuple ? unhex(authorTuple[0]) : "",
    signingKey: authorTuple ? unhex(authorTuple[1]) : "",
    createdAt,
    refs: refsRaw.map(([t, h]) => ({ type: (t === 1 ? 1 : 0) as 0 | 1, hash: unhex(h) })),
    body,
  };
}
