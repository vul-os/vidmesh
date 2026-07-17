//! Executes vectors in-process against `vidmesh-kernel` (the `kernel`
//! runner target).
//!
//! This is the reference target: every other target (`node`, `relay`)
//! is judged against the same vectors this module checks directly
//! against the Rust kernel's public API.

use vidmesh_kernel::blob::{leaf_hash, verify_chunk, CHUNK_SIZE};
use vidmesh_kernel::bundle::Bundle;
use vidmesh_kernel::codec;
use vidmesh_kernel::identity::Identity;
use vidmesh_kernel::ids::RecordId;
use vidmesh_kernel::record::Record;

use crate::vectors::{error_class, IdentityExpected, Layer, Vector, VectorData};

/// The result of checking one vector against a runtime.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The vector's expectation held.
    Pass,
    /// The vector's expectation did not hold; the string explains how.
    Fail(String),
    /// Not checked against this target/configuration. Not a failure —
    /// e.g. a `layer: kind` vector when kind-level validation is not
    /// wired into this build (see the module docs and the `// kinds`
    /// comment below).
    Skip(String),
}

impl Outcome {
    pub fn is_fail(&self) -> bool {
        matches!(self, Outcome::Fail(_))
    }
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    hex::decode(s).map_err(|e| format!("bad hex in fixture: {e}"))
}

/// Run one vector against the in-process kernel.
pub fn run(v: &Vector) -> Outcome {
    match &v.data {
        VectorData::RecordValid {
            cbor_hex,
            expected_id_hex,
            json,
        } => run_record_valid(cbor_hex, expected_id_hex, json),
        VectorData::RecordInvalid {
            cbor_hex,
            expected_error,
            layer,
        } => run_record_invalid(cbor_hex, expected_error, *layer),
        VectorData::ChunkProof {
            n_chunks,
            last_chunk_len,
            chunk_index,
            proof_hex,
            root_hex,
            valid,
        } => run_chunk_proof(
            *n_chunks,
            *last_chunk_len,
            *chunk_index,
            proof_hex,
            root_hex,
            *valid,
        ),
        VectorData::IdentityChain {
            records_hex,
            now,
            observed,
            expected,
            expected_error,
        } => run_identity_chain(records_hex, *now, observed, expected, expected_error),
        VectorData::Bundle {
            bundle_hex,
            expected,
            expected_error,
        } => run_bundle(bundle_hex, expected, expected_error),
        VectorData::JsonRoundtrip {
            json,
            expected_cbor_hex,
            expected_error,
        } => run_json_roundtrip(json, expected_cbor_hex, expected_error),
    }
}

fn run_record_valid(cbor_hex: &str, expected_id_hex: &str, json: &serde_json::Value) -> Outcome {
    let bytes = match hex_decode(cbor_hex) {
        Ok(b) => b,
        Err(e) => return Outcome::Fail(e),
    };
    let record = match Record::from_cbor(&bytes) {
        Ok(r) => r,
        Err(e) => return Outcome::Fail(format!("expected valid record, from_cbor failed: {e}")),
    };
    if let Err(e) = record.verify() {
        return Outcome::Fail(format!("expected valid record, verify() failed: {e}"));
    }
    let id_hex = record.id().to_hex();
    if id_hex != expected_id_hex {
        return Outcome::Fail(format!(
            "id mismatch: got {id_hex}, expected {expected_id_hex}"
        ));
    }
    // JSON interchange: the record's own rendering must equal the
    // fixture's embedded JSON, and re-parsing it must reproduce the
    // same canonical bytes (spec 001 §11).
    let rendered: serde_json::Value = match serde_json::from_str(&record.to_json()) {
        Ok(v) => v,
        Err(e) => return Outcome::Fail(format!("record.to_json() is not valid JSON: {e}")),
    };
    if &rendered != json {
        return Outcome::Fail(format!(
            "json interchange mismatch: got {rendered}, expected {json}"
        ));
    }
    match Record::from_json(&record.to_json()) {
        Ok(back) if back.id() == record.id() => Outcome::Pass,
        Ok(_) => Outcome::Fail("json round trip produced a different record id".into()),
        Err(e) => Outcome::Fail(format!("json round trip failed to re-parse: {e}")),
    }
}

fn run_record_invalid(cbor_hex: &str, expected_error: &str, layer: Layer) -> Outcome {
    let bytes = match hex_decode(cbor_hex) {
        Ok(b) => b,
        Err(e) => return Outcome::Fail(e),
    };
    let result = Record::from_cbor(&bytes).and_then(|r| r.verify().map(|_| r));
    match layer {
        Layer::Envelope => match result {
            Err(e) => {
                let class = error_class::classify(&e);
                if class == expected_error {
                    Outcome::Pass
                } else {
                    Outcome::Fail(format!(
                        "wrong error class: got {class} ({e}), expected {expected_error}"
                    ))
                }
            }
            Ok(_) => Outcome::Fail("expected rejection, envelope validation accepted it".into()),
        },
        Layer::Kind => match result {
            Ok(record) => match vidmesh_kernel::kinds::validate(&record) {
                Err(e) => {
                    let class = error_class::classify(&e);
                    if class == expected_error {
                        Outcome::Pass
                    } else {
                        Outcome::Fail(format!(
                            "wrong error class: got {class} ({e}), expected {expected_error}"
                        ))
                    }
                }
                Ok(()) => {
                    Outcome::Fail("expected kind-level rejection, validate() accepted it".into())
                }
            },
            Err(e) => Outcome::Fail(format!(
                "kind-layer vector rejected at envelope layer instead ({e}); \
                 fixture or kernel envelope rules changed"
            )),
        },
    }
}

/// Reconstruct the synthetic blob's bytes from the vector's compact
/// description (task-specified formula: chunk `i` filled with
/// `(i % 251)` for its whole length).
fn synth_blob(n_chunks: u64, last_chunk_len: u64) -> Vec<u8> {
    if n_chunks == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..n_chunks {
        let len = if i + 1 == n_chunks {
            last_chunk_len as usize
        } else {
            CHUNK_SIZE
        };
        out.resize(out.len() + len, (i % 251) as u8);
    }
    out
}

fn run_chunk_proof(
    n_chunks: u64,
    last_chunk_len: u64,
    chunk_index: u64,
    proof_hex: &[String],
    root_hex: &str,
    valid: bool,
) -> Outcome {
    let blob = synth_blob(n_chunks, last_chunk_len);
    let chunk_start = (chunk_index as usize) * CHUNK_SIZE;
    let chunk_end = if chunk_index + 1 == n_chunks {
        blob.len()
    } else {
        chunk_start + CHUNK_SIZE
    };
    let chunk = match blob.get(chunk_start..chunk_end) {
        Some(c) => c,
        None => return Outcome::Fail("chunk_index out of range for synthesized blob".into()),
    };
    // Sanity: leaf_hash must be usable (exercises the same primitive the
    // kernel's own ChunkTree uses), even though verify_chunk recomputes
    // it internally.
    let _ = leaf_hash(chunk);
    let root: [u8; 32] = match hex_decode(root_hex).and_then(|b| {
        b.try_into()
            .map_err(|_| "root must be 32 bytes".to_string())
    }) {
        Ok(r) => r,
        Err(e) => return Outcome::Fail(e),
    };
    let mut proof = Vec::with_capacity(proof_hex.len());
    for h in proof_hex {
        match hex_decode(h).and_then(|b| {
            b.try_into()
                .map_err(|_| "sibling must be 32 bytes".to_string())
        }) {
            Ok(s) => proof.push(s),
            Err(e) => return Outcome::Fail(e),
        }
    }
    let result = verify_chunk(
        &root,
        n_chunks as usize,
        chunk_index as usize,
        chunk,
        &proof,
    );
    match (result.is_ok(), valid) {
        (true, true) | (false, false) => Outcome::Pass,
        (true, false) => {
            Outcome::Fail("verify_chunk succeeded but the vector expects it to fail".into())
        }
        (false, true) => Outcome::Fail(format!(
            "verify_chunk failed but the vector expects success: {}",
            result.unwrap_err()
        )),
    }
}

fn run_identity_chain(
    records_hex: &[String],
    now: i64,
    observed: &std::collections::BTreeMap<String, i64>,
    expected: &Option<IdentityExpected>,
    expected_error: &Option<String>,
) -> Outcome {
    let mut records = Vec::with_capacity(records_hex.len());
    for h in records_hex {
        let bytes = match hex_decode(h) {
            Ok(b) => b,
            Err(e) => return Outcome::Fail(e),
        };
        match Record::from_cbor(&bytes) {
            Ok(r) => records.push(r),
            Err(e) => return Outcome::Fail(format!("fixture record failed to decode: {e}")),
        }
    }
    let observed_at = |id: &RecordId| -> Option<i64> { observed.get(&id.to_hex()).copied() };
    let result = Identity::verify_chain(&records, &observed_at, now);
    match (result, expected, expected_error) {
        (_, None, None) => {
            Outcome::Fail("vector has neither expected nor expected_error (fixture bug)".into())
        }
        (Ok(state), Some(exp), _) => {
            let head_hex = state.head.to_hex();
            let signing_key_hex = hex::encode(&state.signing_key);
            if head_hex != exp.head_hex {
                Outcome::Fail(format!(
                    "head mismatch: got {head_hex}, expected {}",
                    exp.head_hex
                ))
            } else if signing_key_hex != exp.signing_key_hex {
                Outcome::Fail(format!(
                    "signing key mismatch: got {signing_key_hex}, expected {}",
                    exp.signing_key_hex
                ))
            } else if state.depth != exp.depth {
                Outcome::Fail(format!(
                    "depth mismatch: got {}, expected {}",
                    state.depth, exp.depth
                ))
            } else {
                Outcome::Pass
            }
        }
        (Ok(_), None, Some(_)) => Outcome::Fail("expected an error, verify_chain succeeded".into()),
        (Err(e), _, Some(expected_class)) => {
            let class = error_class::classify(&e);
            if class == expected_class {
                Outcome::Pass
            } else {
                Outcome::Fail(format!(
                    "wrong error class: got {class} ({e}), expected {expected_class}"
                ))
            }
        }
        (Err(e), Some(_), None) => {
            Outcome::Fail(format!("expected success, verify_chain failed: {e}"))
        }
    }
}

fn run_bundle(
    bundle_hex: &str,
    expected: &Option<crate::vectors::BundleExpected>,
    expected_error: &Option<String>,
) -> Outcome {
    let bytes = match hex_decode(bundle_hex) {
        Ok(b) => b,
        Err(e) => return Outcome::Fail(e),
    };
    let import_result = Bundle::import(&bytes[..]);
    let (result, expected) = match (import_result, expected, expected_error) {
        (_, None, None) | (Ok(_), Some(_), Some(_)) => {
            return Outcome::Fail(
                "vector must have exactly one of expected / expected_error".into(),
            )
        }
        (Err(e), _, Some(expected_class)) => {
            let class = error_class::classify(&e);
            return if class == expected_class {
                Outcome::Pass
            } else {
                Outcome::Fail(format!(
                    "wrong error class: got {class} ({e}), expected {expected_class}"
                ))
            };
        }
        (Err(e), Some(_), None) => {
            return Outcome::Fail(format!(
                "Bundle::import returned an error (should salvage, not error): {e}"
            ))
        }
        (Ok(_), None, Some(_)) => {
            return Outcome::Fail(
                "expected Bundle::import to fail at the container level, it succeeded".into(),
            )
        }
        (Ok(r), Some(exp), None) => (r, exp),
    };
    let mut got_records: Vec<String> = result.records.iter().map(|r| r.id().to_hex()).collect();
    got_records.sort();
    let mut want_records = expected.record_ids.clone();
    want_records.sort();
    if got_records != want_records {
        return Outcome::Fail(format!(
            "record ids mismatch: got {got_records:?}, expected {want_records:?}"
        ));
    }
    let mut got_blobs: Vec<String> = result.blobs.iter().map(|(id, _)| id.to_hex()).collect();
    got_blobs.sort();
    let mut want_blobs = expected.blob_ids.clone();
    want_blobs.sort();
    if got_blobs != want_blobs {
        return Outcome::Fail(format!(
            "blob ids mismatch: got {got_blobs:?}, expected {want_blobs:?}"
        ));
    }
    if result.skipped.len() != expected.skipped_count {
        return Outcome::Fail(format!(
            "skipped count mismatch: got {} ({:?}), expected {}",
            result.skipped.len(),
            result.skipped,
            expected.skipped_count
        ));
    }
    if result.truncated != expected.truncated {
        return Outcome::Fail(format!(
            "truncated mismatch: got {}, expected {}",
            result.truncated, expected.truncated
        ));
    }
    Outcome::Pass
}

fn run_json_roundtrip(
    json: &str,
    expected_cbor_hex: &Option<String>,
    expected_error: &Option<String>,
) -> Outcome {
    let parsed = codec::from_json(json);
    match (parsed, expected_cbor_hex, expected_error) {
        (_, None, None) => Outcome::Fail(
            "vector has neither expected_cbor_hex nor expected_error (fixture bug)".into(),
        ),
        (Ok(value), Some(want_hex), _) => match codec::encode_canonical(&value) {
            Ok(bytes) => {
                let got_hex = hex::encode(&bytes);
                if &got_hex == want_hex {
                    Outcome::Pass
                } else {
                    Outcome::Fail(format!("cbor mismatch: got {got_hex}, expected {want_hex}"))
                }
            }
            Err(e) => Outcome::Fail(format!("expected successful encode, got error: {e}")),
        },
        (Ok(_), None, Some(_)) => Outcome::Fail("expected rejection, from_json succeeded".into()),
        (Err(e), _, Some(expected_class)) => {
            let class = error_class::classify(&e);
            if class == expected_class {
                Outcome::Pass
            } else {
                Outcome::Fail(format!(
                    "wrong error class: got {class} ({e}), expected {expected_class}"
                ))
            }
        }
        (Err(e), Some(_), None) => {
            Outcome::Fail(format!("expected success, from_json failed: {e}"))
        }
    }
}
