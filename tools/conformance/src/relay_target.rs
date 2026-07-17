//! The `relay` runner target: connects to a live `vidmesh-relay`'s
//! `WS /sync` (spec 006 §1) and exercises the subset of vectors that
//! are meaningful over the wire: publishing every `record-valid`
//! vector (expecting `OK(true)`), publishing every `record-invalid`
//! vector whose rejection is visible at the envelope layer (expecting
//! `OK(false)`), and one `REQ`/`EOSE` round trip as a connectivity
//! smoke test.
//!
//! Frames are canonical CBOR arrays tagged by a leading text element
//! (spec 006 §1); this module reuses `vidmesh_kernel::codec` for their
//! encoding rather than a second CBOR implementation, exactly as the
//! kernel's own record envelopes do. This keeps the module compiling at
//! all times even when no relay is reachable — connection failures
//! surface as `Err(String)` from [`RelayConn::connect`], not panics; the
//! relay is a runtime flag (`--target relay --relay-url ...`), never a
//! build requirement.

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use vidmesh_kernel::codec::{self, Value};

use crate::kernel_target::Outcome;
use crate::vectors::{Layer, Vector, VectorData};

/// An open `/sync` connection to a relay.
pub struct RelayConn {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

fn encode_frame(items: Vec<Value>) -> Vec<u8> {
    // Frame arrays are small, fixed shapes built entirely in-process, so
    // `encode_canonical` cannot fail (no duplicate map keys are ever
    // constructed here).
    codec::encode_canonical(&Value::Array(items)).expect("frame is always canonically encodable")
}

impl RelayConn {
    /// Connect to `ws://.../sync` (or `wss://`). `url` should already
    /// include the `/sync` path.
    pub async fn connect(url: &str) -> Result<RelayConn, String> {
        let (ws, _response) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| format!("failed to connect to relay at {url}: {e}"))?;
        Ok(RelayConn { ws })
    }

    /// `["PUB", record, null]`, then read frames until the matching
    /// `["OK", id, accepted, reason]` arrives (skipping any interleaved
    /// `REC`s from a previous live subscription, of which this client
    /// has none, so in practice the very next frame).
    pub async fn publish(&mut self, record_bytes: &[u8]) -> Result<(bool, String), String> {
        let frame = encode_frame(vec![
            Value::Text("PUB".into()),
            Value::Bytes(record_bytes.to_vec()),
            Value::Null,
        ]);
        self.ws
            .send(Message::Binary(frame))
            .await
            .map_err(|e| format!("send failed: {e}"))?;
        loop {
            let msg = self
                .ws
                .next()
                .await
                .ok_or("connection closed before an OK frame arrived")?
                .map_err(|e| format!("recv failed: {e}"))?;
            let Message::Binary(bytes) = msg else {
                continue;
            };
            let value = codec::decode_canonical(&bytes)
                .map_err(|e| format!("non-canonical frame from relay: {e}"))?;
            let arr = value.as_array().ok_or("frame is not an array")?;
            if arr.first().and_then(Value::as_text) == Some("OK") {
                let accepted = arr
                    .get(2)
                    .and_then(Value::as_bool)
                    .ok_or("OK frame missing `accepted`")?;
                let reason = arr
                    .get(3)
                    .and_then(Value::as_text)
                    .unwrap_or("")
                    .to_string();
                return Ok((accepted, reason));
            }
        }
    }

    /// `["REQ", sub_id, {}]` then read until `["EOSE", sub_id]`,
    /// followed by `["CLOSE", sub_id]`. A pure connectivity/protocol
    /// smoke test, not tied to any particular vector.
    pub async fn req_roundtrip(&mut self, sub_id: &str) -> Result<(), String> {
        let req = encode_frame(vec![
            Value::Text("REQ".into()),
            Value::Text(sub_id.into()),
            Value::Map(Vec::new()),
        ]);
        self.ws
            .send(Message::Binary(req))
            .await
            .map_err(|e| format!("send failed: {e}"))?;
        loop {
            let msg = self
                .ws
                .next()
                .await
                .ok_or("connection closed before EOSE arrived")?
                .map_err(|e| format!("recv failed: {e}"))?;
            let Message::Binary(bytes) = msg else {
                continue;
            };
            let value = codec::decode_canonical(&bytes)
                .map_err(|e| format!("non-canonical frame from relay: {e}"))?;
            let arr = value.as_array().ok_or("frame is not an array")?;
            match arr.first().and_then(Value::as_text) {
                Some("EOSE") => break,
                Some("REC") => continue, // stored backfill; keep draining
                _ => continue,
            }
        }
        let close = encode_frame(vec![
            Value::Text("CLOSE".into()),
            Value::Text(sub_id.into()),
        ]);
        self.ws
            .send(Message::Binary(close))
            .await
            .map_err(|e| format!("send failed: {e}"))?;
        Ok(())
    }
}

/// Run one vector against an open relay connection.
pub async fn run(conn: &mut RelayConn, v: &Vector) -> Outcome {
    match &v.data {
        VectorData::RecordValid { cbor_hex, .. } => {
            let bytes = match hex::decode(cbor_hex) {
                Ok(b) => b,
                Err(e) => return Outcome::Fail(format!("bad hex in fixture: {e}")),
            };
            match conn.publish(&bytes).await {
                Ok((true, _)) => Outcome::Pass,
                Ok((false, reason)) => {
                    Outcome::Fail(format!("relay rejected a valid record: {reason}"))
                }
                Err(e) => Outcome::Fail(e),
            }
        }
        VectorData::RecordInvalid {
            cbor_hex, layer, ..
        } => {
            if *layer == Layer::Kind {
                return Outcome::Skip(
                    "relays only envelope-validate (spec 006 §4); kind-layer rejections are \
                     invisible over /sync by design"
                        .into(),
                );
            }
            let bytes = match hex::decode(cbor_hex) {
                Ok(b) => b,
                Err(e) => return Outcome::Fail(format!("bad hex in fixture: {e}")),
            };
            match conn.publish(&bytes).await {
                Ok((false, _)) => Outcome::Pass,
                Ok((true, _)) => Outcome::Fail("relay accepted an envelope-invalid record".into()),
                Err(e) => Outcome::Fail(e),
            }
        }
        VectorData::ChunkProof { .. }
        | VectorData::IdentityChain { .. }
        | VectorData::Bundle { .. }
        | VectorData::JsonRoundtrip { .. } => {
            Outcome::Skip("not a wire-protocol concern for /sync; kernel-target-only vector".into())
        }
    }
}
