# Changelog

All notable changes to Vidmesh are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [0.1.0] — 2026-07-21

**Status: pre-alpha.** This is the first tagged snapshot, not a shipped
product: there is no deployment, no swarm/P2P transport, no live-streaming
product surface, and the desktop node is a scaffold. It marks the point where
the protocol kernel, the relay, the WASM/TS bindings, the reference gateway
(backend + frontend), and the cross-implementation conformance suite are all
implemented with test suites that run and pass. The [spec](spec/) is
normative; where code and spec disagree, the spec wins. See the
[Status by component](README.md#status-by-component) table in the README for
the authoritative breakdown of what is implemented versus scaffolded versus
spec'd-but-not-built.

### Added

- **Protocol spec (000–011 + IETF-style Internet-Draft)** — the normative
  description of records, identity and key rotation, the kind registry,
  manifests, claims, the relay sync protocol, bundles, privacy/encryption,
  the gateway (including the uniform reference UI requirement), the
  substrate economics, and the threat model. Licensed CC-BY-SA-4.0
  (`LICENSE-SPEC`).
- **`crates/vidmesh-kernel`** — the protocol kernel: self-certifying signed
  records (CBOR envelope, Ed25519, BLAKE3), all 27 record kinds, identity
  with recovery-precedence key rotation, content-addressed chunked blobs
  with proofs, and the canonical codec. 193 unit tests + 7 property tests.
  Additive, default-off `dmtap-pub` feature consumes `dmtap-core` (Envoir's
  reference crate) to prove byte-for-byte agreement with DMTAP-PUB §22's
  frozen conformance vectors, without touching the native format.
- **`crates/vidmesh-relay`** — an axum `/sync` websocket relay: envelope
  validation, SQLite-backed storage, filtered subscriptions, gossip,
  proof-of-work admission, rate-limiting, retention, and an optional blob
  sidecar (PUT / GET-range / proof). 47 tests.
  - `crates/vidmesh-wasm` — wasm-bindgen bindings over the kernel for
    browsers and Node.
  - `crates/vidmesh-node` — a Tauri 2 desktop-node **scaffold** (pins and
    seeds nothing yet).
- **`packages/kernel-ts`** — a typed TypeScript API over the WASM kernel (5
  tests). `packages/ui` — shared React components (player, verification
  badge) consumed by the gateway frontend.
- **`apps/gateway/server`** — the reference gateway backend (Fastify):
  config, a SQLite index, a policy engine, custodial key handling, relay
  clients, kind-aware ingest, an upload/original-only pipeline, and the
  JSON API. 45 tests; boots and connects to a relay. Ships a mandatory,
  non-configurable CSAM hash-matching integration point (`CSAM.md`) — the
  one moderation decision the spec does not leave to gateway policy.
- **`apps/gateway/web`** — the uniform reference UI (React + Vite +
  Tailwind + TanStack Query) every gateway ships, re-skinnable only through
  its `--vm-*` design tokens. 45 tests; builds.
- **`apps/site`** — vidmesh.org: a static landing page and docs viewer,
  browser-checked (`just site-check`).
- **`tools/conformance`** — 189 deterministic test vectors replayed
  identically against three independent runtimes (in-process kernel,
  Node/WASM via `@vidmesh/kernel`, and a live relay over `/sync`); a
  divergence is treated as a protocol/binding bug, never special-cased.
- **Dual code license** (MIT OR Apache-2.0) plus a separate CC-BY-SA-4.0
  license for everything under `spec/`.
- **CI** — Rust fmt/clippy/test, a dedicated job proving the `dmtap-pub`
  feature's conformance vectors actually ran (not silently skipped), a WASM
  build, and JS lint/test across the pnpm workspace.

### Known gaps (spec'd, not built)

- Swarm/P2P retrieval, WebRTC and BitTorrent-style transport — blob
  retrieval today is the relay's HTTP sidecar only.
- Live streaming — the `live.manifest` / `live.chat` kinds validate in the
  kernel; there is no live ingest, player, or product surface.
- Non-custodial key flows — the reference gateway custodies keys
  server-side; client-held keys are a later phase.

[Unreleased]: https://github.com/vul-os/vidmesh/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/vul-os/vidmesh/releases/tag/v0.1.0
