// Replaced by real tests and shared conformance vectors in Phase 3.
import { test } from "node:test";
import assert from "node:assert/strict";
import { PHASE } from "./index.ts";

test("scaffold loads", () => {
  assert.equal(PHASE, 0);
});
