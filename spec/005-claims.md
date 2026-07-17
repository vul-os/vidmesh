# 005: Claims and Provenance

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md), [003-kinds-registry.md](003-kinds-registry.md), [004-manifest.md](004-manifest.md)
**Depended on by:** [009-gateway.md](009-gateway.md)

Each video accumulates a per-manifest, append-only, tamper-evident chain
of legal and provenance statements built from ordinary records:
authorship claims, license grants, transfers, disputes, and legal
notices. The protocol preserves the evidence trail; courts and gateways
interpret it. This file specifies how the chain composes and the honesty
requirement that governs its presentation.

## 1. The claim chain

The claim kinds ([003](003-kinds-registry.md) §6) all reference the
subject manifest, so the chain for a manifest is simply the set of
claim records referencing it, merged in any arrival order:

| Kind | Statement |
|------|-----------|
| `claim.author` (48) | "I authored this work" |
| `claim.license` (49) | Grant or change of license terms |
| `claim.transfer` (50) | Rights assignment, signed by the assignor |
| `claim.dispute` (51) | Contest of a prior claim or notice, with evidence |
| `notice.takedown` (64) | Structured legal notice as a signed record |
| `notice.counter` (65) | Counter-notice |

Because claims are ordinary records, they merge across partitions like
everything else (Principle 8). Competing claims created in isolation
coexist on merge; disputes make disagreement explicit instead of
resolving it silently.

## 2. Honesty requirement (normative)

Implementations MUST present claims as *assertions with provenance*,
never as verified truth. A signature proves authorship of the
statement, not the statement. User interfaces MUST NOT render an
uncorroborated `claim.author` as "verified author" or equivalent.

Strength comes from composition:

* **anchored timestamps** ([001](001-kernel.md) §10) — priority
  evidence: an anchored claim provably existed before the anchor;
* **capture provenance** — C2PA-compatible capture metadata carried as
  claim `evidence` blobs;
* **off-platform footprint** — the claimant's history, weighed by
  gateways and, ultimately, courts.

## 3. Interpreting the chain

Non-normative guidance for gateways:

* The **rights position** of a manifest is the latest coherent path
  through author claims and transfers: an author claim by the manifest
  author, modified by transfers signed by each successive assignor,
  each ideally anchored.
* A transfer from an identity whose own claim position is unsupported
  is weightless.
* Disputes attach to specific claims; an undisputed anchored claim that
  predates all competitors is the strongest available evidence.
* License interpretation follows the rights position: `claim.license`
  by the current rights holder modifies the manifest's `license`
  field; conflicts are surfaced to users, not silently resolved.

## 4. Notices

`notice.takedown` and `notice.counter` make legal process
machine-readable. A notice obligates no one at the protocol layer:
gateways act on notices according to their own jurisdiction and policy
([009](009-gateway.md) §3), and compliance organizations aggregate
notices into `feed.takedown` feeds. Notices are themselves claims and
are disputable (kind 51).

## Test vectors

* `claims/chain-*` — author + license + transfer chains merged in three
  arrival orders with identical resulting position.
* `claims/dispute-*` — dispute on a claim; dispute on a notice.
* `claims/anchored-*` — anchored claim with inclusion proof; competing
  unanchored claim.
* `kinds/notice.takedown/`, `kinds/notice.counter/` — per
  [003](003-kinds-registry.md).
