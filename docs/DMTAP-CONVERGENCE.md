# Vidmesh ↔ DMTAP-PUB convergence

**Status: phase 1 SHIPPED and proven. Phase 2 (the cutover) is specified below
but NOT started, and remains FOUNDER-GATED.**

Phase 1 was additive and reversible: a §22 encode/decode path now exists
*beside* the native format, behind a default-off feature flag. **No existing
byte format changed and no existing test changed.** Deleting
`crates/vidmesh-kernel/src/dmtap_pub.rs`, its test files, and the optional
dependency returns the repo exactly to where it was.

- **What it proves:** all **15 frozen §22 conformance vectors** from the spec
  repo pass **byte-exact** through vidmesh's §22 path.
- **How it avoids a third dialect:** §22 is **consumed** from envoir's
  `dmtap-core`, not reimplemented.
- **Test counts:** 251 Rust tests before → **251 unchanged** by default,
  **263** with `--features dmtap-pub`. Three-runtime conformance
  **189 / 142 / 115** before and after, zero failures.

Jump to [Phase 1: what was built and proved](#phase-1-what-was-built-and-proved),
the [byte-level mapping](#byte-level-mapping-vidmesh--22), or the
[Phase 2 specification](#phase-2-specification-the-cutover--not-started).

## The question

Vidmesh built its own **self-certifying substrate** — a signed-record kernel
(CBOR envelope, Ed25519, BLAKE3), a content-addressed blob layer with
chunk-tree range proofs, an identity/rotation model, and a `/sync` relay.
Independently, the **DMTAP** protocol (in `~/code/vulos/dmtap`) has grown
**DMTAP-PUB** (§22, "Public Objects"): the *authenticity-without-confidentiality*
quadrant — signed public objects, plaintext-addressed public blobs, and
per-author append-only feeds, served over a `/.well-known/dmtap-pub` HTTP
surface. §23 (the CAD/artifact profile) is the first application over it, and a
**§24 video profile is being authored right now**; **envoir** is implementing
§22 in Rust.

DMTAP-PUB and vidmesh solve the *same substrate problem* — a signed, publicly
verifiable, content-addressed, dedup-friendly, trustlessly-servable object graph
keyed to a sovereign identity — with **different bytes**. That is the
duplication this document is about.

Two routes:

- **(a) Keep the parallel substrate.** Vidmesh stays its own protocol; DMTAP-PUB
  is a sibling that happens to overlap. Two codecs, two identity models, two
  serving surfaces, two conformance suites, two ecosystems.
- **(b) Re-base the application onto DMTAP-PUB.** Vidmesh's *video-specific*
  layer becomes the **DMTAP-PUB §24 video profile** — the exact relationship
  §23 (CAD) has to §22 — riding envoir's Rust §22 implementation. One substrate,
  one identity model, one serving surface; vidmesh keeps everything that makes it
  *video* and contributes its substrate innovations upstream.

**Recommendation: (b), founder-gated.** Reasoning and cost below.

## Why the two substrates are the same shape

| Concern | Vidmesh | DMTAP-PUB §22 |
|---|---|---|
| Signed object | `Record`: CBOR map keys 1–7, `kind`+`refs`+`body`+`sig` (spec 001) | `PubAnnounce` (kind `0x40`): integer-keyed CBOR, `pub`/`roots`/`meta`/`sig` (§22.3) |
| Object id | `BLAKE3-256(canonical bytes)`, bare 32 bytes; sig over `"vidmesh:record:v1" ‖ id` (P2/P3) | `announce_id = 0x1e ‖ BLAKE3-256(det_cbor)`; sig over `"DMTAP-PUB-v0/announce" ‖ 0x00 ‖ det_cbor(∖sig)` (§22.3.1) |
| Content-addressed blob | `Manifest` + chunk tree, 1 MiB chunks, `0x00`/`0x01` domain-sep, odd-node promotion (P6) | `PubManifest`, RFC-6962 tree, DS-tag `"DMTAP-PUB-v0/manifest"` folded into every leaf/node, `h_i = 0x1e ‖ BLAKE3(plaintext_i)` (§22.2.2) |
| Per-blob chunk self-verification | `blob::verify_chunk` + `ChunkTree::prove` (range proofs) | inherited from §5.5, `h_i` self-verify; range-proof construction unspecified |
| Author ordering / anti-rollback | none at substrate level (relay gossip + identity rotation finality) | `FeedHead`/`FeedEntry`: monotonic `seq`, `prev` hash-chain, fork-detectable (§22.4) |
| Identity | `IdentityRef = [identity_id, signing_key]`, rotation with recovery>signing **contest-window finality** (spec 002 §4) | root `IK` + `DeviceCert` chain (§1.2); feeds bind `signer` to `pub` |
| Serving | relay `WS /sync` + `/blob` sidecar (`GET`/range/`/proof`) | `/.well-known/dmtap-pub/{feed,announce,manifest,chunk}` (§22.5.1), `pub-1` capability |
| Moderation posture | gateway *selection = moderation*; no protocol takedown (spec 009) | per-holder serve policy; no protocol takedown (§22.6.2) — **identical philosophy** |

The philosophies already match (self-verifying, trustless serving, no
protocol-level takedown, derived-and-rebuildable indexes, honest irrevocability).
The *bytes* differ. Maintaining both means paying twice for one idea.

## Migration cost (route b)

What actually has to change lives almost entirely in the **substrate crates**
(`vidmesh-kernel`, `vidmesh-relay`, `vidmesh-wasm`). The **application layer
survives** (next section).

| Area | Change | Cost | Notes |
|---|---|---|---|
| **DS-tags** | Replace `"vidmesh:record:v1"` / `"vidmesh:derivation:v1"` with the DMTAP-PUB DS-tag family (`DMTAP-PUB-v0/{announce,feed,manifest}` ‖ `0x00`) | **Low** | Mechanical; touches signing/verify preimages only. Isolated in `record.rs` / `content.rs`. |
| **Envelope shape** | Vidmesh's one universal `Record` becomes a small set of DMTAP-PUB object types (`PubAnnounce` for what is published; `meta` carries the video schema). The **kinds registry stops being a wire concept and becomes a `meta` schema** (like §23's `"artifact"` key) | **Medium** | Conceptual, not just mechanical: "everything is a Record" → "everything published is an announce carrying a profile schema". Kinds like `manifest`, `comment`, `playlist`, `channel` re-express as §24 `meta` schemas or as their own announces. |
| **Multihash prefix** | Prefix every content address with `0x1e` (BLAKE3-256), per §18.1.5 hash-agility | **Low** | `ids.rs` (`BlobId`/`RecordId` gain the prefix on the wire); enables FIPS/SHA-2 migration and Git-LFS interop for free. |
| **Chunk tree** | Move from odd-node-promotion + bare `0x00`/`0x01` to the **RFC-6962 split rule with the DS-tag folded into leaf/node** (§22.2.2) | **Medium–High** | The deepest byte change. But vidmesh's *range-proof* code is exactly what §22 lacks — see "contribute upstream". Reuses `ChunkTree` machinery; only the split rule + domain-sep bytes change. |
| **Feed / anti-rollback** | Adopt `FeedHead`/`FeedEntry`: per-author append-only log, monotonic `seq`, `prev` chain (§22.4) | **Medium (new)** | A primitive vidmesh does not have today. Gives ordering, discovery, and anti-rollback the relay's gossip only approximates. |
| **Serving surface** | Add `/.well-known/dmtap-pub/{feed,announce,manifest,chunk}` and advertise `pub-1` | **Low** | The relay's `/blob` sidecar (content-addressed GET + range + immutable caching) is ~80% of the `chunk`/`manifest`/`announce` endpoints already. `/sync` gossip can remain as an optimization beside the well-known surface. |
| **Error model** | Map kernel `Error` onto the `ERR_PUB_*` (`0x09xx`) registry (§22.10) | **Low** | Naming/telemetry alignment; behavior (fail-closed) already matches. |
| **Conformance** | Re-target the suite's vectors at the DMTAP-PUB object types; contribute them to the §22/§24 conformance corpus | **Medium** | The three-runtime harness is reusable as-is; the vectors change shape. |

**Not on this list — because it survives untouched:** transcode/HLS, the policy
engine, key custody, the gateway API, and the entire web UI + verification badge.

## What survives (the application layer is the moat)

None of the following cares whether the substrate is vidmesh-native or
DMTAP-PUB — they sit *above* the object graph:

- **Kinds registry → §24 video schema.** The 27 kinds become the video
  profile's `meta` schemas and announce conventions, exactly as §23 turned CAD
  concepts into `ArtifactMetadata`. This is *content*, not *crypto*.
- **Transcode / HLS / original-only pipeline** (`apps/gateway/server`) — pure
  application logic over blobs.
- **Policy engine** — *gateway selection = moderation* is already the same
  posture as §22.6.2 per-holder serve policy.
- **Key custody with a documented exit** (spec 002 §7 / 009 §5) — a product
  choice layered over any identity model.
- **Gateway UI + client-side verification badge** — verifies signatures and
  content addresses regardless of which DS-tags/prefixes they use.

The moat was never the CBOR dialect. It is video-specific: transcode economics,
the uniform reference UI (a trademark-level requirement, spec 009 §7), and the
gateway selection model. Route (b) keeps all of it.

## What vidmesh contributes upstream

Re-basing is not a surrender; vidmesh's substrate has three things §22/§24 should
adopt:

1. **Chunk-tree range proofs.** §22.2 gives per-chunk self-verification but no
   *range-proof* construction (prove chunk `i` against the root with a sibling
   path). Vidmesh's `ChunkTree::prove` / `blob::verify_chunk` and the relay's
   `GET /blob/{id}/proof?chunk=i` endpoint are exactly that primitive, already
   tested and wired into the conformance suite. Contribute the range-proof
   grammar to the §22 manifest profile (adjusted to the RFC-6962 + DS-tag tree).
2. **Rotation-log finality / anti-equivocation.** Vidmesh identity rotation
   resolves forks by **recovery > signing** class and a **contest-window
   finality** rule with verifier-local first-seen times (spec 002 §4, P9) — a
   richer anti-equivocation model than a `FeedHead`'s monotonic `seq` alone.
   Contribute it to harden feed/identity fork handling (§22.4.2 already reaches
   for "fork = HALT_ALERT with transferable evidence"; this makes finality
   precise).
3. **Fetch-hint registry.** Vidmesh manifests carry per-blob retrieval **hints**
   (a registry of where/how to fetch a content address). §22 stops at the
   content address and the `/.well-known` surface; a fetch-hint registry gives
   swarm/CDN/mirror discovery a typed home. Contribute it as a §22 (or §24)
   optional `meta` block.

## The case for (a), stated fairly

Route (a) is not indefensible:

- **Zero migration risk now.** The substrate is tested and green today; (b) touches
  the chunk tree and identity, the two places a subtle byte bug is most expensive.
- **§24 is unwritten and §22's Rust impl is in flight.** Betting the substrate on
  a spec being authored *right now* and an implementation that is not yet a
  dependency is real schedule risk.
- **Independence.** A separate substrate cannot be blocked by DMTAP governance
  decisions.

The counter: (a) permanently pays double for one idea, fragments identity and
serving across two incompatible ecosystems, and forfeits the network effect of
sharing envoir/DMTAP's substrate, holders, and identity graph — for a *video*
product whose actual differentiation is not in the CBOR layer. The independence
(a) buys is independence from the exact ecosystem vidmesh would most benefit from
joining.

## Recommendation

**Adopt route (b): re-base vidmesh's video layer as the DMTAP-PUB §24 video
profile, on envoir's Rust §22 substrate, and contribute range proofs,
rotation-log finality, and the fetch-hint registry upstream.** The migration cost
is real but bounded to the substrate crates; the application layer — the moat —
survives intact.

**This is FOUNDER-GATED.** Concretely, before any substrate byte changes:

1. Founder confirms the strategic direction (one substrate vs. two).
2. §24 reaches enough of a draft to target (or vidmesh co-authors it — the video
   profile is vidmesh's to write, the way §23 is CAD's).
3. envoir's §22 Rust implementation is a consumable dependency.

**Gates 2 and 3 are now MET** (see below): §24 is written through §24.15
including a migration note, and `dmtap-core` builds as a pinned git dependency.
Gate 1 — the strategic call — remains open, which is exactly why phase 1 stopped
where it did.

---

# Phase 1: what was built and proved

Phase 1 asked one question: *can a §22 path exist inside vidmesh without
disturbing anything, and does it actually agree with the spec?* Both answers are
yes, with evidence.

## Consumed, not ported

`dmtap-core` is an **optional git dependency** pinned to a reviewed revision:

```toml
dmtap-core = { git = "https://github.com/vul-os/envoir", rev = "12526123…", optional = true }
```

`crates/vidmesh-kernel/src/dmtap_pub.rs` **re-exports** `PubAnnounce`,
`PubManifest`, `FeedHead`/`FeedEntry`, `check_anti_rollback`, `ServePolicy` and
the `ERR_PUB_*` registry verbatim. Vidmesh owns **none** of the §22 object code.

This was the deciding constraint. A vendored copy or a careful re-port would
have produced a *second divergent implementation of §22* — precisely the
duplication this convergence exists to delete. The dependency was viable
because the envoir repo is publicly fetchable over HTTPS and `pubobj.rs` is
committed on `origin/main`, so no private-credential or unpushed-work hazard
applies. It is `optional = true` and `default = []`, so the heavy PQ crypto in
`dmtap-core` (`ml-dsa`, `x-wing`, `hpke`) never enters the default build or the
WASM target.

What vidmesh *does* own is the **bridge**: multihash prefix add/strip, blob →
`PubManifest` construction at vidmesh's 1 MiB chunking, identity reinterpretation,
and the §24-profile `meta` framing of a native record.

## The proof: frozen vectors, byte-exact

`crates/vidmesh-kernel/tests/dmtap_pub_vectors.rs` runs the spec repo's frozen
corpus (`dmtap/conformance/vectors/pub_vectors.json`, vendored verbatim;
sha256 `43a4ab54fee10fea3997f99605e01fb9b7dc9b465da32cd365cd3413c0be81f4`).

Those vectors were generated **from the specification text** by a script that
does not import the reference crate, and are cross-checked by a second
from-scratch implementation. Passing them means vidmesh's §22 path agrees with
**the spec**, not merely with envoir.

| Vector | §22 area | Asserted |
|---|---|---|
| `pub_manifest_single_chunk` | §22.2.2 | chunk addresses + root, `n = 1` |
| `pub_manifest_three_chunks` | §22.2.2 | root under the RFC-6962 `k = 2` split |
| `pub_manifest_type_incompatibility` | §22.2.3 | public root ≠ sealed-style root |
| `pub_manifest_key5_forbidden` | §22.2.1 | reject `0x0902`; key-5-free twin decodes + re-encodes byte-exact |
| `pub_announce_signing_preimage` | §22.3.1 | DS-tag, preimage, signature |
| `pub_announce_id` | §18.9.4 | `announce_id`; re-encode byte-identical |
| `pub_announce_supersede_*` (2) | §22.3.4 | same-author accept / cross-author `0x090B` |
| `pub_feed_entry_chain` | §22.4.1 | three `entry_id`s + `prev` linkage |
| `pub_feed_head_signing_preimage` | §22.4.1 | DS-tag, preimage, signature |
| `pub_feed_rollback_strict_less_than` | §22.4.2 | `0x0907` |
| `pub_feed_equal_seq_identical_tip_idempotent` | §22.4.2 | `AcceptIdempotent`, not an error |
| `pub_feed_equal_seq_different_tip_fork` | §22.4.2 | `0x0908`, never `0x0907` |
| `pub_feed_{genesis_carries,nongenesis_missing}_prev_malformed` (2) | §22.4.1 | `0x0908` at decode |

**15 / 15 pass, byte-exact.** Negative cases assert the exact numeric
`ERR_PUB_*` code, not merely "an error". The harness asserts all 15 execute, so
a corpus that grows without the harness growing fails rather than silently
skipping.

## Two findings that change the phase-2 estimate

### 1. Identity key material is already compatible (cheaper than expected)

A vidmesh `Keypair` and a §22 `IdentityKey` are both Ed25519 keys built from 32
secret seed bytes (RFC 8032). `keypair_to_identity_key` is a reinterpretation,
and the public key is **bit-identical** on both sides
(test: `identity_key_material_is_shared`).

**Consequence: there is no key migration.** An existing vidmesh author can
publish §22 objects under the identity they already have. Only the *signing
preimages* differ. This is the single cheapest thing about the convergence and
it was not previously established.

### 2. §24.14 item 4 is wrong, and blob migration is far more expensive (dearer than expected)

DMTAP §24.14 item 4 states that on migration "the **chunk leaf hashes are
identical** (bare-chunk BLAKE3) … so re-derivation is a **tree recompute over
the existing chunk hashes**, not a re-read of the media bytes."

**This is not true of vidmesh's format.** Vidmesh folds its `0x00` leaf tag
*inside* the hash:

| | preimage | value for the vector's chunk |
|---|---|---|
| vidmesh stored leaf | `BLAKE3(0x00 ‖ chunk)` | `b10e6dab…088339` |
| §22 needs `h_i` | `0x1e ‖ BLAKE3(chunk)` | `1e458cd8…301eaf` |

Vidmesh never persisted `BLAKE3(chunk)` anywhere. (The §22 value above matches
the frozen `pub_manifest_single_chunk` vector byte-for-byte, so the comparison
is corpus-anchored, not hand-derived.)

**Consequence: migrating blobs requires re-reading and re-hashing every stored
media byte** — a full pass over the video corpus, not a cheap metadata-only tree
recompute over retained digests. For a video platform that is the difference
between a metadata migration and an I/O-bound one, and it must be budgeted.

Both findings are pinned by tests (`stored_vidmesh_leaf_is_not_the_pub_chunk_address`,
`divergence::chunk_digests_for`) so neither claim can silently rot. **This should
be filed as an erratum against §24.14 item 4.**

---

# Byte-level mapping: vidmesh → §22

Concrete differences, object by object. "Same shape, different bytes" made precise.

## `Record` → `PubAnnounce` (§22.3)

| Aspect | vidmesh `Record` | §22 `PubAnnounce` | Difference |
|---|---|---|---|
| Envelope | CBOR map, integer keys 1–7 | CBOR map, integer keys 1–9 | §22 splits author into `pub`(3)/`signer`(8) and adds `supersedes`(6) |
| Type discriminator | `kind`(1), a **wire** concept from a 27-entry registry | none — kind `0x40` is the object itself | vidmesh's kinds become §24 `meta` schemas; the wire stops carrying a kind |
| Author | `author`(2) = `[identity_id, signing_key]` | `pub`(3) = root `IK`, `signer`(8) = operational key | §22 separates root identity from the signing device (`DeviceCert` chain) |
| Payload | `body`(5), kind-defined map | `meta`(5), **text-keyed** profile map | integer-keyed body → text-keyed profile metadata |
| References | `refs`(4) = positional `[ref_type, hash]` | `roots`(4) = `PubManifest` addresses; named subjects in `meta` | position-dependent → named |
| Timestamp | `created_at`(3), Unix **seconds**, signed int | `ts`(7), **milliseconds** epoch, uint | unit and signedness both change |
| **Signature preimage** | `"vidmesh:record:v1" ‖ id` — **signs the hash** | `"DMTAP-PUB-v0/announce" ‖ 0x00 ‖ det_cbor(∖sig)` — **signs the bytes** | different DS-tag **and** a hash-then-sign vs sign-the-bytes structural change |
| Object id | `BLAKE3(det_cbor(keys 1–6))` — **excludes** the signature | `0x1e ‖ BLAKE3(det_cbor(all keys))` — **includes** the signature | different preimage *and* multihash prefix |

The signature row is the deepest incompatibility: vidmesh signs a 32-byte digest,
§22 signs the encoded object. No re-framing bridges that — objects must be
**re-signed**, which requires the author's key (see phase 2, step 4).

## Per-author feed → `FeedHead` / `FeedEntry` (§22.4)

**There is no vidmesh counterpart to map.** Vidmesh orders an author's records by
a **relay-local, unauthenticated `seq` receipt counter** (spec 006 §2). That is
not a signed structure; a relay can silently omit or reorder an author's records
and a reader cannot tell.

| Aspect | vidmesh | §22 |
|---|---|---|
| Ordering authority | relay-assigned `seq` | author-signed `FeedHead` |
| Anti-rollback | **none** | reject lower `seq` → `0x0907` |
| Equivocation detection | **none** | two tips at one `seq` → `0x0908`, HALT_ALERT |
| Chain integrity | **none** | `prev` hash-chain over `FeedEntry` |
| Cross-fetch memory | **none** | `FeedFollower` retains accepted `entry_id` per `seq` |

So §22's mandatory anti-rollback is **adopted, not translated** — phase 1 already
exposes it (`build_feed_head`, `feed_genesis`, `feed_append`, `FeedFollower`).
This is the one place convergence **hardens** vidmesh rather than re-encoding it.
Vidmesh's identity-rotation *contest-window finality* (spec 002 §4) is a richer
fork-resolution rule than a monotonic `seq` and remains a genuine upstream
contribution — it complements the feed head, it does not substitute for it.

## Blob manifest → `PubManifest` (§22.2)

| Aspect | vidmesh `ChunkTree` | §22 `PubManifest` | Difference |
|---|---|---|---|
| Chunk size | 1 MiB | 1 MiB (`chunk_sz`) | **same** |
| Chunk digest | `BLAKE3(0x00 ‖ chunk)`, stored as the leaf | `h_i = 0x1e ‖ BLAKE3(chunk)`, stored as an address | **different preimage** — see erratum above |
| Leaf hash | `BLAKE3(0x00 ‖ chunk_bytes)` — over **bytes** | `BLAKE3(DS ‖ 0x00 ‖ h_i)` — over the **address** | DS-tag folded in; hashes an address, not content |
| Interior node | `BLAKE3(0x01 ‖ l ‖ r)` | `BLAKE3(DS ‖ 0x01 ‖ l ‖ r)` | DS-tag folded in |
| Odd-node rule | promote unpaired last node | RFC 6962: split at largest power of two `< n` | **same resulting shape** (brute-forced for all `n ≤ 2999`), different values |
| Empty blob | permitted — empty tree, no root | **forbidden** — `n ≥ 1` required | migration edge case |
| Root address | bare 32 bytes, `b3-256:<hex>` | `0x1e ‖ root` | multihash prefix |
| Range proofs | `ChunkTree::prove` + `verify_chunk` + relay `/blob/{id}/proof` | **not specified** — per-chunk self-verification only | vidmesh's to contribute upstream |

Because the tree *shape* agrees for every `n` but every *value* differs, the two
formats are structurally interchangeable and cryptographically not. A proof from
one substrate will never verify in the other — and, critically, will never
*accidentally* verify. `native_and_pub_roots_differ_for_every_chunk_count`
asserts the roots differ for `n = 1…9`; if they ever coincided, proofs would be
silently interchangeable, which would be far worse than being incompatible.

---

# Phase 2 specification: the cutover — NOT STARTED

Phase 2 is where bytes change. It is written here so it can be reviewed and
costed **before** anyone starts, and deliberately not begun.

## Entry gates

1. **Founder confirms one substrate.** (Gates 2 and 3 are already met.)
2. **File the §24.14 item 4 erratum** and agree the corrected blob-migration
   cost, since it changes the operational plan materially.
3. **Pin `dmtap-core` to a released version, not a git rev.** A cutover must not
   depend on a moving branch of a sibling repo.

## Step order (each step independently shippable and green)

**Step 1 — dual-write blobs.** Every newly ingested blob gets a `PubManifest`
alongside its `ChunkTree`; both are stored, the native one stays authoritative.
Cheap, reversible, and it populates §22 addresses before anything depends on
them. *Exit: every new blob has both roots; all existing tests green.*

**Step 2 — serve the §22 well-known surface.** Add
`/.well-known/dmtap-pub/{feed,announce,manifest,chunk}` to the relay and
advertise `pub-1`. The `/blob` sidecar is ~80% of `chunk`/`manifest` already.
`/sync` keeps working untouched. *Exit: an envoir §22 client can fetch vidmesh
blobs; `/sync` conformance still 115/115.*

**Step 3 — adopt feed heads (no format change).** Start publishing signed
`FeedHead`/`FeedEntry` per author **over the existing native records**, since a
feed entry only references an id. This is pure hardening: it delivers
anti-rollback and equivocation detection *before* any envelope change, and it is
the highest-value step in the whole plan. *Exit: every active author has a signed
head; `FeedFollower` runs in the gateway.*

**Step 4 — dual-format records.** Emit `PubAnnounce` alongside `Record` for new
publications, carrying §24 `meta` schemas. **This is where the 27 kinds become
§24 metadata schemas** — the real design work, and it should be co-authored into
§24 rather than invented locally. *Exit: §24 schemas merged upstream; new
publications exist in both formats; readers prefer §22 and fall back.*

**Step 5 — flip authority, then deprecate.** §22 becomes authoritative; native
records become a compatibility read path; eventually removed.

## The migration story for existing records

This is the part with no clean answer, and it must not be glossed.

**Blobs migrate; records cannot be migrated automatically.**

- **Blobs — mechanical but I/O-bound.** Every stored blob must be re-read and
  re-chunked to compute `h_i = 0x1e ‖ BLAKE3(chunk)`, because vidmesh persisted
  `BLAKE3(0x00 ‖ chunk)` instead (the erratum). Content is unchanged and no key
  is needed, so this is a background job over the corpus. **Budget a full pass
  over all stored media.** New `PubManifest` roots are new addresses; keep a
  native-root → §22-root index so old references keep resolving.

- **Records — require the author's key, so most cannot be migrated.** A §22
  announce signs `det_cbor(∖sig)` under a `DMTAP-PUB-v0/` DS-tag; a vidmesh
  record signed `"vidmesh:record:v1" ‖ id`. There is **no transformation** from
  one signature to the other. Re-signing needs the author's private key, which
  the platform does not hold for self-custodied identities — and *should* not.

  Three options, and the choice is a product decision, not a technical one:

  1. **Re-sign on next publish (recommended).** Authors' historical records stay
     native and readable; their feed starts at §22 from their next publication.
     Honest, requires no key access, but the archive is permanently
     dual-format — readers must support both indefinitely.
  2. **Attestation wrapper.** The gateway publishes a §22 announce *referencing*
     the native record and signed by the **gateway**, not the author. Makes
     history §22-addressable, but it is a **weaker claim** — gateway attestation,
     not author authorship — and must be labelled as such in the UI, never
     rendered with the same verification badge. Silently equating the two would
     be a security misrepresentation.
  3. **Bulk re-sign for custodied keys only.** Applies to the subset under
     platform custody (spec 002 §7). Fast for those, does nothing for
     self-custodied authors, and should be opt-in — re-signing a user's history
     without asking is not defensible even where it is technically possible.

  **Recommendation: (1), with (3) offered opt-in.** Option 2 only if a
  distinctly-labelled, visibly weaker badge is acceptable.

- **Identities need no migration at all** — the Ed25519 seed is already shared.
  The rotation chain (spec 002) is a separate structure that still needs its own
  mapping onto `DeviceCert` (§1.2); that mapping is **not yet designed** and is
  the largest unspecified piece of phase 2.

## Explicitly out of scope for phase 2

Transcode/HLS, the policy engine, key custody, the gateway API, and the web UI +
verification badge. None sits below the object graph; all survive the cutover
untouched. That was true in the original analysis and phase 1 did not disturb it.

## Rollback

Through step 4 every step is additive and reversible by disabling the feature
flag. **Step 5 is the point of no return** — once §22 is authoritative and
authors have re-signed, reverting would orphan every §22-native publication.
Treat step 5 as a one-way door and gate it separately.
