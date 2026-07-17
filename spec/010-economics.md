# 010: Economics

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md), [003-kinds-registry.md](003-kinds-registry.md)
**Depended on by:** [009-gateway.md](009-gateway.md) (gated access)

The protocol ships three neutral primitives — payment pointers,
receipts, and disclosures — and no business model. Fiat is first-class;
no rail is required; a protocol token is permanently out of scope
(Principle 6). Trust arises from auditability and exit, not
cryptographic enforcement.

## 1. Payment pointers

```
PaymentPointer = [ type: uint, value: text ]
```

Ordered by the publisher's preference; carried in `profile` and
`manifest` bodies.

| Id | Type | Value |
|---:|------|-------|
| 0 | reserved | — |
| 1 | `lightning` | Lightning address or LNURL |
| 2 | `usdc-base` | USDC address on Base |
| 3 | `stripe` | Stripe payment link |
| 4 | `paypal` | PayPal.me handle or address |

New rails are new registry entries. Clients render the rails they
understand and MUST ignore unknown types.

## 2. Receipts

Kind `receipt` ([003](003-kinds-registry.md) §7.2) is the zap pattern:
a signed statement linking payer, amount, rail, and message to a
manifest or stream. Normative points:

* A receipt proves the payer *said* they paid; settlement proof lives
  in the rail. Gateways MAY verify `proof` (Lightning preimage,
  transaction reference) before rendering a receipt prominently, and
  SHOULD label unverified receipts as claims.
* Receipts render as tips/superchats at gateway discretion; totals are
  per-gateway aggregates like all counts.

## 3. Disclosures

The manifest `sponsorship` field ([004](004-manifest.md) §1) declares
creator-embedded sponsorship:

```
Sponsor = { start: uint (ms), end: uint (ms), sponsor_label: text }
```

Clients SHOULD surface sponsorship segments on the timeline. The field
is a disclosure primitive, not an ad system.

## 4. Non-normative guidance

Gateways publish signed revenue-share commitments (as `attest` records
about themselves); creators publish `endorse.gateway`; audiences follow
endorsements. Gated access ([008](008-privacy.md) §4) composes payment
with keygrants without new protocol surface.

## Test vectors

* `kinds/receipt/` — per [003](003-kinds-registry.md); plus
  amount-zero invalid.
* `economics/pointer-*` — pointer arrays with unknown types preserved
  and ignored.
