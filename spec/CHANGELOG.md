# Spec changelog

Newest first. Per build-plan §15, implementation-revealed spec bugs are
fixed in the same commit as the implementation and noted here.

## 2026-07-17 — Draft 0.2 split into per-concern files (Phase 1)

- Added `000-overview.md` … `011-threat-model.md`, transcribing and
  deepening `draft-vidmesh-protocol-00.md` (which remains the
  single-document rendering; on conflict the numbered files win).
- New normative material introduced by the split (all logged in
  DECISIONS.md and per-file "Decisions" headings):
  - full body schemas, refs semantics, validation rules, and worked
    examples for all 27 launch kinds (003);
  - deterministic identity fork-resolution algorithm with
    verifier-local finality (002 §4);
  - relay wire protocol: CBOR frames, filters, receipt-sequence
    cursors, PoW placement, blob sidecar with chunk proofs (006);
  - bundle container format `VMSH\x01` + CBOR sequence with salvage
    semantics (007);
  - content-encryption scheme 1 (chunked XChaCha20-Poly1305, per-blob
    keys, content-key wrap) and key-wrap registry (008 §2);
  - derivation-statement construction for renditions (004 §3);
  - uniform-reference-UI requirement for gateways (009 §7);
  - per-file test-vector indexes naming conformance groups.

## 2026-07-17 — Draft 0.2 (single document)

- `draft-vidmesh-protocol-00.md` professionalized Draft 0.1
  (`../VIDMESH_SPEC_PROPOSAL.md`); editorial decisions in its
  Appendix B (envelope keys, id/signature derivation, IdentityRef,
  ref typing, chunk-tree construction, PoW placement, kind ids,
  clean-room kernel).
