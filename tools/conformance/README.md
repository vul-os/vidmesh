# tools/conformance

The Evermesh conformance suite: deterministic JSON fixture vectors covering
the envelope, every registered record kind, identity rotation chains,
chunk-tree range proofs, bundle import/export, and JSON↔CBOR interchange —
plus a Rust runner that replays them against three independent runtimes:
the `evermesh-kernel` crate in-process, `@evermesh/kernel` under Node (via a
small child-process harness), and a live `evermesh-relay` over its
websocket sync protocol.

**This suite is what makes the two-implementations rule (spec
[000-overview.md](../../spec/000-overview.md) §3 Principle 9) enforceable.
It is a first-class product, not test scaffolding**: the golden rule is
*the same vectors must pass identically in every runtime*. If a vector
passes against one implementation and fails against another, that is a
protocol bug — canonical encoding, a validation rule, or a binding is
broken — never a reason to special-case the vector.

## Layout

```
tools/conformance/
├── Cargo.toml
├── node-harness.mjs        the Node side of the `node` target
├── src/
│   ├── vectors.rs          the vector format (serde structs)
│   ├── lib.rs              shared vector loading
│   ├── kernel_target.rs    executes vectors against evermesh-kernel in-process
│   ├── node_target.rs      spawns node-harness.mjs, speaks its protocol
│   ├── relay_target.rs     speaks the spec 006 /sync websocket protocol
│   ├── main.rs             the `evermesh-conformance run` CLI
│   └── bin/generate.rs     the deterministic vector generator
└── vectors/                generated output (see "Regenerating" below)
    ├── envelope/
    ├── kinds/<kind-name>/  one directory per spec 003 registry entry
    ├── identity/
    ├── chunktree/
    ├── bundle/
    └── json/
```

## Vector format

Every vector is one pretty-printed JSON file at
`vectors/<group>/<name>.json`, sorted-key (regenerating the suite produces
a clean diff), with four fields every vector shares —

```jsonc
{
  "group": "kinds/comment",
  "name": "valid-basic",
  "kind": "record-valid",           // discriminates the fields below
  "description": "human-readable, cites the spec section it exercises",
  // ...kind-specific fields...
}
```

— and one of six shapes selected by `kind`:

| `kind` | Fields | Meaning |
|---|---|---|
| `record-valid` | `cbor_hex`, `expected_id_hex`, `json` | The bytes MUST parse, verify, and derive this id; `json` is its JSON interchange form (spec 001 §11), embedded as a real value so it reads/diffs naturally. |
| `record-invalid` | `cbor_hex`, `expected_error`, `layer` | The bytes MUST be rejected. `expected_error` is one of `cbor`, `non-canonical`, `envelope`, `signature`, `unknown-algorithm`, `kind` (mirrors `evermesh_kernel::Error`). `layer` is `"envelope"` (checkable by `Record::from_cbor` + `Record::verify` alone — every target checks these today) or `"kind"` (only checkable by `evermesh_kernel::kinds::validate`, not yet wired into this runner — see below). |
| `chunk-proof` | `n_chunks`, `last_chunk_len`, `chunk_index`, `proof_hex`, `root_hex`, `valid` | Describes a synthetic blob by formula (chunk `i` is `(i % 251)` repeated for its length; every chunk but the last is exactly 1 MiB) rather than embedding megabytes of hex, plus a range proof and whether it MUST verify. |
| `identity-chain` | `records_hex`, `now`, `observed`, `expected` or `expected_error` | Records to feed `Identity::verify_chain`, in the order the vector wants them merged; `observed` maps record-id-hex to a first-observed Unix time (absent = "just observed", not final). |
| `bundle` | `bundle_hex`, `expected` or `expected_error` | A complete bundle byte stream (magic + CBOR item sequence); `expected` is what a correct importer recovers (record/blob ids, skip count, truncation flag), or `expected_error` if the container itself is malformed (e.g. bad magic — `Bundle::import` MUST fail outright, not salvage). |
| `json-roundtrip` | `json`, `expected_cbor_hex` or `expected_error` | A JSON interchange document and either the canonical CBOR it must produce, or the fact that it must be rejected. |

See `src/vectors.rs` for the exact serde definitions (the source of truth).

## Regenerating

```sh
cargo run --bin generate
```

Deletes and rewrites `vectors/` from scratch. The generator
(`src/bin/generate.rs`) is **fully deterministic**: every keypair comes
from a fixed secret-byte seed (`Keypair::from_secret_bytes`, never
`generate()`), every `created_at` is a fixed constant, every blob is
synthesized from a formula, and output is written in sorted order with
sorted-key JSON — so a clean checkout re-running `generate` produces no
diff. Record-kind bodies are built directly with `RecordBuilder` +
`evermesh_kernel::codec::Value` per spec 003/004's schemas, **not** via
`evermesh_kernel::kinds` (that module is being completed in parallel and is
not yet part of the compiled kernel crate — see the report below).

## Running

```sh
# Against the kernel crate in-process (the reference target):
cargo run --bin evermesh-conformance -- run --target kernel

# Against @evermesh/kernel under Node (requires crates/evermesh-wasm built
# into packages/kernel-ts/wasm/, and Node >= 22.6):
cargo run --bin evermesh-conformance -- run --target node

# Against a live relay's /sync:
cargo run --bin evermesh-conformance -- run --target relay --relay-url ws://127.0.0.1:8787/sync
```

`--vectors <dir>` overrides the vector directory (default:
`tools/conformance/vectors`, resolved relative to this crate). Each run
prints a per-group pass/fail/skip table and exits nonzero if anything
failed. `just conformance` (once wired by the lead) should invoke all
three targets in turn.

**Skips are not failures.** A vector is skipped, with a stated reason,
when the target genuinely cannot check it yet — e.g. a `layer: "kind"`
record-invalid vector against the kernel target (kind-level validation
isn't wired in yet, guarded behind a `// kinds` comment in
`src/kernel_target.rs` for the lead to enable once
`evermesh_kernel::kinds` lands), or a `bundle` vector against the node
target (`@evermesh/kernel` exposes no bundle API), or any non-record
vector against the relay target (relays only speak the envelope, spec
006 §4).

## The golden rule in practice

`just conformance` (or three manual `run` invocations) is green only when
**all three targets report the same failures — ideally none.** A vector
that passes against `kernel` but fails against `node` or `relay` means one
of:

* the canonical CBOR encoders disagree (a real, serious bug — stop and
  fix before anything else, per build plan §7);
* a binding is missing surface area (documented gaps below, not bugs);
* the fixture itself encodes an assumption one runtime can't yet honor
  (also documented below).

## Known gaps and mismatches found while building this suite

These are reported rather than silently worked around, per the task's
own instruction — decide what to do about each one as a build-plan/spec
decision, not a fixture hack:

1. **`evermesh_kernel::kinds` is not part of the compiled kernel crate
   yet.** `crates/evermesh-kernel/src/kinds/mod.rs` references
   `claims`, `compliance`, `infra`, `live`, `social`, `trust` submodules
   that do not exist on disk yet (only `content.rs` does), and
   `crates/evermesh-kernel/src/lib.rs` does not declare `pub mod kinds;`
   at all — so the module tree isn't wired in. This suite therefore
   builds every kind record directly with `RecordBuilder` + `Value`
   (per the task brief) and ships every kind-specific invalid mutation
   as a `layer: "kind"` vector that the kernel target currently
   *skips* rather than checks (see `src/kernel_target.rs`'s `// kinds`
   comment block, ready to uncomment once the module lands).
   `crates/evermesh-wasm/src/lib.rs` already assumes `kinds` exists
   (`use evermesh_kernel::{..., kinds, ...}`) and calls
   `kinds::validate` in `validate_kind` — so `evermesh-wasm` likely does
   not compile today either, independent of this suite.

2. **The WASM `identity.verifyChain` binding cannot express
   contest-window finality.** `crates/evermesh-wasm/src/lib.rs`'s
   `verify_chain` hardcodes
   `Identity::verify_chain(&parsed, &|_| None, now)` — every record is
   always treated as "just observed," so a signing-key rotation can
   never become final. `packages/kernel-ts/src/index.ts`'s
   `identity.verifyChain(records, now)` has no parameter for
   first-observed times either. Consequently
   `identity/fork-final-signing` (which depends on one rotation being
   observed long enough ago to resist a later recovery fork) cannot be
   correctly exercised against the `node` target as it stands — it
   will genuinely disagree with the `kernel` target. Fix: add an
   `observedAt: Record<string, number>` parameter through
   `evermesh_wasm::verify_chain` → `kernel-ts`'s `identity.verifyChain`
   → `node-harness.mjs`'s `identity-verify-chain` op (which already
   forwards a request `observed` map that the harness currently drops
   for exactly this reason, documented at the top of the file).

3. **`rotate-alg-migration` is not yet a real cross-algorithm test.**
   Spec 002's test-vectors section calls for an "algorithm-migration
   rotation," but only Ed25519 (`sig_alg` 1) is a registered signature
   algorithm at launch (spec 001 §7) — there is no second algorithm to
   migrate *to*. `identity/rotate-alg-field-present.json` exercises the
   shape of the mechanism (a rotation whose body carries `key_alg`)
   rather than an actual migration; this vector should be replaced or
   supplemented once a second algorithm enters the registry.

4. **The `bundle/roundtrip` and `bundle/corrupted-blob-item` fixtures'
   3 MiB+ parted blob is hand-assembled at the item level, not produced
   by `Bundle::export`.** `evermesh_kernel::bundle::PART_SPLIT_THRESHOLD`
   is 16 MiB (matching spec 007 §1's stated floor — "blobs larger than
   16 MiB"), so `Bundle::export` would not split a 3 MiB blob into `bp`
   parts on its own. The task brief called for a 3 MiB parted blob
   specifically (presumably to keep the fixture small); the spec's
   wording sets a lower bound, not an exclusivity rule, so hand-emitting
   `bp` items below 16 MiB is spec-conformant and exercises the same
   import path a real 16 MiB+ blob would. Worth a follow-up: also add a
   true 16 MiB+ `Bundle::export`-produced fixture for full fidelity, if
   the committed vector size budget allows it.

5. **`chunktree/` uses 0/1/2/3/5-chunk blobs, not the 1000-chunk blob
   spec 001's own test-vectors section calls for.** The task brief
   explicitly asked for 0,1,2,3,5-chunk synthetic blobs; a 1000-chunk
   blob is ~1000 MiB of synthesized bytes per test run (the fixture
   file itself stays tiny — it's described by formula — but every
   runner target would need to materialize and hash that much data
   every run). Also not yet covered: spec 001's "swapped leaf prefix"
   invalid case (hashing a chunk with the interior-node `0x01` prefix
   instead of the leaf `0x00` prefix, to check the domain separation is
   enforced) — only "wrong sibling" and "wrong index" are covered, per
   the task brief. Both are good candidates for a follow-up pass.

6. **Two-record semantic kind-invalids are intentionally absent.**
   Several of spec 003's kind-specific validation rules require a
   *second* record to check (`comment/parent-subject-mismatch`,
   `retract`'s "target must not be a rotation",
   `delegate`'s "revocation author must equal grant author", rendition
   authorization via `delegate`) — these aren't representable as a
   single `record-invalid` vector and are left for a future
   `record-invalid-pair` (or similar) vector shape if the suite grows
   cross-record fixtures.

## Kernel API assumptions this suite makes

Recorded here because they're load-bearing and easy to break silently in
a future kernel refactor:

* `RecordBuilder::new(kind).created_at(t).refs(v).body(b).sign_as(&Keypair,
  IdentityId)` — note `sign_as`, not the build plan's indicative `.sign(&Keypair)`.
* `Record::{from_cbor, verify, id, to_canonical_cbor, to_json, from_json}`.
* `Identity::{genesis, rotate, verify_chain}`; `verify_chain`'s
  `observed_at: &dyn Fn(&RecordId) -> Option<i64>` is verifier-local and
  drives contest-window finality only.
* `blob::{hash_blob, leaf_hash, node_hash, verify_chunk, CHUNK_SIZE}`;
  `ChunkTree::{from_bytes, root, prove, n_chunks, leaves}`.
* `Bundle::{export, import}`; `bundle::MAGIC`, `bundle::PART_SPLIT_THRESHOLD`.
* `codec::{Value, encode_canonical, decode_canonical, to_json, from_json}` —
  this suite's relay-frame codec and derivation-statement hashing both
  reuse this module directly rather than hand-rolling a second CBOR
  encoder (the relay target hashes frames the same way the kernel hashes
  records; the generator hashes derivation statements via
  `blob::hash_blob`, since BLAKE3-256 is BLAKE3-256 regardless of what
  it's hashing — no direct `blake3` dependency needed here).
* `ids::{RecordId, BlobId, IdentityId}::{to_hex, from_hex, as_bytes}`.

None of the kernel's public signatures needed to change for this suite;
the assumptions above are pinned so a signature change shows up as a
compile error here, not a silent drift.
