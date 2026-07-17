# 002: Identity

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md)
**Depended on by:** all kind specifications

An identity is a stable identifier bound to a *current* signing key
through a **rotation log**: a chain of signed `rotation` records stored in
the substrate itself — DID-like without external DID infrastructure. This
file specifies genesis, rotation, the recovery-precedence rule that makes
key theft recoverable, chain verification, delegation, and profiles.
Identities are never derived from, or bound to, domains, gateways, or
relays.

## 1. Rotation record

Kind `rotation` (id 1). Body schema:

| Field | Type | Req | Meaning |
|-------|------|-----|---------|
| `key` | bytes | yes | New (or genesis) signing public key |
| `key_alg` | uint | yes | Signature algorithm of `key` (registry, [001](001-kernel.md) §7) |
| `recovery` | array of `[uint, bytes]` | yes | Recovery keys as `[alg, pubkey]` pairs; MAY be empty |
| `contest_window` | uint | yes | Contest window in seconds (§4); 0 disables recovery precedence |

Refs: genesis has empty `refs`; every later rotation has exactly one ref,
`[0, <previous rotation record id>]`.

## 2. Genesis

An identity is created by publishing a rotation record with empty `refs`
and `author.identity_id` = 32 zero bytes (the identifier cannot appear
inside the record that defines it). The genesis record MUST be signed by
the `key` its own body declares (`author.signing_key = body.key`,
`sig_alg = key_alg`).

**The identity's identifier is the record id of its genesis rotation
record.** Possession of the genesis record proves the binding between
identifier and initial keys; the identifier is therefore
self-certifying.

## 3. Rotation

A non-genesis rotation is **authorized** if it is signed by:

* the identity's current signing key (per the chain state at its
  parent), or
* any recovery key declared in the chain state at its parent.

A rotation record replaces the signing key and the recovery set
atomically: the new state is exactly its body. Rotating to a key of a
newer registered algorithm is the crypto-agility migration path — no
protocol fork is ever required for algorithm migration.

Records other than rotations are attributed to the identity if their
`signing_key` equals the chain's current signing key at verification
time. Consumers MAY additionally accept records signed by a superseded
key when the record's kind rules say so; by default, superseded keys are
not authorized.

## 4. Recovery precedence and fork resolution

Verifiers evaluate the set of rotation records they hold for an identity
as a tree rooted at genesis and select the **active branch**:

1. **Validity.** Discard any rotation not authorized per §3 under the
   state at its parent.
2. **Fork resolution.** Where a parent has multiple valid children,
   compare the children:
   a. A **recovery-authorized** child supersedes a
      **signing-key-authorized** child, *unless* the signing-key child's
      branch is **final** (§4.1). This is the theft-recovery rule: a
      thief holding a stolen signing key cannot outrun the recovery-key
      holder, while the recovery key cannot rewrite history that has
      outlived the contest window.
   b. Between children of the same authorization class, the child with
      the bytewise-lower record id wins. This is arbitrary but
      deterministic and partition-safe: two verifiers with the same
      record set always agree.
3. **Depth.** Along the selected edges, the deepest node is the current
   state.

### 4.1 Contest window and finality

A signing-key-authorized rotation becomes **final** when the verifier
first observed it more than `contest_window` seconds ago (the window
declared in the chain state at its parent). Before finality it is
provisional and can be displaced by a recovery-authorized sibling.

First-observation time is verifier-local. Two verifiers who received the
same records at different times MAY transiently disagree about finality;
this is inherent to partition tolerance (Principle 8) and converges once
both have held the records for the window. Anchoring (`anchor` records,
[001](001-kernel.md) §10) provides portable ordering evidence for human
and legal review of contested rotations; the deterministic rules above
are what verifiers compute.

`contest_window = 0` declares that the identity opts out of recovery
precedence (rule 2a never fires; recovery keys still authorize
rotations).

## 5. Delegation

Kind `delegate` (id 3) grants a named capability from the author to a
grantee identity — for example, the right to produce renditions
([004](004-manifest.md) §3). Delegation never transfers identity
ownership; it authorizes specific, kind-scoped actions that other kinds'
validation rules consult. Grants are revoked by a later `delegate`
record from the same author referencing the grant with `revoked = true`.
Schema and example: [003](003-kinds-registry.md) §4.3.

## 6. Profiles

Kind `profile` (id 2) carries display name, avatar, payment pointers,
declared relays and seed endpoints, and an optional encryption key
([008](008-privacy.md) §4). The current profile is the latest valid
profile record **by rotation-chain order of the signing keys that signed
them, then by supersession** — never by `created_at`, which is
untrusted. Concretely: a profile signed under a later chain state
supersedes one signed under an earlier state; within the same state,
`supersede` records express replacement.

## 7. Custody

Recovery keys MAY be held by the user (hardware key), by the user's
gateway (custodial convenience), or split among social contacts by
threshold arrangements above the protocol. Custody is permitted and
expected for mainstream users. **Custody must never be capture:**
leaving a custodial gateway is a rotation, available at any time,
requiring nothing from the gateway ([009](009-gateway.md) §5).

## Decisions

* Genesis breaks the self-reference cycle with a zero `identity_id`;
  the identifier is the genesis record id.
* Fork resolution is fully deterministic given a record set and local
  first-seen times: recovery-over-signing until finality, then bytewise
  record-id tiebreak. "Longest chain" alone was rejected — it lets a
  thief win by rotating faster than the victim.
* A rotation replaces signing key and recovery set atomically (its body
  is the whole new state) — simpler to verify than incremental deltas
  and merge-safe.

## Test vectors

* `identity/genesis-*` — valid genesis; invalid: nonzero identity_id,
  key/signature mismatch.
* `identity/rotate-*` — signing-key rotation, recovery rotation,
  algorithm-migration rotation.
* `identity/fork-*` — recovery precedence within window; signing branch
  final after window; same-class fork id tiebreak; contested rotation
  (both branches present).
* `identity/chain-order-*` — profile latest-wins by chain order, merge
  in three arrival orders producing identical state.
