# Security Policy

Vidmesh is a decentralized video protocol built on self-certifying signed
records and content-addressed blobs. The kernel's whole job is to make
forgery, tampering and identity hijack detectable; the reference relay and
gateway hold real secrets (signing keys, custodied identities) and face real
adversaries. Security reports against any of these are taken seriously and
handled with priority.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

- Preferred: [GitHub private vulnerability reporting](https://github.com/vul-os/vidmesh/security/advisories/new) on `vul-os/vidmesh`.
- Alternatively, email **vulosorg@gmail.com** with `[vidmesh security]` in the subject.

Include what you can: affected component (kernel, relay, gateway server,
gateway web, WASM bindings, a specific record kind), reproduction steps, and
impact as you understand it. You'll get an acknowledgement within **72
hours** and a status update at least every **14 days** until resolution.
Please give us a reasonable window to ship a fix before public disclosure —
we'll credit you in the release notes unless you'd rather stay anonymous.

## Scope

Especially interested in:

- **Kernel forgery/tamper bypass** — anything that lets a record verify
  under a signature it wasn't actually signed with, a non-canonical encoding
  pass verification, or a record's id not match its content
  ([spec/001-kernel.md](spec/001-kernel.md), [011-threat-model.md](spec/011-threat-model.md) §2.1).
- **Identity and key rotation** — anything that lets an attacker evict a
  legitimate recovery-key holder, forge a rotation, or otherwise hijack an
  identity outside the documented recovery-precedence rules
  ([spec/002-identity.md](spec/002-identity.md), threat model §2.2).
- **Blob integrity** — chunk proofs or content addressing that can be
  satisfied by altered bytes.
- **Relay abuse** — proof-of-work or rate-limit bypass, envelope validation
  gaps that let malformed or oversized data through, gossip amplification,
  or anything letting a relay silently alter content it forwards
  ([spec/006-relay.md](spec/006-relay.md), threat model §2.3–2.4).
- **Custodial key handling in the reference gateway** — the reference
  gateway holds signing/recovery keys server-side today (non-custodial
  flows are a later phase); any path that exposes, logs, or lets an
  unauthorized party use custodied key material is critical
  (threat model §2.5, [spec/009-gateway.md](spec/009-gateway.md) §5).
- **CSAM hash-matching integration** — the one moderation decision the spec
  makes non-configurable at the gateway layer. Bypasses of, or gaps in, this
  integration point are treated as critical regardless of how they're
  found ([apps/gateway/server/CSAM.md](apps/gateway/server/CSAM.md)).
- **Conformance divergence with security impact** — a case where the
  kernel, `@vidmesh/kernel` (WASM/Node), and the relay disagree on whether a
  record or blob is valid. In this protocol a parser/verifier disagreement
  is a security bug, not a compatibility quirk
  ([tools/conformance](tools/conformance)).

Out of scope: content-moderation policy disagreements (gateway selection is
the design, not a bug — see threat model §2.5–2.6), spam that reaches the
substrate but doesn't win surfacing (§2.3, accepted by design), and issues
in third-party services an operator configures for their own gateway.

## Supported versions

Pre-1.0: only the latest release (and `main`) receives fixes.

## Threat model

The full analysis — assets defended, adversaries anticipated, mitigations
per layer, and residual risks accepted on purpose — is
[spec/011-threat-model.md](spec/011-threat-model.md). Read it first; it
tells you where the interesting attack surface is and which residual risks
are already known and accepted rather than new findings.
