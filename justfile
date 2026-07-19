# Vidmesh monorepo task runner. Install `just`: https://github.com/casey/just

# List available recipes
default:
    @just --list

# Install JS dependencies
setup:
    pnpm install

# Run all Rust and JS tests
test: test-rust test-js

test-rust:
    cargo test --workspace

test-js:
    pnpm -r --if-present test

# Lint everything (rustfmt, clippy, JS lint)
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    pnpm -r --if-present lint

# Format all Rust code
fmt:
    cargo fmt --all

# Render the protocol spec to PDF (requires pandoc + tectonic)
spec-pdf:
    mkdir -p dist
    pandoc -d spec/pandoc-pdf.yaml spec/draft-vidmesh-protocol-00.md -o dist/vidmesh-protocol-draft-00.pdf
    @echo "wrote dist/vidmesh-protocol-draft-00.pdf"

# Build the WASM kernel bindings into packages/kernel-ts/wasm
wasm:
    pnpm --filter @vidmesh/kernel build:wasm

# Local smoke run (relay blob sidecar + gateway) — see README "Smoke run"
dev:
    @echo "See the 'Smoke run' section in README.md: boots a relay with the"
    @echo "blob sidecar and the gateway server against it (no ffmpeg needed)."

# (Re)generate the deterministic conformance vectors
conformance-generate:
    cargo run --bin generate

# Run the conformance suite against the in-process kernel (reference target).
# For the node and relay targets, see README "Conformance suite".
conformance:
    cargo run --bin vidmesh-conformance -- run --target kernel
