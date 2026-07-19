# @vidmesh/gateway-server

The reference Vidmesh gateway backend: subscribes to relays, runs a local
selection (moderation) policy over what it ingests, indexes selected
records in SQLite, pins the blobs it serves in a content-addressed store,
runs the upload/transcode pipeline, packages HLS, exposes the REST API
`apps/gateway/web` consumes (`apps/gateway/API.md`), and ships the
compliance toolkit (CSAM gate, notice/counter-notice intake, takedown-feed
subscription, custodial identity + the non-negotiable export path).

Implements build plan §9 and spec 009-gateway.md. **Status: implemented,
not yet run** — see "What hasn't been exercised" below.

## Architecture

```
relay(s) ──ws──▶ relay.ts ──▶ ingest.ts ──▶ policy.ts (selection)
                                 │                │
                                 ▼                ▼
                            ingest-kinds.ts   policy_denylist,
                            (per-kind SQL)     policy_log

fastify ── api/*.ts ── db.ts (SQLite) ── blobstore.ts (content-addressed disk)
   │                                          │
   ├── custody.ts (user-signed records)       ├── media.ts (Range, HLS)
   ├── gateway-identity.ts (gateway-signed)   └── upload.ts → transcode.ts
   └── session.ts (cookie auth)                              (ffmpeg, optional)
```

One code path indexes every record, regardless of where it came from:
`ingest.processRecord()` is called identically by `relay.ts` (records from
peers), `custody.ts`/`gateway-identity.ts` (a user or the gateway itself
just signed something), and `upload.ts` (a freshly published manifest).
There is no "trust our own content more" shortcut — the gateway's own
uploads go through the same envelope-verify → policy-check → kind-dispatch
pipeline as anything received over the wire.

### File map

| File | Responsibility |
|------|-----------------|
| `src/main.ts` | Process entry point: config, DB, relays, fastify, signal handling |
| `src/server.ts` | Builds the fastify app (plugins, error handler, routes) — shared by main.ts and tests |
| `src/config.ts` | JSON config + env overrides, zod-validated |
| `src/db.ts` | SQLite schema + migrations (`PRAGMA user_version`), indexes |
| `src/cbor.ts` | Minimal canonical CBOR codec (relay frames only — NOT the record codec, which is the kernel's) |
| `src/relay-frames.ts` | Typed spec 006 §1 frame encode/decode on top of cbor.ts |
| `src/relay.ts` | Reconnecting websocket client per relay: REQ/PUB, `since`-cursor persistence |
| `src/envelope.ts` | Extracts kind/author/refs/body from the kernel's `"1".."7"`-keyed JSON interchange form |
| `src/ingest.ts` | The selection pipeline: verify → policy → CSAM gate (manifests) → dispatch → store |
| `src/ingest-kinds.ts` | Per-kind table indexers, reused for both first-arrival and `supersede` replacement |
| `src/policy.ts` | Allow/deny by identity/hash/kind, `feed.takedown` application, audit log |
| `src/csam.ts` | `CsamMatcher` interface + `StubMatcher` (verbatim per `CSAM.md`) |
| `src/custody.ts` | v1 custodial identities: register, sign-on-behalf, the export path |
| `src/gateway-identity.ts` | The gateway's own operator identity, for compliance records |
| `src/session.ts` | Cookie session store |
| `src/upload.ts` | Upload pipeline orchestration |
| `src/transcode.ts` | ffmpeg/ffprobe invocation (only ever called when `ffmpegPath` is configured) |
| `src/blobstore.ts` | Content-addressed disk store (`<blobDir>/<ab>/<cd>/<hex>`) |
| `src/media.ts` | Blob/HLS serving with Range support and the serve-time CSAM gate |
| `src/og.ts` | Share-card field computation |
| `src/api/*.ts` | One module per API.md section |

## Config reference

Copy `config.example.json`, fill it in, point `GATEWAY_CONFIG` at it (or
pass a path to `loadConfig()`):

| Field | Meaning |
|-------|---------|
| `port`, `host` | Listen address |
| `dbPath` | SQLite file path (`:memory:` for tests) |
| `blobDir` | Content-addressed blob store root (also holds `tmp/` and `hls/` working dirs) |
| `relays` | Full `wss://.../sync` URLs to subscribe/publish to |
| `policyFilePath` | The moderation policy JSON (see below); hot-reloaded on `SIGHUP` |
| `sessionSecret` | ≥32 chars; signs the session cookie |
| `custody.secret` | ≥32 chars; HKDF input for wrapping custodied secret keys. **Keep distinct from `sessionSecret`** so rotating cookie signing never touches custodied keys |
| `custody.contestWindowSeconds` | Passed to every genesis identity this gateway creates (spec 002 §4.1) |
| `ffmpegPath`, `ffprobePath` | Optional. Omit both to run original-only (see below) |
| `uploadMaxBytes` | Enforced both by `@fastify/multipart`'s `fileSize` limit and a defensive check in the pipeline |
| `gatewayName`, `gatewayDescription`, `publicBaseUrl` | Shown in `/api/info`, embedded in manifest `hints` and OG URLs |
| `uploadEnabled` | `POST /api/upload` returns `upload_failed` when false |

`policyFilePath` points at a JSON file shaped like `policy.example.json`:
`name`/`description`/`moderationPolicyHtml` (the visible moderation-policy
page, spec 009 §1), `denyIdentities`/`denyBlobHashes`/`denyRecordIds`/
`denyKinds` (static config denylists), `geoBlocks`, and `feeds` (the
`{feed, publisher}` pairs this gateway subscribes to — a `feed.takedown`
batch is only ever auto-applied for a subscribed pair; subscribing *is*
the policy decision, per spec 009 §3).

## Custody story and the export guarantee

**This is v1, explicitly not hardware-grade.** A registered user's Ed25519
secret key is encrypted at rest with AES-256-GCM, whose key is derived via
HKDF-SHA256 from a single server-side secret (`custody.secret`) plus the
identity id as salt. There is no hardware enclave, no threshold scheme, no
per-user passphrase gate on the wrapping key itself — anyone with
`custody.secret` and DB access can decrypt every custodied secret. This is
the same trust model as "the gateway can sign on your behalf," stated
plainly rather than dressed up.

What makes this acceptable per spec 009 §5 ("custody must never be
capture") is that **the exit path is real and always available**:
`POST /api/me/export` (password re-confirmed, rate-limited to 5 attempts/
hour via an indexed `export_attempts` log) returns the genesis record and
the raw 32-byte secret key. With those two things alone — no gateway
cooperation, no export format specific to this codebase — a user can
`Keypair.fromSecret()` and `identity.rotate()` to a new signing key on any
other gateway or client, per spec 002 §3. Losing this gateway loses
nothing but convenience.

The gateway's own compliance records (`notice.takedown`/`notice.counter`)
are signed by a *separate* operator identity (`gateway-identity.ts`), not
by any user — created and published once on first boot, encrypted the
same way.

## What degrades without ffmpeg

`ffmpegPath` (and `ffprobePath`) are optional. Without them:

- No `ffprobe` metadata: manifest `original.codec`/`width`/`height`/
  `duration` are `"unknown"`/`0`/`0`/`0`.
- No 720p/480p renditions, no thumbnail, no HLS packaging.
- `Video.playback.hlsUrl` is `null`; `mp4Url` (the original blob) is
  always present — playback falls back to the original file via
  `GET /media/blob/{blobId}`, which always works since every upload is
  hashed and stored regardless of ffmpeg's presence.

With ffmpeg configured, the upload pipeline additionally: probes the
original, generates a thumbnail, transcodes 720p/480p whole-file MP4
renditions (skipping any target taller than the source), signs a
derivation statement per rendition (spec 004 §3.1, see below), and
packages each rendition into fMP4 HLS segments.

### HLS packaging is a serving-layer concern, not substrate state

Spec 004's `Rendition` type (§3) names exactly **one blob per rendition**
— the whole encoded file, covered by one `derivation_sig`. It has no
segment list; segmenting a `Rendition` differently per gateway is exactly
the point (spec 009 §1: "gateways compete on transcoding"). So the signed
manifest only ever references whole-file rendition blobs. HLS segments
(`init.mp4`, `seg_NNN.m4s`) are *also* stored as ordinary content-addressed
blobs, but the mapping from manifest → rendition → ordered segment list
lives in this gateway's own `hls_segments`/`hls_renditions` tables, not in
any signed record. `media.ts` regenerates `master.m3u8`/`index.m3u8` from
those tables on every request. A viewer verifying the manifest
client-side only ever needs to verify the one whole-file rendition blob's
hash — the HLS slicing is this gateway's presentation choice, re-derivable
by anyone from that same blob.

## The CSAM gate

Two integration points per `CSAM.md`, both wired:

1. **Upload, pre-publish** (`upload.ts`): every blob this gateway
   originates — original, each rendition, the thumbnail — is checked
   before being committed to the content store or referenced by a
   manifest. A match here means nothing gets hashed into a servable
   path, published, or pinned.
2. **Serve-time** (`media.ts`), as the practical closure of "relay-ingest,
   pre-index": a manifest ingested from a relay typically references
   blobs this gateway hasn't fetched yet, so there's nothing local to hash
   at ingest time. Every blob is instead gated the moment it's actually
   about to be streamed to a viewer — `blobs.csam_checked`/`csam_match`
   cache the verdict so a cleared blob isn't re-hashed on every view, but
   the check runs at least once before any byte reaches a client, which
   is the `CsamMatcher` contract's actual requirement ("MUST be called
   before any blob is served"). `ingest.ts` also runs the same check
   opportunistically for manifests whose blobs happen to already be
   local (e.g. mirrored content this gateway already pinned).

**`StubMatcher` is active by default and always returns `{ match: false
}`.** The gateway prints a large, impossible-to-miss warning to stderr at
boot whenever it's the configured matcher. Per `CSAM.md`, running this in
front of real users is non-compliant and outside the trademark program —
see that file for what a real integration requires.

## Run instructions

```
pnpm install                                   # from the repo root
pnpm --filter @vidmesh/kernel build:wasm       # required — see below
cp apps/gateway/server/config.example.json apps/gateway/server/config.json
cp apps/gateway/server/policy.example.json apps/gateway/server/policy.json
# edit config.json: real sessionSecret/custody.secret (32+ chars each),
# a relay URL if you have one, ffmpegPath if you have ffmpeg installed
cd apps/gateway/server
GATEWAY_CONFIG=./config.json pnpm dev
```

`pnpm lint` runs `tsc --noEmit`; `pnpm test` runs `node --test` over
`test/**/*.test.ts`.

## Known limitations / not implemented

- **`supersede`/`retract` arriving before their target is dropped, not
  queued.** Partition tolerance means a record can legitimately arrive
  before what it references; this gateway rejects with `target_unknown`
  rather than holding it for replay. A production deployment would want a
  small pending-target buffer with periodic retry.
- **Profile "latest by chain order" is approximated as latest-by-
  received-order.** See "kernel-ts API surface notes" below — exact chain-
  order ranking isn't computable from the exposed API without maintaining
  a full per-identity rotation-chain cache, which this phase doesn't
  build.
- **Retracting a `reaction`** deletes the row outright (reactions have no
  `retracted` flag, since they're normally superseded by a newer reaction
  instead). **Retracting a `profile`** is a no-op beyond storing the
  retract record itself — there's no clear rendering rule for "no
  profile" distinct from "never posted one."
- **`playlist`, `mirror`, `similarity`, `endorse.gateway`, `attest`,
  `anchor`, `keygrant`, `delegate`, `live.*`** pass envelope+kind
  validation and selection and are stored (retrievable via
  `GET /api/records/{id}`), but have no dedicated index table or product
  surface — API.md doesn't define endpoints for them in this phase.
  `delegate` in particular means third-party (non-author) rendition
  producers aren't authorized/checked anywhere yet; every rendition this
  gateway signs is self-produced.
- **Search is a plain `LIKE` scan**, not FTS5 — fine at reference scale,
  called out as a place competing gateways differentiate (spec 009 §6).
- **HLS segment/init CSAM checks are inherited, not re-run.** Only the
  original, each whole rendition file, and the thumbnail are freshly
  checked at upload time; the many small `.m4s` segments derived from an
  already-cleared rendition are stored with `csam_checked` pre-set rather
  than re-hashed individually.

## kernel-ts API surface notes

The build plan's brief assumed `@vidmesh/kernel` had no raw-sign
primitive and asked for derivation signing to be isolated behind a
`TODO(kernel-ts: expose raw sign)` with renditions omitted from published
manifests until it landed. **That assumption doesn't hold in this repo**:
`packages/kernel-ts/src/index.ts` already exports `signDerivation()`, and
cross-checking it against `crates/vidmesh-wasm/src/lib.rs`'s
`sign_derivation` confirms it builds exactly the spec 004 §3.1 statement
(`canonical_cbor([original, rendition, codec, width, height, bitrate])`)
and signs `"vidmesh:derivation:v1" || BLAKE3-256(stmt)`. So this gateway
signs and publishes real renditions with real `derivation_sig`s
(`upload.ts` → `custody.signDerivationFor()`, which is kept as one
isolated function per the build plan's structural intent even though
there's no gap to work around).

The one real gap found: **kernel-ts exposes only the *resolved* identity
chain state** (`identity.verifyChain()` → final signing key/depth/head),
not an ordered list letting a caller rank an arbitrary historical
signing key's position in the chain. Spec 002 §6 defines "current
profile" as "latest by rotation-chain order of the signing key, then
supersession — never `created_at`." Implementing that exactly would need
either a new kernel-ts export (e.g. a chain-order comparator, or the full
ordered rotation list) or this gateway maintaining its own per-identity
rotation cache and walking it manually. Neither exists yet; `ingest-kinds.ts`'s
`indexProfile()` documents the approximation in place (latest-by-received-
order) and where the real fix belongs.

## API.md ambiguities and how they were resolved

- **HLS segment naming/schema**: API.md just says
  `GET /media/hls/{manifestId}/{rendition}/{segment}.m4s` without
  defining what `{segment}` is. Resolved as `init` for the fMP4
  initialization segment and zero-padded ordinals (`000`, `001`, …) for
  media segments, both extensionless in the DB and suffixed with `.m4s`
  only at the route.
- **Compliance notice signer**: API.md doesn't say whose key signs
  `notice.takedown`/`notice.counter`; the build plan does ("signed by the
  gateway's own identity"), so `gateway-identity.ts` exists and
  `api/compliance.ts` uses it exclusively, never the submitter's.
- **`/api/compliance/notice` "applies local policy"**: read as spec 009
  §1's "local policy is absolute and instant" applied directly to the
  named subjects, not just recorded as a `notice.takedown` for someone
  else to act on later — `policy.denylistForNotice()` de-indexes the
  subjects synchronously in the same request.
- **Counter-notice reinstatement**: not automatic. Every regime this
  toolkit targets (see `legal/DMCA.md`) has a waiting/review period
  between counter-notice and reinstatement; auto-reinstating on `POST
  /api/compliance/counter` would be actively wrong.
- **Cursor pagination**: implemented as `received_at` (relay-local
  receive order), never the author-claimed `created_at`, per spec 001
  §10's "consumers MUST NOT... assume arrival order reflects creation
  order" — the untrusted field can't be allowed to drive "newest first."
- **Post-hoc de-indexing**: a `feed.takedown` batch can denylist a
  manifest *after* it was already indexed. Every read route that serves a
  single manifest or lists them re-checks `policy_denylist` live
  (`NOT EXISTS`/lookup), rather than relying on the ingest-time policy
  check alone — moderation stays "instant" even for already-indexed
  content, matching spec 009 §1.
