# 004: Manifest and Content

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md), [002-identity.md](002-identity.md), [003-kinds-registry.md](003-kinds-registry.md)
**Depended on by:** [005](005-claims.md), [008](008-privacy.md), [009](009-gateway.md)

Kind `manifest` (16) is the canonical identity of a video: comments,
claims, receipts, playlists, and similarity assertions all reference the
manifest record, never raw blobs. This file specifies the manifest body,
the verifiable-derivation rule that lets untrusted parties transcode,
the license field, deduplication via `mirror` and `similarity`, and the
live-streaming flow.

## 1. Manifest body

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `title` | text | y | |
| `description` | text | n | |
| `tags` | [text] | n | ≤ 32 tags, each ≤ 64 bytes |
| `language` | text | n | BCP 47 tag |
| `original` | Media | y | The source encoding (§2) |
| `renditions` | [Rendition] | n | Derived encodings (§3) |
| `captions` | [Caption] | n | §2 |
| `thumbnail` | BlobId | n | |
| `encryption` | null or Enc | n | [008](008-privacy.md) §3; absent/null = plaintext |
| `license` | text | y | §4 |
| `sponsorship` | [Sponsor] | n | §5 of [010](010-economics.md) |
| `payment` | [PaymentPointer] | n | [010](010-economics.md) §1 |
| `hints` | [Hint] | n | Retrieval hints for all listed blobs |

Refs: optionally one `[0, <channel record id>]`
([003](003-kinds-registry.md) §4.1).

Worked example (conventions of [003](003-kinds-registry.md) §2):

```json
{ "kind": 16, "refs": [[0, "hex:6c20…"]],
  "body": {
    "title": "One River a Week — 12: Breede",
    "language": "en",
    "original": { "blob": "hex:9a77…", "size": 1289748480,
                  "chunk_root": "hex:2b0c…", "codec": "av01.0.08M.08",
                  "duration": 1841000, "width": 3840, "height": 2160 },
    "renditions": [
      { "blob": "hex:41d8…", "size": 402653184, "chunk_root": "hex:77aa…",
        "codec": "avc1.640028", "width": 1280, "height": 720,
        "bitrate": 2800000,
        "produced_by": ["hex:77d0…", "hex:4f2a…"],
        "derivation_sig": "hex:beef…" } ],
    "captions": [ { "blob": "hex:cc01…", "language": "en", "format": "vtt" } ],
    "thumbnail": "hex:ee59…",
    "license": "CC-BY-4.0",
    "payment": [[1, "asha@ln.example.net"]],
    "hints": [[1, "https://cdn.example.net/blob/"],
              [3, "https://relay.example.net/blob"]] } }
```

## 2. Media, Caption structures

```
Media = {
  blob:       bytes(32)      ; the encoded file
  size:       uint           ; bytes
  chunk_root: bytes(32)      ; required when size > 1 MiB (001 §8)
  codec:      text           ; RFC 6381 codecs string, e.g. "av01.0.08M.08"
  duration:   uint           ; milliseconds
  width:      uint
  height:     uint
}

Caption = { blob: bytes(32), language: text (BCP 47), format: text ("vtt") }
```

Validation: `chunk_root` MUST be present when `size` exceeds one chunk;
consumers MUST verify received ranges against it
([001](001-kernel.md) §8).

## 3. Renditions are verifiable derivations

```
Rendition = Media + {
  produced_by:    IdentityRef    ; who transcoded
  derivation_sig: bytes          ; §3.1
}
```

### 3.1 Derivation statement

The producer signs the derivation statement:

```
stmt = canonical_cbor( [ original.blob, rendition.blob, codec,
                         width, height, bitrate: uint ] )
derivation_sig = Sign( producer_key, "evermesh:derivation:v1" || BLAKE3-256(stmt) )
```

A rendition is **authorized** if `produced_by` is the manifest's author,
or an identity holding an unrevoked, unexpired `delegate` grant with
capability `rendition` from the author
([003](003-kinds-registry.md) §3.3). Gateways MAY serve third-party
renditions without blind trust: the signature proves *who* asserts the
derivation; delegation proves the author authorized them.

A derivation signature proves accountability, not transcoding
fidelity — a malicious transcoder can sign an unfaithful rendition. The
remedy is revocation plus edge reputation, consistent with the honesty
requirement of [005](005-claims.md) §2.

The rendition family shares the manifest's identity: byte-different
encodings of one video do not fragment into separate "videos."

## 4. License

`license` is machine-readable creator consent: an SPDX license
identifier, or one of:

| Value | Meaning |
|-------|---------|
| `all-rights-reserved` | No republication consent expressed |
| `mirror-freely` | Anyone may pin and serve unmodified |
| `endorsed-only` | Serving intended for gateways the creator endorses (kind 80) |

The field enforces nothing; it makes violations legible and gives
compliant gateways and nodes something to honor automatically (for
example, a node auto-subscribing only to `mirror-freely` content).
`claim.license` records ([005](005-claims.md)) modify licensing after
publication; interpreters present the latest position of the rights
chain.

## 5. Deduplication

* **Exact duplicates are impossible by construction**: identical bytes
  are one hash; a re-upload adds a host, not a copy.
* **Hosting is discovery, not registry.** "Who has this blob" is
  answered live (swarm/tracker announce) plus `mirror` records, which
  also drive node subscriptions and archival feeds.
* **Near-duplicates** are edge concerns via `similarity` records
  ([003](003-kinds-registry.md) §4.5). Similarity is evidence, never
  kernel truth: gateways typically fold high-confidence duplicates into
  the canonical manifest page and redirect payment pointers when the
  claims chain supports it — but MUST NOT auto-merge on a similarity
  record alone.

## 6. Live streams

A stream is a chain of `live.manifest` records
([003](003-kinds-registry.md) §9.1): the first record (seq 0) is the
stream id; subsequent records append segment batches; `final: true`
closes the stream, after which the creator SHOULD publish an ordinary
`manifest` as the VOD record, listing the segments' concatenation (or a
proper re-encode) as `original`.

Segments are ordinary blobs (2–6 s of media each); viewers verify each
segment hash against the signed rolling manifest, so live content has
the same integrity as VOD, delayed by one manifest publication.
`live.chat` records reference the stream id and MAY be expired
aggressively by relays.

## Decisions

* The derivation statement covers the codec/resolution/bitrate tuple so
  a signature cannot be replayed onto a different quality claim for the
  same blob pair.
* `chunk_root` is mandatory above one chunk — "optional integrity" for
  streaming-sized media would make range verification unreliable
  exactly where it matters.

## Test vectors

* `kinds/manifest/` — valid plaintext, valid with renditions and
  delegated producer, valid encrypted; invalids per
  [003](003-kinds-registry.md) plus: missing `chunk_root` on large
  media, bad `derivation_sig`, rendition by revoked delegate.
* `kinds/live.manifest/` — stream open/append/final triple; invalid:
  later record with empty refs.
* `derivation/` — statement construction fixtures (bytes-exact).
