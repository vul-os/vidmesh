//! Outbound gossip to peer relays (spec 006 §8).
//!
//! For each configured peer this module maintains one client `/sync`
//! connection: it subscribes with the relay's own ingest filter (here,
//! always empty — "often empty" per spec), forwards every locally
//! accepted record to the peer via `PUB`, and feeds every record
//! received from the peer through the same envelope-validate path used
//! for local `PUB`s. Loop suppression is automatic: ingest is keyed by
//! record id ([`crate::store::Store::insert_record`] is idempotent), so
//! a record bouncing back around a gossip cycle is simply a duplicate
//! insert that is not re-broadcast — "topology cycles are harmless."
//!
//! This is the crate's second kernel-dependent verification point:
//! records arriving from a peer are decoded and verified exactly like
//! a local `PUB`, never trusted merely because they came from a
//! configured peer.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{info, warn};

use vidmesh_kernel::Record;

use crate::filter::Filter;
use crate::frames::{ClientFrame, RelayFrame};
use crate::store::InsertOutcome;
use crate::sync::current_unix_time;
use crate::{AcceptedRecord, AppState};

const RECONNECT_MIN: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(60);

/// The `sub_id` this relay uses for its own gossip ingest subscription
/// on every peer connection. Peer-local; never seen by other clients.
const GOSSIP_SUB_ID: &str = "gossip";

/// Spawn one background task per configured peer that reconnects with
/// exponential backoff for the life of the process. Call once at
/// startup, after the listener is up.
pub fn spawn_all(state: AppState) {
    for peer in state.config.peers.clone() {
        let state = state.clone();
        tokio::spawn(async move {
            run_peer_forever(peer, state).await;
        });
    }
}

async fn run_peer_forever(peer_url: String, state: AppState) {
    let mut backoff = RECONNECT_MIN;
    loop {
        match run_peer_once(&peer_url, &state).await {
            Ok(()) => backoff = RECONNECT_MIN,
            Err(e) => warn!("gossip: {peer_url}: {e}"),
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(RECONNECT_MAX);
    }
}

async fn run_peer_once(peer_url: &str, state: &AppState) -> anyhow::Result<()> {
    info!("gossip: connecting to {peer_url}");
    let (ws_stream, _response) = tokio_tungstenite::connect_async(peer_url).await?;
    let (mut write, mut read) = ws_stream.split();

    let req = ClientFrame::Req {
        sub_id: GOSSIP_SUB_ID.to_string(),
        filter: Filter::default(),
    };
    write.send(WsMessage::Binary(req.encode())).await?;

    let mut accepted_rx = state.accepted_tx.subscribe();

    loop {
        tokio::select! {
            incoming = read.next() => {
                match incoming {
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        if let Err(e) = ingest_from_peer(&bytes, state).await {
                            warn!("gossip: {peer_url}: failed to ingest peer record: {e}");
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => return Ok(()),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                }
            }
            accepted = accepted_rx.recv() => {
                match accepted {
                    Ok(rec) => {
                        // No PoW nonce: this relay already accepted the
                        // record under its own policy; adding work is
                        // optional and left to a future refinement (a
                        // relay MAY grind extra work before re-gossiping
                        // to a stricter peer, per spec's Decisions).
                        let pub_frame = ClientFrame::Pub { record_bytes: rec.bytes, nonce: None };
                        write.send(WsMessage::Binary(pub_frame.encode())).await?;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!("gossip: {peer_url}: outbox lagged, skipped {n} records");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        }
    }
}

/// Ingest one frame received from a peer's `/sync`. Only `REC` carries
/// a record; everything else (`EOSE`, `OK`, `CLOSED`) is acknowledged
/// by simply doing nothing with it.
async fn ingest_from_peer(bytes: &[u8], state: &AppState) -> anyhow::Result<()> {
    let frame = RelayFrame::decode(bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    let record_bytes = match frame {
        RelayFrame::Rec { record_bytes, .. } => record_bytes,
        _ => return Ok(()),
    };

    // Same envelope-validate step as a local PUB (spec §4): a peer
    // relationship is not a trust shortcut around signature checking.
    let record =
        Record::from_cbor(&record_bytes).map_err(|e| anyhow::anyhow!("invalid envelope: {e}"))?;
    record
        .verify()
        .map_err(|e| anyhow::anyhow!("verification failed: {e}"))?;

    let id = *record.id().as_bytes();
    let kind = record.kind();
    let author = record.author_identity_id();
    let ref_hashes = record.ref_hashes();
    let received_at = current_unix_time();

    // Deliberately no PoW/rate-limit gate here: those are anti-spam
    // controls on direct publishers, not on records a peer relay has
    // already accepted under its own policy. Envelope validity is
    // still mandatory and is what makes gossip safe.
    let store = state.store.clone();
    let bytes_for_store = record_bytes.clone();
    let refs_for_store = ref_hashes.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        store.insert_record(
            &id,
            kind,
            &author,
            received_at,
            &refs_for_store,
            &bytes_for_store,
        )
    })
    .await??;

    if let InsertOutcome::Inserted(seq) = outcome {
        let accepted = AcceptedRecord {
            id,
            kind,
            author,
            ref_hashes,
            seq,
            bytes: record_bytes,
        };
        let _ = state.accepted_tx.send(accepted);
    }
    // InsertOutcome::Duplicate: loop suppression in action, spec §8 —
    // not re-stored, not re-broadcast, not an error.
    Ok(())
}
