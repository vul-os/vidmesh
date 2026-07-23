/**
 * Client-side record verification: the substance behind the
 * VerifiedBadge (spec 009 §7 — never remove it, never fake it). Kept as
 * a pure function taking the kernel calls as parameters so it is
 * testable with a mock kernel, without touching the real WASM module.
 */

export interface VerificationOk {
  status: "verified";
  /** First 12 hex chars of the derived record id, for the badge detail popover. */
  shortId: string;
}

export interface VerificationFailed {
  status: "failed";
  reason: string;
}

export type VerificationResult = VerificationOk | VerificationFailed;

/** The subset of @evermesh/kernel's API this module depends on. */
export interface KernelLike {
  verifyRecord(record: Uint8Array): Promise<void>;
  deriveId(record: Uint8Array): Promise<string>;
}

/**
 * Verifies that the record at `recordId` is a validly signed record
 * whose id derives from its own bytes to `recordId` — i.e. the server
 * cannot swap in different bytes for a given id without the browser
 * catching it. This is the whole trust model: no server is trusted for
 * the "Verified" result, only Ed25519 math run locally.
 */
export async function verifyRecordById(
  recordId: string,
  fetchCbor: (id: string) => Promise<ArrayBuffer>,
  kernel: KernelLike,
): Promise<VerificationResult> {
  let bytes: Uint8Array;
  try {
    bytes = new Uint8Array(await fetchCbor(recordId));
  } catch (err) {
    return { status: "failed", reason: `could not fetch record bytes: ${messageOf(err)}` };
  }

  try {
    await kernel.verifyRecord(bytes);
  } catch (err) {
    return { status: "failed", reason: `signature/envelope check failed: ${messageOf(err)}` };
  }

  let derived: string;
  try {
    derived = await kernel.deriveId(bytes);
  } catch (err) {
    return { status: "failed", reason: `could not derive record id: ${messageOf(err)}` };
  }

  if (derived.toLowerCase() !== recordId.toLowerCase()) {
    return { status: "failed", reason: "derived record id does not match the requested id" };
  }

  return { status: "verified", shortId: derived.slice(0, 12) };
}

function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
