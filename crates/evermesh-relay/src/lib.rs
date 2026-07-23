//! # evermesh-relay
//!
//! A relay per `spec/006-relay.md`: it interprets nothing beyond the
//! envelope. It validates envelope integrity, stores, answers filtered
//! subscriptions over `WS /sync`, forwards new records to peers with loop
//! suppression, and optionally serves a content-addressed blob sidecar.
//!
//! This crate is organized so each module can be unit-tested without a
//! running server:
//!
//! - [`config`] — `RelayConfig`, loaded from a JSON file.
//! - [`store`] — SQLite-backed record store.
//! - [`filter`] — subscription `Filter` matching (spec §3).
//! - [`frames`] — the `/sync` wire frames and their hand-rolled canonical
//!   CBOR codec (spec §1).
//! - [`pow`] — proof-of-work check (spec §6).
//! - [`rate`] — per-identity token-bucket rate limiting (spec §6).
//! - [`sync`] — the `/sync` websocket handler tying the above together.
//! - [`gossip`] — outbound peer connections (spec §8).
//! - [`info`] — the `GET /info` policy document (spec §5.1).
//! - [`blobs`] — the optional blob sidecar (spec §5.2).
//!
//! `main.rs` is a thin binary entry point; it wires these modules
//! together and contains no protocol logic of its own.

#![forbid(unsafe_code)]

use std::sync::Arc;

use tokio::sync::broadcast;

pub mod blobs;
pub mod config;
pub mod filter;
pub mod frames;
pub mod gossip;
pub mod info;
pub mod pow;
pub mod rate;
pub mod store;
pub mod sync;

pub use config::RelayConfig;
pub use rate::RateLimiter;
pub use store::Store;

/// A record this relay just accepted — either a fresh local `PUB` or a
/// record ingested from a gossip peer. Broadcast to every live
/// `/sync` subscriber (filtered per-subscription) and to every
/// gossip-outbound task, which is how "forward to peers" (spec 006
/// §8) and "deliver to live subscriptions" (spec 006 §1) share one
/// fan-out point.
#[derive(Debug, Clone)]
pub struct AcceptedRecord {
    pub id: [u8; 32],
    pub kind: u64,
    pub author: [u8; 32],
    pub ref_hashes: Vec<[u8; 32]>,
    pub seq: u64,
    pub bytes: Vec<u8>,
}

/// Shared state threaded through every axum handler and the gossip
/// client tasks. Cheap to clone (everything behind an `Arc` or already
/// `Clone`, e.g. `broadcast::Sender`).
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RelayConfig>,
    pub store: Arc<Store>,
    pub rate_limiter: Arc<RateLimiter>,
    /// Fan-out channel for [`AcceptedRecord`]s. A `broadcast` channel
    /// is used because there are multiple independent, slow-reader-
    /// tolerant consumers (each live `/sync` connection, each gossip
    /// peer task): a lagging reader just skips forward (logged), it
    /// never blocks or crashes the publisher.
    pub accepted_tx: broadcast::Sender<AcceptedRecord>,
}

/// Fan-out channel capacity: how many not-yet-delivered accepted
/// records a slow subscriber can fall behind by before it starts
/// missing them (and gets a `Lagged` notice next `recv()`).
const ACCEPTED_CHANNEL_CAPACITY: usize = 1024;

impl AppState {
    /// Build fresh shared state around a loaded config and an opened
    /// store.
    pub fn new(config: RelayConfig, store: Store) -> Self {
        let rate_limiter = RateLimiter::new(config.rate.records_per_minute_per_key);
        let (accepted_tx, _rx) = broadcast::channel(ACCEPTED_CHANNEL_CAPACITY);
        AppState {
            config: Arc::new(config),
            store: Arc::new(store),
            rate_limiter: Arc::new(rate_limiter),
            accepted_tx,
        }
    }
}
