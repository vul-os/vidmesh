//! The `/sync` websocket handler (spec 006 §1–§4, §6): subscriptions,
//! stored backfill, live delivery, and `PUB` ingest.
//!
//! Each connection owns its own subscription registry (`sub_id ->
//! Filter`); there is no cross-connection state here beyond
//! [`crate::AppState`]. Live fan-out is a `tokio::sync::broadcast`
//! subscription shared with gossip (see [`crate::AppState::accepted_tx`]):
//! every connection task filters the same stream of accepted records
//! against its own subscriptions.
//!
//! This is the first of the two kernel-dependent verification points:
//! every `PUB` is decoded and verified via `vidmesh_kernel::Record`
//! before it touches storage or gossip.

use std::collections::HashMap;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use vidmesh_kernel::Record;

use crate::filter::Filter;
use crate::frames::{ClientFrame, FrameError, RelayFrame};
use crate::pow;
use crate::store::InsertOutcome;
use crate::{AcceptedRecord, AppState};

/// Stored-phase backfill cap applied when a `REQ`'s filter does not
/// specify `limit` itself (spec §3: `limit` is OPTIONAL). Prevents an
/// unbounded filter from dumping the entire store down one connection.
const DEFAULT_BACKFILL_LIMIT: u64 = 500;

/// Axum handler mounted at `GET /sync`.
pub async fn sync_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut subs: HashMap<String, Filter> = HashMap::new();
    let mut accepted_rx = state.accepted_tx.subscribe();

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Binary(bytes))) => {
                        handle_client_frame(&bytes, &state, &mut subs, &mut sender).await;
                    }
                    Some(Ok(Message::Text(_))) => {
                        // Spec §1: text frames MUST be ignored.
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => { /* ping/pong: axum answers these itself */ }
                    Some(Err(e)) => {
                        debug!("sync: websocket read error: {e}");
                        break;
                    }
                }
            }
            accepted = accepted_rx.recv() => {
                match accepted {
                    Ok(rec) => deliver_live(&rec, &subs, &mut sender).await,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("sync: subscriber fell behind, skipped {n} accepted records");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn handle_client_frame(
    bytes: &[u8],
    state: &AppState,
    subs: &mut HashMap<String, Filter>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let frame = match ClientFrame::decode(bytes) {
        Ok(f) => f,
        Err(e) => {
            // Spec §1: unknown frame types get `CLOSED` "for unknown
            // request types"; we cannot in general recover a sub_id
            // from a frame that failed to parse at all (e.g. bad
            // CBOR), so the safe, non-crashing fallback is to log and
            // drop the frame. See the crate README "spec gaps" note.
            log_frame_error(&e);
            return;
        }
    };

    match frame {
        ClientFrame::Req { sub_id, filter } => {
            handle_req(sub_id, filter, state, subs, sender).await;
        }
        ClientFrame::Close { sub_id } => {
            subs.remove(&sub_id);
        }
        ClientFrame::Pub {
            record_bytes,
            nonce,
        } => {
            handle_pub(record_bytes, nonce, state, sender).await;
        }
    }
}

fn log_frame_error(e: &FrameError) {
    match e {
        FrameError::UnknownFrameTag(tag) => debug!("sync: ignoring unknown frame tag {tag:?}"),
        other => warn!("sync: malformed client frame: {other}"),
    }
}

async fn handle_req(
    sub_id: String,
    filter: Filter,
    state: &AppState,
    subs: &mut HashMap<String, Filter>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    subs.insert(sub_id.clone(), filter.clone());

    let store = state.store.clone();
    let backfill_filter = filter;
    let rows =
        tokio::task::spawn_blocking(move || store.query(&backfill_filter, DEFAULT_BACKFILL_LIMIT))
            .await;
    let rows = match rows {
        Ok(Ok(rows)) => rows,
        Ok(Err(e)) => {
            warn!("sync: store query failed: {e}");
            Vec::new()
        }
        Err(e) => {
            warn!("sync: store query task panicked: {e}");
            Vec::new()
        }
    };

    for row in rows {
        send_frame(
            sender,
            &RelayFrame::Rec {
                sub_id: sub_id.clone(),
                seq: row.seq,
                record_bytes: row.bytes,
            },
        )
        .await;
    }
    send_frame(sender, &RelayFrame::Eose { sub_id }).await;
}

/// Handle a `PUB`: this is the crate's first kernel-dependent
/// verification point. `Record::from_cbor` performs the strict
/// canonical decode (envelope shape, spec 001 §1–§2); `Record::verify`
/// checks the signature and id (spec 001 §3–§4). Only after both pass
/// do PoW, rate limiting, and storage run.
async fn handle_pub(
    record_bytes: Vec<u8>,
    nonce: Option<u64>,
    state: &AppState,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    let record = match Record::from_cbor(&record_bytes) {
        Ok(r) => r,
        Err(e) => {
            warn!("sync: PUB envelope decode failed: {e}");
            // Spec's `OK` frame requires an `id: bytes(32)` regardless
            // of outcome, but bytes that fail to decode have no
            // derivable id. Reporting the zero id here is a documented
            // placeholder pending spec guidance (crate README "spec
            // gaps"); it is never a valid record id in practice.
            send_frame(
                sender,
                &RelayFrame::Ok {
                    id: [0u8; 32],
                    accepted: false,
                    reason: format!("invalid envelope: {e}"),
                },
            )
            .await;
            return;
        }
    };
    let id = *record.id().as_bytes();

    if let Err(e) = record.verify() {
        send_frame(
            sender,
            &RelayFrame::Ok {
                id,
                accepted: false,
                reason: format!("verification failed: {e}"),
            },
        )
        .await;
        return;
    }

    if !pow::check(&id, nonce, state.config.pow_min_bits) {
        send_frame(
            sender,
            &RelayFrame::Ok {
                id,
                accepted: false,
                reason: "pow".to_string(),
            },
        )
        .await;
        return;
    }

    let author = record.author_identity_id();
    if !state.rate_limiter.check(&author) {
        send_frame(
            sender,
            &RelayFrame::Ok {
                id,
                accepted: false,
                reason: "rate".to_string(),
            },
        )
        .await;
        return;
    }

    let kind = record.kind();
    let ref_hashes = record.ref_hashes();
    let received_at = current_unix_time();

    let store = state.store.clone();
    let bytes_for_store = record_bytes.clone();
    let refs_for_store = ref_hashes.clone();
    let insert_result = tokio::task::spawn_blocking(move || {
        store.insert_record(
            &id,
            kind,
            &author,
            received_at,
            &refs_for_store,
            &bytes_for_store,
        )
    })
    .await;

    match insert_result {
        Ok(Ok(InsertOutcome::Duplicate)) => {
            // Spec §4: OK(true) but not re-stored, re-sequenced, or
            // re-gossiped.
            send_frame(
                sender,
                &RelayFrame::Ok {
                    id,
                    accepted: true,
                    reason: String::new(),
                },
            )
            .await;
        }
        Ok(Ok(InsertOutcome::Inserted(seq))) => {
            send_frame(
                sender,
                &RelayFrame::Ok {
                    id,
                    accepted: true,
                    reason: String::new(),
                },
            )
            .await;
            let accepted = AcceptedRecord {
                id,
                kind,
                author,
                ref_hashes,
                seq,
                bytes: record_bytes,
            };
            // No receivers (no live subs, gossip disabled) is not an
            // error; ignore it.
            let _ = state.accepted_tx.send(accepted);
        }
        Ok(Err(e)) => {
            warn!("sync: store insert failed: {e}");
            send_frame(
                sender,
                &RelayFrame::Ok {
                    id,
                    accepted: false,
                    reason: "internal error".to_string(),
                },
            )
            .await;
        }
        Err(e) => {
            warn!("sync: store insert task panicked: {e}");
            send_frame(
                sender,
                &RelayFrame::Ok {
                    id,
                    accepted: false,
                    reason: "internal error".to_string(),
                },
            )
            .await;
        }
    }
}

async fn deliver_live(
    rec: &AcceptedRecord,
    subs: &HashMap<String, Filter>,
    sender: &mut SplitSink<WebSocket, Message>,
) {
    for (sub_id, filter) in subs {
        if filter.matches(rec.kind, &rec.author, &rec.id, &rec.ref_hashes, rec.seq) {
            send_frame(
                sender,
                &RelayFrame::Rec {
                    sub_id: sub_id.clone(),
                    seq: rec.seq,
                    record_bytes: rec.bytes.clone(),
                },
            )
            .await;
        }
    }
}

async fn send_frame(sender: &mut SplitSink<WebSocket, Message>, frame: &RelayFrame) {
    if let Err(e) = sender.send(Message::Binary(frame.encode())).await {
        debug!("sync: send failed (client likely disconnected): {e}");
    }
}

/// Current Unix time in seconds, used as `received_at`. Never panics:
/// a clock before 1970 (practically impossible) just yields `0`.
pub(crate) fn current_unix_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_unix_time_is_positive_and_recent() {
        // A loose sanity check: this file was written after 2024-01-01.
        assert!(current_unix_time() > 1_700_000_000);
    }
}
