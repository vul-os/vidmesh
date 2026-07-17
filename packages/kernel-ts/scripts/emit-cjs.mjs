// The package is ESM-first (the WASM binding is an ES module). The CJS
// entry is a shim whose export is a Promise of the ESM namespace:
//   const kernel = await require("@vidmesh/kernel");
import { writeFile } from "node:fs/promises";

await writeFile(
  new URL("../dist/index.cjs", import.meta.url),
  '"use strict";\nmodule.exports = import("./index.js");\n',
);
console.log("wrote dist/index.cjs");
