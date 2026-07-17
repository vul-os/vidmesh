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

# Bring up relay + gateway + web with seeded demo content (Phase 5/6)
dev:
    @echo "Phase 5/6 not implemented yet: will run relay, gateway server, and web app with demo content"
    @exit 1

# Run the conformance suite against all implementations (Phase 7)
conformance:
    @echo "Phase 7 not implemented yet: runs tools/conformance vectors against kernel, kernel-ts, and a live relay"
    @exit 1
