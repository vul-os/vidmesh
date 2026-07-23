# Evermesh specification

The Evermesh protocol specification, licensed CC-BY-SA-4.0 (see
`../LICENSE-SPEC`).

## Contents

The normative specification is the numbered per-concern files
(**Draft 0.2**); `000-overview.md` carries the map and reading order:

| File | Concern |
|------|---------|
| `000-overview.md` | Principles, roles, spec map |
| `001-kernel.md` | Envelope, canonical CBOR, ids, signatures, registries, blobs, chunk trees |
| `002-identity.md` | Rotation log, recovery precedence, delegation, profiles |
| `003-kinds-registry.md` | All 27 launch kinds: schemas, refs, validation, examples |
| `004-manifest.md` | Manifests, renditions, dedup, live streams |
| `005-claims.md` | Claims, disputes, notices |
| `006-relay.md` | Sync protocol, blob sidecar, anti-spam, gossip |
| `007-bundles.md` | Bundle container, partition posture |
| `008-privacy.md` | Encryption modes, keygrants |
| `009-gateway.md` | Selection, compliance, uniform UI |
| `010-economics.md` | Pointers, receipts, disclosures |
| `011-threat-model.md` | Adversaries, mitigations, residual risks |

Also here:

- `draft-evermesh-protocol-00.md` — the single-document rendering of
  Draft 0.2 (source for the PDF). On conflict, the numbered files win.
- `CHANGELOG.md` — spec change log.
- `pandoc-pdf.yaml` — pandoc defaults for the PDF rendering.

## Building the PDF

```sh
just spec-pdf   # requires pandoc and tectonic; writes dist/evermesh-protocol-draft-00.pdf
```

The output is a classic technical-report PDF: Latin Modern type, numbered
sections, hyperlinked table of contents, color only in code highlighting.

## Status

Phase 1 complete: all twelve files written, cross-referenced, and ending
with test-vector indexes that name the conformance groups covering them
(`tools/conformance`, built in Phase 7).
