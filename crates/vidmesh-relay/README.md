# vidmesh-relay

An axum-based relay: the substrate's dumb pipe. Accepts signed records over
`WS /sync` (verified on ingest, unknown kinds accepted opaquely), optionally
serves blobs with verified range reads, publishes its policy at `GET /info`,
and gossips new records to peer relays with loop suppression.

**Status: Phase 0 scaffold — no implementation yet.** Phase 4 fills this in
after the kernel and spec exist.

## Planned surface

- `WS /sync` — subscribe by filter (kinds, authors, refs, since); publish.
- `GET/HEAD /blob/{id}`, `PUT /blob` — optional verified blob sidecar.
- `GET /info` — relay policy: PoW difficulty, rate limits, retention.
- SQLite storage; docker-compose with two gossiping relays for conformance.

## Testing

```sh
cargo test -p vidmesh-relay
```
