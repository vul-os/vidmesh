# evermesh-relay

An axum-based relay: the substrate's dumb pipe. It interprets nothing
beyond the envelope ([spec 006](../../spec/006-relay.md)): validates
envelope integrity, stores, answers filtered subscriptions over
`WS /sync`, forwards new records to peers with loop suppression, and
optionally serves a content-addressed blob sidecar.

**Status: Phase 4 skeleton.** Full module structure, storage schema,
config, frame codec, and HTTP surface are implemented against a kernel
API (`evermesh_kernel::Record`, `ChunkTree`) that is specified but not
yet merged — see "What's pending" below. The crate will not compile
until that lands.

## Running

```sh
cargo run -p evermesh-relay -- path/to/relay.json
# or
EVERMESH_RELAY_CONFIG=path/to/relay.json cargo run -p evermesh-relay
```

With no config path given, the relay runs with all-default settings
(see `src/config.rs` for the exact defaults).

### Example config

Every field is optional; unset fields fall back to their documented
default.

```json
{
  "listen_addr": "0.0.0.0:8787",
  "db_path": "/data/relay.sqlite3",
  "name": "relay.example.net",
  "pow_min_bits": 8,
  "rate": { "records_per_minute_per_key": 60 },
  "retention": {
    "default_days": 365,
    "by_kind_days": { "113": 2 }
  },
  "blob": {
    "enabled": true,
    "dir": "/data/blobs",
    "max_bytes": 4294967296
  },
  "peers": ["wss://relay2.example.org/sync"]
}
```

`retention.by_kind_days` keys are numeric kind ids (spec 003 §1); e.g.
`113` is `live.chat`. `GET /info` renders these back with human-readable
keys (`"live.chat_days"`) for the wire, per the spec's worked example.

## Endpoints

| Endpoint | Behavior |
|----------|----------|
| `WS /sync` | `REQ`/`CLOSE`/`PUB` in, `REC`/`EOSE`/`OK`/`CLOSED` out (spec §1). |
| `GET /info` | Relay policy document, JSON (spec §5.1). |
| `PUT /blob` | Streams + hashes the body, stores content-addressed. `201` with `{"id": "b3-256:..."}`. Requires `blob.enabled`. |
| `GET /blob/{id}` | The blob; supports a single `Range: bytes=...` request. |
| `HEAD /blob/{id}` | Headers only. |
| `GET /blob/{id}/proof?chunk=i` | CBOR `[chunk_index, [sibling hashes]]` chunk-tree range proof (spec 001 §8). |

`{id}` accepts either a bare 64-hex-character hash or the `b3-256:<hex>`
text form (spec 001 §6).

## Storage schema (SQLite)

```sql
CREATE TABLE records (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,  -- relay-local receipt sequence
    id          BLOB UNIQUE NOT NULL,                -- record id (32 bytes)
    kind        INTEGER NOT NULL,
    author      BLOB NOT NULL,                       -- author.identity_id (32 bytes)
    received_at INTEGER NOT NULL,                     -- unix seconds, relay's own clock
    bytes       BLOB NOT NULL                         -- canonical envelope bytes
);
CREATE TABLE refs (record_id BLOB NOT NULL, hash BLOB NOT NULL);
-- + indexes on kind, author, refs.hash
```

Blobs live on disk, sharded as `<blob.dir>/<hex[0..2]>/<hex[2..4]>/<hex>`.

## Module map

| Module | Responsibility |
|--------|----------------|
| `config` | `RelayConfig`, loaded from JSON with documented defaults. |
| `store` | SQLite-backed record store: insert/dedup, filtered query, retention pruning. |
| `filter` | Subscription `Filter` (spec §3): parsing and per-record matching. |
| `frames` | The `/sync` wire frames and a small hand-rolled canonical-CBOR codec (kept private to this crate — the kernel's codec exists for records, not arbitrary frames). |
| `pow` | Proof-of-work check (spec §6): `BLAKE3-256(id \|\| nonce_le64)`, leading-zero-bit difficulty. |
| `rate` | Per-identity token-bucket rate limiting (spec §6). |
| `sync` | The `/sync` handler: subscriptions, backfill, live delivery, `PUB` ingest. **Kernel verification point #1**: every `PUB` goes through `Record::from_cbor` + `Record::verify`. |
| `gossip` | One reconnecting client per configured peer (spec §8). **Kernel verification point #2**: every record ingested from a peer goes through the same decode+verify as a local `PUB`. |
| `info` | `GET /info` policy document (spec §5.1). |
| `blobs` | The optional blob sidecar (spec §5.2), including the isolated kernel `ChunkTree` call site for range proofs. |

## Testing

```sh
cargo test -p evermesh-relay
```

Every module ships unit tests (frame round-trips and canonical-encoding
rejection, filter matching including `since`/`limit`, a PoW
brute-forced-nonce check, rate-bucket exhaustion, store insert/dup/query
against an in-memory SQLite database, blob hash/range/dedup behavior).
**Tests have not been run as part of this skeleton pass** — the crate
depends on `evermesh_kernel::Record`/`ChunkTree`, which are specified
but not yet merged, so `cargo test` cannot succeed yet.

## What works vs. what's pending kernel merge

Works today, independent of the kernel:

- Config loading and defaults (`config.rs`).
- The `/sync` frame codec, including canonical-encoding rejection
  (`frames.rs`).
- Filter parsing and matching (`filter.rs`).
- The SQLite store: insert, dedup, filtered query, retention pruning
  (`store.rs`).
- PoW difficulty check (`pow.rs`) and rate limiting (`rate.rs`).
- The blob sidecar's hashing/storage/range-read logic (`blobs.rs`),
  except the chunk-proof endpoint.
- `GET /info` document rendering (`info.rs`).

Blocked on the kernel crate implementing `Record` (currently only
`RecordId`/`BlobId`/`IdentityId` newtypes and the `Error` type exist)
and `ChunkTree`:

- `sync.rs`'s `PUB` handling (`Record::from_cbor`, `Record::verify`,
  `Record::id`, `Record::kind`, `Record::author_identity_id`,
  `Record::ref_hashes`).
- `gossip.rs`'s peer-ingest path (same `Record` calls).
- `blobs.rs`'s `GET /blob/{id}/proof` (`ChunkTree::from_bytes`,
  `ChunkTree::prove`, `blob::CHUNK_SIZE`).

## Known spec gaps / open questions for the lead

1. **`OK` frame `id` when the envelope fails to decode.** Spec 006 §1
   requires `["OK", id: bytes(32), accepted, reason]` for every `PUB`
   result, but an id can only be derived from a record that decoded
   successfully. This skeleton reports the all-zero id with a
   descriptive `reason` in that case; it is a placeholder, never a real
   record id, and should be revisited once the kernel lands (e.g. maybe
   the spec intends best-effort id extraction from the raw bytes before
   full validation, or an explicit "no id available" convention).
2. **Unknown top-level frame tags.** Spec 006 §1 says unknown frame
   types get answered with `CLOSED` "for unknown request types," but a
   frame with an unrecognized tag (or one that fails to parse as an
   array at all) may not carry a usable `sub_id`. This skeleton logs
   and drops such frames rather than guessing a `sub_id` to close.
3. **Blob `PUT` hash-mismatch semantics (spec §5.2).** The spec says "a
   mismatch is `422`," but a basic `PUT /blob` has no client-declared
   hash to mismatch against (the server always derives the id from
   what it received). This skeleton supports an optional
   `X-Expected-Blob-Id` request header for a client to pre-declare the
   hash it expects, and returns `422` only if that header is present
   and disagrees with the computed hash — flagging this as an
   interpretation, not a spec quote.
4. **Filter map key encoding.** Spec 006 §3 doesn't state whether
   filter map keys are text or integers; this skeleton follows the
   spec 003 §2 body convention (UTF-8 text keys: `"kinds"`, `"authors"`,
   `"refs"`, `"ids"`, `"since"`, `"limit"`) since the filter travels
   outside the signed envelope. Worth confirming against the
   conformance vectors once they exist.
5. **Backfill default `limit`.** Spec 006 §3 says `limit` is optional
   with no relay-side default specified. This skeleton caps
   unspecified-limit backfills at 500 records (`sync::DEFAULT_BACKFILL_LIMIT`)
   to avoid one `REQ` dumping an entire large store down a connection;
   confirm the intended default (if any) once conformance vectors exist.

## Docker / gossip pair

`Dockerfile` here builds the relay binary; the root
[`docker-compose.yml`](../../docker-compose.yml) runs two relays
peered with each other, for the conformance suite's `relay/gossip-*`
vectors.
