//! The conformance vector format.
//!
//! Each vector is one JSON file at `vectors/<group>/<name>.json`. Every
//! vector shares four top-level fields (`group`, `name`, `kind`,
//! `description`); the remaining fields depend on `kind`, which is one
//! of:
//!
//! * `record-valid` — a canonically-encoded record that MUST parse and
//!   verify against the kernel; `expected_id_hex` is its record id and
//!   `json` is its JSON interchange form (spec 001 §11), embedded as a
//!   real JSON value (not a re-escaped string) so a human or a diff
//!   tool can read it directly.
//! * `record-invalid` — a record that MUST be rejected.
//!   `expected_error` names the rejection class using the same
//!   vocabulary as [`evermesh_kernel::Error`]: `cbor`, `non-canonical`,
//!   `envelope`, `signature`, `unknown-algorithm`, or `kind`. `layer`
//!   says which validation pass produces the error: `"envelope"` means
//!   `Record::from_cbor` + `Record::verify` alone must reject it (every
//!   runner target can check this today); `"kind"` means the rejection
//!   only happens once kind-level validation
//!   (`evermesh_kernel::kinds::validate`) runs, which is not wired into
//!   this runner by default (see `src/kernel_target.rs`) because the
//!   kinds module is still being completed in parallel. Kind-layer
//!   vectors are still generated and shipped; the runner reports them
//!   as skipped rather than failed until kind validation is enabled.
//! * `chunk-proof` — describes a synthetic blob (by formula, so no
//!   megabytes of hex need to live in the fixture file), a chunk index,
//!   a sibling proof, a root, and whether the proof MUST verify.
//! * `identity-chain` — a set of records to feed to
//!   `Identity::verify_chain`, the verifier's `now` and per-record
//!   first-observed times, and either the expected resulting state or
//!   an expected error.
//! * `bundle` — a complete bundle (magic + CBOR sequence) and what a
//!   correct importer must recover from it.
//! * `json-roundtrip` — a JSON interchange document and either the
//!   canonical CBOR it must produce or the fact that it must be
//!   rejected.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// One vector file's full contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector {
    /// The `vectors/<group>/` directory this vector lives in.
    pub group: String,
    /// The `<name>.json` filename stem.
    pub name: String,
    /// One-sentence human-readable description of what this vector
    /// exercises and why.
    pub description: String,
    /// The kind-specific payload.
    #[serde(flatten)]
    pub data: VectorData,
}

/// The rejection-class vocabulary shared by every implementation's
/// error reporting. Mirrors [`evermesh_kernel::Error`]'s variants.
pub mod error_class {
    pub const CBOR: &str = "cbor";
    pub const NON_CANONICAL: &str = "non-canonical";
    pub const ENVELOPE: &str = "envelope";
    pub const SIGNATURE: &str = "signature";
    pub const UNKNOWN_ALGORITHM: &str = "unknown-algorithm";
    pub const KIND: &str = "kind";
    pub const IDENTITY: &str = "identity";
    pub const BUNDLE: &str = "bundle";

    /// Classify a [`evermesh_kernel::Error`] into this vocabulary.
    pub fn classify(e: &evermesh_kernel::Error) -> &'static str {
        use evermesh_kernel::Error;
        match e {
            Error::Cbor(_) => CBOR,
            Error::NonCanonical(_) => NON_CANONICAL,
            Error::Envelope(_) => ENVELOPE,
            Error::Signature => SIGNATURE,
            Error::UnknownAlgorithm(_) => UNKNOWN_ALGORITHM,
            Error::Kind(_) => KIND,
            Error::Identity(_) => IDENTITY,
            Error::Bundle(_) => BUNDLE,
            Error::ChunkProof(_) => "chunk-proof",
            Error::Io(_) => "io",
            // Error is #[non_exhaustive]; future variants classify as
            // "other" so old vectors keep failing loudly, not silently.
            _ => "other",
        }
    }
}

/// Which validation pass is expected to produce a `record-invalid`
/// vector's error. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    /// `Record::from_cbor` + `Record::verify` alone must reject it.
    Envelope,
    /// Only kind-level validation (`evermesh_kernel::kinds::validate`)
    /// rejects it; envelope validation alone accepts the record.
    Kind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum VectorData {
    #[serde(rename = "record-valid")]
    RecordValid {
        cbor_hex: String,
        expected_id_hex: String,
        json: JsonValue,
    },
    #[serde(rename = "record-invalid")]
    RecordInvalid {
        cbor_hex: String,
        expected_error: String,
        layer: Layer,
    },
    #[serde(rename = "chunk-proof")]
    ChunkProof {
        /// Number of chunks in the synthetic blob.
        n_chunks: u64,
        /// Length of the final chunk in bytes; every earlier chunk is
        /// exactly `CHUNK_SIZE` (1 MiB). Chunk `i`'s bytes are
        /// `(i % 251)` repeated for its whole length (task-specified
        /// synthesis formula), so the runner reconstructs the blob from
        /// just these two numbers.
        last_chunk_len: u64,
        chunk_index: u64,
        proof_hex: Vec<String>,
        root_hex: String,
        /// Whether `verify_chunk` MUST succeed against this proof/root.
        valid: bool,
    },
    #[serde(rename = "identity-chain")]
    IdentityChain {
        /// Canonical CBOR records to feed to `Identity::verify_chain`,
        /// in the order the vector wants them merged.
        records_hex: Vec<String>,
        now: i64,
        /// Record id (hex) -> seconds first observed. Absent = "just
        /// observed" (`None`, i.e. not final).
        observed: BTreeMap<String, i64>,
        expected: Option<IdentityExpected>,
        expected_error: Option<String>,
    },
    #[serde(rename = "bundle")]
    Bundle {
        bundle_hex: String,
        /// Present when `Bundle::import` is expected to succeed (with
        /// possible per-item salvage reported inside `skipped_count`).
        expected: Option<BundleExpected>,
        /// Present when the bundle is malformed at the container level
        /// (bad magic, unreadable magic) and `Bundle::import` itself
        /// MUST return `Err` rather than a salvage report — e.g.
        /// `bundle/bad-magic`.
        expected_error: Option<String>,
    },
    #[serde(rename = "json-roundtrip")]
    JsonRoundtrip {
        json: String,
        expected_cbor_hex: Option<String>,
        expected_error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityExpected {
    pub head_hex: String,
    pub signing_key_hex: String,
    pub depth: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleExpected {
    pub record_ids: Vec<String>,
    pub blob_ids: Vec<String>,
    pub skipped_count: usize,
    pub truncated: bool,
}

impl Vector {
    /// Serialize with sorted keys (via `serde_json`'s default `BTreeMap`
    /// backing for objects) and 2-space pretty printing, so regenerating
    /// the suite produces a clean, stable diff.
    pub fn to_pretty_json(&self) -> serde_json::Result<String> {
        let value = serde_json::to_value(self)?;
        serde_json::to_string_pretty(&value)
    }
}
