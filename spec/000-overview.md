# 000: Overview

**Status:** Draft 0.2
**Depends on:** none (this file is the root of the specification)
**Companion:** [draft-evermesh-protocol-00.md](draft-evermesh-protocol-00.md) (single-document rendering)

Evermesh is a protocol for publishing, distributing, and discussing video
without a central operator. It divides the system into a minimal, permanent
**kernel** — signed records and content-addressed blobs, verifiable from
their bytes alone — and a competitive **edge** of gateways, nodes, and
clients, where every contested concern (moderation, economics, discovery,
presentation) is resolved by participant choice rather than by protocol
rule. This file states the design principles, the participant roles, and
the map of the specification.

## 1. Requirements language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT",
"SHOULD", "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and
"OPTIONAL" in all files of this specification are to be interpreted as
described in BCP 14 (RFC 2119, RFC 8174) when, and only when, they appear
in all capitals.

## 2. Survival test

The design goal is stated as a survival test:

> The network must remain fully functional after the death or betrayal of
> every organization involved in it, including the one that wrote this
> document.

A secondary, equally binding goal is partition tolerance at civilizational
scale: the protocol MUST operate across links measured in minutes
(interplanetary), days (sneakernet), or never (permanently isolated local
networks), degrading only in freshness, never in integrity.

## 3. Design principles (normative)

A proposed change that violates any of these MUST be rejected regardless
of its benefits.

1. **Minimal kernel.** The kernel defines only: the record envelope, the
   signature and hashing scheme (with algorithm agility), the identity
   rotation log, and blob addressing. Everything else is an extension
   expressed as record kinds.
2. **Self-certifying data.** Every record and blob MUST be verifiable
   using only its own bytes and mathematics. No record's validity may
   depend on reaching a server, a blockchain, a DNS name, or a
   certificate authority.
3. **Forkability.** The full index MUST be replicable by anyone;
   identities MUST be portable across all infrastructure; blobs MUST be
   re-hostable by hash.
4. **Transport and storage agnosticism.** The kernel never names a
   transport. Blobs are hashes; records carry advisory, additive *hints*.
5. **No mandatory dependencies.** No company, blockchain, token, or
   external network may be required for correct operation.
6. **Economic neutrality.** The protocol carries payment primitives and
   takes no position on business models. No protocol token exists or will
   exist.
7. **Edge-resolved moderation.** The substrate never deletes. Gateways
   select what they index and serve. Compliance is subscribable, signed,
   auditable feeds — plural and opt-in.
8. **Partition tolerance.** No record kind may require global consensus,
   global ordering, or synchronous availability. Arrival order is never
   assumed to be creation order.
9. **Two-implementations rule.** No extension enters the specification
   until two independent implementations interoperate against the public
   conformance suite.
10. **Legibility.** Formats are plain, documented, and boring. A future
    reader must be able to reconstruct a working implementation from the
    specification alone.

## 4. Roles

Participation is a ladder; every rung is opt-in and independently useful.

| Role | Runs | Responsibilities |
|------|------|------------------|
| Viewer | Browser or app only | None; contributes swarm bandwidth while watching |
| Node | Background app | Pins chosen content; seeds watched content; honors its own budgets |
| Gateway | Public web service on its own domain | Indexes and serves selected content; moderation policy; optional transcoding, search, economics; jurisdiction compliance |
| Foundation | Nothing operational | Stewards the spec, registries, conformance suite, trademark |

The creator is not an infrastructure role: a creator is any keyholder who
publishes manifests. Creators MAY operate their own node and gateway.

## 5. Specification map

| File | Concern |
|------|---------|
| [001-kernel.md](001-kernel.md) | Record envelope, canonical CBOR, ids, signatures, registries, blobs, chunk trees, hints |
| [002-identity.md](002-identity.md) | Rotation log, recovery precedence, delegation, profiles |
| [003-kinds-registry.md](003-kinds-registry.md) | Numeric kind registry; body schema, refs semantics, validation, example per kind |
| [004-manifest.md](004-manifest.md) | Video manifests, renditions, deduplication, live streams |
| [005-claims.md](005-claims.md) | Claims, disputes, notices, provenance |
| [006-relay.md](006-relay.md) | Relay sync protocol, blob sidecar, anti-spam, gossip |
| [007-bundles.md](007-bundles.md) | Bundle container format, partition posture |
| [008-privacy.md](008-privacy.md) | Encryption modes, key grants, metadata privacy |
| [009-gateway.md](009-gateway.md) | Gateway selection, compliance, legal toolkit, discovery |
| [010-economics.md](010-economics.md) | Payment pointers, receipts, disclosures |
| [011-threat-model.md](011-threat-model.md) | Adversaries, attack surfaces, mitigations, residual risks |

A reader implementing from scratch reads 001, 002, and 003 closely; the
rest specify launch kinds and expected edge behavior.

## 6. Conformance

An implementation is conforming if it passes the public conformance suite
(`tools/conformance/`) for the components it implements. The suite's
vector groups are referenced from the **Test vectors** section that closes
every file of this specification.

## Test vectors

This file defines no wire formats and is covered indirectly by all vector
groups.
