# evermesh-kernel

The permanent core library of the Evermesh protocol. One Rust
implementation of the kernel, compiled natively and to WASM, so the same
crypto and canonical encoding runs everywhere.

Implements `spec/001-kernel.md`, `spec/002-identity.md`,
`spec/003-kinds-registry.md` (typed wrappers), and `spec/007-bundles.md`.

## API surface

- `codec` — canonical CBOR `Value`, `encode_canonical` /
  `decode_canonical` (strict: rejects all non-canonical input), JSON
  interchange (`to_json` / `from_json`).
- `record` — `RecordBuilder … .sign_as(&Keypair, IdentityId)`,
  `Record::{from_cbor, verify, id, to_canonical_cbor, to_json}`, `Ref`,
  `IdentityRef`.
- `identity` — `Keypair`, `Identity::{genesis, rotate, verify_chain}`
  with recovery-precedence fork resolution, `IdentityState`.
- `blob` — `hash_blob` / `hash_stream`, `ChunkTree::{build, from_bytes,
  root, prove}`, `verify_chunk` (1 MiB Merkle range proofs).
- `kinds` — typed parse/build wrappers and validation for all 27 launch
  kinds.
- `bundle` — `Bundle::{export, import}` and `import_streaming`
  (salvaging, memory-bounded).

Design rules: `#![forbid(unsafe_code)]`, no panics on untrusted input,
no dependencies beyond `blake3`, `ed25519-dalek`, `getrandom`.

## Testing

Unit tests in every module; property tests in `tests/properties.rs`
(canonical round-trips, merge-order independence); fuzz harnesses in
`fuzz/` (see `FUZZING.md`). Byte-exact cross-implementation fixtures
live in `tools/conformance/vectors`.

```sh
cargo test -p evermesh-kernel
```
