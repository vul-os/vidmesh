//! The `node` runner target: exercises `@vidmesh/kernel` (TypeScript,
//! `packages/kernel-ts`) by spawning `node node-harness.mjs` and
//! speaking a small newline-delimited JSON protocol over its stdio.
//!
//! # Protocol
//!
//! One JSON object per line, both directions, `\n`-terminated. Every
//! request has an `op` field; every response is either a success object
//! (shape depends on `op`) or `{"error": "<message>", "error_class":
//! "<class>"}` where `<class>` uses the same vocabulary as
//! [`crate::vectors::error_class`] (`cbor`, `non-canonical`,
//! `envelope`, `signature`, `unknown-algorithm`, `kind`, `identity`).
//!
//! | Request | Success response |
//! |---|---|
//! | `{"op":"verify-record","cbor_hex":...}` | `{"ok":true}` |
//! | `{"op":"derive-id","cbor_hex":...}` | `{"id_hex":...}` |
//! | `{"op":"record-to-json","cbor_hex":...}` | `{"json":<value>}` |
//! | `{"op":"record-from-json","json":<value>}` | `{"cbor_hex":...}` |
//! | `{"op":"verify-chunk","root_hex":...,"n_chunks":u64,"index":u64,"chunk_hex":...,"proof_hex":[...]}` | `{"ok":true}` |
//! | `{"op":"identity-verify-chain","records_hex":[...],"now":i64,"observed":{<id_hex>:seconds}}` | `{"head_hex":...,"signing_key_hex":...,"depth":u64}` |
//!
//! `@vidmesh/kernel` only exposes record-level and chunk/identity
//! helpers (see `packages/kernel-ts/src/index.ts`), not a generic
//! CBOR-Value-level JSON codec or bundle import/export, so
//! `json-roundtrip` vectors that are not record-shaped and all `bundle`
//! vectors are reported as skipped against this target rather than
//! failed — that is a gap in surface area, not a protocol violation.
//!
//! Requires **Node >= 22.6** run with `--experimental-strip-types` (the
//! harness imports `packages/kernel-ts/src/index.ts` directly), and
//! requires `crates/vidmesh-wasm` to have been built into
//! `packages/kernel-ts/wasm/` (Phase 3 of the build plan) — until then
//! the harness process will fail at import time, which this module
//! surfaces as a normal I/O/spawn error rather than a panic.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};

use serde_json::{json, Value as JsonValue};

use crate::kernel_target::Outcome;
use crate::vectors::{IdentityExpected, Layer, Vector, VectorData};

/// A running `node node-harness.mjs` child process.
pub struct NodeHarness {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl NodeHarness {
    /// Spawn the harness. `harness_path` is the `.mjs` file
    /// (`tools/conformance/node-harness.mjs` by default).
    pub fn spawn(harness_path: &Path) -> std::io::Result<NodeHarness> {
        let mut child = std::process::Command::new("node")
            .arg("--experimental-strip-types")
            .arg(harness_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(NodeHarness {
            child,
            stdin,
            stdout,
        })
    }

    fn call(&mut self, request: JsonValue) -> std::io::Result<JsonValue> {
        let mut line = serde_json::to_string(&request).expect("request is always serializable");
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;
        let mut response = String::new();
        let n = self.stdout.read_line(&mut response)?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "node harness closed stdout (crashed or exited)",
            ));
        }
        serde_json::from_str(response.trim_end())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl Drop for NodeHarness {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn response_error_class(resp: &JsonValue) -> Option<String> {
    resp.get("error_class")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Run one vector against the node harness.
pub fn run(harness: &mut NodeHarness, v: &Vector) -> Outcome {
    match &v.data {
        VectorData::RecordValid {
            cbor_hex,
            expected_id_hex,
            json,
        } => run_record_valid(harness, cbor_hex, expected_id_hex, json),
        VectorData::RecordInvalid {
            cbor_hex,
            expected_error,
            layer,
        } => run_record_invalid(harness, cbor_hex, expected_error, *layer),
        VectorData::ChunkProof {
            n_chunks,
            last_chunk_len,
            chunk_index,
            proof_hex,
            root_hex,
            valid,
        } => run_chunk_proof(
            harness,
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
        } => run_identity_chain(
            harness,
            records_hex,
            *now,
            observed,
            expected,
            expected_error,
        ),
        VectorData::Bundle { .. } => {
            Outcome::Skip("@vidmesh/kernel exposes no bundle import/export API".into())
        }
        VectorData::JsonRoundtrip {
            json,
            expected_cbor_hex,
            expected_error,
        } => run_json_roundtrip(harness, json, expected_cbor_hex, expected_error),
    }
}

fn call_or_fail(harness: &mut NodeHarness, req: JsonValue) -> Result<JsonValue, Outcome> {
    harness
        .call(req)
        .map_err(|e| Outcome::Fail(format!("node harness I/O error: {e}")))
}

fn run_record_valid(
    harness: &mut NodeHarness,
    cbor_hex: &str,
    expected_id_hex: &str,
    json: &JsonValue,
) -> Outcome {
    let resp = match call_or_fail(
        harness,
        json!({"op": "verify-record", "cbor_hex": cbor_hex}),
    ) {
        Ok(r) => r,
        Err(o) => return o,
    };
    if resp.get("ok") != Some(&JsonValue::Bool(true)) {
        return Outcome::Fail(format!(
            "expected valid record, verify-record returned {resp}"
        ));
    }
    let resp = match call_or_fail(harness, json!({"op": "derive-id", "cbor_hex": cbor_hex})) {
        Ok(r) => r,
        Err(o) => return o,
    };
    let id_hex = resp
        .get("id_hex")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if id_hex != expected_id_hex {
        return Outcome::Fail(format!(
            "id mismatch: got {id_hex}, expected {expected_id_hex}"
        ));
    }
    let resp = match call_or_fail(
        harness,
        json!({"op": "record-to-json", "cbor_hex": cbor_hex}),
    ) {
        Ok(r) => r,
        Err(o) => return o,
    };
    match resp.get("json") {
        Some(rendered) if rendered == json => Outcome::Pass,
        Some(rendered) => Outcome::Fail(format!(
            "json interchange mismatch: got {rendered}, expected {json}"
        )),
        None => Outcome::Fail(format!("record-to-json did not return `json`: {resp}")),
    }
}

fn run_record_invalid(
    harness: &mut NodeHarness,
    cbor_hex: &str,
    expected_error: &str,
    layer: Layer,
) -> Outcome {
    let resp = match call_or_fail(
        harness,
        json!({"op": "verify-record", "cbor_hex": cbor_hex}),
    ) {
        Ok(r) => r,
        Err(o) => return o,
    };
    let accepted = resp.get("ok") == Some(&JsonValue::Bool(true));
    match layer {
        Layer::Envelope => {
            if accepted {
                return Outcome::Fail("expected rejection, node harness accepted it".into());
            }
            match response_error_class(&resp) {
                Some(class) if class == expected_error => Outcome::Pass,
                Some(class) => Outcome::Fail(format!(
                    "wrong error class: got {class}, expected {expected_error}"
                )),
                None => Outcome::Fail(format!("rejected but no error_class reported: {resp}")),
            }
        }
        Layer::Kind => {
            if accepted {
                Outcome::Skip(
                    "kind-layer vector: node harness accepts at envelope layer as expected; \
                     kind-level validation is a separate, not-yet-wired check"
                        .into(),
                )
            } else {
                Outcome::Fail(format!(
                    "kind-layer vector rejected at envelope layer instead ({resp}); \
                     fixture or envelope rules disagree between kernel and node"
                ))
            }
        }
    }
}

fn run_chunk_proof(
    harness: &mut NodeHarness,
    n_chunks: u64,
    last_chunk_len: u64,
    chunk_index: u64,
    proof_hex: &[String],
    root_hex: &str,
    valid: bool,
) -> Outcome {
    let chunk_len = if chunk_index + 1 == n_chunks {
        last_chunk_len
    } else {
        1u64 << 20
    };
    let chunk_byte = (chunk_index % 251) as u8;
    let chunk_hex = hex::encode(vec![chunk_byte; chunk_len as usize]);
    let resp = match call_or_fail(
        harness,
        json!({
            "op": "verify-chunk",
            "root_hex": root_hex,
            "n_chunks": n_chunks,
            "index": chunk_index,
            "chunk_hex": chunk_hex,
            "proof_hex": proof_hex,
        }),
    ) {
        Ok(r) => r,
        Err(o) => return o,
    };
    let ok = resp.get("ok") == Some(&JsonValue::Bool(true));
    if ok == valid {
        Outcome::Pass
    } else {
        Outcome::Fail(format!(
            "verify-chunk returned {resp}, vector expects valid={valid}"
        ))
    }
}

fn run_identity_chain(
    harness: &mut NodeHarness,
    records_hex: &[String],
    now: i64,
    observed: &std::collections::BTreeMap<String, i64>,
    expected: &Option<IdentityExpected>,
    expected_error: &Option<String>,
) -> Outcome {
    let observed_json: serde_json::Map<String, JsonValue> = observed
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    let resp = match call_or_fail(
        harness,
        json!({
            "op": "identity-verify-chain",
            "records_hex": records_hex,
            "now": now,
            "observed": observed_json,
        }),
    ) {
        Ok(r) => r,
        Err(o) => return o,
    };
    match (resp.get("head_hex"), expected, expected_error) {
        (_, None, None) => {
            Outcome::Fail("vector has neither expected nor expected_error (fixture bug)".into())
        }
        (Some(head), Some(exp), _) => {
            let head_hex = head.as_str().unwrap_or_default();
            let signing_key_hex = resp
                .get("signing_key_hex")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let depth = resp
                .get("depth")
                .and_then(|v| v.as_u64())
                .unwrap_or_default();
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
            } else if depth != exp.depth {
                Outcome::Fail(format!(
                    "depth mismatch: got {depth}, expected {}",
                    exp.depth
                ))
            } else {
                Outcome::Pass
            }
        }
        (None, _, Some(expected_class)) => match response_error_class(&resp) {
            Some(class) if &class == expected_class => Outcome::Pass,
            Some(class) => Outcome::Fail(format!(
                "wrong error class: got {class}, expected {expected_class}"
            )),
            None => Outcome::Fail(format!("expected error, got {resp}")),
        },
        (Some(_), None, Some(_)) => {
            Outcome::Fail("expected an error, verify-chain succeeded".into())
        }
        (None, Some(_), None) => Outcome::Fail(format!("expected success, got {resp}")),
    }
}

fn run_json_roundtrip(
    harness: &mut NodeHarness,
    json_text: &str,
    expected_cbor_hex: &Option<String>,
    expected_error: &Option<String>,
) -> Outcome {
    let value: JsonValue = match serde_json::from_str(json_text) {
        Ok(v) => v,
        Err(_) => {
            return Outcome::Skip(
                "fixture json is not valid JSON text at all; kernel-only vector".into(),
            )
        }
    };
    // @vidmesh/kernel only exposes record-level JSON (recordFromJson),
    // which requires the 7 envelope keys "1".."7". Anything else is
    // outside this target's surface area.
    let is_record_shaped = value
        .as_object()
        .is_some_and(|m| (1..=7).all(|k| m.contains_key(&k.to_string())));
    if !is_record_shaped {
        return Outcome::Skip(
            "not a record-shaped JSON document; @vidmesh/kernel exposes no generic \
             CBOR-Value JSON codec, only record-from-json"
                .into(),
        );
    }
    let resp = match call_or_fail(harness, json!({"op": "record-from-json", "json": value})) {
        Ok(r) => r,
        Err(o) => return o,
    };
    match (resp.get("cbor_hex"), expected_cbor_hex, expected_error) {
        (_, None, None) => Outcome::Fail(
            "vector has neither expected_cbor_hex nor expected_error (fixture bug)".into(),
        ),
        (Some(hex_value), Some(want), _) => {
            let got = hex_value.as_str().unwrap_or_default();
            if got == want {
                Outcome::Pass
            } else {
                Outcome::Fail(format!("cbor mismatch: got {got}, expected {want}"))
            }
        }
        (None, _, Some(expected_class)) => match response_error_class(&resp) {
            Some(class) if &class == expected_class => Outcome::Pass,
            Some(class) => Outcome::Fail(format!(
                "wrong error class: got {class}, expected {expected_class}"
            )),
            None => Outcome::Fail(format!("expected rejection, got {resp}")),
        },
        (Some(_), None, Some(_)) => {
            Outcome::Fail("expected rejection, record-from-json succeeded".into())
        }
        (None, Some(_), None) => Outcome::Fail(format!("expected success, got {resp}")),
    }
}
