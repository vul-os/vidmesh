# evermesh-wasm

wasm-bindgen bindings over `evermesh-kernel`, built with wasm-pack and consumed
by `packages/kernel-ts`. One crypto implementation everywhere: the same Rust
code verifies records natively, in Node, and in the browser.

**Status: Phase 0 scaffold — no implementation yet.** Phase 3 fills this in.

## Build

```sh
just wasm   # wasm-pack build into packages/kernel-ts/wasm/
```
