# 011: Threat Model

**Status:** Draft 0.2
**Depends on:** all other files
**Depended on by:** none (terminal analysis document)

This file enumerates the assets the protocol defends, the adversaries it
anticipates, the mitigations each layer provides, and — deliberately —
the residual risks it accepts. The kernel's attack surface is
signature verification, hashing, and CBOR parsing; everything above it
is designed so that compromise is bounded, visible, and recoverable.

## 1. Assets

| Asset | Property defended |
|-------|-------------------|
| Records | Integrity, attribution, immutability |
| Blobs | Integrity, availability-by-choice |
| Identities | Continuity under key loss/theft; portability |
| Claims chains | Tamper-evidence; evidence preservation |
| The network itself | Survivability (the §000 survival test), censorship resistance, forkability |
| Private content | Confidentiality of content; existence of unpublished metadata |

## 2. Adversaries and mitigations

### 2.1 Forger
Fabricates records or mutates existing ones.
**Mitigations:** ids and signatures over canonical bytes
([001](001-kernel.md) §§2–4); verifiers reject non-canonical
encodings; algorithm agility with `sig_alg` under the id (downgrade
resistance). **Bound:** forgery reduces to breaking Ed25519/BLAKE3 or
stealing keys.

### 2.2 Key thief
Steals a signing key; publishes as the victim; rotates to lock them
out.
**Mitigations:** recovery-precedence rotation with contest window
([002](002-identity.md) §4) — the recovery holder evicts the thief;
the thief cannot evict the recovery holder. **Bound:** theft of *all*
recovery keys is unrecoverable identity loss; records signed before
eviction remain validly signed (consumers see the rotation and MAY
discount the interval).

### 2.3 Spammer
Floods relays and discovery.
**Mitigations:** layered — optional PoW, per-key rate limits, relay
selectivity ([006](006-relay.md) §6); read-side reputation at
gateways. **Accepted:** spam may exist in the substrate; it competes
for surfacing it never wins.

### 2.4 Malicious relay
Drops, delays, reorders, or refuses records; serves stale views.
**Mitigations:** relays are untrusted plumbing — clients use several;
any relay is replaceable; `seq` cursors are per-relay so lying about
order affects only its own cursor; records self-verify so a relay can
never alter content. **Accepted:** a relay can always refuse service;
censorship requires *all* paths (relays, gossip, bundles) to refuse.

### 2.5 Malicious or coerced gateway
Refuses content, manipulates rankings, miscounts, absconds with
custodied keys.
**Mitigations:** selection is the design, not an attack — exit is the
remedy: identical UI elsewhere ([009](009-gateway.md) §7), portable
identity ([002](002-identity.md) §7), replicable index (Principle 3);
counts are labeled per-gateway claims; custody exits by rotation.
**Bound:** a custodial gateway holding the *only* recovery key can
hijack that identity — hence multiple recovery keys are RECOMMENDED.

### 2.6 Censor (network-level or legal)
Seeks removal of content everywhere.
**Mitigations:** no protocol deletion exists; takedown feeds are
opt-in per gateway; bundles cross any link ([007](007-bundles.md));
partitions run complete replicas without DNS/CA. **Accepted:** any
single jurisdiction can clear its own gateways; it cannot reach the
substrate.

### 2.7 Fraudulent claimant
Asserts authorship/rights over others' work; abusive takedowns.
**Mitigations:** claims are assertions with provenance, never verified
truth ([005](005-claims.md) §2); anchoring gives honest claimants
priority evidence; disputes and counter-notices are first-class;
over-blocking feeds lose subscribers. **Accepted:** the protocol
preserves evidence; it does not adjudicate.

### 2.8 Privacy attacker
Reads private content; maps who watches/holds what.
**Mitigations:** chunked AEAD encryption ([008](008-privacy.md) §2);
private manifests never on public relays; keygrants on private paths;
nodes hold ciphertext by explicit subscription only.
**Accepted residuals:** size and timing of encrypted blobs; swarm
participation reveals interest in a hash to swarm peers; URL-fragment
keys are as safe as the channel carrying the URL.

### 2.9 Poisoner (bad derived content)
Delegated transcoder signs unfaithful renditions; similarity spam
merges wrong videos.
**Mitigations:** derivations are signed and delegation is revocable
([004](004-manifest.md) §3); similarity is evidence gateways weigh,
never auto-merge ([004](004-manifest.md) §5).

### 2.10 Implementation attacker
Malformed CBOR/bundles to crash or exploit parsers.
**Mitigations:** no-panic rule on untrusted input
([001](001-kernel.md) §1); fuzzed parsers as a reference-implementation
requirement; bundles salvage rather than abort
([007](007-bundles.md) §2).

## 3. Systemic risks

* **Canonicalization divergence** between implementations silently
  forks ids — the conformance suite's byte-exact vectors across Rust,
  Node, and browser exist precisely to catch this
  (Principle 9, [001](001-kernel.md) §2).
* **Registry capture:** governance is constitutionally narrow; the
  ultimate check is forkability — the community leaves with everything
  except the brand.
* **Monoculture:** one dominant gateway re-centralizes discovery.
  Accepted as a market condition; the design keeps it *contestable*
  (replicable corpus, portable identity, uniform UI, portable
  recommendation feeds).

## 4. Out of scope

* Fraud-proof global view counting (the ad-fraud problem).
* Anonymity of publishers (Evermesh is pseudonymous; network-layer
  anonymity composes externally, e.g. via Tor).
* DRM / copy protection of decrypted content.
* Byzantine consensus of any form — deliberately absent by
  Principle 8.

## Test vectors

Threats map to vectors rather than defining new ones: `envelope/*`
(forger), `identity/fork-*` (thief), `relay/pow-*` (spammer),
`bundle/salvage-*` (implementation attacker), `privacy/enc-*`
(privacy). Systemic risk 3.1 is the cross-runtime byte-exactness gate
of the whole suite.
