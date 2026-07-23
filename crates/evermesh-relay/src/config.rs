//! Relay configuration, loaded from a JSON file (spec 006 §5.1 fields
//! mirror `/info`, since `/info` is largely a public view of config).
//!
//! Every field is optional in the file; [`RelayConfig::default`] (via
//! `#[serde(default = ...)]`) documents the effective defaults for a
//! skeleton relay: no PoW requirement, generous rate limit, one-year
//! retention, blob sidecar disabled, no peers.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Top-level relay configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    /// Address (`host:port`) axum binds and listens on.
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// Path to the SQLite database file (created if absent).
    #[serde(default = "default_db_path")]
    pub db_path: String,

    /// Human-readable relay name, advertised in `/info`.
    #[serde(default = "default_name")]
    pub name: String,

    /// Minimum accepted proof-of-work difficulty in leading zero bits.
    /// `0` disables the PoW requirement (spec 006 §6).
    #[serde(default)]
    pub pow_min_bits: u32,

    /// Per-identity rate limiting policy.
    #[serde(default)]
    pub rate: RateConfig,

    /// Retention policy, by kind and by default.
    #[serde(default)]
    pub retention: RetentionConfig,

    /// Optional blob sidecar settings (spec 006 §5.2).
    #[serde(default)]
    pub blob: BlobConfig,

    /// Peer relay `/sync` URLs (`wss://...`) to gossip with (spec 006 §8).
    #[serde(default)]
    pub peers: Vec<String>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        RelayConfig {
            listen_addr: default_listen_addr(),
            db_path: default_db_path(),
            name: default_name(),
            pow_min_bits: 0,
            rate: RateConfig::default(),
            retention: RetentionConfig::default(),
            blob: BlobConfig::default(),
            peers: Vec::new(),
        }
    }
}

impl RelayConfig {
    /// Load and parse a config file. Missing fields fall back to their
    /// documented defaults; unknown fields are ignored (forward
    /// compatible: an older relay binary can load a newer config).
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading config {}: {e}", path.display()))?;
        Self::from_json(&text)
    }

    /// Parse config from a JSON string (split out from [`Self::load`]
    /// for testing without touching the filesystem).
    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        let cfg: RelayConfig =
            serde_json::from_str(text).map_err(|e| anyhow::anyhow!("parsing config: {e}"))?;
        Ok(cfg)
    }

    /// Retention in days for a given kind, falling back to the default.
    pub fn retention_days_for_kind(&self, kind: u64) -> u32 {
        self.retention
            .by_kind_days
            .get(&kind)
            .copied()
            .unwrap_or(self.retention.default_days)
    }
}

fn default_listen_addr() -> String {
    "127.0.0.1:8787".to_string()
}

fn default_db_path() -> String {
    "evermesh-relay.sqlite3".to_string()
}

fn default_name() -> String {
    "evermesh-relay".to_string()
}

/// Anti-spam rate limiting (spec 006 §6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateConfig {
    /// Token-bucket refill rate: accepted `PUB`s per minute per identity.
    #[serde(default = "default_records_per_minute")]
    pub records_per_minute_per_key: u32,
}

impl Default for RateConfig {
    fn default() -> Self {
        RateConfig {
            records_per_minute_per_key: default_records_per_minute(),
        }
    }
}

fn default_records_per_minute() -> u32 {
    60
}

/// Retention policy (spec 006 §4): expiry by age, optionally overridden
/// per kind (e.g. `live.chat` records expiring faster than the default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Default retention, in days, for kinds with no specific override.
    #[serde(default = "default_retention_days")]
    pub default_days: u32,

    /// Per-kind overrides: kind id -> retention in days.
    #[serde(default)]
    pub by_kind_days: BTreeMap<u64, u32>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        RetentionConfig {
            default_days: default_retention_days(),
            by_kind_days: BTreeMap::new(),
        }
    }
}

fn default_retention_days() -> u32 {
    365
}

/// Optional blob sidecar configuration (spec 006 §5.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobConfig {
    /// Whether `/blob` endpoints are mounted at all.
    #[serde(default)]
    pub enabled: bool,

    /// Directory blobs are stored under, content-addressed as
    /// `dir/<hex[0..2]>/<hex[2..4]>/<hex>`.
    #[serde(default = "default_blob_dir")]
    pub dir: String,

    /// Maximum accepted blob size in bytes; larger `PUT`s are `413`.
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
}

impl Default for BlobConfig {
    fn default() -> Self {
        BlobConfig {
            enabled: false,
            dir: default_blob_dir(),
            max_bytes: default_max_bytes(),
        }
    }
}

fn default_blob_dir() -> String {
    "./blobs".to_string()
}

fn default_max_bytes() -> u64 {
    4 * 1024 * 1024 * 1024 // 4 GiB, matches the spec 006 §5.1 worked example
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_on_empty_object() {
        let cfg = RelayConfig::from_json("{}").unwrap();
        assert_eq!(cfg.listen_addr, "127.0.0.1:8787");
        assert_eq!(cfg.pow_min_bits, 0);
        assert_eq!(cfg.rate.records_per_minute_per_key, 60);
        assert_eq!(cfg.retention.default_days, 365);
        assert!(!cfg.blob.enabled);
        assert!(cfg.peers.is_empty());
    }

    #[test]
    fn overrides_and_retention_lookup() {
        let cfg = RelayConfig::from_json(
            r#"{
                "name": "relay.example.net",
                "pow_min_bits": 8,
                "retention": { "default_days": 365, "by_kind_days": { "113": 2 } },
                "peers": ["wss://relay2.example.org/sync"]
            }"#,
        )
        .unwrap();
        assert_eq!(cfg.name, "relay.example.net");
        assert_eq!(cfg.pow_min_bits, 8);
        assert_eq!(cfg.retention_days_for_kind(113), 2);
        assert_eq!(cfg.retention_days_for_kind(1), 365);
        assert_eq!(cfg.peers, vec!["wss://relay2.example.org/sync".to_string()]);
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(RelayConfig::from_json("not json").is_err());
    }
}
