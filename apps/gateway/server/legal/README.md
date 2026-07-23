# Legal toolkit for gateway operators

**Template — not legal advice.**

This folder is the compliance starter kit that ships with the Evermesh
reference gateway (build plan §9, [spec/009-gateway.md](../../../../spec/009-gateway.md)
§4). It exists because running a public video service carries real
legal obligations, and most of those obligations are the same shape for
every operator. Shipping the paperwork alongside the code lowers the
cost of running a compliant gateway — that is decentralization
infrastructure, not a nice-to-have.

## What's in here

| File | Purpose |
|------|---------|
| `TERMS.md` | Templated Terms of Service |
| `AUP.md` | Acceptable Use Policy |
| `DMCA.md` | US DMCA §512 operator guide and notice/counter-notice workflow |
| `DSA.md` | EU Digital Services Act orientation for notice-and-action |
| `GEO-BLOCKING.md` | How to use the per-item geo-block policy feature honestly |
| `JURISDICTIONS.md` | Index of compliance profiles (which docs and feeds apply where) |
| `../CSAM.md` | The mandatory hash-matching integration point (one directory up — it documents a non-configurable part of the reference gateway, not an optional legal template) |

## What none of this is

None of these files are legal advice, and none of them were written by
a lawyer who knows your business, your users, or your jurisdiction. They
are starting drafts written to be *honest about the protocol* — what a
gateway can and cannot do, what removal actually means on a substrate
that has no delete operation — and to point you at the right statutes
and official processes. They are not a substitute for review by a
lawyer licensed where you operate.

**Before you launch a gateway with real users, get these documents
reviewed by counsel in your jurisdiction.** Requirements vary by
country, by whether you serve minors, by whether you accept payments,
and by what content categories you allow. This toolkit cannot know any
of that for you.

## How to adapt these templates

1. Read [spec/009-gateway.md](../../../../spec/009-gateway.md) first —
   it defines what a gateway actually is (an independent selection over
   a substrate it doesn't own) and what obligations attach to running
   the reference implementation.
2. Fill in every `{{PLACEHOLDER}}` token. Search this folder for `{{` to
   find them all — do not ship a gateway with unfilled placeholders.
3. Have a lawyer review the filled-in result, not just this template.
   Laws referenced here (DMCA, DSA, etc.) are cited at the
   statute/article level so your lawyer can go straight to the source;
   they are not a full restatement of those laws.
4. Keep `CSAM.md` and its non-configurable hash-matching requirement
   intact. It is a trademark-program and reference-implementation
   requirement, not a legal template you're free to water down
   ([spec/009-gateway.md](../../../../spec/009-gateway.md) §4).
5. Decide which feeds to subscribe (`feed.takedown`,
   [spec/003-kinds-registry.md](../../../../spec/003-kinds-registry.md)
   §6.7) and which jurisdiction profile in `JURISDICTIONS.md` applies to
   you, then wire the notice-intake endpoints described in `DMCA.md` and
   `DSA.md` to emit `notice.takedown` / `notice.counter` records
   ([spec/005-claims.md](../../../../spec/005-claims.md) §4).

## The one honest sentence that has to survive every edit

Removing something from your gateway removes it from *your selection*.
It does not remove it from the substrate. Say this plainly to your
users and in your Terms — see `TERMS.md` §1.
