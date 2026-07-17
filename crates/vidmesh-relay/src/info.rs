//! `GET /info` — the relay policy document (spec 006 §5.1).

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Map, Value as JsonValue};

use crate::config::RelayConfig;
use crate::AppState;

/// Software identifier advertised in `/info`. The only field the spec
/// requires unconditionally.
pub const SOFTWARE: &str = concat!("vidmesh-relay/", env!("CARGO_PKG_VERSION"));

/// Kind id -> registry name (spec 003 §1), used only to render a
/// human-readable retention key (`"live.chat_days"`) in `/info`. Purely
/// a presentation nicety: the relay itself stores and forwards records
/// by their opaque numeric kind and never interprets the name.
fn kind_name(kind: u64) -> Option<&'static str> {
    Some(match kind {
        1 => "rotation",
        2 => "profile",
        3 => "delegate",
        16 => "manifest",
        17 => "supersede",
        18 => "retract",
        19 => "mirror",
        20 => "similarity",
        32 => "comment",
        33 => "reaction",
        34 => "follow",
        35 => "playlist",
        36 => "channel",
        48 => "claim.author",
        49 => "claim.license",
        50 => "claim.transfer",
        51 => "claim.dispute",
        64 => "notice.takedown",
        65 => "notice.counter",
        66 => "feed.takedown",
        80 => "endorse.gateway",
        81 => "receipt",
        82 => "attest",
        96 => "anchor",
        97 => "keygrant",
        112 => "live.manifest",
        113 => "live.chat",
        _ => return None,
    })
}

/// Build the `/info` JSON document for a given config (spec 006 §5.1
/// worked example). All fields but `software` are OPTIONAL in the
/// spec's sense of "absent means unspecified"; this skeleton always
/// includes them since [`RelayConfig`] always has a value (possibly a
/// default one) for each.
pub fn policy_document(config: &RelayConfig) -> JsonValue {
    let mut retention = Map::new();
    retention.insert(
        "default_days".to_string(),
        json!(config.retention.default_days),
    );
    for (&kind, &days) in &config.retention.by_kind_days {
        let key = match kind_name(kind) {
            Some(name) => format!("{name}_days"),
            None => format!("kind{kind}_days"),
        };
        retention.insert(key, json!(days));
    }

    json!({
        "name": config.name,
        "software": SOFTWARE,
        "pow_min_bits": config.pow_min_bits,
        "rate": { "records_per_minute_per_key": config.rate.records_per_minute_per_key },
        "retention": retention,
        "blob": { "enabled": config.blob.enabled, "max_bytes": config.blob.max_bytes },
        "peers": config.peers,
    })
}

/// `GET /info` handler.
pub async fn get_info(State(state): State<AppState>) -> impl IntoResponse {
    Json(policy_document(&state.config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_spec_worked_example_shape() {
        let mut config = RelayConfig {
            name: "relay.example.net".to_string(),
            pow_min_bits: 8,
            ..RelayConfig::default()
        };
        config.rate.records_per_minute_per_key = 60;
        config.retention.default_days = 365;
        config.retention.by_kind_days.insert(113, 2); // live.chat
        config.blob.enabled = true;
        config.blob.max_bytes = 4_294_967_296;
        config.peers = vec!["wss://relay2.example.org/sync".to_string()];

        let doc = policy_document(&config);
        assert_eq!(doc["name"], "relay.example.net");
        assert!(doc["software"]
            .as_str()
            .unwrap()
            .starts_with("vidmesh-relay/"));
        assert_eq!(doc["pow_min_bits"], 8);
        assert_eq!(doc["rate"]["records_per_minute_per_key"], 60);
        assert_eq!(doc["retention"]["default_days"], 365);
        assert_eq!(doc["retention"]["live.chat_days"], 2);
        assert_eq!(doc["blob"]["enabled"], true);
        assert_eq!(doc["blob"]["max_bytes"], 4_294_967_296u64);
        assert_eq!(doc["peers"][0], "wss://relay2.example.org/sync");
    }

    #[test]
    fn unknown_kind_falls_back_to_numeric_key() {
        let mut config = RelayConfig::default();
        config.retention.by_kind_days.insert(9999, 7);
        let doc = policy_document(&config);
        assert_eq!(doc["retention"]["kind9999_days"], 7);
    }
}
