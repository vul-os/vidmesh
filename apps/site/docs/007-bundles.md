# 007: Bundles

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md)
**Depended on by:** [009-gateway.md](009-gateway.md)

A **bundle** is a single-file, self-verifying export of records and
blobs. One well-defined pair of operations — `export(filter) → bundle`,
`import(bundle) → verified records and blobs` — makes every
non-interactive transport equivalent: hard drives, radio bursts,
satellite windows, delay-tolerant networking, inter-site sync, backups,
and archival ingestion. The bundle format is normative in v1: it is the
partition-tolerance story made concrete.

## 1. Container layout

A bundle is a byte stream:

```
bundle = magic || cbor_sequence
magic  = 45 56 4D 53 01            ; "EVMS" + format version 0x01
```

followed by a CBOR sequence (RFC 8742) of **items**, each a canonically
encoded CBOR array tagged by its first element:

| Item | Layout | Meaning |
|------|--------|---------|
| header | `["hdr", meta: map]` | MUST be first. `meta` MAY carry `description: text`, `source: text`; importers MUST tolerate unknown keys |
| record | `["r", record: bytes]` | One record as canonical envelope bytes |
| blob | `["b", id: bytes(32), content: bytes]` | A complete blob |
| blob part | `["bp", id: bytes(32), index: uint, part: bytes]` | One 1 MiB part of a large blob, in order; parts of one blob MUST be contiguous and index-consecutive from 0 |
| end | `["end", records: uint, blobs: uint]` | MUST be last; counts of records and distinct blobs |

Rules:

* Writers SHOULD emit records before the blobs they reference, and MUST
  split blobs larger than 16 MiB into `bp` parts at exactly 1 MiB per
  part (final part may be shorter) — aligned with the chunk tree of
  [001](001-kernel.md) §8 so importers can verify incrementally.
* Ordering is otherwise unconstrained; duplicate records (by id) and
  duplicate blobs are permitted and MUST be deduplicated on import.

## 2. Import verification (normative)

An importer MUST, before surfacing anything:

1. check magic and version;
2. envelope-validate every record ([001](001-kernel.md) §3);
3. hash-verify every blob (`b` content, or concatenated `bp` parts)
   against its declared id;
4. verify the `end` counts match what was read; a bundle without a
   well-formed `end` item is **truncated** — the importer MAY keep
   verified items read so far, and MUST report the truncation.

A bundle is trusted for exactly nothing beyond what its contents prove.
Invalid items are skipped with a report; one bad item MUST NOT abort
the import of valid items (radio and sneakernet links corrupt bytes;
salvage is the point).

Import is idempotent and merge-safe: importing the same bundle twice,
or two bundles with overlapping content, yields the same store as
importing once (records dedupe by id, blobs by hash).

## 3. Export filters

`export` accepts the same filter shape as relay subscriptions
([006](006-relay.md) §3, minus `since`/`limit`) plus:

| Key | Meaning |
|-----|---------|
| `blobs` | `"none"`, `"referenced"` (default: blobs referenced by exported records that the exporter holds), or explicit list of blob ids |
| `follow_refs` | uint — also export records referenced by matched records, to this depth (default 0) |

Exporters MUST NOT emit blobs they cannot hash-verify at export time.

## 4. Uses

* **Sneakernet / DTN:** a bundle is one file; any medium that moves
  files moves the substrate.
* **Relay seeding:** a new relay imports a bundle instead of syncing
  from zero.
* **Archival:** an archive ingests bundles and can prove integrity of
  every item forever.
* **Partition merge:** two isolated communities exchange bundles;
  records merge in any order (Principle 8), claims conflicts surface
  per [005](005-claims.md).

## Decisions

* CBOR sequence, not a custom binary layout or tar: one codec
  everywhere, streaming-writable, and salvageable after truncation.
* `bp` parts are fixed at 1 MiB to coincide with chunk-tree leaves, so
  a partially received bundle still yields verifiable ranges.
* No compression in the container: media blobs don't compress, and
  transparency beats cleverness (Principle 10). Whole-file compression
  MAY be applied externally (`.blka.zst`).

## Test vectors

* `bundle/roundtrip-*` — export→import fixture: records + small blob +
  3 MiB parted blob; byte-exact expected bundle.
* `bundle/salvage-*` — truncated bundle (valid prefix), one corrupted
  blob part among valid items, bad magic, missing `end`.
* `bundle/merge-*` — two overlapping bundles importing to the same
  final store.
