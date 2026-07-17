# @vidmesh/kernel

Ergonomic, fully typed TypeScript API over the Vidmesh WASM kernel
(`crates/vidmesh-wasm`). One crypto implementation everywhere: the exact
Rust kernel that verifies natively also verifies here, in Node and in
browsers, so ids, signatures, and canonical bytes are always identical.

## Build

```sh
pnpm build:wasm   # wasm-pack build of crates/vidmesh-wasm into ./wasm
pnpm build        # tsc → dist (ESM + .d.ts) + CJS shim
```

The package is ESM-first; the CJS entry resolves to a Promise of the
module namespace (`const kernel = await require("@vidmesh/kernel")`).

## API

```ts
import { init, Keypair, identity, createRecord, verifyRecord,
         deriveId, validateKind, recordToJson, recordFromJson,
         hashBlob, hashBlobStream, verifyChunk } from "@vidmesh/kernel";

await init();                      // optional; all calls await it internally
const kp = await Keypair.generate();
const { identityId, record } = await identity.genesis(kp);
const comment = await createRecord(kp, identityId, {
  kind: 32,
  refs: [{ type: 0, hash: manifestId }],
  body: { text: "hello" },
});
await verifyRecord(comment);       // throws on failure
const summary = await hashBlobStream(file.stream());  // {id, size, nChunks, chunkRoot}
```

Records are `Uint8Array` canonical CBOR; hashes are lowercase hex;
bodies use the JSON interchange form of spec 001 §11 (bytes as
`"hex:<hex>"` strings, integers only).

## Testing

```sh
pnpm --filter @vidmesh/kernel test   # requires pnpm build:wasm first
```

Golden rule (build plan §7): the same conformance vectors must pass in
Rust, Node, and a headless browser. A vector passing in one runtime and
failing in another means the canonical encoding is broken — stop and
fix.
