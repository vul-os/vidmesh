# vidmesh-kernel

The permanent core library of the Vidmesh protocol. One Rust implementation of
the kernel, compiled natively and to WASM, so the same crypto and canonical
encoding runs everywhere.

**Status: Phase 0 scaffold — no implementation yet.** Phase 2 fills this in
after the spec (`spec/001-kernel.md` and friends) is written.

## Planned API surface

- `Keypair` / `Identity` — key generation, genesis, rotation, chain
  verification with recovery precedence.
- `RecordBuilder` / `Record` — build, sign, verify, and canonically encode
  records; derive record ids.
- `Blob` / `ChunkTree` — BLAKE3 blob addressing, 1 MiB chunk Merkle trees,
  verified range reads.
- `kinds::*` — typed wrappers over the registered record kinds.
- `Bundle` — offline export/import with full verification on ingest.

## Testing

```sh
cargo test -p vidmesh-kernel
```

Conformance vectors live in `tools/conformance/vectors` and are run by
`just conformance`.
