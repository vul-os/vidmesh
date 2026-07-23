# CSAM hash-matching: the required integration point

**Template — not legal advice**, but unlike the rest of `legal/`, the
requirement described here is not adaptable. This document describes a
**mandatory, non-configurable** part of the reference gateway. Read
[spec/009-gateway.md](../../../spec/009-gateway.md) §4 before changing
anything in this area.

## Why this is the one non-configurable element

Every other compliance decision in this toolkit is local gateway policy:
what to allow, what to geo-block, which feeds to subscribe, how to
respond to a takedown notice. Evermesh's design principle is that
moderation is selection, and selection is instant, local, and reversible
([spec/009-gateway.md](../../../spec/009-gateway.md) §1).

Child sexual abuse material is different in kind, not degree. It is not
a policy call a gateway operator gets to make differently from its
competitors, and the kernel itself has no way to enforce this — the
substrate has no content review at all, by design (Principle 2:
self-certifying, no server-side gatekeeping). So the reference
implementation enforces it at the one layer that *can*: the gateway
software itself, as a build plan and trademark-program requirement.

**Running the reference gateway in production without a real
hash-matching integration is non-compliant by definition**
([spec/009-gateway.md](../../../spec/009-gateway.md) §4), and it is not
covered by the trademark program — see the stub section below.

## The pluggable interface

The reference gateway defines the matcher as a TypeScript interface, not
a hardcoded vendor integration, because jurisdictional hash databases
differ (NCMEC's hash lists are US-centric; other countries run their
own bodies and lists) and because operators may already have commercial
relationships with a provider. The interface is intentionally small:

```ts
export interface CsamMatcher {
  /** Check a blob at upload and at index time. MUST be called before any blob is served. */
  checkBlob(blob: ReadableStream<Uint8Array>, meta: { size: number; blobId: string }): Promise<CsamVerdict>;
  /** Where verdicts are reported. */
  reportingChannel(): ReportingInfo;
}
export type CsamVerdict = { match: false } | { match: true; listId: string; action: "block-and-report" };
```

Where `ReportingInfo` describes the destination(s) a confirmed match
must be reported to — e.g. an authority name, an API/contact endpoint,
and any credentials/config the real matcher implementation needs to
actually file a report (this shape is implementation-specific; the
contract is only that `reportingChannel()` returns enough for the
gateway's reporting workflow to act on a match without guessing).

### Contract, in plain terms

- `checkBlob` MUST be called on every blob before it is served, both at
  **upload time** (before the blob is accepted into the content-addressed
  store) and at **index time** (before a record referencing the blob is
  indexed for serving) — see integration points below. A blob that
  hasn't cleared this check MUST NOT reach any code path that serves it
  to a viewer.
- A `{ match: false }` verdict means "no known match" — it is evidence
  of absence in the lists checked, never a positive certification that
  content is safe. Don't render it to operators or users as "verified
  clean."
- A `{ match: true, ... }` verdict is terminal: `action` is currently
  always `"block-and-report"` — there is no "match but allow" outcome in
  this interface, by design.

## Integration points

The reference gateway calls `checkBlob` at two points, and both are
required — one is not a substitute for the other:

1. **Upload pipeline, pre-publish.** Before a freshly uploaded blob is
   accepted into the content-addressed store and before the gateway
   signs/publishes a manifest referencing it
   ([EVERMESH_BUILD_PLAN.md](../../../EVERMESH_BUILD_PLAN.md) §9, upload
   pipeline). A match here means the upload never gets published,
   never gets pinned, and never reaches a relay from this gateway.
2. **Relay-ingest, pre-index.** Before the gateway indexes a record it
   received from a relay (i.e., content it did not originate but is
   choosing to select and serve). A match here means the gateway MUST
   NOT index or serve the blob, regardless of who published it or which
   relay it came from. This is the path that catches content the
   substrate already carries from elsewhere — the gateway's own upload
   check only covers what's uploaded directly to it.

Both checks run against the *blob*, not the manifest record — a manifest
can be re-signed, renamed, or re-referenced, but the underlying bytes
and their hash don't change, so the check needs to happen on the content
itself at every point it's about to become servable.

## Mandatory reporting workflow

A confirmed match is not just a block — it is a legal reporting
obligation in most jurisdictions, independent of what the protocol or
this software does.

- **United States:** operators MUST report to the **National Center for
  Missing & Exploited Children (NCMEC)** via its CyberTipline. This is a
  federal reporting obligation for US-based providers under 18 U.S.C.
  §2258A when they have actual knowledge of an apparent violation — the
  exact scope, timing, and retention duties are statutory; verify
  current requirements directly at **report.cybertip.org** and with
  counsel, because failure to report is itself a legal violation, not
  just a policy gap.
- **Other jurisdictions:** report to the equivalent national or
  regional body — e.g., the **Internet Watch Foundation (IWF)** in the
  UK, or your country's designated hotline/authority. There is no single
  global equivalent to NCMEC; `JURISDICTIONS.md` in `legal/` is the
  place to record which body applies per profile as the community adds
  jurisdictions.
- Your `reportingChannel()` implementation is where you wire the actual
  API/contact details for whichever body applies to your operation.
  Do not ship a matcher whose `reportingChannel()` is a stub or a dead
  address — an unreachable reporting channel defeats the entire point
  of this interface.

## Operator onboarding: getting a real matcher

The reference repo does not ship a production matcher — hash databases
are licensed, access-controlled, and jurisdiction-specific, and are not
something an open-source repo can bundle. To go live, an operator
typically needs to:

1. **Apply for NCMEC hash-sharing access** (US) or the equivalent
   program in your jurisdiction, if you intend to consume/match against
   an authority-provided hash list directly.
2. **Or integrate a commercial/industry provider**, such as
   **PhotoDNA** (Microsoft) or **Safer** (Thorn), or another
   industry-recognized CSAM detection service. These typically require
   an application/vetting process — they are not self-serve signups —
   because access to CSAM hash data and detection tooling is
   deliberately gated.
3. **Implement `CsamMatcher`** against the chosen provider's API,
   handling both perceptual-hash and cryptographic-hash matching
   approaches as the provider supports, and wire real credentials into
   `reportingChannel()`.
4. **Test the integration path**, not just the matcher's positive
   result — verify a match actually blocks the blob at both integration
   points (§"Integration points") and actually reaches the reporting
   channel end-to-end, using your provider's test/sandbox facilities
   (never test against real CSAM).
5. **Document your matcher and reporting setup** internally so it
   survives operator/staff turnover — this is exactly the kind of
   control that must not silently regress when someone redeploys the
   gateway.

## The stub: `StubMatcher`

The reference repo ships a `StubMatcher` implementation of
`CsamMatcher` that **always returns `{ match: false }`**. It exists so
the gateway server can boot and be developed against without requiring
a live vendor integration for every contributor.

`StubMatcher` is:

- **Clearly named** — the name itself says what it is; it is not
  disguised as a real provider or given a name that could be mistaken
  for production-ready.
- **Logged loudly at startup** — the gateway process MUST emit a
  prominent warning on boot whenever `StubMatcher` is the active
  implementation, so it is never silently running in a deployment
  someone assumed was production-configured.
- **NOT-FOR-PRODUCTION** — running real user traffic, real uploads, or
  any publicly reachable deployment on `StubMatcher` is **non-compliant**
  with the reference gateway's own requirements
  ([spec/009-gateway.md](../../../spec/009-gateway.md) §4) and is
  **not covered by the Evermesh trademark program** — a deployment on
  the stub matcher is not entitled to represent itself as a compliant
  Evermesh reference gateway, regardless of how the rest of the stack is
  configured.

If you are reading this because you're about to deploy a gateway for
real users: stop, and complete the onboarding steps above first. There
is no configuration flag that makes `StubMatcher` acceptable for
production — the only fix is a real `CsamMatcher` implementation.
