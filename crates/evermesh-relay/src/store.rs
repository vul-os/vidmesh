//! SQLite-backed record store.
//!
//! Schema:
//!
//! ```sql
//! CREATE TABLE records (
//!     seq INTEGER PRIMARY KEY AUTOINCREMENT,
//!     id BLOB UNIQUE NOT NULL,
//!     kind INTEGER NOT NULL,
//!     author BLOB NOT NULL,
//!     received_at INTEGER NOT NULL,
//!     bytes BLOB NOT NULL
//! );
//! CREATE TABLE refs (record_id BLOB NOT NULL, hash BLOB NOT NULL);
//! ```
//!
//! with indexes on `kind`, `author`, and `refs.hash`. `seq` is the
//! relay-local receipt sequence of spec 006 §2: strictly increasing,
//! never reused, carrying no meaning beyond "arrived after `seq - 1`".
//!
//! [`Store`] wraps a single `rusqlite::Connection` behind a
//! `std::sync::Mutex`, which is the documented skeleton approach: every
//! method call is synchronous SQLite I/O, so callers on the async side
//! (`sync.rs`) are expected to run it inside `tokio::task::spawn_blocking`
//! rather than holding an executor thread. The type itself makes no
//! async assumptions, which keeps it trivially unit-testable.

use std::collections::BTreeMap;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::config::RetentionConfig;
use crate::filter::Filter;

/// The result of attempting to insert a newly-received record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// Newly stored, assigned this receipt sequence.
    Inserted(u64),
    /// A record with this id was already stored (spec §4): not
    /// re-stored, re-sequenced, or re-gossiped, but not an error either.
    Duplicate,
}

/// A stored record as returned by [`Store::query`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRecord {
    /// Relay-local receipt sequence.
    pub seq: u64,
    /// The canonical envelope bytes as received.
    pub bytes: Vec<u8>,
}

/// The SQLite-backed store.
pub struct Store {
    conn: Mutex<Connection>,
}

fn u64_to_sql(v: u64) -> i64 {
    // Kind ids, sequence numbers, and timestamps are all small in
    // practice; clamp rather than panic in the pathological case of a
    // value that does not fit in SQLite's 64-bit signed INTEGER.
    i64::try_from(v).unwrap_or(i64::MAX)
}

impl Store {
    /// Open (creating if absent) a SQLite database file at `path` and
    /// ensure the schema exists.
    pub fn open(path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    /// Open an in-memory database (used by tests and short-lived tools).
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS records (
                seq         INTEGER PRIMARY KEY AUTOINCREMENT,
                id          BLOB UNIQUE NOT NULL,
                kind        INTEGER NOT NULL,
                author      BLOB NOT NULL,
                received_at INTEGER NOT NULL,
                bytes       BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_records_kind ON records(kind);
            CREATE INDEX IF NOT EXISTS idx_records_author ON records(author);

            CREATE TABLE IF NOT EXISTS refs (
                record_id BLOB NOT NULL,
                hash      BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_refs_hash ON refs(hash);
            CREATE INDEX IF NOT EXISTS idx_refs_record_id ON refs(record_id);
            ",
        )
    }

    /// Insert a newly-accepted, already envelope-verified record.
    /// Idempotent by id (spec §4): re-publishing an existing id returns
    /// [`InsertOutcome::Duplicate`] without touching storage.
    ///
    /// Parameters are the record's constituent, already-extracted
    /// parts (id, kind, author, ref hashes, canonical bytes) rather
    /// than a `evermesh_kernel::Record`, so this module has no kernel
    /// dependency and is unit-testable on its own.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_record(
        &self,
        id: &[u8; 32],
        kind: u64,
        author: &[u8; 32],
        received_at: i64,
        ref_hashes: &[[u8; 32]],
        bytes: &[u8],
    ) -> rusqlite::Result<InsertOutcome> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;

        let existing: Option<i64> = tx
            .query_row(
                "SELECT seq FROM records WHERE id = ?1",
                params![id.as_slice()],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some() {
            return Ok(InsertOutcome::Duplicate);
        }

        tx.execute(
            "INSERT INTO records (id, kind, author, received_at, bytes) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.as_slice(),
                u64_to_sql(kind),
                author.as_slice(),
                received_at,
                bytes
            ],
        )?;
        let seq = tx.last_insert_rowid();

        for hash in ref_hashes {
            tx.execute(
                "INSERT INTO refs (record_id, hash) VALUES (?1, ?2)",
                params![id.as_slice(), hash.as_slice()],
            )?;
        }

        tx.commit()?;
        Ok(InsertOutcome::Inserted(seq as u64))
    }

    /// Query stored records matching `filter`, most-recent-first,
    /// capped at `filter.limit` (falling back to `default_limit` if the
    /// filter did not specify one). This is the `REQ` stored-phase
    /// backfill (spec §1, §3).
    ///
    /// SQL-level predicates narrow by `kinds`/`authors`/`ids`/`refs`/
    /// `since`; every value is bound as a parameter, never
    /// string-interpolated, so this is injection-safe regardless of
    /// filter contents.
    pub fn query(
        &self,
        filter: &Filter,
        default_limit: u64,
    ) -> rusqlite::Result<Vec<StoredRecord>> {
        // An empty list for any list-typed condition can never match
        // (an OR over zero alternatives is always false).
        if matches!(&filter.kinds, Some(v) if v.is_empty())
            || matches!(&filter.authors, Some(v) if v.is_empty())
            || matches!(&filter.ids, Some(v) if v.is_empty())
            || matches!(&filter.refs, Some(v) if v.is_empty())
        {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut sql = String::from("SELECT seq, bytes FROM records WHERE 1=1");
        let mut bound: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(kinds) = &filter.kinds {
            push_in_clause(&mut sql, "kind", kinds.len());
            for k in kinds {
                bound.push(Box::new(u64_to_sql(*k)));
            }
        }
        if let Some(authors) = &filter.authors {
            push_in_clause(&mut sql, "author", authors.len());
            for a in authors {
                bound.push(Box::new(a.to_vec()));
            }
        }
        if let Some(ids) = &filter.ids {
            push_in_clause(&mut sql, "id", ids.len());
            for i in ids {
                bound.push(Box::new(i.to_vec()));
            }
        }
        if let Some(refs) = &filter.refs {
            let placeholders = "?,".repeat(refs.len());
            let placeholders = placeholders.trim_end_matches(',');
            sql.push_str(&format!(
                " AND id IN (SELECT record_id FROM refs WHERE hash IN ({placeholders}))"
            ));
            for r in refs {
                bound.push(Box::new(r.to_vec()));
            }
        }
        if let Some(since) = filter.since {
            sql.push_str(" AND seq > ?");
            bound.push(Box::new(u64_to_sql(since)));
        }

        sql.push_str(" ORDER BY seq DESC LIMIT ?");
        let limit = filter.limit.unwrap_or(default_limit);
        bound.push(Box::new(u64_to_sql(limit)));

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(StoredRecord {
                seq: row.get::<_, i64>(0)? as u64,
                bytes: row.get(1)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// The highest assigned `seq`, or `0` if nothing is stored yet.
    pub fn max_seq(&self) -> rusqlite::Result<u64> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let v: Option<i64> = conn.query_row("SELECT MAX(seq) FROM records", [], |r| r.get(0))?;
        Ok(v.unwrap_or(0) as u64)
    }

    /// Delete records older than their kind's retention window (spec
    /// §4). `now_unix` is injected rather than read from the system
    /// clock so this is deterministically testable. Returns the number
    /// of deleted records. Expiry here is local-store bookkeeping only
    /// (spec: "not deletion from the substrate").
    pub fn prune_expired(
        &self,
        retention: &RetentionConfig,
        now_unix: i64,
    ) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut deleted = 0usize;

        for (&kind, &days) in &retention.by_kind_days {
            let cutoff = now_unix - i64::from(days) * 86_400;
            deleted += conn.execute(
                "DELETE FROM records WHERE kind = ?1 AND received_at < ?2",
                params![u64_to_sql(kind), cutoff],
            )?;
        }

        let default_cutoff = now_unix - i64::from(retention.default_days) * 86_400;
        deleted += delete_default_expired(&conn, &retention.by_kind_days, default_cutoff)?;

        // Best-effort orphan cleanup; refs are only ever read via a
        // join against still-existing records, so this is cosmetic.
        conn.execute(
            "DELETE FROM refs WHERE record_id NOT IN (SELECT id FROM records)",
            [],
        )?;

        Ok(deleted)
    }
}

fn push_in_clause(sql: &mut String, column: &str, count: usize) {
    let placeholders = "?,".repeat(count);
    let placeholders = placeholders.trim_end_matches(',');
    sql.push_str(&format!(" AND {column} IN ({placeholders})"));
}

fn delete_default_expired(
    conn: &Connection,
    overrides: &BTreeMap<u64, u32>,
    default_cutoff: i64,
) -> rusqlite::Result<usize> {
    if overrides.is_empty() {
        return conn.execute(
            "DELETE FROM records WHERE received_at < ?1",
            params![default_cutoff],
        );
    }
    let override_kinds: Vec<i64> = overrides.keys().map(|k| u64_to_sql(*k)).collect();
    let placeholders = "?,".repeat(override_kinds.len());
    let placeholders = placeholders.trim_end_matches(',');
    let sql = format!("DELETE FROM records WHERE received_at < ? AND kind NOT IN ({placeholders})");
    let mut bound: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(default_cutoff)];
    for k in override_kinds {
        bound.push(Box::new(k));
    }
    let param_refs: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, param_refs.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        id: u8,
        kind: u64,
        author: u8,
        _refs: &[[u8; 32]],
        at: i64,
    ) -> ([u8; 32], u64, [u8; 32], i64, Vec<u8>) {
        ([id; 32], kind, [author; 32], at, vec![id, kind as u8])
    }

    #[test]
    fn insert_then_duplicate() {
        let store = Store::open_in_memory().unwrap();
        let (id, kind, author, at, bytes) = sample(1, 10, 5, &[], 1000);
        let first = store
            .insert_record(&id, kind, &author, at, &[], &bytes)
            .unwrap();
        assert_eq!(first, InsertOutcome::Inserted(1));

        let second = store
            .insert_record(&id, kind, &author, at, &[], &bytes)
            .unwrap();
        assert_eq!(second, InsertOutcome::Duplicate);
        assert_eq!(store.max_seq().unwrap(), 1);
    }

    #[test]
    fn seq_increases_and_query_orders_most_recent_first() {
        let store = Store::open_in_memory().unwrap();
        for i in 1..=3u8 {
            let (id, kind, author, at, bytes) = sample(i, 10, 5, &[], 1000 + i as i64);
            store
                .insert_record(&id, kind, &author, at, &[], &bytes)
                .unwrap();
        }
        assert_eq!(store.max_seq().unwrap(), 3);

        let filter = Filter::default();
        let rows = store.query(&filter, 100).unwrap();
        let seqs: Vec<u64> = rows.iter().map(|r| r.seq).collect();
        assert_eq!(seqs, vec![3, 2, 1]);
    }

    #[test]
    fn query_filters_by_kind_author_since_limit() {
        let store = Store::open_in_memory().unwrap();
        let (id1, k1, a1, at1, b1) = sample(1, 10, 5, &[], 1);
        let (id2, k2, a2, at2, b2) = sample(2, 20, 6, &[], 2);
        let (id3, k3, a3, at3, b3) = sample(3, 10, 6, &[], 3);
        store.insert_record(&id1, k1, &a1, at1, &[], &b1).unwrap();
        store.insert_record(&id2, k2, &a2, at2, &[], &b2).unwrap();
        store.insert_record(&id3, k3, &a3, at3, &[], &b3).unwrap();

        let by_kind = Filter {
            kinds: Some(vec![10]),
            ..Default::default()
        };
        let rows = store.query(&by_kind, 100).unwrap();
        assert_eq!(rows.len(), 2);

        let by_author = Filter {
            authors: Some(vec![[6u8; 32]]),
            ..Default::default()
        };
        let rows = store.query(&by_author, 100).unwrap();
        assert_eq!(rows.len(), 2);

        let since = Filter {
            since: Some(1),
            ..Default::default()
        };
        let rows = store.query(&since, 100).unwrap();
        let seqs: Vec<u64> = rows.iter().map(|r| r.seq).collect();
        assert_eq!(seqs, vec![3, 2]);

        let limited = Filter {
            limit: Some(1),
            ..Default::default()
        };
        let rows = store.query(&limited, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seq, 3);
    }

    #[test]
    fn query_filters_by_refs() {
        let store = Store::open_in_memory().unwrap();
        let target = [0xABu8; 32];
        let (id1, k1, a1, at1, b1) = sample(1, 10, 5, &[target], 1);
        let (id2, k2, a2, at2, b2) = sample(2, 10, 5, &[], 2);
        store
            .insert_record(&id1, k1, &a1, at1, &[target], &b1)
            .unwrap();
        store.insert_record(&id2, k2, &a2, at2, &[], &b2).unwrap();

        let filter = Filter {
            refs: Some(vec![target]),
            ..Default::default()
        };
        let rows = store.query(&filter, 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seq, 1);
    }

    #[test]
    fn empty_list_condition_matches_nothing() {
        let store = Store::open_in_memory().unwrap();
        let (id, kind, author, at, bytes) = sample(1, 10, 5, &[], 1);
        store
            .insert_record(&id, kind, &author, at, &[], &bytes)
            .unwrap();

        let filter = Filter {
            kinds: Some(vec![]),
            ..Default::default()
        };
        let rows = store.query(&filter, 100).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn prune_expired_respects_kind_override_and_default() {
        let store = Store::open_in_memory().unwrap();
        let now = 1_000_000i64;
        // kind 113 (live.chat-like) retained 2 days; this row is 3 days old.
        let (id1, k1, a1, at1, b1) = sample(1, 113, 5, &[], now - 3 * 86_400);
        // default retention 365 days; this row is 1 day old, kept.
        let (id2, k2, a2, at2, b2) = sample(2, 10, 5, &[], now - 86_400);
        store.insert_record(&id1, k1, &a1, at1, &[], &b1).unwrap();
        store.insert_record(&id2, k2, &a2, at2, &[], &b2).unwrap();

        let mut retention = RetentionConfig::default();
        retention.by_kind_days.insert(113, 2);

        let deleted = store.prune_expired(&retention, now).unwrap();
        assert_eq!(deleted, 1);
        let rows = store.query(&Filter::default(), 100).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].seq, 2);
    }
}
