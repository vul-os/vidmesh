# vidmesh

> Many gateways. One substrate. Video that outlives its platforms.

Vidmesh is a decentralized video protocol built on a substrate of
self-certifying data: signed records (CBOR, Ed25519, BLAKE3) and
content-addressed blobs. No servers of record, no token, no mandatory
dependencies. Independent **gateways** index and serve their own selection of
the substrate; **nodes** pin and seed chosen content; **viewers** verify
everything client-side while they watch.

**Status: pre-alpha, Phase 0 (scaffold).** Nothing is implemented yet — this
repository currently contains the monorepo skeleton and build tooling. The
spec (Phase 1) comes before any code.

## Monorepo map

| Path | What it is | Phase |
|---|---|---|
| `spec/` | The normative protocol spec (CC-BY-SA-4.0) | 1 |
| `crates/vidmesh-kernel` | Protocol kernel: records, identity, blobs, bundles | 2 |
| `crates/vidmesh-wasm` | wasm-bindgen bindings over the kernel | 3 |
| `packages/kernel-ts` | Typed TS API over the WASM kernel | 3 |
| `crates/vidmesh-relay` | Websocket sync relay with gossip + blob sidecar | 4 |
| `apps/gateway/server` | Gateway backend: ingest, index, policy, HLS | 5 |
| `apps/gateway/web` | Gateway frontend: React + Vite + Tailwind | 6 |
| `packages/ui` | Shared React components (player, record cards) | 6 |
| `tools/conformance` | Cross-implementation test vectors + runner | 7 |
| `crates/vidmesh-node` | Node app (Tauri 2 scaffold) | 8 |
| `apps/site` | vidmesh.org static landing page | 8 |

## Development

Prerequisites: Rust stable, Node ≥ 22, pnpm, [`just`](https://github.com/casey/just).

```sh
just setup   # install JS deps
just test    # rust + js tests
just lint    # rustfmt, clippy, tsc
```

`just dev` (full demo) and `just conformance` arrive in later phases.

## License

Code: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).
Spec: CC-BY-SA-4.0 (`LICENSE-SPEC`).
