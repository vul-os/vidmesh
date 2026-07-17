# 001: Kernel

**Status:** Draft 0.2
**Depends on:** [000-overview.md](000-overview.md)
**Depended on by:** every other file

The kernel is the invariant core of Vidmesh: the record envelope, the
canonical encoding, identifier and signature derivation, the algorithm
registries, blob addressing, the chunk-tree layout for verified range
reads, and fetch hints. Everything else in the protocol is a record kind
layered on these definitions. This file is normative and complete: an
implementation of this file alone can create and verify any record and any
blob, including records of kinds it has never seen.

## 1. Record envelope

A **record** is a small, signed, immutable statement encoded as a CBOR map
with integer keys. The envelope fields are:

| Key | Name | CBOR type | Description |
|----:|------|-----------|-------------|
| 1 | `kind` | unsigned int | Registered record kind ([003](003-kinds-registry.md)) |
| 2 | `author` | IdentityRef | Signing identity (§5) |
| 3 | `created_at` | int | Author-asserted Unix time, seconds (untrusted; §10) |
| 4 | `refs` | array of Ref | Parents, subjects, threading (§6) |
| 5 | `body` | map | Kind-defined CBOR map |
| 6 | `sig_alg` | unsigned int | Registered signature algorithm (§7) |
| 7 | `sig` | byte string | Signature (§4) |

Envelope rules (normative):

* The envelope MUST contain exactly keys 1–7; unknown envelope keys MUST
  cause rejection. Extension happens in `body`, never in the envelope.
* Records are immutable. "Edit" and "delete" are new records (kinds
  `supersede`, `retract`) referencing the target. Retraction is a
  request, not an erasure.
* `refs` MAY be empty. `body` MAY be empty. Both MUST be present.
* Relays and other index infrastructure MUST accept, store, and forward
  records of unknown kind opaquely, provided the envelope verifies
  (§3). Clients MUST ignore records of unknown kind safely.
* Implementations MUST NOT crash or exhibit undefined behavior on
  malformed input. A record failing any rule is rejected, never
  partially processed.

## 2. Canonical encoding

The canonical encoding of a record — and of any structure for which this
specification requires one — is CBOR (RFC 8949) constrained by the Core
Deterministic Encoding Requirements (RFC 8949 §4.2.1) plus:

1. All lengths MUST be definite.
2. Integers MUST use the shortest encoding representing the value.
3. Map keys MUST be unique and sorted in bytewise lexicographic order of
   their canonical encodings.
4. Floating-point values MUST NOT appear in the envelope, and MUST NOT
   appear in bodies of kinds defined by this specification (fractional
   values use fixed-point integers, e.g. basis points).
5. Tags MUST NOT appear unless a kind specification explicitly requires
   a specific tag.

Two conforming encoders produce byte-identical output for the same
abstract record. Verifiers MUST reject non-canonical encodings rather
than re-canonicalize them: any encoder divergence is a
consensus-integrity bug, not a formatting preference.

## 3. Record identifier and validity

```
id = BLAKE3-256( canonical_cbor( envelope map with keys 1..6 ) )
```

The id covers everything except the signature. Because `sig_alg` (key 6)
is under the hash, a record is committed to its algorithm and downgrade
substitution is precluded. The id is 32 bytes; its text form is lowercase
hexadecimal. The id is derived, never serialized inside the envelope.

A record is **envelope-valid** if and only if:

1. it decodes as a CBOR map with exactly keys 1–7 and the types of §1;
2. its encoding is canonical (§2);
3. `sig_alg` is a registered algorithm known to the verifier — a
   verifier MUST reject records whose `sig_alg` it does not implement,
   never skip signature verification; and
4. the signature verifies (§4).

Kind-level validity ([003](003-kinds-registry.md)) is layered above
envelope validity. Infrastructure that stores records opaquely performs
envelope validation only.

## 4. Signatures

```
sig = Sign( signing_key, "vidmesh:record:v1" || id )
```

`||` is byte concatenation; the domain-separation prefix is the 17-byte
ASCII string `vidmesh:record:v1` with no terminator. Verification
recomputes `id` from the received bytes and verifies `sig` against
`author.signing_key` under `sig_alg`.

Every signing context in Vidmesh uses a distinct prefix. Contexts defined
by this specification:

| Prefix | Context |
|--------|---------|
| `vidmesh:record:v1` | Record signatures (this section) |
| `vidmesh:derivation:v1` | Rendition derivation statements ([004](004-manifest.md) §3) |

## 5. Identity references

```
IdentityRef = [ identity_id: bytes(32), signing_key: bytes ]
```

encoded as a two-element CBOR array. `identity_id` is the identity's
stable identifier ([002](002-identity.md)); `signing_key` is the public
key, in the encoding defined by `sig_alg`, that produced the signature.

A record carries its verification key so it is checkable before the
rotation chain is available. Whether `signing_key` was an **authorized**
key of `identity_id` is determined by the rotation log: consumers holding
the chain MUST check authorization ([002](002-identity.md) §4);
consumers without it MAY treat the record as provisionally attributed.

In an identity's genesis record — and only there — `identity_id` is 32
zero bytes ([002](002-identity.md) §2).

## 6. Refs and blob addressing

```
Ref = [ ref_type: uint, hash: bytes(32) ]
```

| `ref_type` | Meaning |
|-----------:|---------|
| 0 | Record reference: `hash` is a record id |
| 1 | Blob reference: `hash` is a blob id |

`refs` is ordered; kind specifications assign meaning to positions. The
kernel assigns none.

A **blob** is an opaque byte sequence addressed by the BLAKE3-256 hash of
its bytes. Text form:

```
b3-256:<64 lowercase hex characters>
```

The kernel never interprets blob contents.

## 7. Algorithm registries

**Signature algorithms (`sig_alg`)**

| Id | Algorithm | Public key | Signature | Reference |
|---:|-----------|-----------:|----------:|-----------|
| 0 | reserved | — | — | — |
| 1 | Ed25519 | 32 bytes | 64 bytes | RFC 8032 |

**Hash algorithms**

| Id | Algorithm | Digest | Reference |
|---:|-----------|-------:|-----------|
| 0 | reserved | — | — |
| 1 | BLAKE3-256 | 32 bytes | BLAKE3 specification |

At launch the hash algorithm is fixed at BLAKE3-256 for record ids, blob
ids, and chunk trees; the registry exists so future migration is a
registry entry plus identity rotation ([002](002-identity.md) §3), never
a fork. New entries in either registry require the governance process and
the two-implementations rule.

## 8. Chunk trees and verified range reads

Large blobs use a chunked Merkle layout so byte ranges can be fetched and
verified independently of the whole.

Construction:

1. Split the blob into chunks of exactly **1 MiB** (1,048,576 bytes); the
   final chunk MAY be shorter. The empty blob has zero chunks and no
   chunk tree.
2. Leaf hash: `BLAKE3-256( 0x00 || chunk_bytes )`.
3. Pair nodes left to right; interior hash:
   `BLAKE3-256( 0x01 || left || right )`. A level's unpaired final node
   is promoted unchanged.
4. The **chunk root** is the last remaining node. A one-chunk blob's
   chunk root is its single leaf hash.

The `0x00`/`0x01` prefixes domain-separate leaves from interior nodes.

A record referencing a large blob SHOULD carry, per its kind's schema,
the blob's `size` (bytes) and `chunk_root`. A **range proof** for chunk
`i` is the chunk's bytes plus the sibling hashes on the path to the
root; a verifier trusting the signed record verifies any chunk in
O(log n) hashes without the rest of the blob. Verifying the flat blob id
(§6) still requires the complete bytes; the chunk root exists for
incremental and out-of-order retrieval.

Servers advertising range support ([006](006-relay.md) §5) MUST serve
range proofs aligned to chunk boundaries.

## 9. Fetch hints

```
Hint = [ hint_type: uint, value: text ]
```

| Id | Name | Value |
|---:|------|-------|
| 0 | reserved | — |
| 1 | `https` | URL serving the blob with HTTP range requests |
| 2 | `torrent-v2` | BitTorrent v2 infohash, lowercase hex |
| 3 | `relay-blob` | Base URL of a relay blob sidecar ([006](006-relay.md) §5) |
| 4 | `bundle` | Locator of a bundle ([007](007-bundles.md)) containing the blob |

Hints are advisory and additive. Any host answering for a hash is
equivalent: a client MUST NOT treat a blob fetched from an unlisted
source differently, since the hash proves integrity regardless of
origin. New transports are new hint types.

## 10. Time

`created_at` is an unverified author claim. Consumers MUST NOT treat it
as proof of creation time and MUST NOT assume arrival order reflects
creation order. Where ordering matters, records SHOULD be strengthened by
anchoring (kind `anchor`, [003](003-kinds-registry.md) §4.24). The kernel
guarantees tamper evidence, never absolute time.

## 11. JSON interchange

A JSON representation is permitted for debugging, documentation, and
text-preferring APIs. Mapping: integer map keys become decimal strings;
byte strings become `"hex:<lowercase hex>"`; arrays, maps, ints, and text
map naturally. The JSON form is never the signed form: implementations
accepting JSON records MUST convert to canonical CBOR to derive ids and
verify signatures. A JSON record that cannot round-trip to canonical CBOR
is invalid.

## Decisions

* Envelope keys are integers 1–7; unknown envelope keys reject
  (extension lives in bodies).
* The id covers `sig_alg` (downgrade resistance); the signature is over
  the domain-separated id rather than raw envelope bytes.
* `IdentityRef` carries the signing key so records verify without the
  rotation chain; authorization is a separate, chain-dependent check.
* Chunk-tree leaves/interiors are domain-separated with `0x00`/`0x01`;
  odd nodes promote unchanged.

## Test vectors

Covered by conformance groups:

* `envelope/` — valid records; mutations: bad signature, non-canonical
  encoding, wrong id, unknown envelope key, unknown `sig_alg`.
* `chunktree/` — roots and range proofs for 0, 1, 2, 3, and 1000-chunk
  blobs, including a truncated final chunk; invalid: wrong sibling,
  swapped leaf prefix.
* `json/` — JSON↔CBOR round-trip fixtures, including a JSON record that
  MUST fail (non-round-trippable).
