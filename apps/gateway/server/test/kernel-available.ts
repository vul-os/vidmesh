/**
 * Shared guard for tests that need @evermesh/kernel's WASM build. The
 * kernel package ships its WASM artifact via `pnpm build:wasm`
 * (packages/kernel-ts/package.json); it is not committed, so a fresh
 * checkout won't have it until that's run. Tests that fabricate records
 * skip loudly rather than fail when it's absent, mirroring
 * packages/kernel-ts/src/kernel.test.ts's own pattern.
 */
import { test, type TestContext } from "node:test";

let cached: Promise<boolean> | undefined;

export function kernelAvailable(): Promise<boolean> {
  cached ??= (async () => {
    try {
      const kernel = await import("@evermesh/kernel");
      await kernel.init();
      return true;
    } catch {
      return false;
    }
  })();
  return cached;
}

/** `test()` that self-skips with a loud message when the kernel isn't built. */
export async function kernelTest(
  name: string,
  fn: (t: TestContext) => void | Promise<void>,
): Promise<void> {
  if (await kernelAvailable()) {
    test(name, fn);
  } else {
    test(`${name} (SKIPPED: run 'pnpm --filter @evermesh/kernel build:wasm' first)`, { skip: true }, () => {});
  }
}
