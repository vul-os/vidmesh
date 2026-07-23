# Evermesh monorepo task runner. Install `just`: https://github.com/casey/just

# List available recipes
default:
    @just --list

# Install JS dependencies
setup:
    pnpm install

# Run all Rust and JS tests
test: test-rust test-js

# crates/evermesh-node embeds its frontend at compile time
# (`tauri::generate_context!()`, `frontendDist: "./ui"`, gitignored —
# see .gitignore); build it before any cargo command touches that crate.
node-web-build:
    pnpm --filter @evermesh/node-web build

test-rust: node-web-build
    cargo test --workspace

test-js:
    pnpm -r --if-present test

# Lint everything (rustfmt, clippy, JS lint)
lint: node-web-build
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    pnpm -r --if-present lint

# Format all Rust code
fmt:
    cargo fmt --all

# Render the protocol spec to PDF (requires pandoc + tectonic)
spec-pdf:
    mkdir -p dist
    pandoc -d spec/pandoc-pdf.yaml spec/draft-evermesh-protocol-00.md -o dist/evermesh-protocol-draft-00.pdf
    @echo "wrote dist/evermesh-protocol-draft-00.pdf"

# Build the WASM kernel bindings into packages/kernel-ts/wasm
wasm:
    pnpm --filter @evermesh/kernel build:wasm

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
    cargo run --bin evermesh-conformance -- run --target kernel

# Copy spec/ + docs into apps/site/docs (the site is deployable on its own)
site-docs:
    node tools/site/sync-docs.mjs

# Verify the site in a real browser: console errors, links, every docs route
site-check:
    node tools/site/sync-docs.mjs --check
    node tools/site/check.mjs

# Same, and refresh apps/site/screenshots/
site-shots:
    node tools/site/check.mjs --shots

# Screenshot the gateway reference UI against a stubbed API
# (apps/site/screenshots/ui-{dark,light}.png) — build first.
ui-shots:
    pnpm --filter @evermesh/gateway-web build
    node tools/brand/ui-shots.mjs

# Screenshot the desktop (Tauri) node client against a stubbed IPC
# boundary (apps/site/screenshots/ui-node-{dark,light}.png) — build first.
node-shots:
    pnpm --filter @evermesh/node-web build
    node tools/brand/node-shots.mjs

# Refresh every screenshot in apps/site/screenshots/
shots: site-shots ui-shots node-shots

# Re-render the raster brand exports (OG card, apple-touch-icon)
brand:
    node tools/brand/render.mjs

# Serve apps/site locally at http://127.0.0.1:8080
site-serve:
    cd apps/site && python3 -m http.server 8080
