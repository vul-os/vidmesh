# vidmesh-node

The Vidmesh node app: a background desktop app (Tauri 2) that pins and
seeds the content its owner chooses — their own videos first, subscriptions
second — while honoring its own disk and bandwidth budgets. Per spec
[000-overview.md §4](../../spec/000-overview.md), a node has **no
public-facing duties**: it does not index, serve, or moderate for anyone
but its owner. That's the gateway's job.

## What the node will do (once built out past scaffold)

- **Pin by explicit choice.** The owner picks specific manifests/blobs to
  keep seeded indefinitely, regardless of whether they're actively watched.
- **Seed by subscription.** Content from followed channels is pinned
  automatically as it's watched or as new manifests arrive, subject to
  budget pressure (oldest subscription-pins evicted first — explicit pins
  are never auto-evicted).
- **Honor its own budgets.** A configurable disk-space ceiling and upload
  bandwidth ceiling; the node never seeds past what its owner allowed.

None of this is implemented yet — see Status below.

## Status: Phase 8 scaffold

This crate is a **Tauri 2 scaffold only**, per build plan §2 ("Node app
(Tauri 2) is Phase 8 — scaffold only in v1") and the Phase 8 gate ("Tauri
shell builds"). Concretely, right now:

- `src/main.rs` boots a plain Tauri window over the static `ui/` shell and
  registers two commands that return canned data:
  - `node_status() -> { version, pinned_count: 0, seeding: false }`
  - `budgets() -> { disk_gb: 0, bandwidth_mbps: 0 }`
- `ui/index.html` + `ui/style.css` are a framework-free static shell with
  three placeholder panels — Pinned content, Subscriptions, Budgets — each
  clearly marked `SCAFFOLD` in-page. No devUrl/dev server is configured;
  `frontendDist` points straight at `./ui`.
- `src/pinning.rs` is the **real v1 design surface**, written now so the
  storage design is decided ahead of implementation: a `PinStore` backed by
  a single SQLite file (`<app-data-dir>/pins.sqlite3`), typed around
  `vidmesh-kernel`'s `BlobId`/`RecordId`, with every method already
  signatured. Every stub is marked `// SCAFFOLD(phase-8):`, returns an
  empty/default value, and never panics — no placeholder that could
  silently misbehave if called.

What's explicitly **not** here yet: real sqlite I/O, swarm participation
(seeding transport), budget enforcement/eviction, and any settings UI to
configure pins/subscriptions/budgets from the shell.

## Building

Plain compilation must work with only the Rust toolchain — no Tauri CLI,
no Node.js, no frontend build step:

```sh
cargo check -p vidmesh-node
cargo test -p vidmesh-node
```

To actually run the scaffold window (requires the `tauri` CLI, see
[tauri.app](https://tauri.app) for install instructions):

```sh
cargo install tauri-cli --version "^2"
cargo tauri dev -p vidmesh-node
```

There is no `beforeDevCommand`/`beforeBuildCommand` configured in
`tauri.conf.json` — the frontend is the static `ui/` directory as-is, so
`cargo tauri dev` serves it directly with no build step of its own.

## Layout

| Path | Purpose |
|---|---|
| `Cargo.toml` | crate manifest: `tauri`, `tauri-build`, `serde`/`serde_json`, `vidmesh-kernel` |
| `build.rs` | `tauri_build::build()` — bundle glue from `tauri.conf.json` |
| `tauri.conf.json` | Tauri 2 config: identifier `org.vidmesh.node`, static `ui/` frontend, window config |
| `icons/icon.png` | app icon (512×512, derived from `assets/favicon.svg`) |
| `ui/` | static shell UI: `index.html`, `style.css` |
| `src/main.rs` | Tauri builder + the two scaffold commands |
| `src/pinning.rs` | `PinStore` — the documented v1 pinning/budget design |
