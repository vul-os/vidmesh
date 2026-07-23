# 009: Gateways

**Status:** Draft 0.2
**Depends on:** [003-kinds-registry.md](003-kinds-registry.md), [005-claims.md](005-claims.md), [006-relay.md](006-relay.md), [008-privacy.md](008-privacy.md)
**Depended on by:** [010-economics.md](010-economics.md)

A gateway is a public web service, on its own domain, that indexes and
serves a **selection** of the substrate. Selection is the moderation
model: nothing a gateway does removes anything from the substrate, and
every gateway answers only for what it chooses to serve. This file
specifies selection, compliance feeds, the reference gateway's
obligations, custody, and discovery.

## 1. Selection

* **Local policy is absolute and instant.** Allow/deny by hash, key,
  kind, or category is gateway configuration, not protocol action.
  Non-serving is a first-class, cheap operation.
* A gateway MUST publish a human-readable moderation policy page
  ("what this gateway serves") and SHOULD log every selection action
  locally for its own audit.
* Gateways compete on product: transcoding, search, recommendation,
  custodial key management, economics. The protocol layer is identical
  across all of them — one substrate, many doors. The reference UI is
  likewise identical across gateways ([§7](#7-uniform-reference-ui)):
  a viewer moving between gateways changes URL and catalog, not
  interface.

## 2. Ingest

A gateway subscribes to its configured relays
([006](006-relay.md) §1) with filters matching its policy, maintains
its own index of selected records, and pins the blobs it serves in a
content-addressed store. The index is derived state: any gateway can be
rebuilt from relays and bundles (Principle 3).

## 3. Compliance feeds

Organizations publish `feed.takedown` batches
([003](003-kinds-registry.md) §6.7). Gateways subscribe per their
jurisdiction and policy; on a matching `add` entry the gateway
de-indexes the subject automatically and logs the feed, entry, and
notice reference. Feeds are plural, opt-in, and auditable — an
over-blocking feed loses subscribers to a competitor. Gateways SHOULD
surface to users that an item was removed and under which feed/notice
(transparency), except where law forbids it.

Notices themselves (`notice.takedown` / `notice.counter`,
[005](005-claims.md) §4) are machine-readable; the reference gateway
ships notice and counter-notice intake that emits them as records.

## 4. Reference-gateway obligations

These are reference-implementation and trademark-program requirements,
not kernel rules (the kernel cannot enforce them):

* **CSAM handling is non-configurable:** industry hash-matching at
  upload and at index time, plus a mandatory reporting workflow. The
  matching interface is pluggable (jurisdictional databases differ);
  running the reference gateway in production with the stub matcher is
  non-compliant by definition.
* **Legal toolkit ships with the software:** templated ToS/AUP, DMCA
  agent guidance, notice/counter-notice UI, per-item geo-blocking,
  age-gating hooks, and jurisdiction compliance profiles. Lowering the
  legal cost of running a gateway is decentralization infrastructure.
* **Uniform UI** (§7).

Rogue gateways face their own jurisdictions alone. Liability does not
propagate: the foundation operates nothing, other gateways serve their
own selections, nodes pin only by choice. Remedies against bad actors
are social and structural — disassociation feeds, trademark denial,
creator non-endorsement — never protocol deletion, because any
mechanism strong enough to force one gateway's compliance is strong
enough to censor the network.

## 5. Custody

Gateways MAY custody keys for mainstream users (signing on their
behalf, holding a recovery key). Custody obligations
([002](002-identity.md) §7): the user can export their identity
(genesis + chain + keys they hold) at any time, and leaving is a
rotation requiring nothing from the gateway. A custodial gateway that
cannot demonstrate the exit path is not Evermesh-compliant.

## 6. Discovery and recommendation

**Portable recommendation feeds:** a feed is a signed object — an
algorithm reference or a service endpoint — that any gateway can embed;
recommenders need not be gateways. Because the metadata corpus is fully
replicable, any competitor can bootstrap search without permission. The
win condition is not preventing dominance but keeping dominance
permanently contestable.

Aggregates shown to users (views, likes) are the gateway's own claims
([006](006-relay.md) §7) and SHOULD be labeled as such ("views on this
gateway").

## 7. Uniform reference UI

The reference frontend is a single shared interface deployed by every
gateway: same pages, same player, same verification badge, same
interaction patterns. A gateway differs by its **domain, catalog
(selection), and branding accents** — never by relearning the product.
This is deliberate: users can switch gateways at zero interface cost,
which keeps exit real and selection honest. Gateways MAY extend the UI;
they SHOULD NOT remove the verification badge, the moderation-policy
page, or the identity-export flow, and trademark compliance requires
all three.

## Decisions

* Uniform UI across gateways is a reference/trademark requirement:
  differentiation lives in selection, catalog, and services — not in
  interface lock-in. (Requested by the project owner; recorded
  2026-07-17.)
* De-index on feed match is automatic (subscribing *is* the policy);
  transparency to users is SHOULD-level because some legal regimes
  forbid disclosure.

## Test vectors

Gateways are edge behavior; conformance covers their protocol surface
only:

* `gateway/feed-apply-*` — feed batch → expected index state
  (add/remove/gap tolerance).
* `gateway/ingest-*` — selection filters over a fixed record set →
  expected index.
* Compliance-toolkit behavior (notice intake, CSAM interface) is
  covered by the reference implementation's own test suite, not
  protocol vectors.
