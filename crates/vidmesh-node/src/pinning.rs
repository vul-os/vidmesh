//! Pin store: the real v1 design surface for what a Vidmesh node persists.
//!
//! ## Why this file exists now
//!
//! The build plan locks the node app as a **scaffold only** in v1 (build
//! plan §2, §4 — "Tauri shell builds" is the whole Phase 8 gate). This
//! module is not runnable pinning/seeding logic: it is the documented
//! shape a future phase will implement, written now so the storage design
//! is decided ahead of time instead of invented later. Every stub is
//! marked `SCAFFOLD(phase-8)`, returns an empty/default value, and MUST
//! NOT panic on any input — see build plan §15 ("no placeholder/dead code
//! paths in shipped phases; stubs must be clearly named and documented").
//!
//! `#![allow(dead_code)]` below is the module-level acknowledgment that
//! this is the one sanctioned scaffold surface for Phase 8: only a subset
//! of these methods are wired into Tauri commands yet (see `main.rs`), and
//! the rest exist to pin down the design before it is implemented.
//!
//! ## Design (decided now, implemented later)
//!
//! - Backing store: a single SQLite database, one file per node install, at
//!   `<app-data-dir>/pins.sqlite3` (see [`PinStore::DB_FILE_NAME`]).
//! - Two tables (planned): `pins` (blob_id, manifest_id, reason, pinned_at)
//!   and `budget` (disk_gb, bandwidth_mbps, updated_at).
//! - `reason` is one of [`PinReason::Explicit`] (spec 000 §4: nodes "pin
//!   chosen content") or [`PinReason::Subscription`] (nodes "seed watched
//!   content").
//! - Pin priority: explicit pins are never evicted by budget pressure;
//!   subscription pins are evicted oldest-first once over budget.
//! - Everything here is content-addressed against `vidmesh-kernel` ids
//!   ([`BlobId`] for the bytes, [`RecordId`] for the manifest that named
//!   them) — the node never invents its own identifiers.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use vidmesh_kernel::{BlobId, RecordId};

/// Why a piece of content is pinned (spec 000 §4: "pins chosen content;
/// seeds watched content").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinReason {
    /// The node owner explicitly chose to pin this content.
    Explicit,
    /// Pinned because it belongs to a followed/subscribed channel.
    Subscription,
}

/// A single pinned item as it will be persisted.
///
/// SCAFFOLD(phase-8): shape only; nothing constructs or persists this yet.
#[derive(Debug, Clone)]
pub struct PinnedItem {
    /// The pinned blob's content hash.
    pub blob_id: BlobId,
    /// The manifest record that named this blob, if known.
    pub manifest_id: Option<RecordId>,
    /// Why this item is pinned.
    pub reason: PinReason,
}

/// This node's disk/bandwidth budget (spec 000 §4: "honors its own
/// budgets").
///
/// SCAFFOLD(phase-8): not yet persisted or enforced anywhere.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budget {
    /// Disk space reserved for pinned/seeded content, in gigabytes.
    pub disk_gb: u64,
    /// Upload bandwidth ceiling while seeding, in megabits per second.
    pub bandwidth_mbps: u64,
}

/// Handle to the node's pin database.
///
/// SCAFFOLD(phase-8): holds no real database connection yet. [`Self::open`]
/// never touches disk; every accessor returns an empty/default value; no
/// method panics.
#[derive(Debug, Clone)]
pub struct PinStore {
    db_path: PathBuf,
}

impl PinStore {
    /// The database file name within the node's app-data directory.
    pub const DB_FILE_NAME: &'static str = "pins.sqlite3";

    /// Open (or, in the real implementation, create and migrate) the pin
    /// store rooted at `data_dir`.
    ///
    /// SCAFFOLD(phase-8): computes the intended path only; does not open
    /// sqlite, does not touch disk, and cannot fail. The real
    /// implementation will run migrations and return a `Result`.
    pub fn open(data_dir: impl AsRef<Path>) -> Self {
        PinStore {
            db_path: data_dir.as_ref().join(Self::DB_FILE_NAME),
        }
    }

    /// The resolved database file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// List every currently pinned item.
    ///
    /// SCAFFOLD(phase-8): always empty.
    pub fn list_pins(&self) -> Vec<PinnedItem> {
        // TODO(phase-8): `SELECT * FROM pins ORDER BY pinned_at`.
        Vec::new()
    }

    /// Number of pinned items.
    ///
    /// SCAFFOLD(phase-8): always zero. Defined in terms of
    /// [`Self::list_pins`] so the two can never disagree once implemented.
    pub fn pinned_count(&self) -> u64 {
        self.list_pins().len() as u64
    }

    /// Pin a blob by explicit user choice or subscription membership.
    ///
    /// SCAFFOLD(phase-8): no-op; does not persist anything. Never panics.
    pub fn pin(&mut self, _blob_id: BlobId, _manifest_id: Option<RecordId>, _reason: PinReason) {
        // TODO(phase-8): upsert into `pins`, honoring budget eviction order.
    }

    /// Unpin a blob.
    ///
    /// SCAFFOLD(phase-8): reports "not found" for everything; no-op.
    /// Returns whether a row would have existed.
    pub fn unpin(&mut self, _blob_id: &BlobId) -> bool {
        // TODO(phase-8): `DELETE FROM pins WHERE blob_id = ?`.
        false
    }

    /// Whether a given blob is currently pinned.
    ///
    /// SCAFFOLD(phase-8): always false.
    pub fn is_pinned(&self, _blob_id: &BlobId) -> bool {
        // TODO(phase-8): `SELECT 1 FROM pins WHERE blob_id = ?`.
        false
    }

    /// This node's current budget configuration.
    ///
    /// SCAFFOLD(phase-8): always the zero/unconfigured budget.
    pub fn budget(&self) -> Budget {
        // TODO(phase-8): `SELECT * FROM budget LIMIT 1`, falling back to
        // the zero/unconfigured default on first run.
        Budget::default()
    }

    /// Update the node's budget configuration.
    ///
    /// SCAFFOLD(phase-8): no-op; does not persist. Never panics.
    pub fn set_budget(&mut self, _budget: Budget) {
        // TODO(phase-8): persist to `budget` table, then re-evaluate
        // subscription-pin eviction against the new disk_gb ceiling.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_never_touches_disk_and_never_panics() {
        let mut store = PinStore::open("/nonexistent/path/that/does/not/exist");
        assert!(store.db_path().ends_with(PinStore::DB_FILE_NAME));
        assert_eq!(store.pinned_count(), 0);
        assert!(store.list_pins().is_empty());
        assert_eq!(store.budget(), Budget::default());

        // Mutating scaffold methods must not panic even though they are
        // no-ops.
        store.pin(BlobId([0u8; 32]), None, PinReason::Explicit);
        assert!(!store.unpin(&BlobId([0u8; 32])));
        assert!(!store.is_pinned(&BlobId([0u8; 32])));
        store.set_budget(Budget {
            disk_gb: 10,
            bandwidth_mbps: 5,
        });
    }
}
