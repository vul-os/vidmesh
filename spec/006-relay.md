# 006: Relay Protocol

**Status:** Draft 0.2
**Depends on:** [001-kernel.md](001-kernel.md), [003-kinds-registry.md](003-kinds-registry.md)
**Depended on by:** [007](007-bundles.md), [009](009-gateway.md)

Relays store and gossip records. A relay interprets nothing beyond the
envelope: it validates envelope integrity, stores, answers filtered
subscriptions, and forwards new records to peers with loop suppression.
A relay is not a gateway — it carries no serving obligations and no
moderation duties beyond its own resource policy. This file specifies
the websocket sync protocol, the optional blob sidecar, the policy
document, anti-spam, and gossip.

## 1. Transport

The sync protocol runs over WebSocket at path `/sync`. All frames are
**binary** WebSocket messages containing one canonically encoded CBOR
array whose first element is a text type tag. Unknown frame types MUST
be ignored (client) or answered with `["CLOSED", …]` (relay, for
unknown request types). Text frames MUST be ignored.

Client → relay:

| Frame | Meaning |
|-------|---------|
| `["REQ", sub_id: text, filter: map]` | Subscribe; relay sends matching stored records, then `EOSE`, then live matches |
| `["CLOSE", sub_id]` | End a subscription |
| `["PUB", record: bytes, nonce: uint or null]` | Publish a record (canonical envelope bytes) with optional PoW nonce (§6) |

Relay → client:

| Frame | Meaning |
|-------|---------|
| `["REC", sub_id: text, seq: uint, record: bytes]` | A matching record; `seq` is the relay-local receipt sequence (§2) |
| `["EOSE", sub_id]` | End of stored events; subsequent `REC`s are live |
| `["OK", id: bytes(32), accepted: bool, reason: text]` | Result of a `PUB` |
| `["CLOSED", sub_id, reason: text]` | Relay ended the subscription |

`sub_id` is client-chosen, ≤ 64 bytes, unique per connection.

## 2. Receipt sequence

A relay assigns each accepted record a strictly increasing local
integer `seq` (its receipt order). `seq` is relay-local and carries no
global meaning (Principle 8); it exists so clients can resume: a client
that has seen up to `seq = N` from this relay passes `since: N` in its
next filter.

## 3. Filters

A filter is a CBOR map; all present conditions must hold (AND); each
list condition matches any element (OR within the list):

| Key | Type | Matches records… |
|-----|------|-------------------|
| `kinds` | [uint] | whose kind is listed |
| `authors` | [bytes(32)] | whose `author.identity_id` is listed |
| `refs` | [bytes(32)] | any of whose refs' hash is listed |
| `ids` | [bytes(32)] | whose id is listed |
| `since` | uint | with relay `seq` strictly greater |
| `limit` | uint | stored-phase cap: relay sends at most this many stored records (most recent first), then `EOSE` |

An empty filter matches everything the relay will allow; relays MAY
reject over-broad filters via `CLOSED`.

## 4. Relay obligations and policy

* Envelope-validate every published record ([001](001-kernel.md) §3);
  reject invalid ones with `OK(false)`.
* Accept unknown kinds that pass envelope validation (kernel rule).
* Deduplicate by record id: a re-published record is `OK(true)` but not
  re-stored, not re-sequenced, and not re-gossiped.
* Retention is relay policy, advertised in `/info` (§5). Relays MAY
  expire by age, kind (e.g. `live.chat`), or size pressure. Expiry is
  not deletion from the substrate — other copies are unaffected.

## 5. HTTP endpoints

### 5.1 `GET /info`

Returns the relay policy document, JSON,
`application/json`:

```json
{ "name": "relay.example.net",
  "software": "vidmesh-relay/0.1",
  "pow_min_bits": 8,
  "rate": { "records_per_minute_per_key": 60 },
  "retention": { "default_days": 365, "live.chat_days": 2 },
  "blob": { "enabled": true, "max_bytes": 4294967296 },
  "peers": ["wss://relay2.example.org/sync"] }
```

All fields except `software` are OPTIONAL; absent means unspecified.

### 5.2 Blob sidecar (optional)

| Endpoint | Behavior |
|----------|----------|
| `PUT /blob` | Body = blob bytes. Relay hashes, stores, returns `201` with `{"id": "b3-256:…"}`. Reject over `max_bytes` with `413`; policy rejections `403`. |
| `GET /blob/{b3-256:hex}` | The blob; MUST support HTTP Range. `404` if absent. |
| `HEAD /blob/{b3-256:hex}` | Headers only; `Content-Length` present. |
| `GET /blob/{b3-256:hex}/proof?chunk=i` | CBOR `[chunk_index, [sibling hashes]]` — the range proof of [001](001-kernel.md) §8. Optional; advertised by presence. |

A server MUST verify the hash of a `PUT` blob before acknowledging;
a mismatch is `422`. Blob storage policy (who may PUT, quotas) is the
relay's own.

## 6. Anti-spam

* **Proof-of-work (optional).** Work function:
  `BLAKE3-256( id || nonce_le64 )` where `nonce_le64` is the nonce as 8
  little-endian bytes; difficulty = leading zero bits of the digest. The
  nonce travels in the `PUB` frame, outside the signed envelope, so
  work can be added or strengthened for an existing record without
  re-signing. Relays advertising `pow_min_bits` MUST reject
  publications below it (`OK(false, "pow")`); relays MUST NOT require
  PoW on `REQ`.
* **Rate limits.** Per-identity token buckets, advertised in `/info`,
  enforced with `OK(false, "rate")`.
* **Selectivity.** A relay MAY refuse any publication for any reason;
  refusal is `OK(false, reason)` and never affects other relays.

## 7. Aggregates

Counts (views, reactions) are **per-gateway computed claims**, not relay
functions. Gateways MAY publish signed tallies (as `attest` records)
that others sum; different gateways showing different numbers is
expected and embraced. Fraud-proof global counting is out of scope.

## 8. Gossip

A relay configured with peers maintains one client connection to each
peer's `/sync`:

* subscribe with the relay's own ingest filter (often empty);
* forward each locally accepted record to each peer via `PUB`;
* **loop suppression:** before storing/forwarding, drop records whose
  id is already stored. Ids make gossip idempotent; topology cycles are
  harmless.

Gossip is best-effort replication, not consensus. Two relays with the
same record set hold the same substrate slice regardless of arrival
order.

## Decisions

* Frames are CBOR (not JSON) — one codec everywhere, and records embed
  as bytes without re-encoding.
* `since` cursors are relay-local receipt sequences, not timestamps —
  untrusted `created_at` must not drive sync, and per-relay cursors
  survive partitions.
* PoW nonce is little-endian 8 bytes beside the record, never inside
  it: work is additive and re-spendable by third parties (a relay may
  grind extra work before re-gossiping to a stricter peer).

## Test vectors

* `relay/sync-*` — REQ/REC/EOSE flow against a live relay: stored
  backfill, live delivery, `since` resumption, every filter key.
* `relay/pub-*` — accept; duplicate id; envelope-invalid reject;
  unknown-kind accept.
* `relay/pow-*` — under-difficulty reject, exact-difficulty accept
  (fixed nonce fixtures).
* `relay/blob-*` — PUT/GET/HEAD round-trip, Range read, hash-mismatch
  422, chunk proof.
* `relay/gossip-*` — two relays, publish to one, receive from the
  other; loop suppression (no echo storm) — exercised by the
  docker-compose pair.
