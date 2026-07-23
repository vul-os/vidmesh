# Fuzzing the kernel

Build plan §6 requires the kernel to never panic on untrusted input, and
for the CBOR and bundle parsers to have `cargo-fuzz` harnesses in-repo.
Those harnesses live in `fuzz/` as a standalone crate (`evermesh-kernel-fuzz`,
detached from the root workspace so its nightly-only toolchain requirement
never leaks into `cargo build`/`cargo test` elsewhere in the monorepo).

## Setup

```sh
cargo install cargo-fuzz
```

`cargo-fuzz` requires a nightly toolchain (it builds with sanitizer
instrumentation `cargo build` can't). Install one if you don't have it:

```sh
rustup toolchain install nightly
```

## Running a target

From `crates/evermesh-kernel/`:

```sh
cargo +nightly fuzz run decode_canonical
cargo +nightly fuzz run record_from_cbor
cargo +nightly fuzz run bundle_import
cargo +nightly fuzz run json_from
```

Each command runs indefinitely, mutating a growing corpus under
`fuzz/corpus/<target>/`, until stopped (Ctrl-C) or a crash is found. Useful
flags: `-- -max_total_time=300` to cap a CI run to 5 minutes,
`-- -runs=100000` to cap by iteration count.

## What each target checks

- **`decode_canonical`** — feeds arbitrary bytes to
  `codec::decode_canonical`. Beyond "must not panic," this is the target
  that enforces canonicality itself: any value it accepts is re-encoded
  with `encode_canonical` and asserted byte-identical to the original
  input. A canonical decode that doesn't round-trip means the decoder
  accepted a non-canonical form, or the encoder and decoder disagree
  about what canonical means — this is the key invariant of spec 001 §2,
  and this target is where a violation would first surface.
- **`record_from_cbor`** — feeds arbitrary bytes to `Record::from_cbor`.
  Also exercises `verify()` (including against garbage keys/signatures
  and unknown `sig_alg`, which must return `Err`, not panic), `id()`, and
  asserts `to_canonical_cbor()` reproduces the original bytes for any
  record that parsed.
- **`bundle_import`** — feeds arbitrary bytes to `bundle::Bundle::import`
  as the bundle stream. `import` already bounds per-item size (64 MiB)
  and nesting depth (64) while streaming and salvages what it can
  (spec 007 §2), so this target is about catching any place those bounds
  are missed, not bounding memory itself. Every input is expected to
  return `Ok` (with a possibly-empty skip report) or `Err` on a bad magic
  / unreadable stream — never panic, never allocate unboundedly.
- **`json_from`** — feeds arbitrary UTF-8 to `codec::from_json` (the
  hand-rolled JSON parser, spec 001 §11). Must reject malformed JSON with
  `Err` rather than panicking, including on truncated `\uXXXX` escapes,
  lone surrogates, oversized integers, and excessive nesting. Any value
  it accepts is round-tripped through `to_json` -> `from_json` and
  asserted equal.

## What a crash means

A crash is a real bug: either a panic on untrusted input (violating the
kernel's core safety guarantee) or a canonicality/round-trip invariant
failure. `cargo fuzz run` writes the failing input to
`fuzz/artifacts/<target>/crash-<hash>`.

1. Reduce it if `cargo-fuzz` didn't already minimize it well:
   `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-<hash>`.
2. File the minimized input as a regression vector in
   `tools/conformance` (build plan §15: "every bug found gets a
   regression vector in `tools/conformance`"), alongside a note of what
   invariant it broke and which spec section that invariant comes from.
3. Fix the bug, add a matching unit/property test in the kernel crate
   itself, and keep the crashing input in the fuzz corpus
   (`fuzz/corpus/<target>/`) so `cargo fuzz run` regresses on it forever.
4. Do not silence the finding by relaxing the harness assertion — if the
   invariant the harness checks turns out to be wrong, fix the harness
   *and* explain why in the commit, since the harness encodes a spec
   guarantee.
