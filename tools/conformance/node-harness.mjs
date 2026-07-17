// node-harness.mjs
//
// The Node side of the conformance suite's "golden rule": the same
// vectors that `vidmesh-conformance`'s kernel target checks against
// `vidmesh-kernel` (Rust) are replayed here against `@vidmesh/kernel`
// (packages/kernel-ts), which wraps the same kernel compiled to WASM
// (crates/vidmesh-wasm). One newline-delimited JSON request/response
// pair per op; the protocol is documented in
// tools/conformance/src/node_target.rs — keep the two in sync.
//
// Requirements:
//   * Node >= 22.6, invoked with --experimental-strip-types:
//       node --experimental-strip-types node-harness.mjs
//     (this file imports packages/kernel-ts/src/index.ts directly, a
//     .ts file, with no separate build step — that flag is what lets
//     Node load it).
//   * crates/vidmesh-wasm built into packages/kernel-ts/wasm/ (build
//     plan Phase 3: `just wasm`). Until that WASM package exists,
//     importing @vidmesh/kernel below fails at startup with a "module
//     not found" error — that is a build prerequisite, not a bug in
//     this harness, and `vidmesh-conformance run --target node`
//     surfaces it as a normal spawn/I/O error.
//
// Known API gap (also noted in the top-level conformance report): the
// current `identity.verifyChain` binding
// (packages/kernel-ts/src/index.ts -> crates/vidmesh-wasm/src/lib.rs's
// `verify_chain`) hardcodes `Identity::verify_chain(&parsed, &|_| None,
// now)` — every record is treated as "just observed", so nothing is
// ever final. This harness's "identity-verify-chain" op therefore
// cannot honor a request's `observed` map (first-observed times per
// record id); vectors that depend on contest-window finality
// (`identity/fork-final-signing`) will legitimately disagree with the
// Rust kernel target until the WASM binding grows an `observedAt`
// parameter and `kernel-ts`'s `identity.verifyChain` forwards it. That
// divergence is exactly what this suite exists to surface — see build
// plan §7: "if a vector passes in one runtime and fails in another,
// the canonical encoding [or, here, the binding surface] is broken —
// stop and fix."

import readline from "node:readline";
import * as kernel from "../../packages/kernel-ts/src/index.ts";

/**
 * Classify a kernel error message into the same vocabulary
 * `tools/conformance/src/vectors.rs::error_class` uses. The WASM
 * bindings (`crates/vidmesh-wasm/src/lib.rs`'s `js_err`) surface every
 * kernel error as `JsError::new(&e.to_string())`, i.e. the exact
 * `Display` string of `vidmesh_kernel::Error` — so classification here
 * is prefix-matching against that Display impl
 * (`crates/vidmesh-kernel/src/error.rs`), duplicated deliberately
 * rather than shared, since JS and Rust can't share one source of
 * truth for a string format.
 */
function classifyError(message) {
  if (message.startsWith("malformed CBOR:")) return "cbor";
  if (message.startsWith("non-canonical encoding:")) return "non-canonical";
  if (message.startsWith("invalid envelope:")) return "envelope";
  if (message === "signature verification failed") return "signature";
  if (message.startsWith("unknown algorithm id")) return "unknown-algorithm";
  if (message.startsWith("invalid identity chain:")) return "identity";
  if (message.startsWith("chunk proof failed:")) return "chunk-proof";
  if (message.startsWith("kind validation failed:")) return "kind";
  if (message.startsWith("invalid bundle:")) return "bundle";
  if (message.startsWith("i/o error:")) return "io";
  return "unknown";
}

function errorResponse(e) {
  const message = e && e.message ? e.message : String(e);
  return { error: message, error_class: classifyError(message) };
}

async function handle(req) {
  switch (req.op) {
    case "verify-record": {
      const bytes = kernel.fromHex(req.cbor_hex);
      try {
        await kernel.verifyRecord(bytes);
        return { ok: true };
      } catch (e) {
        return { ok: false, ...errorResponse(e) };
      }
    }
    case "derive-id": {
      const bytes = kernel.fromHex(req.cbor_hex);
      try {
        const idHex = await kernel.deriveId(bytes);
        return { id_hex: idHex };
      } catch (e) {
        return errorResponse(e);
      }
    }
    case "record-to-json": {
      const bytes = kernel.fromHex(req.cbor_hex);
      try {
        const json = await kernel.recordToJson(bytes);
        return { json };
      } catch (e) {
        return errorResponse(e);
      }
    }
    case "record-from-json": {
      try {
        const bytes = await kernel.recordFromJson(req.json);
        return { cbor_hex: kernel.toHex(bytes) };
      } catch (e) {
        return errorResponse(e);
      }
    }
    case "verify-chunk": {
      const chunk = kernel.fromHex(req.chunk_hex);
      try {
        await kernel.verifyChunk({
          root: req.root_hex,
          nChunks: req.n_chunks,
          index: req.index,
          chunk,
          proof: req.proof_hex,
        });
        return { ok: true };
      } catch (e) {
        return { ok: false, ...errorResponse(e) };
      }
    }
    case "identity-verify-chain": {
      const records = req.records_hex.map((h) => kernel.fromHex(h));
      try {
        // `req.observed` is intentionally not forwarded: see the API
        // gap documented at the top of this file.
        const state = await kernel.identity.verifyChain(records, req.now);
        return {
          head_hex: state.head,
          signing_key_hex: state.signingKey,
          depth: state.depth,
        };
      } catch (e) {
        return errorResponse(e);
      }
    }
    default:
      return { error: `unknown op ${JSON.stringify(req.op)}`, error_class: "unknown" };
  }
}

async function main() {
  const rl = readline.createInterface({ input: process.stdin, terminal: false });
  for await (const line of rl) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    let req;
    try {
      req = JSON.parse(trimmed);
    } catch (e) {
      process.stdout.write(
        JSON.stringify({ error: `invalid request JSON: ${e.message}`, error_class: "cbor" }) + "\n",
      );
      continue;
    }
    let resp;
    try {
      resp = await handle(req);
    } catch (e) {
      resp = errorResponse(e);
    }
    process.stdout.write(JSON.stringify(resp) + "\n");
  }
}

main().catch((e) => {
  process.stderr.write(`node-harness.mjs fatal error: ${e && e.stack ? e.stack : e}\n`);
  process.exit(1);
});
