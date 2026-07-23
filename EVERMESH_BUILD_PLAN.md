## Agent prompt: build the Evermesh monorepo from zero

You are an expert protocol engineer and full-stack developer. Your task is to build
**Evermesh** — a decentralized video protocol and its reference implementations — as a
production-quality monorepo. Work through the phases in order. Do not skip the spec
phase: code follows spec, never the reverse.

A companion document, `EVERMESH_SPEC_PROPOSAL.md`, is provided alongside this plan and
is the authoritative source for all protocol semantics. Where this plan summarizes the
protocol, the spec proposal wins on conflict.

---

## 1. Mission and mental model

Evermesh is "many gateways, one substrate" video:

- A **substrate** of self-certifying data: signed records (CBOR, Ed25519, BLAKE3) and
  content-addressed blobs. It has no owner, no servers of record, and survives
  partition indefinitely (LANs, sneakernet, interplanetary links).
- **Gateways**: independent websites on their own domains that index/serve their own
  *selection* of the substrate. Moderation = selection. Compliance = subscribable
  signed takedown feeds. Gateways compete on product.
- **Nodes**: background apps that pin and seed chosen content (own videos first,
  subscriptions second). No public-facing duties.
- **Viewers**: browsers that verify signatures/hashes client-side via WASM and assist
  the swarm while watching.
- A **foundation** stewards the spec and operates nothing.

Non-negotiable design principles (violations are bugs):

1. **Minimal kernel** — only the record envelope, algorithm registry, identity
   rotation log, and blob addressing are kernel. Everything else is a record kind.
2. **Self-certifying** — no record's validity may depend on DNS, TLS, a server, or a
   chain. Bytes + math only.
3. **Forkability** — index replicable by anyone; identity portable; blobs re-hostable.
4. **Transport/storage agnostic** — blobs are hashes; retrieval info is additive hints.
5. **No mandatory dependencies, no token, economically neutral.**
6. **Partition tolerant** — no kind may require global consensus or ordering; records
   merge cleanly in any arrival order.
7. **Crypto agility** — every signature/hash carries an algorithm id; identity
   rotation is the migration path.
8. **Legibility** — boring formats, exhaustive docs, a stranger can reimplement from
   the spec alone.

## 2. Locked technology decisions

Do not revisit these:

- **Kernel, relay, node, WASM bindings: Rust** (2021 edition, stable toolchain).
  Single kernel crate compiled natively and to WASM — one crypto implementation
  everywhere. Crates: `ed25519-dalek`, `blake3`, `ciborium` (or `minicbor`), `tokio`,
  `axum` for the relay, `wasm-bindgen`/`wasm-pack` for bindings.
- **Gateway backend: TypeScript on Node 22+** (Fastify or Hono), consuming the kernel
  via the WASM package. SQLite (better-sqlite3) for the index store in v1.
- **Gateway frontend: React 18 + TypeScript (.tsx) + Vite + Tailwind CSS.** Video
  playback via `hls.js` behind a thin player component.
- **Monorepo tooling:** Cargo workspace for Rust; pnpm workspaces for JS; a top-level
  `justfile` (or Makefile) orchestrating both. CI via GitHub Actions.
- **Licenses:** spec CC-BY-SA-4.0; Rust crates MIT OR Apache-2.0; JS packages MIT.
- **Node app (Tauri 2)** is Phase 8 — scaffold only in v1.

## 3. Repository layout

Create exactly this structure (add files as needed, don't rearrange top level):

```
evermesh/
├── README.md                  ← flagship readme, see §12
├── LICENSE-MIT / LICENSE-APACHE / LICENSE-SPEC
├── justfile                   ← dev, build, test, conformance targets
├── Cargo.toml                 ← [workspace] members = crates/*
├── pnpm-workspace.yaml
├── .github/workflows/ci.yml   ← rust test+clippy+fmt, js lint+test, wasm build
├── assets/
│   ├── logo.svg               ← see §13
│   ├── logo-dark.svg
│   └── favicon.svg
├── spec/                      ← Phase 1 output, md files, see §5
│   ├── 000-overview.md
│   ├── 001-kernel.md
│   ├── 002-identity.md
│   ├── 003-kinds-registry.md
│   ├── 004-manifest.md
│   ├── 005-claims.md
│   ├── 006-relay.md
│   ├── 007-bundles.md
│   ├── 008-privacy.md
│   ├── 009-gateway.md
│   ├── 010-economics.md
│   └── 011-threat-model.md
├── crates/
│   ├── evermesh-kernel/        ← the permanent library
│   ├── evermesh-relay/         ← axum websocket+http relay binary
│   ├── evermesh-wasm/          ← wasm-bindgen wrapper over kernel
│   └── evermesh-node/          ← Tauri 2 scaffold (Phase 8)
├── packages/
│   ├── kernel-ts/             ← npm pkg wrapping the wasm build, typed API
│   └── ui/                    ← shared React components (player, record cards)
├── apps/
│   ├── gateway/
│   │   ├── server/            ← TS backend (index, blob store, feeds, moderation)
│   │   └── web/               ← React tsx frontend
│   └── site/                  ← evermesh.org landing page, static, see §14
└── tools/
    └── conformance/           ← test vectors + runner, see §11
```

## 4. Phase order and definition of done

Work strictly in this order; each phase has a "done" gate:

| Phase | Deliverable | Done when |
|---|---|---|
| 0 | Repo scaffold | layout above exists; CI green on empty workspaces; README stub |
| 1 | `spec/` written | all 12 files complete, cross-referenced, self-consistent |
| 2 | `evermesh-kernel` | all §6 APIs implemented; unit tests + test vectors pass; no `unsafe` |
| 3 | `evermesh-wasm` + `kernel-ts` | browser + node can verify/create records identically to Rust tests |
| 4 | `evermesh-relay` | relay passes conformance suite; two relays gossip records |
| 5 | gateway server | publish/index/serve flow works end-to-end against relay |
| 6 | gateway web | watch page, channel page, upload, comments working in browser |
| 7 | conformance suite | vectors cover every kind; `just conformance` green |
| 8 | node scaffold, site, polish | Tauri shell builds; landing page live; README final |

Commit per phase minimum; conventional commits; every crate/package has its own
focused README.

## 5. Phase 1 — write the spec as md files

Transcribe and expand `EVERMESH_SPEC_PROPOSAL.md` into the `spec/` folder. Rules:

- One concern per file, numbered as in §3 layout. Each file opens with Status,
  depends-on links, and a one-paragraph summary.
- Use RFC 2119 keywords (MUST/SHOULD/MAY) consistently.
- `001-kernel.md` MUST fully specify: the record envelope fields and canonical
  deterministic-CBOR encoding rules, the record-id derivation, the algorithm registry
  (launch: `sig_alg 1 = Ed25519`, `hash 1 = BLAKE3-256`), blob addressing
  (`b3-256:<hex>`), and the 1 MiB-chunk BLAKE3 Merkle layout for large blobs with
  verified range reads.
- `003-kinds-registry.md` assigns stable numeric ids to the launch kinds:
  `profile, rotation, delegate, manifest, supersede, retract, comment, reaction,
  follow, playlist, channel, mirror, similarity, claim.author, claim.license,
  claim.transfer, claim.dispute, notice.takedown, notice.counter, feed.takedown,
  endorse.gateway, receipt, attest, anchor, keygrant, live.manifest, live.chat`.
  Each kind gets: body schema (CBOR map, field types), refs semantics, validation
  rules, and one worked example.
- Every spec file ends with a **Test vectors** section listing which conformance
  vectors (§11) cover it.
- Write like the reader will reimplement from scratch in 2050 with no other context.

## 6. Phase 2 — `evermesh-kernel` public API

Implement (names indicative, keep the shape):

```rust
// identity
Keypair::generate() -> Keypair
Identity::genesis(&Keypair, recovery: &[PublicKey]) -> (Identity, Record)
Identity::rotate(...) -> Record            // signing-key or recovery-key authorized
Identity::verify_chain(&[Record]) -> Result<IdentityState>  // recovery precedence rule

// records
RecordBuilder::new(kind).refs(..).body(cbor).sign(&Keypair) -> Record
Record::verify(&self) -> Result<()>        // sig + id + canonical form
Record::id(&self) -> RecordId
codec::to_canonical_cbor / from_cbor / to_json / from_json

// blobs
Blob::hash_stream(reader) -> BlobId
ChunkTree::build(reader) -> ChunkTree      // 1 MiB chunks, blake3 merkle
ChunkTree::verify_range(range, chunks) -> Result<()>

// kinds (typed wrappers over body maps)
kinds::Manifest, kinds::Comment, kinds::Claim, ... ::parse(&Record) / ::build(..)

// bundles
Bundle::export(records, blobs, filter) -> impl Write
Bundle::import(reader) -> ImportResult      // verifies everything on ingest
```

Requirements: `#![forbid(unsafe_code)]`; no panics on untrusted input (fuzz the CBOR
and bundle parsers with `cargo-fuzz`, include the harnesses); `no_std`-compatible core
behind a feature flag if cheap, otherwise skip; exhaustive rustdoc; property tests
(proptest) for canonical-encoding round-trips and merge-order independence of chains.

## 7. Phase 3 — WASM + kernel-ts

`evermesh-wasm` exposes the kernel via wasm-bindgen. `packages/kernel-ts` wraps it in
an ergonomic typed API (`createRecord`, `verifyRecord`, `deriveId`, `hashBlobStream`
via WHATWG streams, `Identity` chain helpers) and ships dual ESM/CJS with `.d.ts`.
Golden rule: the same test vectors must pass in Rust, Node, and a headless browser
(playwright test in CI). If a vector passes in one runtime and fails in another, the
canonical encoding is broken — stop and fix.

## 8. Phase 4 — `evermesh-relay`

Axum binary. Endpoints:

- `WS /sync` — subscribe by filter (kinds, authors, refs, since), receive records;
  publish records (verified on ingest, unknown kinds accepted opaquely).
- `GET/HEAD /blob/{id}` and `PUT /blob` — optional blob sidecar (Blossom-style),
  verified against hash on write, range requests supported via chunk tree.
- `GET /info` — relay policy document: min PoW difficulty, rate limits, retention.

Storage: SQLite. Anti-spam: optional PoW-over-record-id check, per-key token-bucket
rate limits. Gossip: config lists peer relays; new records are forwarded with loop
suppression by record id. Ship a `docker-compose.yml` running two relays that gossip,
used by the conformance suite.

## 9. Phase 5 — gateway server (`apps/gateway/server`)

TypeScript service, consumes `kernel-ts`. Responsibilities:

- **Ingest**: subscribe to configured relays; maintain SQLite index of records it
  *selects* (policy engine below); store/pin blobs it serves in a content-addressed
  disk store.
- **Policy engine**: allow/deny lists by hash, key, kind; subscription to
  `feed.takedown` records with automatic de-index; per-item geo-block flags; every
  moderation action is local config, instant, and logged.
- **Upload pipeline**: accept file → hash → optional transcode via ffmpeg (720p/480p
  renditions, each signed as verifiable derivations per spec §5.1) → build + sign
  manifest (server-custodied keys for v1 users; document the custody/rotation story)
  → publish to relays → pin blobs.
- **Serving**: HLS packaging from pinned blobs; manifest/channel/comment REST API for
  the frontend; OpenGraph endpoints for share cards.
- **Compliance toolkit**: notice/counter-notice endpoints producing spec
  `notice.takedown`/`notice.counter` records; templated ToS/AUP md files in
  `apps/gateway/server/legal/`; a `CSAM.md` documenting the required hash-matching
  integration point with a pluggable interface (stub implementation clearly marked
  NOT-FOR-PRODUCTION).

## 10. Phase 6 — gateway web (`apps/gateway/web`)

React + tsx + Vite + Tailwind. Pages: home (latest/selected videos), watch page
(player, verified-badge showing client-side WASM verification of manifest signature
and chunk hashes, comments with threading, tip button rendering `receipt` records),
channel page (profile, videos, follow), upload flow, and a visible moderation-policy
page ("what this gateway serves"). Dark mode. Player: `hls.js` wrapped in
`packages/ui`. State: TanStack Query. No Redux. Accessibility: keyboard-navigable
player, captions track support from day one.

## 11. Phase 7 — conformance suite (`tools/conformance`)

- `vectors/` — JSON+CBOR fixture pairs for: every record kind (valid + a minimum of
  three invalid mutations each: bad sig, non-canonical encoding, wrong id), identity
  chains (rotation, recovery precedence, contested rotation), chunk-tree range
  proofs, and one full bundle round-trip fixture.
- A runner (Rust binary) that executes vectors against: the kernel crate, kernel-ts
  under Node, and a live relay over websocket.
- `just conformance` runs everything; CI-gated. This suite is what makes the
  two-implementations rule enforceable later — treat it as a first-class product.

## 12. README.md requirements

The root README is the project's front door — make it excellent:

- Logo at top (light/dark variants via `<picture>`), one-line tagline
  ("Many gateways. One substrate. Video that outlives its platforms."), badges
  (CI, license, spec version).
- A 10-line "what is this" in plain language, then the architecture diagram — commit
  a clean SVG diagram at `assets/architecture.svg` (creators → substrate
  [records+blobs] → gateways → viewers/nodes) drawn in the same visual style as the
  logo.
- Quickstart: `just dev` brings up relay + gateway + web with seeded demo content in
  under 5 minutes; document exact prerequisites.
- Monorepo map table, spec index links, "run your own gateway" section, contributing
  + RFC process pointer, license matrix, and an honest Status section (pre-alpha,
  what works, what doesn't).
- Every sub-crate/package README: purpose, API sketch, test instructions — no empty
  boilerplate.

## 13. Logo (assets/logo.svg)

Design a clean, original mark: a **mesh/lattice motif converging into a play
triangle** — e.g. a hexagonal mesh of nodes and edges where the negative space (or
the central cluster) forms a subtle play button. Requirements: hand-written SVG,
`viewBox="0 0 256 256"`, no raster effects, no gradients (flat, 2 colors max),
must read at 16px (favicon) and 512px; provide `logo.svg` (color on transparent),
`logo-dark.svg` (for dark backgrounds), `favicon.svg` (simplified mark only).
Suggested palette: deep indigo `#3C3489` + signal teal `#1D9E75` on transparent —
but ensure both variants pass contrast on their intended backgrounds. Include the
wordmark "evermesh" in lowercase, set in an open-license geometric sans (embed as
path outlines, not font-family, so it renders everywhere).

## 14. Landing page (apps/site)

Static, no framework — one hand-crafted `index.html` + `style.css` + assets, deployable
to any static host. Requirements:

- Semantic HTML5, lighthouse ≥ 95 across the board, no JS required for content.
- Complete head: title, meta description, canonical, `og:title/description/image/url/type`,
  `twitter:card summary_large_image`, theme-color (light+dark), favicon.svg + png
  fallbacks + apple-touch-icon, webmanifest, robots.txt, sitemap.xml.
- Generate `assets/og-image` (1200×630 SVG source + exported png) matching the brand.
- Sections: hero (logo, tagline, "Read the spec" + "Run a gateway" CTAs), the
  three-role explainer (viewer/node/gateway), principles summary, spec index,
  FAQ (incl. "how is this different from PeerTube / Nostr / Bluesky"), footer with
  repo + license. Accessible (landmarks, alt text, prefers-color-scheme support).

## 15. Working rules for the agent

- Spec before code; when implementation reveals a spec bug, fix the spec file in the
  same commit and note it in `spec/CHANGELOG.md`.
- No placeholder/dead code paths in shipped phases; stubs must be clearly named and
  documented (the CSAM interface stub is the only sanctioned one).
- Test-first for the kernel; every bug found gets a regression vector in
  `tools/conformance`.
- Keep dependencies minimal and audited (`cargo deny`, `pnpm audit` in CI).
- Never invent protocol semantics — if the spec proposal is ambiguous, choose the
  interpretation that best satisfies the eight principles in §1, and document the
  decision in the relevant spec file under a "Decisions" heading.
- All public-facing text (README, site, error messages) in clear, plain English —
  no crypto-hype vocabulary.

**Final acceptance:** `just dev` demo works end-to-end (upload → publish → watch on a
second gateway instance pointed at the same relays → comment → records verified in
the browser), `just conformance` passes, CI green, README and site complete.
