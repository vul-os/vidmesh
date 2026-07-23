---
title: Terms of Service
---

**Template — not legal advice.** Fill in every `{{PLACEHOLDER}}`, then
have this reviewed by a lawyer licensed in {{JURISDICTION}} before you
publish it. See `README.md` for how this folder is meant to be adapted.

# Terms of Service for {{GATEWAY_NAME}}

**Operator:** {{OPERATOR_ENTITY}}
**Jurisdiction:** {{JURISDICTION}}
**Contact:** {{CONTACT_EMAIL}}
**Effective date:** {{EFFECTIVE_DATE}}

These are the terms for using {{GATEWAY_NAME}} (the "Service"), operated
by {{OPERATOR_ENTITY}} ("we", "us"). By creating an account or using the
Service you agree to them.

## 1. What this service actually is

{{GATEWAY_NAME}} runs on the Evermesh protocol. Plainly, that means:

- Video and metadata on Evermesh live as signed records and
  content-addressed blobs on an open substrate that no one owns and no
  single party controls (see the protocol spec at
  `spec/000-overview.md` if you want the full picture).
- {{GATEWAY_NAME}} is one **gateway**: we choose a selection of that
  substrate to index and serve on this domain. We are not the only
  door, and we do not control the substrate itself.
- **If we remove something from {{GATEWAY_NAME}}, that removes it from
  our selection — it does not remove it from the substrate.** The same
  content may still be reachable through other gateways, direct P2P
  retrieval, or anyone who mirrors the underlying blobs. We say this
  plainly because it's true, not because we want it to be: no gateway
  operator, including us, can delete data from the substrate. If a law
  or a court requires the underlying content itself to be destroyed
  everywhere, that is not something any gateway operator can promise,
  and you should not rely on us for that outcome.
- Conversely, our decision to serve something is also just a decision.
  We can and do change it. Selection is our moderation model
  ([spec/009-gateway.md](../../../../spec/009-gateway.md) §1): allowing
  or denying content by hash, key, or category is our own configuration,
  applied instantly, and logged for our own audit.

## 2. Accounts and custodial keys

Your Evermesh identity is a cryptographic keypair, not a row in our
database that only we control. Depending on how you set up your
account:

- **You may hold your own signing key** (e.g., via a browser wallet or
  hardware key), in which case we never see it and cannot act on your
  behalf without your signature.
- **We may custody a signing key for you** ("custodial convenience") —
  we sign on your behalf and may hold a recovery key. This is common for
  mainstream accounts and is permitted under the protocol
  ([spec/009-gateway.md](../../../../spec/009-gateway.md) §5,
  [spec/002-identity.md](../../../../spec/002-identity.md) §7), but it
  comes with a hard limit we cannot avoid and would not want to:

### Your exit right (guaranteed, not a courtesy)

If we custody keys for you, **you can export your full identity — the
genesis record, the rotation chain, and any keys we hold for you — at
any time, and leave.** Leaving is a key rotation you can perform
without our cooperation, permission, or advance notice. We cannot lock
you into {{GATEWAY_NAME}}, and if we ever cannot produce your export on
request, that is a bug in our custody implementation, not a right we
are choosing to withhold. This is a protocol-level requirement of
running Evermesh's reference gateway
([spec/009-gateway.md](../../../../spec/009-gateway.md) §5): a custodial
gateway that cannot demonstrate the exit path is not
Evermesh-compliant, whatever else it calls itself.

Practically: use the "Export identity" function in account settings.
It gives you everything needed to continue publishing, signing, and
proving your history on any other Evermesh gateway or with your own
tools.

## 3. Acceptable use

Full rules are in `AUP.md`, which is part of these Terms. In short: no
illegal content, zero tolerance for child sexual abuse material (see
`CSAM.md` for how we handle that — it is not configurable, by us or by
anyone running this software), no harassment, no spam or proof-of-work
abuse of the network, and no knowing infringement.

## 4. Moderation and selection rights

We decide what {{GATEWAY_NAME}} serves. We may remove, de-index, or
decline to index any content, for any reason or none, including in
response to legal notices, our own policy, or automated compliance
feeds we subscribe to (`feed.takedown` records — see
[spec/009-gateway.md](../../../../spec/009-gateway.md) §3). We publish
our moderation policy at {{MODERATION_POLICY_URL}} and will keep it
current. Selection decisions on {{GATEWAY_NAME}} do not bind, and are
not bound by, any other gateway's decisions.

## 5. Notice-and-takedown

If you believe content on {{GATEWAY_NAME}} infringes your rights or
violates law, see `DMCA.md` (US) or `DSA.md` (EU) for how to send a
notice. Notices we receive are recorded as structured, signed
`notice.takedown` records
([spec/003-kinds-registry.md](../../../../spec/003-kinds-registry.md)
§6.5); counter-notices as `notice.counter` records (§6.6). Submitting a
notice does not obligate us to remove anything — we act according to
our own legal obligations and policy — and a notice itself can later be
disputed (`claim.dispute`,
[spec/005-claims.md](../../../../spec/005-claims.md) §4).

Our designated agent for legal notices: {{DMCA_AGENT}}.

## 6. Disclaimers and limitation of liability

THE SERVICE IS PROVIDED "AS IS" WITHOUT WARRANTIES OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE,
AND NON-INFRINGEMENT. We do not verify the truth of claims, comments, or
other user statements — see the honesty requirement in
[spec/005-claims.md](../../../../spec/005-claims.md) §2: a signature
proves who said something, not that it's true, and our interface will
not tell you otherwise.

TO THE MAXIMUM EXTENT PERMITTED BY LAW, {{OPERATOR_ENTITY}} WILL NOT BE
LIABLE FOR INDIRECT, INCIDENTAL, SPECIAL, CONSEQUENTIAL, OR PUNITIVE
DAMAGES, OR FOR ANY LOSS OF DATA, REVENUE, OR GOODWILL, ARISING FROM
YOUR USE OF THE SERVICE. {{LIABILITY_CAP_CLAUSE}}

*(This section is a placeholder structure. Liability limits that will
actually hold up are jurisdiction- and product-specific — have counsel
draft the real language.)*

## 7. Termination

We may suspend or terminate your account for violating these Terms or
the AUP. Termination ends your use of {{GATEWAY_NAME}} — it does not
and cannot revoke your Evermesh identity, delete your content from the
substrate, or prevent another gateway from serving it. If we custody
your keys, terminating your account does not forfeit your exit right
under §2: you may still export your identity.

## 8. Changes to these terms

We may update these Terms. {{CHANGE_NOTICE_POLICY — e.g., "We will post
changes at this URL at least N days before they take effect and notify
account holders by email."}} Continued use after the effective date of
a change means you accept it.

## 9. Contact

Questions about these Terms: {{CONTACT_EMAIL}}.
Legal notices: {{DMCA_AGENT}}.
