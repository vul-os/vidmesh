# DECISIONS.md

Running log of judgment calls made while building Vidmesh, so work never
blocks on questions. Newest entries at the bottom of each section.
Protocol-level decisions are also recorded in the relevant spec file
under a "Decisions" heading; this file is the master index plus
process/product decisions that don't belong in the spec.

## Standing directives (from the project owner)

- **Goal:** a complete, future-proof, secure, decentralized video-sharing
  system (YouTube-class use case). Gateways choose *what* they serve;
  every gateway ships the **same UI** — different URL/catalog, identical
  interface (spec/009 §7).
- **Work loop:** continue autonomously on a 15-minute wakeup cadence for
  up to 10 hours (2026-07-17); stop the loop when implementation is
  complete. Do not stop to ask questions — decide, log here, move on.
  **AMENDED 2026-07-17 ~04:10: owner said "pause after this wave" — after
  the in-flight conformance/gateway-server/gateway-web agents complete
  and their work is integrated+committed, STOP the loop (ScheduleWakeup
  stop:true), launch no new agents, and wait for the owner.**
- **Testing:** implement extensive tests throughout (unit, property,
  vectors), but **do not run test suites yet** — the owner will say
  when. Compilation/type checks are still performed so shipped code is
  sound.
- Commit per phase minimum, conventional commits (build plan §4/§15).
- **Git identity:** author/committer `imranparuk <imranparuk@live.com>`;
  no Co-Authored-By or Generated-with trailers, ever (owner directive
  2026-07-17; history rewritten to comply).
- **Parallelism:** use Sonnet subagents, five at a time, for separable
  work; the lead session integrates, reviews, and owns consensus-critical
  code (canonical codec review, record/identity semantics).

## Protocol decisions (index — details in spec files)

| # | Decision | Where |
|---|----------|-------|
| P1 | Envelope = CBOR map, integer keys 1–7; unknown envelope keys reject; bodies are text-keyed and forward-extensible | 001 §1, 003 §2 |
| P2 | Record id covers keys 1–6 incl. `sig_alg` (downgrade resistance) | 001 §3 |
| P3 | Signature over domain-separated id: `"vidmesh:record:v1" \|\| id` | 001 §4 |
| P4 | `IdentityRef = [identity_id, signing_key]`; genesis uses zero id | 001 §5, 002 §2 |
| P5 | `Ref = [ref_type, hash]`, 0=record 1=blob | 001 §6 |
| P6 | Chunk tree: 1 MiB chunks, leaf `0x00\|\|`, interior `0x01\|\|`, odd promoted | 001 §8 |
| P7 | Clean-room kernel (not Nostr superset), Ed25519=1, BLAKE3-256=1 | draft-00 App. B |
| P8 | Kind numeric ids grouped with gaps (rotation=1 … live.chat=113) | 003 §1 |
| P9 | Identity fork resolution: recovery>signing until finality (contest window, verifier-local first-seen), then bytewise id tiebreak; rotation body is the whole new state | 002 §4 |
| P10 | `supersede` = complete replacement body; collections live in bodies (playlist `entries`) so supersession is universal | 003 §§4.2, 5.4 |
| P11 | `feed.takedown` subjects in body (bulk data), not refs | 003 §6.7 |
| P12 | PoW nonce in transport frame (`PUB`), outside envelope; `BLAKE3-256(id \|\| nonce_le64)` leading zero bits | 006 §6 |
| P13 | Relay frames are CBOR; `since` cursors are relay-local receipt sequences | 006 §§1–3 |
| P14 | Bundle = magic `VMSH\x01` + CBOR sequence; 1 MiB blob parts aligned to chunk leaves; salvage-not-abort on corruption; no built-in compression | 007 |
| P15 | Encryption: per-blob random keys wrapped by a per-manifest content key; chunked XChaCha20-Poly1305 with 1 MiB ciphertext chunks; dedicated profile `enc_key` (no Ed25519→X25519 reuse) | 008 §2 |
| P16 | Derivation statement covers (orig, rendition, codec, w, h, bitrate); prefix `vidmesh:derivation:v1` | 004 §3 |
| P17 | Uniform reference UI across gateways is a trademark-level requirement | 009 §7 |
| P18 | JSON interchange is a strict bijection: `txt:` escape also applies to text map keys that would re-parse as integer keys (codec agent finding) | codec.rs module docs |
| P19 | Relay edge cases: all-zero id in `OK` for undecodable PUBs; unparseable frames dropped not CLOSED; `X-Expected-Blob-Id` header for PUT 422s; filter keys are text | 006 §§1,3,5.2 |

## Implementation decisions

| # | Decision | Rationale |
|---|----------|-----------|
| I1 | `ciborium` for CBOR in the kernel, with a hand-rolled canonical encoder for the envelope (canonical form is load-bearing; we control every byte) | build plan allows ciborium or minicbor |
| I2 | Kernel tests: unit + proptest live in-crate; byte-exact fixtures live in `tools/conformance/vectors` and are consumed by kernel tests via path, so vectors are single-source | build plan §6/§11 |
| I3 | Workspace deps pinned at workspace root `[workspace.dependencies]` | consistency across crates |
| I4 | PDF pipeline: pandoc + tectonic, classic Latin Modern style, black links, syntax-highlight-only color (owner preference, 2026-07-17) | spec/pandoc-pdf.yaml |
| I5 | v1 gateway custodies keys server-side (documented custody/rotation story per 002 §7 / 009 §5); non-custodial flows come later | build plan §9 |

## Open items intentionally deferred

- Contest-window recommended default (spec draft App. C) — reference
  implementation ships 604800 s (7 days) as the default; revisit before
  v1 freeze.
- Encrypted-DM kind for key delivery (008 §3) — post-v1; keygrant covers
  the launch path.
- Anchor system launch set — reference implements `opentimestamps` proof
  *carriage* only (verification is external tooling) in v1.

## Phase 1 — first test-suite run (2026-07-19)

The owner authorized running the never-executed suites and fixing what
breaks (non-destructive: no kernel/codec/relay byte-format rewrites). All
suites now run and pass; the fixes made, as decisions:

| # | Decision | Rationale |
|---|----------|-----------|
| T1 | A freshly built signed `Record` stores its body in **canonical key order** (`Value::into_canonical`, applied in `RecordBuilder::sign_as`) | The `Value::Map` invariant was documented but only held for decoded values; a hand-built body kept insertion order, so byte-identical records (same id/sig) compared unequal via derived `PartialEq`. Ordering only — the signed bytes are unchanged (the encoder already sorts). Fixed bundle round-trip + salvage tests; the `attest_round_trip` fixture's non-canonical literal was corrected to match. |
| T2 | `kinds::validate()` verifies each manifest rendition's `derivation_sig` against the manifest's own original blob (spec 004 §3.1) | `Manifest::parse` stops at structure by design; `validate()` is the caller it documents as responsible for the crypto check. Conformance vector `kinds/manifest/bad-derivation-sig` required it; the kernel accepted a forged sig before. |
| T3 | `verifyChain` (WASM + kernel-ts + node harness) takes an `observedAt` map threaded into `Identity::verify_chain`'s `observed_at` closure | The binding hardcoded `observed = None`, so contest-window finality could never be exercised under Node and `identity/fork-final-signing` genuinely diverged from the kernel. Closing the divergence (not special-casing the vector) is the golden rule (build plan §7). |
| T4 | JS test/dev runners: gateway-server uses `--experimental-transform-types`; kernel-ts expands its one parameter property to a field | Node's strip-only mode rejects TS parameter-property constructors and enums; the gateway uses many parameter properties (a deliberate style), so the flag switch is the least-invasive correct fix there. |
| T5 | Conformance verdict recorded: kernel 189/0/0, node 142/0/47, relay 115/0/74 — 0 failures; all differences are documented per-runtime skips | The three-runtime golden rule holds. Skips are each runtime checking only what it is responsible for (relay = envelope only; node = no bundle/json/kind-invalid surface). |

**2026-07-19, post-relocation build breakage** — the repo's move from
`~/code/vidmesh` to `~/code/vulos/vidmesh` left a *pre-existing, gitignored*
`target/` directory in place, and two of its cached build artifacts kept
serving absolute paths from the old location instead of being invalidated:
`vidmesh-node`'s Tauri build script replayed a stale cached
`cargo:tauri-core-*-permission-files=/Users/pc/code/vidmesh/target/...`
line from `target/debug/build/tauri-*/output` (a previous build script
run's captured stdout, reused because cargo's fingerprint check didn't
consider it stale), and the `vidmesh-conformance` binary had
`env!("CARGO_MANIFEST_DIR")` baked in from the last time it was actually
relinked, pre-move. Neither is a source bug — both are local, gitignored
build cache. Fix: `cargo clean -p tauri -p vidmesh-node` and
`cargo clean -p vidmesh-conformance`, then rebuild; both regenerate
correctly from `CARGO_MANIFEST_DIR` at the new path. No source or absolute
path was hardcoded anywhere in the tree (repo-wide grep for the old path
and for `/Users/*` came back empty). While investigating, found
`crates/vidmesh-node/gen/` (Tauri's regenerated schema/capability JSON)
was neither committed nor gitignored — added to `.gitignore` so build
output never accidentally gets committed or relied upon across machines.
Any dev hitting this after a future relocation: `cargo clean -p tauri -p
vidmesh-node -p vidmesh-conformance` (or a full `cargo clean` if that
doesn't clear it) before assuming a real regression.

| # | Decision | Rationale |
|---|----------|-----------|
| T6 | `crates/vidmesh-node/gen/` added to `.gitignore`; stale-path build breakage fixed via targeted `cargo clean`, not a hardcoded path | Generated/cached artifacts must never bake in machine-specific absolute paths that outlive a relocation; the correct fix is always "regenerate", never "hardcode the new path" |

**DMTAP-PUB convergence** — recorded as a full decision document at
[docs/DMTAP-CONVERGENCE.md](docs/DMTAP-CONVERGENCE.md). Recommendation:
re-base vidmesh's video layer as the DMTAP-PUB §24 video profile (route b),
contributing range proofs / rotation-log finality / a fetch-hint registry
upstream. **FOUNDER-GATED** — no substrate byte changes until the founder
confirms direction, §24 is targetable, and envoir's §22 Rust impl is a
consumable dependency. Phase 1 stayed non-destructive per this gate.
