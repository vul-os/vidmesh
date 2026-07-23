---
title: "The Evermesh Protocol"
subtitle: "Specification — Draft 0.2 (draft-evermesh-protocol-00)"
author: "Evermesh Protocol Editors"
date: "July 2026"
abstract: |
  Evermesh is a protocol for publishing, distributing, and discussing video
  without a central operator. It divides the system into a minimal, permanent
  kernel — signed records and content-addressed blobs — and a competitive
  edge of gateways, nodes, and clients, where every contested concern
  (moderation, economics, discovery, presentation) is resolved by participant
  choice rather than by protocol rule. This document specifies the kernel
  data model, the identity rotation log, the launch registries, the content
  and claims layers, the relay and bundle distribution mechanisms, and the
  obligations of conforming implementations. The design is governed by a
  survival test: the network must remain fully functional after the death or
  betrayal of every organization involved in it, including the one that
  wrote this document.
---

# Introduction

## Purpose

Evermesh is a protocol for publishing, distributing, and discussing video
without any central operator. It separates the system into two strata:

* a minimal, permanent **kernel**: signed, immutable records and
  content-addressed blobs, verifiable from their bytes alone; and
* a competitive **edge**: gateways, nodes, and clients, where all contested
  concerns — moderation, economics, discovery, presentation — are resolved
  by choice rather than by protocol.

The design goal is stated as a survival test:

> The network must remain fully functional after the death or betrayal of
> every organization involved in it, including the one that wrote this
> document.

A secondary, equally binding goal is partition tolerance at civilizational
scale: the protocol MUST operate across links measured in minutes
(interplanetary), days (sneakernet), or never (permanently isolated local
networks), degrading only in freshness, never in integrity.

## Document status

This document is Draft 0.2 of the Evermesh protocol specification. It is a
pre-RFC draft published for discussion and is not yet a stable standard.
It supersedes Draft 0.1 ("Specification Proposal"); the substantive changes
and editorial decisions introduced in this draft are enumerated in
Appendix B.

The specification text is licensed under CC-BY-SA 4.0. Reference
implementations are licensed under MIT and Apache-2.0.

## Reading guide

Section 3 states the design principles, which are normative for the
evolution of the protocol itself. Sections 5 through 7 define the kernel:
the record envelope, canonical encoding, identifiers, algorithm registries,
blob addressing, and the identity rotation log. Everything from Section 8
onward is defined as record kinds and edge behavior layered on that kernel.
A reader implementing the protocol from scratch should read Sections 2, 5,
6, and 7 closely; the remainder specifies the launch kinds and the expected
behavior of relays, gateways, and nodes.

# Conventions and Terminology

## Requirements language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in this document are to be interpreted as described in BCP 14
[RFC 2119] [RFC 8174] when, and only when, they appear in all capitals, as
shown here.

## Terminology

Record
:   A small, signed, immutable statement in the envelope format of
    Section 5.1. All application data in Evermesh is records.

Blob
:   An opaque byte sequence addressed by its hash (Section 5.6). The kernel
    never interprets blob contents.

Substrate
:   The union of all records and blobs in existence, regardless of where
    they are stored. The substrate has no owner and no servers of record.

Kernel
:   The invariant core of the protocol: the record envelope, canonical
    encoding, algorithm registries, identity rotation log, and blob
    addressing. Everything else is an extension expressed as record kinds.

Kind
:   A registered numeric type identifying the schema and semantics of a
    record's body (Section 7).

Identity
:   A stable identifier bound to a current signing key through a rotation
    log (Section 6).

Relay
:   A service that stores and gossips records without interpreting them
    (Section 10).

Gateway
:   A public web service, on its own domain, that indexes and serves a
    selection of the substrate (Section 14).

Node
:   A background application that pins and seeds chosen content
    (Section 4).

Bundle
:   A single-file, self-verifying export of records and blobs
    (Section 11.2).

Creator
:   Any keyholder who publishes manifests. The creator is a usage of
    identity, not an infrastructure role.

# Design Principles

The following principles are normative for the protocol's evolution. A
proposed change that violates any of them MUST be rejected regardless of
its benefits.

1.  **Minimal kernel.** The kernel defines only: the record envelope, the
    signature and hashing scheme (with algorithm agility), the identity
    rotation log, and blob addressing. Everything else is an extension
    expressed as record kinds.

2.  **Self-certifying data.** Every record and blob MUST be verifiable
    using only its own bytes and mathematics. No record's validity may
    depend on reaching a server, a blockchain, a DNS name, or a
    certificate authority.

3.  **Forkability.** The full index MUST be replicable by anyone;
    identities MUST be portable across all infrastructure; blobs MUST be
    re-hostable by hash. Any community must be able to leave with
    everything except the brand.

4.  **Transport and storage agnosticism.** The kernel never names a
    transport. Blobs are hashes; records carry *hints* (HTTPS, torrent,
    future transports), which are additive and advisory.

5.  **No mandatory dependencies.** No company, blockchain, token, or
    external network may be required for correct operation. All such
    integrations are optional pointer or hint types.

6.  **Economic neutrality.** The protocol carries payment primitives
    (pointers, receipts, disclosures) and takes no position on business
    models. No protocol token exists or will exist.

7.  **Edge-resolved moderation.** The substrate never deletes. Gateways
    select what they index and serve. Compliance is implemented as
    subscribable, signed, auditable feeds — plural and opt-in.

8.  **Partition tolerance.** No record kind may require global consensus,
    global ordering, or synchronous availability. Arrival order is never
    assumed to be creation order.

9.  **Two-implementations rule.** No extension enters the specification
    until two independent implementations interoperate against the public
    conformance suite.

10. **Legibility.** Formats are plain, documented, and boring (CBOR/JSON,
    standard codecs). A future reader must be able to reconstruct a working
    implementation from the specification alone.

# Roles and Architecture

Participation is a ladder; every rung is opt-in and independently useful.

| Role | Runs | Responsibilities |
|------|------|------------------|
| Viewer | Browser or app only | None; contributes swarm bandwidth while watching |
| Node | Background app (desktop or server) | Pins chosen content; seeds watched content; honors its own storage and bandwidth budget |
| Gateway | Public web service on its own domain | Indexes and serves selected content; sets moderation policy; optionally provides transcoding, search, and economic services; complies with its jurisdiction |
| Foundation | Nothing operational | Stewards the specification, the kind registry, the conformance suite, and the trademark |

The expected legal posture of each role is informative, not normative: a
viewer is a consumer; a node is comparable to a BitTorrent seeder that pins
only by explicit choice or subscription, never arbitrary anonymous data; a
gateway is an ordinary website hosting user content, subject to its local
intermediary-liability regime (DMCA, DSA, or equivalent); and the
foundation deliberately never operates a gateway.

The creator is not an infrastructure role: a creator is any keyholder who
publishes manifests. Creators MAY operate their own node and their own
gateway.

# Kernel

## Record envelope

A record is a small, signed, immutable statement. All application data —
manifests, comments, claims, follows, takedowns, receipts — is records.

A record is a CBOR map with integer keys. The following fields are defined;
no other keys are permitted in the envelope:

| Key | Name | Type | Description |
|----:|------|------|-------------|
| 1 | `kind` | uint | Registered record kind (Section 7) |
| 2 | `author` | IdentityRef | The signing identity (Section 5.5) |
| 3 | `created_at` | int | Author-asserted Unix time in seconds (untrusted; Section 5.11) |
| 4 | `refs` | array of Ref | Parents, subjects, and threading (Section 5.6) |
| 5 | `body` | map | Kind-defined CBOR map |
| 6 | `sig_alg` | uint | Registered signature algorithm (Section 5.7) |
| 7 | `sig` | bytes | Signature (Section 5.4) |

The record identifier (`id`) is derived, never serialized inside the
envelope (Section 5.3).

The following rules are normative:

* Records are immutable. "Edit" and "delete" are new records (kinds
  `supersede` and `retract`) that reference the target; consumers decide
  how to render them. Retraction is a request, not an erasure.
* Signatures are always computed over the canonical CBOR form
  (Section 5.2). JSON is a permitted interchange representation
  (Section 5.10), but a JSON-borne record MUST be converted to canonical
  CBOR before verification.
* `sig_alg` and the hash algorithm are registry entries, not constants
  (Section 5.7). Post-quantum algorithms are added by registry entry, and
  identities migrate via the rotation log (Section 6.3), never by protocol
  fork.
* Relays and other index infrastructure MUST accept, store, and forward
  records of unknown kind opaquely, provided the envelope verifies. Relays
  cannot veto innovation. Clients MUST ignore records of unknown kind
  safely.
* Implementations MUST NOT crash or exhibit undefined behavior on
  malformed input. A record that fails any validation rule is rejected,
  not partially processed.

## Canonical encoding

The canonical encoding of a record (and of any CBOR structure for which
this specification requires a canonical form) is CBOR [RFC 8949]
constrained by the Core Deterministic Encoding Requirements
(RFC 8949, Section 4.2.1), with the following additional constraints:

1. All lengths MUST be definite. Indefinite-length items are forbidden.
2. Integers MUST be encoded in the shortest form that represents the
   value.
3. Map keys MUST be unique and MUST be sorted in bytewise lexicographic
   order of their canonical encodings.
4. Floating-point values MUST NOT appear in the envelope. Kind
   specifications SHOULD avoid floats in bodies; where a fractional value
   is needed, fixed-point integers are RECOMMENDED.
5. Tags MUST NOT appear unless a kind specification explicitly requires a
   specific tag.

Two encoders that follow these rules produce byte-identical output for the
same abstract record. This property is load-bearing: identifiers and
signatures are computed over these bytes.

## Record identifier

The identifier of a record is:

```
id = BLAKE3-256( canonical_cbor( envelope without key 7 ) )
```

that is, the BLAKE3-256 hash of the canonical CBOR encoding of the
envelope map containing keys 1 through 6 (all fields except the
signature). Including `sig_alg` under the hash commits the record to its
algorithm and prevents downgrade substitution.

The `id` is 32 bytes. Its text representation, where one is needed, is
lowercase hexadecimal.

A record is **valid** if and only if:

1. its envelope decodes as a CBOR map containing exactly keys 1–7 with
   the types of Section 5.1;
2. its encoding is canonical per Section 5.2 (a non-canonical encoding of
   an otherwise-correct record MUST be rejected);
3. `sig_alg` is a registered algorithm known to the verifier; and
4. the signature verifies per Section 5.4.

Kind-level validation (Section 7 onward) is layered above envelope
validity: infrastructure that stores records opaquely performs only
envelope validation.

## Signatures

The signature is computed over the record identifier with a domain
separation prefix:

```
sig = Sign( signing_key, "evermesh:record:v1" || id )
```

where `||` is byte concatenation and the prefix is the 17-byte ASCII
string shown, with no terminator. Verification recomputes `id` from the
received envelope and verifies `sig` against the `author`'s signing key
(Section 5.5) under the algorithm named by `sig_alg`.

The domain separation prefix ensures that a record signature can never be
confused with a signature produced for any other Evermesh context (future
contexts will use distinct prefixes).

## Identity references

An `IdentityRef` names the author of a record and carries the key needed
to verify it:

```
IdentityRef = [ identity_id: bytes(32), signing_key: bytes ]
```

* `identity_id` is the identity's stable identifier (Section 6.1).
* `signing_key` is the public key, in the encoding defined by the record's
  `sig_alg`, that produced the record's signature.

A record is *envelope-valid* if its signature verifies under
`signing_key`. Whether `signing_key` was an **authorized** key of
`identity_id` at the relevant time is determined by the identity's
rotation log (Section 6); consumers that have the rotation chain MUST
additionally check authorization. Consumers that do not yet have the chain
MAY treat the record as provisionally attributed.

In the genesis record of an identity — and only there — `identity_id` is
32 zero bytes, because the identifier is the hash of the genesis record
itself and cannot appear inside it (Section 6.2).

## References and blob addressing

Each entry of `refs` is a typed pair:

```
Ref = [ ref_type: uint, hash: bytes(32) ]
```

| `ref_type` | Meaning |
|-----------:|---------|
| 0 | Record reference: `hash` is a record id |
| 1 | Blob reference: `hash` is a blob id |

Kind specifications assign meaning to ref positions (parent, subject,
target); the kernel assigns none.

A **blob** is an opaque byte sequence addressed by its hash. The blob
identifier is the BLAKE3-256 hash of the blob's bytes. Its text
representation is:

```
b3-256:<64 lowercase hex characters>
```

The kernel does not know what a blob contains. Video, thumbnails,
captions, encrypted ciphertext — all are blobs referenced from records.

## Algorithm registries

Every signature and hash in Evermesh names its algorithm by registry id.
The launch registries are:

**Signature algorithms (`sig_alg`)**

| Id | Algorithm | Public key | Signature | Reference |
|---:|-----------|-----------:|----------:|-----------|
| 1 | Ed25519 | 32 bytes | 64 bytes | [RFC 8032] |

**Hash algorithms**

| Id | Algorithm | Digest | Reference |
|---:|-----------|-------:|-----------|
| 1 | BLAKE3-256 | 32 bytes | [BLAKE3] |

Registry id 0 is reserved in both registries and MUST NOT be assigned.
New entries are added through the governance process (Section 16) and are
subject to the two-implementations rule. Verifiers MUST reject records
whose `sig_alg` they do not implement rather than skip signature
verification.

At launch, the hash algorithm is fixed at BLAKE3-256 for record ids and
blob ids; the hash registry exists so that future migrations are a
registry change plus identity rotation, not a fork.

## Chunked blobs and verified ranges

Large blobs use a chunked Merkle layout so that byte ranges can be fetched
and verified independently of the whole — a requirement for streaming and
swarm exchange.

The **chunk tree** of a blob is defined as follows:

1. Split the blob into chunks of exactly 1 MiB (1,048,576 bytes); the
   final chunk MAY be shorter but MUST NOT be empty unless the blob is
   empty.
2. Compute each leaf as `BLAKE3-256( 0x00 || chunk_bytes )`.
3. Pair adjacent nodes left to right and compute each interior node as
   `BLAKE3-256( 0x01 || left || right )`. Where a level has an odd number
   of nodes, the final node is promoted unchanged to the next level.
4. The **chunk root** is the single node remaining at the top. For a blob
   of one chunk, the chunk root is that chunk's leaf hash.

The prefix bytes `0x00`/`0x01` domain-separate leaves from interior nodes
and preclude second-preimage reinterpretation of interior nodes as
leaves.

A record that references a large blob SHOULD carry, in its body per the
kind's schema, the blob's total size in bytes and its chunk root. A
verifier that trusts the signed record can then verify any received chunk
against the chunk root through a Merkle path of sibling hashes, without
possessing the rest of the blob. Verifying the flat blob id
(Section 5.6) still requires hashing the complete bytes; the chunk root
exists to make incremental and out-of-order retrieval verifiable.

## Fetch hints

Records that reference blobs MAY carry hints: a typed, ordered list of
retrieval suggestions, in the position the kind's schema defines.

```
Hint = [ hint_type: uint, value: text or bytes ]
```

**Hint types**

| Id | Name | Value |
|---:|------|-------|
| 1 | `https` | URL from which the blob may be fetched with HTTP range requests |
| 2 | `torrent-v2` | BitTorrent v2 infohash |
| 3 | `relay-blob` | Base URL of a relay blob sidecar (Blossom-compatible) |
| 4 | `bundle` | Locator for a bundle (Section 11.2) containing the blob |

Hints are advisory and additive. Any host answering for a hash is
equivalent; a client MUST NOT treat a blob fetched from an unlisted
source differently, since the hash proves integrity regardless of origin.
New transports are new hint types, added by registry.

## JSON interchange

A JSON representation is permitted for debugging, documentation, and APIs
that prefer text. The mapping is mechanical: integer map keys become their
decimal-string forms, byte strings become lowercase hex strings prefixed
with `"hex:"`, and all other values map naturally. The JSON form is never
the signed form: any implementation accepting JSON records MUST convert
them to canonical CBOR to derive ids and verify signatures.

## Time

`created_at` is an unverified author claim. Consumers MUST NOT treat it
as proof of when a record was created, and MUST NOT assume arrival order
reflects creation order (Principle 8).

Where ordering matters — claims, disputes, contested rotations — records
SHOULD be strengthened by **anchoring**: a periodic `anchor` record
(Section 7) commits a Merkle root of recently observed record ids to one
or more external timestamp systems (for example, OpenTimestamps).
Anchoring is an optional extension. The kernel guarantees tamper
evidence, never absolute time.

# Identity

## Identifiers

An identity is a stable identifier bound to a *current* signing key
through a **rotation log**: a chain of signed `rotation` records stored in
the substrate itself. The design is deliberately DID-like without
requiring any external DID infrastructure.

* Identities are never derived from, or bound to, domains, gateways, or
  relays.
* An identity's identifier is the record id of its genesis rotation
  record (Section 6.2). It is therefore self-certifying: possession of
  the genesis record proves the binding between identifier and initial
  keys.

## Genesis

An identity is created by publishing a `rotation` record whose body
declares the initial signing key and zero or more recovery keys, and
whose `author.identity_id` is 32 zero bytes (Section 5.5). The identity's
identifier is the record id of this genesis record.

Genesis records are self-signed: the signature is produced by the
declared initial signing key.

## Rotation

A non-genesis `rotation` record references the previous rotation record
in `refs` and names, in its body, the new signing key and (optionally) a
new recovery key set. A rotation record is authorized if it is signed by:

* the identity's current signing key (per the chain so far), or
* any of the identity's currently declared recovery keys.

Rotation is also the crypto-agility migration path: rotating to a key of
a newer registered algorithm moves the identity forward without any
protocol fork.

## Recovery precedence

Verifiers accept the longest valid chain, with one exception that gives
theft recovery precedence over the thief: within a **contest window**
declared in the identity's rotation records, a recovery-key-authorized
rotation supersedes any signing-key-authorized rotation that forks from
the same predecessor. An attacker who steals a signing key can therefore
be evicted by the holder of a recovery key even after the attacker has
rotated; the attacker cannot evict the recovery-key holder, because
recovery keys cannot be replaced by a signing-key-authorized rotation
during the window.

Recovery keys MAY be held by the user (for example, a hardware key), by
the user's gateway (custodial convenience), or split among social
contacts by threshold. Custody is permitted and expected for mainstream
users. **Custody must never be capture:** leaving a custodial gateway is
a rotation, available at any time, requiring nothing from the gateway.

## Delegation

A `delegate` record grants a named capability (for example, "produce
renditions for my manifests") from one identity to another, and is
revocable by a later record from the grantor. Delegation never transfers
identity ownership; it authorizes specific, kind-scoped actions that
other kinds' validation rules consult (see Section 8.2).

## Profiles

Kind `profile` carries display name, avatar blob, payment pointers
(Section 13), and declared relays and seed endpoints. Profiles are
ordinary records. The current profile of an identity is the latest valid
profile record by chain order of the identity — not by `created_at`,
which is untrusted.

# Record Kind Registry

Kinds are registered numeric identifiers. Kind 0 is reserved. The launch
registry is:

| Id | Kind | Section | Summary |
|---:|------|---------|---------|
| 1 | `rotation` | 6 | Identity genesis and key rotation |
| 2 | `profile` | 6.6 | Display name, avatar, pointers, endpoints |
| 3 | `delegate` | 6.5 | Capability grant to another identity |
| 16 | `manifest` | 8.1 | Canonical identity of a video |
| 17 | `supersede` | 5.1 | Replaces a prior record by the same author |
| 18 | `retract` | 5.1 | Requests withdrawal of a prior record |
| 19 | `mirror` | 8.4 | "Identity X pins blob H" |
| 20 | `similarity` | 8.4 | Near-duplicate assertion with method and score |
| 32 | `comment` | 15 | Threaded discussion on manifests and comments |
| 33 | `reaction` | 15 | Typed reaction to a record |
| 34 | `follow` | 15 | Follows an identity |
| 35 | `playlist` | 15 | Ordered list of manifests |
| 36 | `channel` | 15 | Curated collection under an identity |
| 48 | `claim.author` | 9 | "I authored this work" |
| 49 | `claim.license` | 9 | Grant or change of license terms |
| 50 | `claim.transfer` | 9 | Rights assignment, signed by assignor |
| 51 | `claim.dispute` | 9 | Contest of a prior claim, with evidence |
| 64 | `notice.takedown` | 9 | Structured legal notice as a signed record |
| 65 | `notice.counter` | 9 | Counter-notice |
| 66 | `feed.takedown` | 14 | Subscribable signed feed of takedown subjects |
| 80 | `endorse.gateway` | 15 | Creator designates official gateways |
| 81 | `receipt` | 13 | Signed payment receipt (tips, superchats) |
| 82 | `attest` | 15 | Portable third-party attestation |
| 96 | `anchor` | 5.11 | External timestamp commitment |
| 97 | `keygrant` | 12 | Content key wrapped to a recipient |
| 112 | `live.manifest` | 11.1 | Rolling signed manifest of live segments |
| 113 | `live.chat` | 15 | Ephemeral live-stream chat message |

Identifiers are grouped by concern with deliberate gaps for future
assignments in each group. Assignments are stable: an id, once assigned,
is never reused, even if a kind is deprecated.

New kinds enter this registry through the governance process
(Section 16) and only after two independent implementations interoperate
against the conformance suite. The kernel is never versioned for
features; new features are new kinds.

Full body schemas, ref semantics, validation rules, and worked examples
for each kind are specified in the kind registry companion documents
(`spec/003-kinds-registry.md` and Sections 8–15 of this document at the
level of interoperability requirements).

# Content Layer

## Video manifest

Kind `manifest` is the canonical identity of a video. Its body carries:

```
body {
  title:        text
  description:  text
  tags:         [ text ]
  language:     text (BCP 47)
  original:     { blob, size, chunk_root, codec, duration, dimensions }
  renditions:   [ { blob, size, chunk_root, codec, resolution, bitrate,
                    produced_by: IdentityRef, derivation_sig: bytes } ]
  captions:     [ { blob, language, format } ]
  thumbnail:    blob
  encryption:   null / { scheme, key_hint }        ; Section 12
  license:      SPDX id / "all-rights-reserved" /
                "mirror-freely" / "endorsed-only"
  sponsorship:  [ { start, end, sponsor_label } ]  ; disclosure, Section 13
  payment:      [ PaymentPointer ]                 ; Section 13
  hints:        [ Hint ]                           ; Section 5.9
}
```

The manifest's record id is the video's identity for reference purposes:
comments, claims, receipts, playlists, and similarity assertions all
reference the manifest, not the underlying blobs.

## Renditions as verifiable derivations

Renditions are **verifiable derivations**. Each rendition entry:

* declares the original blob hash it derives from (implicitly, the
  manifest's `original.blob`);
* names its producer (`produced_by`) — the creator, or a transcoder
  identity holding a `delegate` grant from the creator; and
* carries the producer's signature (`derivation_sig`) over the derivation
  statement: the tuple (original blob id, rendition blob id, codec,
  resolution, bitrate), canonically encoded and domain-separated with the
  prefix `"evermesh:derivation:v1"`.

Gateways can therefore serve third-party renditions without blind trust:
the signature proves *who* asserts the derivation, and delegation proves
the creator authorized that producer. The rendition family shares the
manifest's identity: byte-different encodings of the same video do not
fragment into separate "videos."

Note that a derivation signature proves accountability, not transcoding
correctness — a malicious delegated transcoder could sign an unfaithful
rendition. The remedy is revocation of the delegation and edge
reputation, consistent with Section 9's honesty requirement.

## License field

`license` is machine-readable creator consent: an SPDX license
identifier, or one of the Evermesh-specific values
`all-rights-reserved`, `mirror-freely`, `endorsed-only`. It enforces
nothing; it makes violations legible and gives compliant gateways and
nodes something to honor automatically (for example, a node
auto-subscribing only to `mirror-freely` content).

## Deduplication

* **Exact duplicates** are impossible by construction: identical bytes
  are one hash, and a re-upload merely adds a host for the existing
  blob.
* **Hosting is discovery, not registry.** "Who has this blob" is
  answered live by swarm and tracker announce, plus explicit `mirror`
  records ("identity X pins blob H"), which also drive node
  subscriptions and archival feeds.
* **Near-duplicates** (re-encodes, trims) are handled at the edge. Any
  party MAY publish `similarity` records ("blob J resembles manifest M,
  method=phash-v2, score=0.97"). Similarity is deliberately not kernel
  truth — algorithms improve and matches are contestable — it is
  evidence that gateways weigh, typically folding duplicates into the
  canonical manifest page and redirecting payment pointers when the
  claims chain (Section 9) supports it.

# Claims and Provenance

Each video accumulates a per-manifest, append-only, tamper-evident chain
of legal and provenance statements, built from ordinary records
referencing the manifest:

| Kind | Statement |
|------|-----------|
| `claim.author` | "I authored this work" |
| `claim.license` | Grant or change of license terms |
| `claim.transfer` | Rights assignment to another identity, signed by the assignor |
| `claim.dispute` | Contest of a prior claim, with free-text or blob evidence |
| `notice.takedown` | A structured legal notice (for example, DMCA) as a signed record |
| `notice.counter` | Counter-notice |

**Honesty requirement (normative):** implementations MUST present claims
as *assertions with provenance*, never as verified truth. A signature
proves authorship of the statement, not the statement. Strength comes
from composition: anchored timestamps (Section 5.11) for priority
evidence; C2PA-compatible capture metadata carried as claim attachments;
and the author's off-platform footprint. Courts and gateways interpret;
the protocol only preserves the evidence trail.

Because claims are ordinary records, they merge across partitions like
everything else. Competing claims created in isolation coexist on merge;
anchoring provides ordering evidence, and disputes make disagreement
explicit rather than resolving it silently.

# Relay Layer

## Function

Relays store and gossip records. A relay interprets nothing beyond the
envelope: it validates envelope integrity (Section 5.3), stores the
record, answers filtered subscriptions (by kind, author, refs, and time
of receipt), and forwards new records to peer relays with loop
suppression by record id.

The full metadata corpus is small relative to blobs and is designed for
whole or filtered replication by anyone (Principle 3). A relay MAY
additionally operate a blob sidecar, serving and accepting blobs verified
against their hash, with range requests served via the chunk tree
(Section 5.8).

Relays MUST accept records of unknown kind that pass envelope
validation. A relay is not a gateway: it carries no serving obligations
and no moderation duties beyond its own resource policy.

## Anti-spam

Anti-spam is layered, never centralized:

* **Write friction.** Records MAY be accompanied by a proof-of-work
  nonce. The work function is
  `BLAKE3-256( id || nonce )`; difficulty is the count of leading zero
  bits of the digest. The nonce travels in the transport frame beside
  the record, not inside the signed envelope, so proof-of-work can be
  added or strengthened for an existing record without re-signing.
  Relays advertise their minimum difficulty in their policy document.
  Difficulty is calibrated to be free-feeling for humans and expensive
  in bulk.
* **Relay policy.** Per-key rate limits; relays are free to be
  selective.
* **Read-side reputation.** Ranking, counting, and surfacing are gateway
  concerns, weighted by key age, proof-of-work spent, web-of-trust
  distance, and gateway-local signals. Spam may exist in the substrate;
  it competes for surfacing it never wins.

## Aggregates

Aggregate numbers — view counts, like counts — are **per-gateway
computed claims**, optionally published as signed tallies that others
MAY sum. The specification explicitly embraces that different gateways
show different numbers. Fraud-proof global counting is declared out of
scope: it is the ad-fraud problem in disguise, and any mechanism strong
enough to solve it would violate Principles 1 and 8.

# Distribution and Bundles

## Live transports

Reference transports for interactive retrieval are: HTTPS range
requests; BitTorrent v2 swarms; and a WebRTC viewer-assist swarm for HLS
segments, in the style proven by PeerTube. None is mandatory
(Principle 4); each corresponds to a hint type (Section 5.9).

Live streams hash segments as they are produced under a rolling signed
`live.manifest`; at stream end the accumulated segment list is published
as an ordinary `manifest`, becoming the video-on-demand record.

## Bundle format

A **bundle** is a single-file, self-verifying export: a container of
records plus blobs plus an index, in the spirit of CAR files. Two
operations define it:

```
export(records, blobs, filter) -> bundle
import(bundle)                 -> verified records and blobs
```

Import MUST verify everything: envelope validity of every record, hash
integrity of every blob, and index consistency. A bundle is trusted for
exactly nothing beyond what its contents prove.

The bundle makes every non-interactive transport equivalent: hard
drives, radio bursts, satellite windows, delay-tolerant networking
(Bundle Protocol [RFC 9171]), inter-site synchronization, backups, and
archival ingestion. The bundle format is normative in v1: it is the
partition-tolerance story made concrete.

## Partition posture

* No kind may require global consensus or synchronous availability
  (Principle 8).
* Signed parent references make comment threads and claim chains merge
  cleanly regardless of arrival order or delay, in the manner of git
  reconciliation.
* Isolated partitions — a LAN, a habitat, a continent — run complete
  replicas. Gateways are discoverable by local announcement (mDNS); no
  DNS or certificate authority is required for correctness.
* Freshness degrades with link delay; integrity never does.

# Privacy

Privacy is key distribution, not server permission. The substrate never
inspects blobs.

| Mode | Mechanism |
|------|-----------|
| Public | Plain blobs; manifest published to relays |
| Unlisted | Encrypted blobs; manifest unpublished; decryption key travels in the URL fragment |
| Private | Encrypted blobs; content key wrapped to each recipient's public key via `keygrant` records or encrypted messages |
| Gated | As Private, with a gateway acting as key vendor (pay, then receive a wrapped key) |

Metadata privacy is explicit: private manifests are never published to
public relays. The manifest format carries encryption fields from v1
(Section 8.1); retrofitting privacy onto a plaintext-assuming format is
rejected as a known failure mode.

Nodes holding ciphertext they cannot inspect — by explicit subscription
only — is the intended and most legally defensible posture.

# Economic Primitives

The protocol ships three primitives and no business model
(Principle 6):

* **Payment pointers**, in profiles and manifests: a typed, ordered
  list. Registered pointer types at launch: `lightning`, `usdc-base`,
  `stripe`, `paypal`. The list is rail-agnostic by design; fiat is
  first-class; no rail is required.
* **Receipts** (kind `receipt`): signed statements linking payer,
  amount, rail, and an optional message to a manifest or stream.
  Gateways render them as tips or superchats at their discretion.
* **Disclosures**: the manifest `sponsorship` field for
  creator-embedded sponsorship segments.

Non-normative guidance: gateways publish signed revenue-share
commitments; creators publish `endorse.gateway` records; audiences
follow endorsements. Trust arises from auditability and exit, not
cryptographic enforcement. A protocol token is permanently out of scope.

# Gateways

A gateway is a public web service, on its own domain, that indexes and
serves a *selection* of the substrate. Selection is the moderation
model.

* **Local policy is absolute and instant.** Allow and deny decisions by
  hash, key, kind, or category are gateway configuration, not protocol
  actions. Non-serving is a first-class, cheap operation. Nothing a
  gateway does removes anything from the substrate.
* **Compliance feeds.** Organizations publish signed feeds of
  takedown-subject hashes and keys (kind `feed.takedown`). Gateways
  subscribe according to their jurisdiction and policy, inheriting the
  ecosystem's compliance work the way mail servers consume blocklists.
  Feeds are plural, opt-in, and auditable: an over-blocking feed loses
  subscribers to a competitor. Legal notices themselves are
  machine-readable records (Section 9).
* **CSAM handling** is the single non-configurable element of the
  *reference* gateway: industry hash-matching at upload and index time,
  plus a mandatory reporting workflow. This is a
  reference-implementation requirement and a condition of the trademark
  program (Section 16) — not a kernel rule, because the kernel cannot
  enforce it.
* **A legal toolkit ships with the software:** templated terms of
  service and acceptable-use policies, DMCA agent guidance, notice and
  counter-notice interfaces, per-item geo-blocking, age-gating hooks,
  and jurisdiction compliance profiles. Lowering the legal cost of
  running a gateway is decentralization infrastructure.
* **Rogue gateways** face their own jurisdictions alone. Liability does
  not propagate: the foundation operates nothing, other gateways serve
  their own selections, nodes pin only by choice. The network's
  remedies are social and structural — disassociation feeds, trademark
  denial, creator non-endorsement — never protocol deletion, because
  any mechanism strong enough to force one gateway's compliance is
  strong enough to censor the network.

Gateways also carry the competitive product surface: transcoding
services, search, recommendation, custodial key management, and
economic services.

**Portable recommendation feeds.** A feed is a signed object — an
algorithm reference or a service endpoint — that any gateway can embed;
recommenders need not be gateways. This keeps discovery contestable
even when one gateway dominates. Because the metadata corpus is fully
replicable, any competitor can bootstrap search without permission. The
win condition for decentralization is not preventing dominance but
keeping dominance permanently contestable.

# Social and Interactive Kinds

All state lives in the substrate as signed records; interpretation,
ranking, and presentation live at gateways.

| Kind | Notes |
|------|-------|
| `comment` | References a manifest or a parent comment; threads merge by refs |
| `reaction` | Typed reactions; counts are per-gateway aggregates (Section 10.3) |
| `follow`, `playlist`, `channel` | Signed lists; fully portable across gateways |
| `live.manifest`, `live.chat` | Rolling signed segment manifests; ephemeral publish–subscribe chat over relays |
| `receipt` | Tips and superchats (Section 13) |
| `attest` | Portable reputation: "gateway G attests identity K reached milestone X" — achievements survive any gateway |
| `endorse.gateway` | Creator designates official gateways |
| `mirror`, `similarity`, `anchor`, `feed.takedown` | As defined in earlier sections |

New features are new kinds in the registry, subject to the
two-implementations rule. The kernel is never versioned for features.

# Governance

* The specification, the registries, and the conformance suite live
  under open licenses at the foundation, which is funded by grants and
  donations, holds the trademark, and is constitutionally barred from
  operating gateways and from issuing tokens.
* Changes follow a lightweight RFC process. Kinds graduate into the
  registry only via the two-implementations rule against the public
  conformance suite.
* "Evermesh-compliant" is a certifiable trademark claim, carrying the
  reference-gateway obligations (including Section 14's CSAM handling).
  It is separable from protocol participation, which is permissionless.
* The constitution's real enforcement mechanism is Principle 3: the
  community's credible ability to fork with everything. This document
  is written to survive its authors.

# Security Considerations

* **Signatures authenticate statements, not truth.** The claims layer
  (Section 9) is evidence preservation, not adjudication;
  implementations MUST present it as such.
* **Counts are claims.** Aggregates are per-gateway assertions
  (Section 10.3); consumers must expect divergence.
* **Spam.** Proof-of-work raises bulk-writing cost without excluding
  humans; relay rate limits and read-side reputation complete the
  defense in depth (Section 10.2).
* **Key theft** is bounded by recovery-precedence rotation
  (Section 6.4). The contest window is the recovery holder's guaranteed
  eviction period; its length trades convenience against exposure.
* **Encrypted content** leaks size and timing, but not content, and —
  if the manifest is unpublished — not metadata (Section 12).
* **Attack surface.** The kernel's attack surface is deliberately
  limited to signature verification, hashing, and CBOR parsing.
  Implementations MUST NOT panic on untrusted input; parsers are
  expected to be fuzzed.
* **Canonicalization.** Because ids and signatures are computed over
  canonical bytes, any encoder divergence is a consensus-integrity bug.
  Verifiers MUST reject non-canonical encodings rather than
  re-canonicalize them.

A full threat model is a required v1 companion document
(`spec/011-threat-model.md`).

# Relationship to Prior Art

Nostr contributes the permissionless signed-record substrate and the
zap pattern. AT Protocol contributes rotation-based identity and the
separation between substrate and application views. PeerTube
contributes the production playbook for federated video and
swarm-assisted HLS. Scuttlebutt contributes the offline-first,
bundle-syncable posture, with content addressing correcting its
single-log limitation.

Evermesh's contribution is the combination, plus the two layers all of
them lack: an accountable-but-optional gateway compliance model, and a
claims and provenance chain.

# References

## Normative references

* **[RFC 2119]** Bradner, S., "Key words for use in RFCs to Indicate
  Requirement Levels", BCP 14, RFC 2119, March 1997.
* **[RFC 8174]** Leiba, B., "Ambiguity of Uppercase vs Lowercase in
  RFC 2119 Key Words", BCP 14, RFC 8174, May 2017.
* **[RFC 8949]** Bormann, C. and P. Hoffman, "Concise Binary Object
  Representation (CBOR)", STD 94, RFC 8949, December 2020.
* **[RFC 8032]** Josefsson, S. and I. Liusvaara, "Edwards-Curve
  Digital Signature Algorithm (EdDSA)", RFC 8032, January 2017.
* **[BLAKE3]** O'Connor, J., Aumasson, J-P., Neves, S., and Z.
  Wilcox-O'Hearn, "BLAKE3: one function, fast everywhere", 2020.
  <https://github.com/BLAKE3-team/BLAKE3-specs>
* **[BCP 47]** Phillips, A. and M. Davis, "Tags for Identifying
  Languages", BCP 47, RFC 5646, September 2009.
* **[SPDX]** The Linux Foundation, "SPDX License List".
  <https://spdx.org/licenses/>

## Informative references

* **[RFC 9171]** Burleigh, S., Fall, K., and E. Birrane, "Bundle
  Protocol Version 7", RFC 9171, January 2022.
* **[CAR]** InterPlanetary File System project, "Content Addressable
  aRchives (CAR)". <https://ipld.io/specs/transport/car/>
* **[C2PA]** Coalition for Content Provenance and Authenticity,
  "C2PA Technical Specification". <https://c2pa.org/specifications/>
* **[OTS]** Todd, P., "OpenTimestamps: Scalable, Trust-Minimized,
  Distributed Timestamping". <https://opentimestamps.org/>
* **[HLS]** Pantos, R. and W. May, "HTTP Live Streaming", RFC 8216,
  August 2017.

# Appendix A. Registry Initial Assignments {-}

For ease of implementation, all launch registry assignments are
collected here. Id 0 is reserved in every registry.

**Signature algorithms:** 1 = Ed25519.

**Hash algorithms:** 1 = BLAKE3-256.

**Hint types:** 1 = `https`, 2 = `torrent-v2`, 3 = `relay-blob`,
4 = `bundle`.

**Payment pointer types:** 1 = `lightning`, 2 = `usdc-base`,
3 = `stripe`, 4 = `paypal`.

**Ref types:** 0 = record, 1 = blob.

**Record kinds:** as tabulated in Section 7.

# Appendix B. Changes from Draft 0.1 {-}

This draft professionalizes Draft 0.1 ("Specification Proposal") without
altering its architecture. Where Draft 0.1 was ambiguous, this draft
chooses the interpretation that best satisfies the design principles and
records the decision here:

1. **Envelope layout.** The envelope is fixed as a CBOR map with integer
   keys 1–7 (Section 5.1). Draft 0.1 named the fields but not their
   encoding.
2. **Identifier coverage.** The record id covers keys 1–6, *including*
   `sig_alg`, to prevent algorithm-downgrade substitution
   (Section 5.3).
3. **Signature input.** The signature is over the domain-separated id
   (`"evermesh:record:v1" || id`), not over raw envelope bytes
   (Section 5.4).
4. **IdentityRef shape.** Defined as
   `[identity_id, signing_key]` so records are verifiable before the
   rotation chain is available, with authorization checked against the
   chain when present (Section 5.5). Genesis records carry a zero
   `identity_id` to break the self-reference cycle (Section 6.2).
5. **Ref shape.** Refs are typed pairs `[ref_type, hash]`
   distinguishing record and blob references (Section 5.6).
6. **Chunk tree construction.** Fixed 1 MiB chunks; leaves
   `BLAKE3-256(0x00 || chunk)`; interior nodes
   `BLAKE3-256(0x01 || left || right)`; odd nodes promoted
   (Section 5.8). Draft 0.1 said only "chunked Merkle layout."
7. **Proof-of-work placement.** The PoW nonce travels in transport
   frames, outside the signed envelope, so work can be added or
   strengthened without re-signing (Section 10.2).
8. **Kind registry numbers.** Numeric ids assigned, grouped by concern
   with gaps (Section 7).
9. **Clean-room kernel.** Draft 0.1 left open the choice between a
   clean-room protocol and a Nostr superset (its Section 15). The
   reference-implementation program has locked the clean-room envelope
   with Ed25519 and BLAKE3-256 launch algorithms; this draft therefore
   specifies the clean-room kernel. The question is retained in
   Appendix C for the record.

# Appendix C. Open Questions {-}

1. **Nostr interoperability.** With the clean-room kernel locked
   (Appendix B, item 9), what bridging layer, if any, should map
   Evermesh records into Nostr events and Blossom blob conventions for
   ecosystem reach? A bridge can be an edge service and requires no
   kernel change.
2. **Contest window default.** The recovery contest window
   (Section 6.4) needs a recommended default duration, balancing theft
   recovery against rotation latency for legitimate high-frequency key
   hygiene.
3. **Encrypted messaging.** Private mode references "encrypted
   messages" for key delivery (Section 12); the message kind and its
   sealing construction are unspecified and needed before Private mode
   is implementable end to end.
4. **Anchor targets.** Which external timestamp systems the reference
   implementation should support at launch (Section 5.11).

---

*Everything contentious is at the edge. Everything at the center is
math.*
