/**
 * SQLite schema + migrations (better-sqlite3, no ORM). One migration per
 * schema version, applied in order and tracked via `PRAGMA user_version`.
 * Every table an API route queries by something other than its primary
 * key gets an explicit index — policy denials in particular must stay
 * cheap (indexed lookups), per the build-plan rules.
 */
import Database from "better-sqlite3";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

export type Db = Database.Database;

const MIGRATIONS: string[] = [
  // v1: base schema.
  `
  CREATE TABLE records (
    id          TEXT PRIMARY KEY,
    kind        INTEGER NOT NULL,
    author      TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    cbor        BLOB NOT NULL,
    json        TEXT NOT NULL
  );
  CREATE INDEX idx_records_kind ON records(kind);
  CREATE INDEX idx_records_author ON records(author);
  CREATE INDEX idx_records_received_at ON records(received_at);

  CREATE TABLE refs (
    record_id TEXT NOT NULL,
    ref_type  INTEGER NOT NULL,
    hash      TEXT NOT NULL,
    position  INTEGER NOT NULL
  );
  CREATE INDEX idx_refs_record_id ON refs(record_id);
  CREATE INDEX idx_refs_hash ON refs(hash);

  CREATE TABLE videos (
    manifest_id      TEXT PRIMARY KEY,
    author           TEXT NOT NULL,
    title            TEXT NOT NULL,
    description      TEXT NOT NULL DEFAULT '',
    tags_json        TEXT NOT NULL DEFAULT '[]',
    language         TEXT,
    duration_ms      INTEGER NOT NULL DEFAULT 0,
    thumbnail_blob   TEXT,
    channel_id       TEXT,
    license          TEXT NOT NULL DEFAULT 'all-rights-reserved',
    created_at       INTEGER NOT NULL,
    received_at      INTEGER NOT NULL,
    body_json        TEXT NOT NULL,
    retracted        INTEGER NOT NULL DEFAULT 0
  );
  CREATE INDEX idx_videos_author ON videos(author);
  CREATE INDEX idx_videos_channel ON videos(channel_id);
  CREATE INDEX idx_videos_received_at ON videos(received_at);

  CREATE TABLE comments (
    record_id   TEXT PRIMARY KEY,
    manifest_id TEXT NOT NULL,
    author      TEXT NOT NULL,
    text        TEXT NOT NULL,
    media_json  TEXT NOT NULL DEFAULT '[]',
    parent      TEXT,
    created_at  INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    retracted   INTEGER NOT NULL DEFAULT 0
  );
  CREATE INDEX idx_comments_manifest ON comments(manifest_id);
  CREATE INDEX idx_comments_parent ON comments(parent);

  CREATE TABLE reactions (
    target_id   TEXT NOT NULL,
    author      TEXT NOT NULL,
    record_id   TEXT NOT NULL,
    reaction    TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (target_id, author)
  );
  CREATE INDEX idx_reactions_target ON reactions(target_id);
  CREATE INDEX idx_reactions_record_id ON reactions(record_id);

  CREATE TABLE follows (
    record_id   TEXT PRIMARY KEY,
    author      TEXT NOT NULL,
    target      TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    retracted   INTEGER NOT NULL DEFAULT 0
  );
  CREATE INDEX idx_follows_pair ON follows(author, target, received_at);

  CREATE TABLE channels (
    record_id   TEXT PRIMARY KEY,
    author      TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    avatar_blob TEXT,
    banner_blob TEXT,
    created_at  INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    retracted   INTEGER NOT NULL DEFAULT 0
  );
  CREATE INDEX idx_channels_author ON channels(author);

  CREATE TABLE profiles (
    identity_id TEXT PRIMARY KEY,
    record_id   TEXT NOT NULL,
    name        TEXT NOT NULL,
    about       TEXT,
    avatar_blob TEXT,
    payment_json TEXT NOT NULL DEFAULT '[]',
    relays_json  TEXT NOT NULL DEFAULT '[]',
    created_at  INTEGER NOT NULL,
    received_at INTEGER NOT NULL
  );

  CREATE TABLE claims (
    record_id          TEXT PRIMARY KEY,
    kind               INTEGER NOT NULL,
    author             TEXT NOT NULL,
    subject_manifest_id TEXT,
    target_record_id   TEXT NOT NULL,
    body_json          TEXT NOT NULL,
    created_at         INTEGER NOT NULL,
    received_at        INTEGER NOT NULL,
    retracted          INTEGER NOT NULL DEFAULT 0
  );
  CREATE INDEX idx_claims_manifest ON claims(subject_manifest_id);
  CREATE INDEX idx_claims_target ON claims(target_record_id);

  CREATE TABLE receipts (
    record_id   TEXT PRIMARY KEY,
    manifest_id TEXT NOT NULL,
    payer       TEXT NOT NULL,
    amount      INTEGER NOT NULL,
    currency    TEXT NOT NULL,
    rail        INTEGER NOT NULL,
    payee       TEXT NOT NULL,
    proof       TEXT,
    message     TEXT,
    created_at  INTEGER NOT NULL,
    received_at INTEGER NOT NULL
  );
  CREATE INDEX idx_receipts_manifest ON receipts(manifest_id);

  CREATE TABLE users (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    handle          TEXT NOT NULL UNIQUE,
    pw_hash         TEXT NOT NULL,
    identity_id     TEXT NOT NULL UNIQUE,
    secret_key_enc  TEXT NOT NULL,
    created_at      INTEGER NOT NULL
  );

  CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,
    user_id     INTEGER NOT NULL,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL
  );
  CREATE INDEX idx_sessions_user ON sessions(user_id);
  CREATE INDEX idx_sessions_expires ON sessions(expires_at);

  CREATE TABLE export_attempts (
    user_id INTEGER NOT NULL,
    ts      INTEGER NOT NULL
  );
  CREATE INDEX idx_export_attempts_user ON export_attempts(user_id, ts);

  CREATE TABLE uploads (
    id          TEXT PRIMARY KEY,
    user_id     INTEGER NOT NULL,
    status      TEXT NOT NULL,
    progress    INTEGER NOT NULL DEFAULT 0,
    manifest_id TEXT,
    error       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
  );
  CREATE INDEX idx_uploads_user ON uploads(user_id);

  CREATE TABLE blobs (
    id          TEXT PRIMARY KEY,
    size        INTEGER NOT NULL,
    path        TEXT NOT NULL,
    mime        TEXT,
    csam_checked INTEGER NOT NULL DEFAULT 0,
    csam_match   INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
  );

  CREATE TABLE hls_segments (
    manifest_id TEXT NOT NULL,
    rendition   TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    blob_id     TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    is_init     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (manifest_id, rendition, seq)
  );

  CREATE TABLE hls_renditions (
    manifest_id TEXT NOT NULL,
    rendition   TEXT NOT NULL,
    height      INTEGER NOT NULL,
    width       INTEGER NOT NULL DEFAULT 0,
    bandwidth   INTEGER NOT NULL,
    codec       TEXT NOT NULL,
    PRIMARY KEY (manifest_id, rendition)
  );

  CREATE TABLE policy_denylist (
    scope  TEXT NOT NULL,
    value  TEXT NOT NULL,
    reason TEXT,
    feed   TEXT,
    notice TEXT,
    ts     INTEGER NOT NULL,
    PRIMARY KEY (scope, value)
  );

  CREATE TABLE feed_batches (
    feed      TEXT NOT NULL,
    seq       INTEGER NOT NULL,
    publisher TEXT NOT NULL,
    record_id TEXT NOT NULL,
    applied_at INTEGER NOT NULL,
    PRIMARY KEY (feed, seq)
  );

  CREATE TABLE feed_state (
    feed        TEXT PRIMARY KEY,
    publisher   TEXT NOT NULL,
    last_seq    INTEGER NOT NULL DEFAULT -1
  );

  CREATE TABLE policy_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    ts         INTEGER NOT NULL,
    action     TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject    TEXT NOT NULL,
    reason     TEXT,
    detail     TEXT
  );
  CREATE INDEX idx_policy_log_ts ON policy_log(ts);

  CREATE TABLE relay_state (
    url       TEXT PRIMARY KEY,
    since_seq INTEGER NOT NULL DEFAULT 0
  );

  CREATE TABLE gateway_identity (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    identity_id    TEXT NOT NULL,
    secret_key_enc TEXT NOT NULL,
    created_at     INTEGER NOT NULL
  );
  `,
];

/** Open (creating parent dirs) and migrate the SQLite index database. */
export function openDb(path: string): Db {
  if (path !== ":memory:") mkdirSync(dirname(path), { recursive: true });
  const db = new Database(path);
  db.pragma("journal_mode = WAL");
  db.pragma("foreign_keys = ON");
  migrate(db);
  return db;
}

function migrate(db: Db): void {
  const current = db.pragma("user_version", { simple: true }) as number;
  const target = MIGRATIONS.length;
  if (current >= target) return;
  const txn = db.transaction(() => {
    for (let v = current; v < target; v++) {
      db.exec(MIGRATIONS[v]);
      db.pragma(`user_version = ${v + 1}`);
    }
  });
  txn();
}
