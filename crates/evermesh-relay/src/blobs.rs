//! The optional blob sidecar (spec 006 §5.2).
//!
//! `PUT /blob` streams the request body to a temp file while hashing
//! it with BLAKE3, then moves it into a content-addressed path
//! `dir/<hex[0..2]>/<hex[2..4]>/<hex>` once the hash is known — the
//! server never trusts a client-declared id, it always derives its own.
//! `GET`/`HEAD /blob/{id}` serve it back, with single-range support.
//! `GET /blob/{id}/proof?chunk=i` serves a chunk-tree range proof; the
//! one call into the kernel's (not-yet-merged) `ChunkTree` API is kept
//! isolated in [`compute_chunk_proof`] per the build brief, so a
//! mismatch between this sketch and the real API is a one-function fix.

use std::path::{Path as StdPath, PathBuf};

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use evermesh_kernel::BlobId;

use crate::AppState;

/// Query params for the chunk-proof endpoint: `?chunk=<index>`.
#[derive(Debug, Deserialize)]
pub struct ProofQuery {
    pub chunk: u64,
}

/// `PUT /blob` handler.
pub async fn put_blob(State(state): State<AppState>, headers: HeaderMap, body: Body) -> Response {
    if !state.config.blob.enabled {
        return (StatusCode::NOT_FOUND, "blob sidecar disabled").into_response();
    }
    let expected = headers
        .get("x-expected-blob-id")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_blob_id);

    match receive_and_store_blob(&state, body, expected).await {
        Ok(BlobPutOutcome::Stored(id)) => {
            (StatusCode::CREATED, Json(json!({ "id": id.to_uri() }))).into_response()
        }
        Ok(BlobPutOutcome::TooLarge) => {
            (StatusCode::PAYLOAD_TOO_LARGE, "blob exceeds max_bytes").into_response()
        }
        Ok(BlobPutOutcome::HashMismatch) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "declared blob id does not match content",
        )
            .into_response(),
        Err(e) => {
            tracing::warn!("blob PUT failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

/// `GET /blob/{id}` handler.
pub async fn get_blob(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    headers: HeaderMap,
) -> Response {
    serve_blob(&state, &id_str, &headers, true).await
}

/// `HEAD /blob/{id}` handler: identical to `GET` but never sends a body.
pub async fn head_blob(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    headers: HeaderMap,
) -> Response {
    serve_blob(&state, &id_str, &headers, false).await
}

/// `GET /blob/{id}/proof?chunk=i` handler.
pub async fn get_blob_proof(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Query(q): Query<ProofQuery>,
) -> Response {
    if !state.config.blob.enabled {
        return (StatusCode::NOT_FOUND, "blob sidecar disabled").into_response();
    }
    let Some(id) = parse_blob_id(&id_str) else {
        return (StatusCode::NOT_FOUND, "malformed blob id").into_response();
    };
    let path = blob_path(&state.config.blob.dir, &id);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::NOT_FOUND, "blob not found").into_response(),
    };
    match compute_chunk_proof(&bytes, q.chunk) {
        Ok((index, siblings)) => {
            let cbor = crate::frames::encode_chunk_proof(index, &siblings);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/cbor")],
                cbor,
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("blob proof failed: {e}");
            (StatusCode::BAD_REQUEST, "invalid chunk index").into_response()
        }
    }
}

// ---------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------

#[derive(Debug)]
enum BlobPutOutcome {
    Stored(BlobId),
    TooLarge,
    HashMismatch,
}

/// Stream `body` to a temp file while hashing it, then move it into its
/// content-addressed final path. Never buffers the whole blob in memory
/// at once, and aborts as soon as `max_bytes` is exceeded rather than
/// after reading it all.
async fn receive_and_store_blob(
    state: &AppState,
    body: Body,
    expected: Option<BlobId>,
) -> anyhow::Result<BlobPutOutcome> {
    let max_bytes = state.config.blob.max_bytes;
    tokio::fs::create_dir_all(&state.config.blob.dir).await?;

    let tmp = tempfile::NamedTempFile::new_in(&state.config.blob.dir)?;
    let (std_file, tmp_path) = tmp.into_parts();
    let mut file = tokio::fs::File::from_std(std_file);
    let mut hasher = blake3::Hasher::new();
    let mut total: u64 = 0;

    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        total += chunk.len() as u64;
        if total > max_bytes {
            // tmp_path drops here, cleaning up the partial temp file.
            return Ok(BlobPutOutcome::TooLarge);
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;

    let id = BlobId(*hasher.finalize().as_bytes());
    if let Some(expected_id) = expected {
        if expected_id != id {
            return Ok(BlobPutOutcome::HashMismatch);
        }
    }

    let dest = blob_path(&state.config.blob.dir, &id);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Idempotent: if another upload already produced this exact blob,
    // keep the existing file and drop the new temp file.
    if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
        return Ok(BlobPutOutcome::Stored(id));
    }
    tokio::task::spawn_blocking(move || tmp_path.persist(dest)).await??;
    Ok(BlobPutOutcome::Stored(id))
}

fn blob_path(dir: &str, id: &BlobId) -> PathBuf {
    let hex = id.to_hex();
    StdPath::new(dir)
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(&hex)
}

fn parse_blob_id(s: &str) -> Option<BlobId> {
    BlobId::from_uri(s).or_else(|| BlobId::from_hex(s))
}

// ---------------------------------------------------------------------
// Serving (GET/HEAD, single-range)
// ---------------------------------------------------------------------

async fn serve_blob(
    state: &AppState,
    id_str: &str,
    headers: &HeaderMap,
    include_body: bool,
) -> Response {
    if !state.config.blob.enabled {
        return (StatusCode::NOT_FOUND, "blob sidecar disabled").into_response();
    }
    let Some(id) = parse_blob_id(id_str) else {
        return (StatusCode::NOT_FOUND, "malformed blob id").into_response();
    };
    let path = blob_path(&state.config.blob.dir, &id);
    let meta = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "blob not found").into_response(),
    };
    let total_len = meta.len();

    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let range = range_header.and_then(|s| parse_range(s, total_len));
    if range_header.is_some() && range.is_none() {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total_len}"))
            .body(Body::empty())
            .expect("static response is well-formed");
    }

    let (status, start, len) = match range {
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e - s + 1),
        None => (StatusCode::OK, 0, total_len),
    };

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, len.to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{total_len}", start + len - 1),
        );
    }

    if !include_body {
        return builder
            .body(Body::empty())
            .expect("static response is well-formed");
    }

    match read_range(&path, start, len).await {
        Ok(bytes) => builder
            .body(Body::from(bytes))
            .expect("static response is well-formed"),
        Err(e) => {
            tracing::warn!("blob GET read failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "read error").into_response()
        }
    }
}

/// Parse a single `Range: bytes=...` value against a known total
/// length. Returns `None` for anything unsatisfiable, malformed, or a
/// multi-range request (only single ranges are supported, per the
/// build brief).
fn parse_range(header: &str, total_len: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (start_s, end_s) = spec.split_once('-')?;
    if start_s.is_empty() {
        // Suffix range: `bytes=-N` means the last N bytes.
        let suffix: u64 = end_s.parse().ok()?;
        if suffix == 0 || total_len == 0 {
            return None;
        }
        let suffix = suffix.min(total_len);
        return Some((total_len - suffix, total_len - 1));
    }
    let start: u64 = start_s.parse().ok()?;
    if start >= total_len {
        return None;
    }
    let end: u64 = if end_s.is_empty() {
        total_len.saturating_sub(1)
    } else {
        end_s.parse().ok()?
    };
    if end < start {
        return None;
    }
    Some((start, end.min(total_len.saturating_sub(1))))
}

async fn read_range(path: &StdPath, start: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut file = tokio::fs::File::open(path).await?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf).await?;
    Ok(buf)
}

// ---------------------------------------------------------------------
// Chunk-tree range proofs (kernel API call isolated here)
// ---------------------------------------------------------------------

/// The single call site into `evermesh_kernel::ChunkTree` (spec 001 §8).
/// Everything else in this module is agnostic to the kernel's exact
/// chunk-tree API surface; if it differs from the guaranteed sketch
/// (`ChunkTree::from_bytes`, `ChunkTree::prove`, `blob::CHUNK_SIZE`),
/// only this function needs to change.
fn compute_chunk_proof(
    blob_bytes: &[u8],
    chunk_index: u64,
) -> anyhow::Result<(u64, Vec<[u8; 32]>)> {
    use evermesh_kernel::blob::CHUNK_SIZE;
    use evermesh_kernel::ChunkTree;

    let total_chunks: u64 = if blob_bytes.is_empty() {
        0
    } else {
        (blob_bytes.len() as u64).div_ceil(CHUNK_SIZE as u64)
    };
    if chunk_index >= total_chunks {
        anyhow::bail!("chunk index {chunk_index} out of range ({total_chunks} chunks total)");
    }

    let tree = ChunkTree::from_bytes(blob_bytes);
    let siblings = tree
        .prove(chunk_index as usize)
        .map_err(|e| anyhow::anyhow!("kernel chunk proof failed: {e}"))?;
    Ok((chunk_index, siblings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RelayConfig;
    use crate::store::Store;

    fn test_state(dir: &std::path::Path, max_bytes: u64) -> AppState {
        let mut config = RelayConfig::default();
        config.blob.enabled = true;
        config.blob.dir = dir.to_string_lossy().to_string();
        config.blob.max_bytes = max_bytes;
        let store = Store::open_in_memory().unwrap();
        AppState::new(config, store)
    }

    #[test]
    fn blob_path_is_content_addressed_and_sharded() {
        let id = BlobId([0xABu8; 32]);
        let path = blob_path("blobs", &id);
        let hex = id.to_hex();
        assert_eq!(
            path,
            std::path::PathBuf::from("blobs")
                .join(&hex[0..2])
                .join(&hex[2..4])
                .join(&hex)
        );
    }

    #[test]
    fn parse_blob_id_accepts_hex_and_uri_forms() {
        let id = BlobId([0x11u8; 32]);
        let hex = id.to_hex();
        assert_eq!(parse_blob_id(&hex), Some(id));
        assert_eq!(parse_blob_id(&format!("b3-256:{hex}")), Some(id));
        assert_eq!(parse_blob_id("not-a-hash"), None);
    }

    #[test]
    fn parse_range_basic_and_edge_cases() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=100-", 1000), Some((100, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=1000-2000", 1000), None);
        assert_eq!(parse_range("bytes=0-99,200-299", 1000), None);
        assert_eq!(parse_range("nonsense", 1000), None);
        assert_eq!(parse_range("bytes=50-10", 1000), None);
        assert_eq!(parse_range("bytes=900-10000", 1000), Some((900, 999)));
    }

    #[tokio::test]
    async fn put_then_read_back_matches_and_hash_is_derived_not_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path(), 1024);
        let payload = b"hello evermesh blob".to_vec();

        let outcome = receive_and_store_blob(&state, Body::from(payload.clone()), None)
            .await
            .unwrap();
        let id = match outcome {
            BlobPutOutcome::Stored(id) => id,
            other => panic!("expected Stored, got {other:?}"),
        };

        let expected_hash = *blake3::hash(&payload).as_bytes();
        assert_eq!(id.as_bytes(), &expected_hash);

        let path = blob_path(&state.config.blob.dir, &id);
        let stored = tokio::fs::read(&path).await.unwrap();
        assert_eq!(stored, payload);
    }

    #[tokio::test]
    async fn put_over_max_bytes_is_rejected_mid_stream() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path(), 4);
        let outcome = receive_and_store_blob(
            &state,
            Body::from(b"this is definitely more than four bytes".to_vec()),
            None,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, BlobPutOutcome::TooLarge));
    }

    #[tokio::test]
    async fn put_with_mismatched_declared_id_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path(), 1024);
        let wrong = BlobId([0xEEu8; 32]);
        let outcome = receive_and_store_blob(&state, Body::from(b"content".to_vec()), Some(wrong))
            .await
            .unwrap();
        assert!(matches!(outcome, BlobPutOutcome::HashMismatch));
    }

    #[tokio::test]
    async fn duplicate_put_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(dir.path(), 1024);
        let payload = b"same bytes twice".to_vec();
        let first = receive_and_store_blob(&state, Body::from(payload.clone()), None)
            .await
            .unwrap();
        let second = receive_and_store_blob(&state, Body::from(payload.clone()), None)
            .await
            .unwrap();
        match (first, second) {
            (BlobPutOutcome::Stored(a), BlobPutOutcome::Stored(b)) => assert_eq!(a, b),
            other => panic!("expected two Stored outcomes, got {other:?}"),
        }
    }
}
