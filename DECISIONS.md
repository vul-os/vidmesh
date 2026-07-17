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
- **Testing:** implement extensive tests throughout (unit, property,
  vectors), but **do not run test suites yet** — the owner will say
  when. Compilation/type checks are still performed so shipped code is
  sound.
- Commit per phase minimum, conventional commits (build plan §4/§15).

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
