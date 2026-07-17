# 003: Record Kind Registry

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md), [002-identity.md](002-identity.md)
**Depended on by:** [004](004-manifest.md), [005](005-claims.md), [006](006-relay.md), [008](008-privacy.md), [009](009-gateway.md), [010](010-economics.md)

This file is the registry of record kinds: stable numeric identifiers,
and for each launch kind its body schema, refs semantics, validation
rules, and one worked example. Kind-level validity is layered above
envelope validity ([001](001-kernel.md) §3): infrastructure stores
records opaquely; clients and gateways apply the rules here before
*interpreting* a record. A record that is envelope-valid but
kind-invalid MUST be ignored by interpreters (not rendered, not
counted), and MAY still be stored and relayed.

## 1. Registry

Kind 0 is reserved. Ids are grouped by concern with deliberate gaps; an
id, once assigned, is never reused, even if a kind is deprecated. New
kinds enter only via the governance process and the two-implementations
rule. The kernel is never versioned for features; new features are new
kinds.

| Id | Kind | Group |
|---:|------|-------|
| 1 | `rotation` | identity |
| 2 | `profile` | identity |
| 3 | `delegate` | identity |
| 16 | `manifest` | content |
| 17 | `supersede` | content |
| 18 | `retract` | content |
| 19 | `mirror` | content |
| 20 | `similarity` | content |
| 32 | `comment` | social |
| 33 | `reaction` | social |
| 34 | `follow` | social |
| 35 | `playlist` | social |
| 36 | `channel` | social |
| 48 | `claim.author` | claims |
| 49 | `claim.license` | claims |
| 50 | `claim.transfer` | claims |
| 51 | `claim.dispute` | claims |
| 64 | `notice.takedown` | compliance |
| 65 | `notice.counter` | compliance |
| 66 | `feed.takedown` | compliance |
| 80 | `endorse.gateway` | trust |
| 81 | `receipt` | economics |
| 82 | `attest` | trust |
| 96 | `anchor` | infrastructure |
| 97 | `keygrant` | privacy |
| 112 | `live.manifest` | live |
| 113 | `live.chat` | live |

## 2. Conventions

* Bodies are CBOR maps with **UTF-8 text keys**, canonically encoded
  ([001](001-kernel.md) §2). Unknown body keys MUST be ignored by
  interpreters (bodies are forward-extensible; the envelope is not).
* "Req" columns: **y** = required, **n** = optional. A missing required
  field, or a wrong type, makes the record kind-invalid.
* `IdentityId` = 32-byte identity identifier ([002](002-identity.md)
  §2). `BlobId` = 32-byte blob hash. `Hint` and `PaymentPointer` are
  defined in [001](001-kernel.md) §9 and [010](010-economics.md) §1.
* Text fields are UTF-8. Interpreters SHOULD enforce these length
  ceilings (bytes): `title` 512, `name` 256, `text` 8192,
  `description` 16384; oversized fields make the record kind-invalid.
* Worked examples show `kind`, `refs`, and `body` in JSON interchange
  form ([001](001-kernel.md) §11); `author`, `created_at`, `sig_alg`,
  and `sig` are elided and 32-byte hashes are truncated to
  `hex:ab12…` for readability. Byte-exact fixtures live in the
  conformance suite.

## 3. Identity kinds

### 3.1 `rotation` (1)

Schema, refs, and validation are specified in
[002-identity.md](002-identity.md) §§1–4 and are not repeated here.

Example (genesis):

```json
{ "kind": 1,
  "refs": [],
  "body": { "key": "hex:4f2a…", "key_alg": 1,
            "recovery": [[1, "hex:9c81…"]],
            "contest_window": 604800 } }
```

### 3.2 `profile` (2)

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `name` | text | y | Display name |
| `about` | text | n | Bio |
| `avatar` | BlobId | n | Avatar image blob |
| `payment` | [PaymentPointer] | n | [010](010-economics.md) §1 |
| `relays` | [text] | n | Relay URLs the identity publishes to |
| `seeds` | [text] | n | Endpoints seeding the identity's blobs |
| `enc_key` | [uint, bytes] | n | Encryption public key `[alg, key]` ([008](008-privacy.md) §4) |

Refs: none defined; MUST be empty.
Validation: none beyond schema. Latest-wins per
[002](002-identity.md) §6.

```json
{ "kind": 2, "refs": [],
  "body": { "name": "asha", "about": "field recordings",
            "relays": ["wss://relay.example.net/sync"],
            "payment": [[1, "asha@ln.example.net"]] } }
```

### 3.3 `delegate` (3)

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `grantee` | IdentityId | y | Identity receiving the capability |
| `capability` | text | y | Registered capability name |
| `expires_at` | int | n | Unix seconds; absent = until revoked |
| `revoked` | bool | n | `true` revokes the grant in `refs[0]` |

Registered capabilities at launch: `rendition`
([004](004-manifest.md) §3). New capabilities are registered alongside
the kinds that consume them.

Refs: empty for a grant. A revocation has exactly one ref
`[0, <grant record id>]` and `revoked = true`.
Validation: a revocation's author MUST equal the grant's author;
`grantee`/`capability` in a revocation MUST match the grant.

```json
{ "kind": 3, "refs": [],
  "body": { "grantee": "hex:77d0…", "capability": "rendition",
            "expires_at": 1795000000 } }
```

## 4. Content kinds

### 4.1 `manifest` (16)

The canonical identity of a video. Schema, rendition rules, and
validation are specified in [004-manifest.md](004-manifest.md) §§1–4.

Refs: MAY contain one `[0, <channel record id>]` placing the video in a
channel; the channel's author MUST equal the manifest's author.

### 4.2 `supersede` (17)

Replaces an earlier record by the same author.

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `target_kind` | uint | y | Kind of the target record |
| `body` | map | y | Complete replacement body, valid per `target_kind` |

Refs: exactly one, `[0, <target record id>]`.
Validation: author MUST equal the target's author (same identity, per
chain); `target_kind` MUST equal the target's kind; `target_kind` MUST
NOT be `rotation` (chains are never edited). Interpreters render the
latest valid supersede in a chain of supersessions; the original id
remains the stable reference for refs.

```json
{ "kind": 17, "refs": [[0, "hex:e410…"]],
  "body": { "target_kind": 32,
            "body": { "text": "corrected: it was 1971, not 1972" } } }
```

### 4.3 `retract` (18)

Requests withdrawal of an earlier record by the same author.

| Field | Type | Req |
|-------|------|-----|
| `reason` | text | n |

Refs: exactly one, `[0, <target record id>]`.
Validation: author MUST equal the target's author; the target MUST NOT
be a `rotation`. Retraction is a request, not an erasure: interpreters
SHOULD stop rendering the target; relays and archives MAY retain it.

```json
{ "kind": 18, "refs": [[0, "hex:e410…"]], "body": { "reason": "posted in error" } }
```

### 4.4 `mirror` (19)

"This identity pins these blobs."

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `hints` | [Hint] | n | Where the author serves the blobs |

Refs: one or more; each is either `[1, <blob id>]` (pin the blob) or
`[0, <manifest record id>]` (pin every blob the manifest references).
Validation: at least one ref. Mirrors drive discovery and node
subscriptions ([004](004-manifest.md) §5); they assert intent, not an
enforceable promise.

```json
{ "kind": 19, "refs": [[0, "hex:aa31…"]],
  "body": { "hints": [[3, "https://node7.example.org/blob"]] } }
```

### 4.5 `similarity` (20)

Near-duplicate assertion: evidence, never kernel truth.

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `method` | text | y | Algorithm identifier, e.g. `phash-v2` |
| `score` | uint | y | Similarity in basis points, 0–10000 |

Refs: exactly two — `[0 or 1, <candidate>]` then
`[0, <canonical manifest record id>]`.
Validation: `score` ≤ 10000. Gateways weigh similarity records by
author reputation and method; they MUST NOT auto-merge content on a
similarity record alone ([004](004-manifest.md) §5).

```json
{ "kind": 20,
  "refs": [[1, "hex:0be2…"], [0, "hex:aa31…"]],
  "body": { "method": "phash-v2", "score": 9700 } }
```

## 5. Social kinds

### 5.1 `comment` (32)

| Field | Type | Req |
|-------|------|-----|
| `text` | text | y |
| `media` | [BlobId] | n |

Refs: `refs[0]` = `[0, <manifest or live.manifest record id>]` (the
subject); optional `refs[1]` = `[0, <parent comment record id>]` for
threading. Threads merge by refs in any arrival order.
Validation: `text` non-empty; a parent, when present, MUST itself
reference the same subject.

```json
{ "kind": 32,
  "refs": [[0, "hex:aa31…"], [0, "hex:5d09…"]],
  "body": { "text": "the second movement is extraordinary" } }
```

### 5.2 `reaction` (33)

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `reaction` | text | y | Single emoji grapheme cluster or registered token |

Refs: exactly one, `[0, <target record id>]`.
Validation: `reaction` ≤ 32 bytes. Counts are per-gateway aggregates
([006](006-relay.md) §7); an identity's later reaction to the same
target supersedes its earlier one for counting purposes.

```json
{ "kind": 33, "refs": [[0, "hex:aa31…"]], "body": { "reaction": "🔥" } }
```

### 5.3 `follow` (34)

| Field | Type | Req |
|-------|------|-----|
| `note` | text | n |

Refs: exactly one, `[0, <genesis rotation record id of the followed
identity>]` — i.e. the followed identity's identifier used as a record
ref. Unfollow = `retract`.
Validation: none beyond schema. Follows are portable: any gateway can
reconstruct any identity's graph.

```json
{ "kind": 34, "refs": [[0, "hex:77d0…"]], "body": {} }
```

### 5.4 `playlist` (35)

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `title` | text | y | |
| `description` | text | n | |
| `entries` | [bytes(32)] | y | Manifest record ids, in order |

Refs: MUST be empty. Entries live in the body (not refs) so that
reordering is an ordinary `supersede` with a replacement body.
Validation: `entries` non-empty.

```json
{ "kind": 35, "refs": [],
  "body": { "title": "winter mixes",
            "entries": ["hex:aa31…", "hex:bb42…"] } }
```

### 5.5 `channel` (36)

| Field | Type | Req |
|-------|------|-----|
| `title` | text | y |
| `description` | text | n |
| `avatar` | BlobId | n |
| `banner` | BlobId | n |

Refs: MUST be empty. Manifests join a channel by referencing it
(§4.1); an identity MAY have many channels.
Validation: none beyond schema.

```json
{ "kind": 36, "refs": [],
  "body": { "title": "Field Notes", "description": "one river a week" } }
```

## 6. Claims and compliance kinds

Shared context: [005-claims.md](005-claims.md). All claim kinds
reference the subject manifest as `refs[0]` = `[0, <manifest id>]`.
Interpreters MUST present all of these as assertions with provenance,
never as verified truth.

### 6.1 `claim.author` (48)

| Field | Type | Req |
|-------|------|-----|
| `statement` | text | n |
| `evidence` | [BlobId] | n |

Refs: exactly one (subject manifest).

```json
{ "kind": 48, "refs": [[0, "hex:aa31…"]],
  "body": { "statement": "recorded by me, 2025-11-02",
            "evidence": ["hex:c3d4…"] } }
```

### 6.2 `claim.license` (49)

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `license` | text | y | SPDX id or Vidmesh value ([004](004-manifest.md) §4) |
| `terms` | text | n | Free-text terms or pointer |

Refs: exactly one (subject manifest).
Validation: `license` per [004](004-manifest.md) §4. Later grants by
the same rights chain modify earlier ones; conflicts are surfaced, not
resolved ([005](005-claims.md) §3).

```json
{ "kind": 49, "refs": [[0, "hex:aa31…"]],
  "body": { "license": "CC-BY-4.0" } }
```

### 6.3 `claim.transfer` (50)

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `assignee` | IdentityId | y | Identity receiving the rights |
| `note` | text | n | |

Refs: exactly one (subject manifest). Author is the assignor.
Validation: interpreters weigh a transfer only when the assignor's own
claim position is supported ([005](005-claims.md) §3).

```json
{ "kind": 50, "refs": [[0, "hex:aa31…"]],
  "body": { "assignee": "hex:77d0…", "note": "per agreement of 2026-01-15" } }
```

### 6.4 `claim.dispute` (51)

| Field | Type | Req |
|-------|------|-----|
| `statement` | text | y |
| `evidence` | [BlobId] | n |

Refs: exactly one, `[0, <disputed claim or notice record id>]`.
Validation: target MUST be kind 48–50 or 64–65.

```json
{ "kind": 51, "refs": [[0, "hex:91f2…"]],
  "body": { "statement": "prior publication 2024-06; see archive capture",
            "evidence": ["hex:d915…"] } }
```

### 6.5 `notice.takedown` (64)

A structured legal notice as a signed record.

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `regime` | text | y | e.g. `us-dmca-512`, `eu-dsa-16` |
| `claimant` | map | y | `{ name: text, contact: text, on_behalf_of: text? }` |
| `statement` | text | y | The sworn/good-faith statement the regime requires |
| `work` | text | y | Identification of the allegedly infringed work |
| `signature_name` | text | y | Natural-person signature line |

Refs: one or more — each `[0, <manifest id>]` or `[1, <blob id>]`
identifying the subject material.
Validation: at least one ref; all required fields non-empty. A notice
obligates no one by protocol; gateways act on notices per their
jurisdiction ([009](009-gateway.md) §3).

```json
{ "kind": 64, "refs": [[0, "hex:aa31…"]],
  "body": { "regime": "us-dmca-512",
            "claimant": { "name": "Example Pictures LLC",
                          "contact": "legal@example.com" },
            "work": "\"Winter Film\" (2024), reg. PA0002419331",
            "statement": "I have a good faith belief…",
            "signature_name": "J. Doe" } }
```

### 6.6 `notice.counter` (65)

Same schema as `notice.takedown` except `work` is optional.
Refs: exactly one, `[0, <notice.takedown record id>]`.
Validation: required fields non-empty.

```json
{ "kind": 65, "refs": [[0, "hex:91f2…"]],
  "body": { "regime": "us-dmca-512",
            "claimant": { "name": "A. Creator", "contact": "a@example.net" },
            "statement": "I have a good faith belief the material was removed as a result of mistake or misidentification…",
            "signature_name": "A. Creator" } }
```

### 6.7 `feed.takedown` (66)

A subscribable compliance feed ([009](009-gateway.md) §3). Each record
is one batch; the feed is the sequence of batches by one identity.

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `feed` | text | y | Feed name, stable per publisher |
| `add` | array | y | Entries `[ref_type, hash, reason: text, notice: bytes32?]` |
| `remove` | array | y | Entries `[ref_type, hash]` — reverses a prior add |
| `seq` | uint | y | Monotonic batch number within the feed |

Refs: MUST be empty (subjects are in the body: feeds routinely list
thousands of entries and refs semantics don't apply).
Validation: `add`/`remove` MAY be empty but MUST be present;
subscribers apply batches in `seq` order per feed and MUST tolerate
gaps (partition posture). `reason` SHOULD be a stable code
(`copyright`, `court-order`, `csam`, `other`).

```json
{ "kind": 66, "refs": [],
  "body": { "feed": "example-org/us", "seq": 4182,
            "add": [[1, "hex:0be2…", "copyright", "hex:91f2…"]],
            "remove": [] } }
```

## 7. Trust and economics kinds

### 7.1 `endorse.gateway` (80)

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `url` | text | y | Gateway origin, e.g. `https://watch.example.net` |
| `note` | text | n | |

Refs: MUST be empty. Withdrawal = `retract`.
Validation: `url` MUST be an https origin. Endorsements let audiences
find a creator's designated gateways; they carry no exclusivity.

```json
{ "kind": 80, "refs": [],
  "body": { "url": "https://watch.example.net" } }
```

### 7.2 `receipt` (81)

Signed payment statement — the tip/superchat primitive
([010](010-economics.md) §2).

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `amount` | uint | y | Minor units (cents, sats, …) |
| `currency` | text | y | ISO 4217 code or rail-native unit (`USD`, `sat`) |
| `rail` | uint | y | Payment pointer type id ([010](010-economics.md) §1) |
| `payee` | IdentityId | y | Recipient identity |
| `proof` | text | n | Rail-specific proof (preimage, tx ref) |
| `message` | text | n | Display message |

Refs: exactly one, `[0, <manifest or live.manifest record id>]`.
Validation: `amount` > 0. A receipt asserts the payer's statement;
rails, not the protocol, prove settlement — gateways MAY verify
`proof` before rendering ([010](010-economics.md) §2).

```json
{ "kind": 81, "refs": [[0, "hex:aa31…"]],
  "body": { "amount": 21000, "currency": "sat", "rail": 1,
            "payee": "hex:77d0…", "message": "for the river week 12" } }
```

### 7.3 `attest` (82)

Portable third-party reputation: "G attests that K reached X."

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `statement` | text | y | Human-readable attestation |
| `data` | map | n | Machine-readable payload, attester-defined |

Refs: exactly one — `[0, <subject identity genesis or record id>]`.
Validation: none beyond schema. Attestations are worth exactly the
attester's reputation; achievements survive any gateway's death.

```json
{ "kind": 82, "refs": [[0, "hex:77d0…"]],
  "body": { "statement": "100000 verified views on watch.example.net, 2026-06",
            "data": { "metric": "views", "value": 100000 } } }
```

## 8. Infrastructure kinds

### 8.1 `anchor` (96)

Commits a batch of observed record ids to external timestamp systems
([001](001-kernel.md) §10).

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `root` | bytes(32) | y | Merkle root over the batch (§8.1.1) |
| `count` | uint | y | Number of ids in the batch |
| `system` | text | y | e.g. `opentimestamps` |
| `proof` | bytes or BlobId | y | System-specific proof, inline if ≤ 4096 bytes else a blob |

Refs: MUST be empty (the batch is under the root, not enumerated).
Validation: none beyond schema; proofs are evaluated against the named
system by interested verifiers.

**8.1.1 Batch root.** Sort the batch's record ids ascending bytewise;
build the Merkle tree of [001](001-kernel.md) §8 over them (each id is
one leaf; leaf hash `BLAKE3-256(0x00 || id)`). A proof of inclusion for
one id is the sibling path, as in a chunk-tree range proof.

```json
{ "kind": 96, "refs": [],
  "body": { "root": "hex:31ce…", "count": 18211,
            "system": "opentimestamps", "proof": "hex:0063…" } }
```

### 8.2 `keygrant` (97)

Wraps a content key to one recipient ([008](008-privacy.md) §4).

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `recipient` | IdentityId | y | |
| `wrap_alg` | uint | y | Key-wrap algorithm ([008](008-privacy.md) §4) |
| `wrapped_key` | bytes | y | |
| `note` | text | n | |

Refs: exactly one, `[0, <manifest record id>]` (the encrypted content).
Validation: `wrap_alg` registered. Keygrants for private content are
distributed out of band or via private relays — publishing one to a
public relay leaks the *existence* of the grant
([008](008-privacy.md) §5).

```json
{ "kind": 97, "refs": [[0, "hex:aa31…"]],
  "body": { "recipient": "hex:77d0…", "wrap_alg": 1,
            "wrapped_key": "hex:8e44…" } }
```

## 9. Live kinds

### 9.1 `live.manifest` (112)

A rolling signed manifest of live segments
([004](004-manifest.md) §6).

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `title` | text | y* | Required in the first record of a stream |
| `seq` | uint | y | 0 for the stream's first record, then +1 |
| `segments` | array | y | `[blob: bytes32, duration_ms: uint]` pairs, in order |
| `final` | bool | y | `true` closes the stream |

Refs: first record of a stream: empty. Every later record: exactly one,
`[0, <first live.manifest record id of the stream>]`. The first
record's id is the stream id.
Validation: `seq` gaps are tolerated (partition posture); consumers
order by `seq`, not arrival. After `final = true`, the creator SHOULD
publish an ordinary `manifest` covering the stream (the VOD record).

```json
{ "kind": 112, "refs": [[0, "hex:f00d…"]],
  "body": { "seq": 42, "final": false,
            "segments": [["hex:0aa1…", 2000], ["hex:0bb2…", 2000]] } }
```

### 9.2 `live.chat` (113)

| Field | Type | Req |
|-------|------|-----|
| `text` | text | y |

Refs: exactly one, `[0, <stream id (first live.manifest record id)>]`.
Validation: `text` non-empty, ≤ 2048 bytes. Live chat is ephemeral in
spirit: relays MAY expire `live.chat` records aggressively
([006](006-relay.md) §4); archives MAY keep them.

```json
{ "kind": 113, "refs": [[0, "hex:f00d…"]], "body": { "text": "gg 🔥" } }
```

## Decisions

* Bodies use text keys and are forward-extensible (unknown body keys
  ignored); the envelope is closed. This keeps kind evolution cheap
  without ever touching the kernel.
* `supersede` carries a complete replacement body (not a diff), and
  refs-carried collections (`playlist` was moved to body `entries`) so
  that supersession is universal and body-only.
* `feed.takedown` subjects live in the body, not refs — feeds are bulk
  data, and refs semantics (threading) don't apply.
* `follow` uses the followed identity's genesis record id as a record
  ref, keeping the social graph in refs where relays can filter on it.
* `anchor` batches are sorted-set Merkle trees reusing the kernel's
  domain-separated construction — one tree algorithm everywhere.

## Test vectors

For **every** kind above, the conformance group `kinds/<kind>/` contains
at least: one valid fixture (JSON + canonical CBOR pair), and three
invalid mutations — bad signature, non-canonical encoding, wrong id —
plus kind-specific invalids for each validation rule stated here
(wrong refs count/type, missing required field, constraint violations
such as `similarity/score-overflow`, `supersede/target-rotation`,
`comment/parent-subject-mismatch`).
