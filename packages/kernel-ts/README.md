# @vidmesh/kernel

Ergonomic, fully typed TypeScript API over the Vidmesh WASM kernel
(`crates/vidmesh-wasm`). Ships dual ESM/CJS with `.d.ts`.

**Status: Phase 0 scaffold — no implementation yet.** Phase 3 fills this in.

## Planned API

- `createRecord`, `verifyRecord`, `deriveId`
- `hashBlobStream` (WHATWG streams)
- `Identity` chain helpers (genesis, rotate, verifyChain)

Golden rule: the same conformance vectors must pass in Rust, Node, and a
headless browser. A vector passing in one runtime and failing in another means
the canonical encoding is broken.

## Test

```sh
pnpm --filter @vidmesh/kernel test
```
