# Evermesh Protocol — Specification Proposal (Draft 0.1)

**Status:** Pre-RFC draft for discussion
**Editor:** (you)
**Intended home:** evermesh.org
**License:** CC-BY-SA 4.0 (spec text); reference implementations MIT/Apache-2.0

---

## Abstract

Evermesh is a protocol for publishing, distributing, and discussing video without any
central operator. It separates the system into a minimal, permanent **kernel** (signed
records and content-addressed blobs) and a competitive **edge** (gateways, nodes, and
clients) where all contested concerns — moderation, economics, discovery, presentation —
are resolved by choice rather than by protocol.

The design goal is stated as a survival test:

> The network must remain fully functional after the death or betrayal of every
> organization involved in it, including the one that wrote this document.

A secondary, equally binding goal is partition tolerance at civilizational scale: the
protocol must operate across links measured in minutes (interplanetary), days
(sneakernet), or never (permanently isolated local networks), degrading only in
freshness, never in integrity.

---

## 1. Design principles

These principles are normative. A proposal that violates one of them is rejected
regardless of its benefits.

1. **Minimal kernel.** The kernel defines only: the record envelope, the signature and
   hashing scheme (with algorithm agility), the identity rotation log, and blob
   addressing. Everything else is an extension expressed as record kinds.
2. **Self-certifying data.** Every record and blob is verifiable using only its own
   bytes and mathematics. No record's validity may depend on reaching a server, a
   blockchain, a DNS name, or a certificate authority.
3. **Forkability.** The full index must be replicable by anyone; identities must be
   portable across all infrastructure; blobs must be re-hostable by hash. Any community
   must be able to leave with everything except the brand.
4. **Transport and storage agnosticism.** The kernel never names a transport. Blobs are
   hashes; manifests carry *hints* (HTTP, torrent, future transports) which are additive.
5. **No mandatory dependencies.** No company, blockchain, token, or external network may
   be required for correct operation. All such integrations are optional pointer/hint
   types.
6. **Economic neutrality.** The protocol carries payment primitives (pointers, receipts,
   disclosures) and takes no position on business models. No protocol token exists or
   will exist.
7. **Edge-resolved moderation.** The substrate never deletes. Gateways select what they
   index and serve. Compliance is implemented as subscribable, signed, auditable feeds —
   plural and opt-in.
8. **Partition tolerance.** No record kind may require global consensus, global
   ordering, or synchronous availability. Arrival order is never assumed to be creation
   order.
9. **Two-implementations rule.** No extension enters the spec until two independent
   implementations interoperate against the conformance suite.
10. **Legibility.** Formats are plain, documented, and boring (CBOR/JSON, standard
    codecs). A future reader must be able to reconstruct a working implementation from
    the spec alone.

---

## 2. Roles

Participation is a ladder; every rung is opt-in and independently useful.

| Role | Runs | Responsibilities | Legal posture |
|---|---|---|---|
| **Viewer** | Browser/app only | None; contributes swarm bandwidth while watching | Consumer |
| **Node** | Background app (desktop/server) | Pins chosen content; seeds watched content; honors its own storage/bandwidth budget | Comparable to a BitTorrent seeder; pins only by explicit choice or subscription, never arbitrary anonymous data |
| **Gateway** | Public web service on its own domain | Indexes/serves selected content; moderation policy; optional transcoding, search, economics; jurisdiction compliance | An ordinary website hosting user content (DMCA / DSA / local intermediary regimes) |
| **Foundation** | Nothing operational | Stewards the spec, record-kind registry, conformance suite, trademark | Deliberately never operates a gateway |

The **creator** is not an infrastructure role: a creator is any keyholder who publishes
manifests. Creators MAY be their own node and their own gateway.

---

## 3. Kernel

### 3.1 Record envelope

A record is a small, signed, immutable statement. All application data — manifests,
comments, claims, follows, takedowns, receipts — is records.

```cbor
Record {
  id:        blake3-256 hash of the canonical encoding of (kind..sig excluded)
  kind:      uint            ; registered record kind
  author:    IdentityRef     ; see §4
  created_at: int64          ; author-asserted unix time (untrusted; see §3.4)
  refs:      [RecordId | BlobId]  ; parents, subjects, threading
  body:      kind-defined CBOR map
  sig_alg:   uint            ; registered signature algorithm id
  sig:       bytes
}
```

Normative rules:

- Records are immutable. "Edit" and "delete" are new records (kinds `supersede`,
  `retract`) that reference the target; consumers decide how to render them. Retraction
  is a request, not an erasure.
- Canonical encoding is deterministic CBOR (RFC 8949 §4.2.1). JSON is a permitted
  interchange representation; signatures are always over the CBOR canonical form.
- `sig_alg` and the hash function are registry entries, not constants (crypto agility).
  Launch set: Ed25519 signatures, BLAKE3-256 hashing. Post-quantum entries are added by
  registry, and identities migrate via the rotation log (§4.2), not by protocol fork.
- Unknown kinds MUST be relayed and stored opaquely by index infrastructure (relays
  cannot veto innovation) and MUST be ignored safely by clients.

### 3.2 Blobs

A blob is an opaque byte sequence addressed by its hash (`b3-256:<hex>`). Large blobs
use a chunked Merkle layout (fixed 1 MiB chunks, BLAKE3 tree) so that ranges can be
fetched and verified independently — required for streaming and swarm exchange.

The kernel does not know what a blob contains. Video, thumbnails, captions, encrypted
ciphertext — all are just blobs referenced from records.

### 3.3 Fetch hints

Records that reference blobs MAY carry hints: a typed, ordered list of retrieval
suggestions. Registered hint types at launch: `https`, `torrent-v2`, `relay-blob`
(Blossom-compatible), `bundle` (§8). Hints are advisory; any host answering for a hash
is equivalent. New transports are new hint types.

### 3.4 Time

`created_at` is an unverified author claim. Where ordering matters (claims, disputes),
records SHOULD be strengthened by **anchoring**: a periodic `anchor` record commits a
Merkle root of recently seen record ids to one or more external timestamp systems
(e.g., OpenTimestamps). Anchoring is an optional extension; the kernel only guarantees
tamper-evidence, not absolute time.

---

## 4. Identity

### 4.1 Identifiers

An identity is a stable identifier bound to a *current* signing key through a
**rotation log** — an ordered chain of signed `rotation` records stored in the
substrate itself. This is deliberately DID-like without requiring any external DID
infrastructure.

- Identities are never derived from or bound to domains, gateways, or relays.
- A fresh identity's id is the hash of its genesis rotation record.

### 4.2 Rotation and recovery

A rotation record names the new signing key and is authorized by either the current
signing key or any of the identity's declared **recovery keys**. Recovery keys may be
held by the user (hardware key), their gateway (custodial convenience), or a threshold
of social contacts. Verifiers accept the longest valid chain; a recovery-key rotation
supersedes signing-key rotations within a declared contest window, giving theft
recovery precedence over the thief.

Custody is permitted and expected for mainstream users; **custody must never be
capture** — leaving a custodial gateway is a rotation, available at any time.

### 4.3 Profiles

Kind `profile`: display name, avatar blob, payment pointers (§10), declared relays and
seed endpoints. Profiles are ordinary records; latest-wins per identity by chain order.

---

## 5. Content layer

### 5.1 Video manifest

Kind `manifest` is the canonical identity of a video.

```
body {
  title, description, tags, language
  original:   { blob, codec, duration, dimensions }
  renditions: [ { blob, codec, resolution, bitrate, produced_by: IdentityRef, sig } ]
  captions:   [ { blob, language, format } ]
  thumbnail:  blob
  encryption: null | { scheme, key_hint }        ; §9
  license:    SPDX id | "all-rights-reserved" | "mirror-freely" | "endorsed-only"
  sponsorship: [ { start, end, sponsor_label } ]  ; disclosure field
  payment:    [ PaymentPointer ]                  ; §10
}
```

- Renditions are **verifiable derivations**: each declares the original hash it derives
  from and is signed by its producer (the creator, or a transcoder identity the creator
  delegated to via a `delegate` record). Gateways may serve third-party renditions
  without blind trust.
- The rendition family shares the manifest's identity: byte-different encodings of the
  same video do not fragment into separate "videos."
- `license` is machine-readable creator consent. It does not enforce anything; it makes
  violations legible and gives compliant gateways something to honor automatically.

### 5.2 Deduplication

- **Exact duplicates** are impossible by construction: identical bytes are one hash, and
  a re-upload merely adds a host for the existing blob.
- **Hosting is discovery, not registry**: "who has this blob" is answered live by
  swarm/tracker announce plus explicit `mirror` records ("identity X pins blob H"),
  which also drive node subscriptions and archival feeds.
- **Near-duplicates** (re-encodes, trims) are handled at the edge: any party may publish
  `similarity` assertion records ("blob J ≈ manifest M, method=phash-v2, score=0.97").
  Similarity is deliberately NOT kernel truth — algorithms improve and matches are
  contestable — it is evidence that gateways weigh, typically folding duplicates into
  the canonical manifest page and redirecting payment pointers when the claims chain
  (§6) supports it.

---

## 6. Claims and provenance

A per-video, append-only, tamper-evident chain of legal and provenance statements,
built from ordinary records referencing the manifest:

| Kind | Statement |
|---|---|
| `claim.author` | "I authored this work" |
| `claim.license` | grant or change of license terms |
| `claim.transfer` | rights assignment to another identity (signed by assignor) |
| `claim.dispute` | contest of a prior claim, with free-text/blob evidence |
| `notice.takedown` | a structured legal notice (e.g., DMCA) as a signed record |
| `notice.counter` | counter-notice |

Normative honesty requirement: implementations MUST present claims as *assertions with
provenance*, never as verified truth. Signatures prove authorship of the statement, not
the statement. Strength comes from composition: anchored timestamps (§3.4) for
priority evidence, C2PA-compatible capture metadata carried as claim attachments, and
the author's off-platform footprint. Courts and gateways interpret; the protocol only
preserves the evidence trail.

---

## 7. Index and relay layer

Relays store and gossip records. The full metadata corpus is small relative to blobs
and is designed for whole or filtered replication by anyone (forkability).

Anti-spam is layered, never centralized:

- **Write friction:** records carry optional proof-of-work over their id; relays
  advertise their minimum difficulty. Calibrated to be free-feeling for humans,
  expensive in bulk.
- **Relay policy:** per-key rate limits; relays are free to be selective (a relay is
  not a gateway and carries no serving obligations).
- **Read-side reputation:** ranking, counting, and surfacing are gateway concerns,
  weighted by key age, PoW spent, web-of-trust distance, and gateway-local signals.
  Spam may exist in the substrate; it competes for surfacing it never wins.

Aggregate numbers (view counts, like counts) are **per-gateway computed claims**,
optionally published as signed tallies others may sum. The spec explicitly embraces
that different gateways show different numbers. Fraud-proof global counting is
declared out of scope (it is the ad-fraud problem in disguise).

---

## 8. Distribution, bundles, and partition tolerance

### 8.1 Live transports

Reference transports: HTTPS range requests, torrent-v2 swarms, and a WebRTC
viewer-assist swarm for HLS segments (PeerTube-style). Live streams hash segments as
produced under a rolling signed manifest that becomes the VOD record at stream end.

### 8.2 Bundle format (normative in v1)

A **bundle** is a single-file, self-verifying export: a CAR-like container of records
plus blobs plus an index. One well-defined operation — `export(filter) → bundle`,
`import(bundle)` — makes every non-interactive transport equivalent: hard drives,
radio bursts, satellite windows, delay-tolerant networking (Bundle Protocol),
inter-site sync, backups, and archival ingestion (Internet Archive).

### 8.3 Partition posture

- No kind may require global consensus or synchronous availability.
- Signed parent references make forums, comment threads, and claim chains merge
  cleanly regardless of arrival order or delay (git-like reconciliation).
- Isolated partitions (a LAN, a habitat, a continent) run complete replicas:
  gateways discoverable by local announcement (mDNS), no DNS or CA required for
  correctness. Competing claims created in isolation coexist on merge; anchoring
  provides ordering evidence.
- Freshness degrades with link delay; integrity never does.

---

## 9. Privacy

Privacy is key distribution, not server permission. The substrate never inspects blobs.

| Mode | Mechanism |
|---|---|
| Public | Plain blobs; manifest published to relays |
| Unlisted | Encrypted blobs; manifest unpublished; decryption key travels in the URL fragment |
| Private | Encrypted blobs; content key wrapped to each recipient's public key via `keygrant` records or encrypted DMs |
| Gated | As Private, with a gateway acting as key vendor (pay → receive wrapped key) |

Metadata privacy is explicit: private manifests are never published to public relays.
Encryption fields exist in the manifest format from v1 (retrofitting privacy onto a
plaintext-assuming format is rejected as a known failure mode). Nodes holding
ciphertext they cannot inspect, by explicit subscription only, is the intended and
most legally defensible posture.

---

## 10. Economics (neutral primitives only)

The protocol ships three primitives and no business model:

- **Payment pointers** (in profiles and manifests): a typed, ordered list —
  `lightning`, `usdc-base`, `stripe`, `paypal`, future types. Rail-agnostic by design;
  fiat is first-class; no rail is required.
- **Receipts** (kind `receipt`): signed statements linking payer, amount, rail, and an
  optional message to a manifest or stream — the zap pattern; renders as
  tips/superchats at gateway discretion.
- **Disclosures**: the manifest `sponsorship` field for creator-embedded sponsorship.

Non-normative guidance: gateways publish signed revenue-share commitments; creators
publish `endorse.gateway` records; audiences follow endorsements. Trust through
auditability and exit, not cryptographic enforcement. A protocol token is permanently
out of scope (Principle 6).

---

## 11. Gateways

A gateway is a public web service, on its own domain, that indexes and serves a
*selection* of the substrate. Selection is the moderation model:

- **Local policy is absolute and instant:** allow/deny by hash, key, kind, or category
  is gateway configuration, not protocol action. Non-serving is a first-class, cheap
  operation. Nothing a gateway does removes anything from the substrate.
- **Compliance feeds:** organizations publish signed feeds of takedown-subject hashes
  and keys (kind `feed.takedown`). Gateways subscribe per their jurisdiction and
  policy, inheriting the compliance work of the ecosystem the way mail servers consume
  blocklists. Feeds are plural, opt-in, and auditable — an over-blocking feed loses
  subscribers to a competitor. Notices themselves (§6) are machine-readable records.
- **CSAM handling is the single non-configurable element of the reference gateway:**
  industry hash-matching at upload and index time plus mandatory reporting workflow.
  This is a reference-implementation requirement, not a kernel rule (the kernel cannot
  enforce it), stated here as a condition of the trademark program (§13).
- **Legal toolkit ships with the software:** templated ToS/AUP, DMCA agent guidance,
  notice/counter-notice UI, per-item geo-blocking, age-gating hooks, and jurisdiction
  compliance profiles (US/DMCA, EU/DSA, others as contributed). Lowering the legal
  cost of running a gateway is decentralization infrastructure.
- **Rogue gateways** face their own jurisdictions alone. Liability does not propagate:
  the foundation operates nothing, other gateways serve their own selections, nodes
  pin only by choice. The network's remedies are social and structural —
  disassociation feeds, trademark denial, creator non-endorsement — never protocol
  deletion, because any mechanism strong enough to force one gateway's compliance is
  strong enough to censor the network.

Gateways also carry the competitive product surface: transcoding services, search,
recommendation (see feeds below), custodial key management, economics.

**Portable recommendation feeds:** a feed is a signed object (algorithm reference or
service endpoint) any gateway can embed — recommenders need not be gateways. This
keeps discovery contestable even when one gateway dominates; the metadata corpus
being fully replicable guarantees any competitor can bootstrap search without
permission. The win condition for decentralization is not preventing dominance but
keeping it permanently contestable.

## 12. Social and interactive kinds

All state lives in the substrate as signed records; interpretation, ranking, and
presentation live at gateways.

| Kind | Notes |
|---|---|
| `comment` | references manifest or parent comment; threads merge by refs |
| `reaction` | typed reactions; counts are per-gateway aggregates (§7) |
| `follow`, `playlist`, `channel` | signed lists; fully portable across gateways |
| `live.manifest`, `live.chat` | rolling signed segment manifests; ephemeral pub-sub chat over relays |
| `receipt` | tips/superchats (§10) |
| `attest` | portable reputation: "gateway G attests K reached milestone X" — achievements survive any gateway |
| `endorse.gateway` | creator designates official gateways |
| `mirror`, `similarity`, `anchor`, `feed.*` | as defined above |

New features are new kinds in the registry, subject to the two-implementations rule.
The kernel is never versioned for features.

---

## 13. Governance

- The spec, registry, and conformance suite live under open licenses at the
  foundation, which is funded by grants/donations, holds the trademark, and is
  constitutionally barred from operating gateways or issuing tokens.
- Changes follow a lightweight RFC process; kinds graduate only via the
  two-implementations rule against the public conformance suite.
- "Evermesh-compliant" is a certifiable trademark claim (reference-gateway obligations,
  including §11 CSAM handling), separable from protocol participation, which is
  permissionless.
- The constitution's real enforcement mechanism is Principle 3: the community's
  credible ability to fork with everything. This document is written to survive its
  authors.

## 14. Security considerations (summary)

Signatures authenticate statements, not truth (§6). Counts are claims (§7). PoW
raises spam cost without excluding humans (§7). Key theft is bounded by
recovery-precedence rotation (§4.2). Encrypted content leaks size and timing but not
content or (if unpublished) metadata (§9). The kernel's attack surface is
deliberately limited to signature verification, hashing, and CBOR parsing. A full
threat model is a required v1 companion document.

## 15. Relationship to prior art

Nostr contributes the permissionless signed-record substrate and zap pattern; AT
Protocol the rotation-based identity and the app-view/gateway separation; PeerTube
the production playbook for federated video and swarm-assisted HLS; Scuttlebutt the
offline-first, bundle-syncable posture (with content addressing correcting its
single-log limitation). Evermesh's contribution is the combination, plus the two
layers all of them lack: an accountable-but-optional gateway compliance model and a
claims/provenance chain.

**Open strategic question (decide before v1):** clean-room protocol vs. Nostr
superset — adopting Nostr's envelope and Blossom blob conventions and expressing
§4–§12 as extension kinds would inherit a live ecosystem at the cost of kernel
purity (notably the envelope format and secp256k1 launch algorithms). The editors
lean superset; the decision gates §3.1.

## 16. v1 deliverables

1. Kernel spec: envelope, canonical encoding, algorithm registry, identity rotation
   log (the document everything else hangs off).
2. Kind registry with the launch set (§5, §6, §12) and manifest/rendition format
   including encryption fields.
3. Bundle format spec.
4. Conformance test suite.
5. Reference gateway (TypeScript/React; ships the legal toolkit and CSAM matching)
   and reference node (Tauri/Rust; pinning, budgets, subscriptions).
6. Threat model document.
7. Foundation charter embodying §13.

---

*Everything contentious is at the edge. Everything at the center is math.*
