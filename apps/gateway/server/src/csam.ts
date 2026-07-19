/**
 * The CsamMatcher interface, verbatim per apps/gateway/server/CSAM.md, plus
 * the mandatory StubMatcher. Read CSAM.md before touching this file: the
 * stub is the one sanctioned "not really implemented" stub in this codebase
 * and it must stay loudly, unmistakably a stub.
 */

export type CsamVerdict =
  | { match: false }
  | { match: true; listId: string; action: "block-and-report" };

export interface ReportingInfo {
  authority: string;
  contact: string;
  notes?: string;
}

export interface CsamMatcher {
  /** Check a blob at upload and at index time. MUST be called before any blob is served. */
  checkBlob(
    blob: ReadableStream<Uint8Array>,
    meta: { size: number; blobId: string },
  ): Promise<CsamVerdict>;
  /** Where verdicts are reported. */
  reportingChannel(): ReportingInfo;
}

/**
 * Always returns `{ match: false }`. NOT-FOR-PRODUCTION (CSAM.md): running
 * real user traffic on this implementation is non-compliant with the
 * reference gateway's own requirements (spec 009-gateway.md §4) and is not
 * covered by the Vidmesh trademark program, regardless of how the rest of
 * the stack is configured. It exists purely so the gateway can boot and be
 * developed against without a live vendor integration.
 *
 * The startup warning required by CSAM.md is emitted by main.ts when this
 * class is selected, not here, so it is impossible to miss in process logs
 * regardless of log level.
 */
export class StubMatcher implements CsamMatcher {
  async checkBlob(
    blob: ReadableStream<Uint8Array>,
    _meta: { size: number; blobId: string },
  ): Promise<CsamVerdict> {
    // Drain the stream so callers can treat every CsamMatcher
    // implementation identically (a real matcher must read the bytes).
    const reader = blob.getReader();
    try {
      for (;;) {
        const { done } = await reader.read();
        if (done) break;
      }
    } finally {
      reader.releaseLock();
    }
    return { match: false };
  }

  reportingChannel(): ReportingInfo {
    return {
      authority: "NONE — StubMatcher is not wired to a real reporting channel",
      contact: "unreachable://stub-matcher-not-for-production",
      notes:
        "This is a deliberate dead end. A production CsamMatcher MUST return a " +
        "real authority (e.g. NCMEC CyberTipline, IWF) and a reachable contact/API " +
        "endpoint. See CSAM.md 'Mandatory reporting workflow'.",
    };
  }
}
