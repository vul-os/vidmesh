---
title: Acceptable Use Policy
---

**Template — not legal advice.** Fill in every `{{PLACEHOLDER}}`, adapt
the categories below to your jurisdiction and risk tolerance, then have
this reviewed by a lawyer licensed in {{JURISDICTION}}. This document is
part of the Terms of Service (`TERMS.md` §3).

# Acceptable Use Policy for {{GATEWAY_NAME}}

This policy describes what you may not do on {{GATEWAY_NAME}}. It
applies to everything you upload, publish, or say through our service,
including content you sign yourself and content we sign for you under
custody.

Remember: this policy governs **our selection**, not the substrate.
Removing something for violating this policy means we stop serving and
indexing it — it does not delete the underlying record or blob from
the Evermesh network (see `TERMS.md` §1).

## 1. Illegal content

You may not use {{GATEWAY_NAME}} to publish content that is illegal
under the law of {{JURISDICTION}} or, where applicable, the law
governing the viewer's location. This includes content that violates
export control, incitement, or terrorism laws in force where we
operate. {{JURISDICTION_SPECIFIC_ILLEGAL_CONTENT_NOTES}}

## 2. Child sexual abuse material — zero tolerance

We have zero tolerance for child sexual abuse material (CSAM) in any
form. All uploads are checked against industry hash-matching databases
before publication and again at index time — see `../CSAM.md` for how
this works. This check is not configurable and cannot be disabled by
this gateway or any gateway claiming Evermesh trademark compliance
([spec/009-gateway.md](../../../../spec/009-gateway.md) §4).

A confirmed match results in immediate blocking, no serving, and a
mandatory report to the relevant authority (NCMEC in the US, or the
equivalent body in your jurisdiction — see `../CSAM.md` §"Reporting").
We do not warn, appeal, or negotiate this category. Accounts responsible
are permanently terminated and reported.

## 3. Harassment and abuse

You may not use {{GATEWAY_NAME}} to threaten, stalk, dox, or
persistently harass another person, or to organize coordinated
harassment. This includes using comments (`comment` records), reactions,
or live chat (`live.chat` records) for the same purpose.

## 4. Spam and proof-of-work / network abuse

You may not flood the network with low-value or automated publications,
attempt to exhaust relay rate limits or proof-of-work budgets to deny
service to others, or use {{GATEWAY_NAME}}'s custodial signing to mass
produce spam records. Relays we use may enforce their own per-key rate
limits and PoW-over-record-id checks
([spec/006-relay.md](../../../../spec/006-relay.md)); attempting to
circumvent them is a violation here regardless of whether it succeeds
against the relay.

## 5. Infringement

You may not upload content you don't have the rights to publish, or
falsely claim authorship or license terms over someone else's work
(`claim.author`, `claim.license` —
[spec/003-kinds-registry.md](../../../../spec/003-kinds-registry.md)
§6.1–6.2). Repeat or willful infringement is grounds for termination
under our repeat-infringer policy — see `DMCA.md` §"Repeat infringers."
Rights holders can send a formal notice under `DMCA.md` or `DSA.md`.

## 6. What happens when you violate this policy

Depending on severity, we may: remove or de-index the specific content,
apply a geo-block instead of a full removal where that's the accurate
scope of the problem (see `GEO-BLOCKING.md`), suspend your account, or
terminate it. CSAM violations (§2) are always full removal, termination,
and mandatory reporting — there is no lesser response for that category.

## 7. Reporting a violation

To report content that violates this policy: {{ABUSE_REPORT_URL_OR_EMAIL}}.
To report copyright or trademark infringement specifically, use the
formal notice process in `DMCA.md` or `DSA.md` instead — it creates an
auditable record and starts statutory timelines that this general
inbox does not.

## 8. Changes

We may update this policy as our understanding of emerging abuse
patterns changes. See `TERMS.md` §8 for how changes are communicated.
