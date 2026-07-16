# tools/conformance

The Vidmesh conformance suite: JSON+CBOR fixture vectors covering every record
kind (valid + invalid mutations), identity chains, chunk-tree range proofs,
and bundle round-trips — plus a Rust runner that executes them against the
kernel crate, `@vidmesh/kernel` under Node, and a live relay over websocket.

This suite is what makes the two-implementations rule enforceable — it is a
first-class product.

**Status: Phase 0 scaffold — built in Phase 7.** Run with `just conformance`.
