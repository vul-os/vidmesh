//! wasm-bindgen bindings over `vidmesh-kernel`.
//!
//! One crypto implementation everywhere: these bindings expose the exact
//! kernel that runs natively, to Node and browsers, via
//! `packages/kernel-ts`. The API favors plain types (byte slices, JSON
//! interchange strings, hex) so the TypeScript wrapper owns ergonomics.
//!
//! Conventions:
//! - Records cross the boundary as canonical CBOR bytes.
//! - Structured values (bodies, refs, states) cross as JSON interchange
//!   strings (spec 001 §11).
//! - `kind` and algorithm ids are `u32` here (registry values are small);
//!   `created_at` is an `i64` and surfaces as a JS `BigInt`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use vidmesh_kernel::{codec, kinds, Identity, IdentityId, Keypair, Record, RecordId};
use wasm_bindgen::prelude::*;

fn js_err(e: vidmesh_kernel::Error) -> JsError {
    JsError::new(&e.to_string())
}

fn take32(bytes: &[u8], what: &str) -> Result<[u8; 32], JsError> {
    bytes
        .try_into()
        .map_err(|_| JsError::new(&format!("{what} must be exactly 32 bytes")))
}

/// Derive a record's id from its canonical bytes (validates envelope
/// shape and canonicality, not the signature).
#[wasm_bindgen]
pub fn record_id(record: &[u8]) -> Result<Vec<u8>, JsError> {
    let r = Record::from_cbor(record).map_err(js_err)?;
    Ok(r.id().as_bytes().to_vec())
}

/// Full envelope verification: canonical decode + signature (spec 001 §3).
#[wasm_bindgen]
pub fn verify_record(record: &[u8]) -> Result<(), JsError> {
    let r = Record::from_cbor(record).map_err(js_err)?;
    r.verify().map_err(js_err)
}

/// Kind-level validation for known kinds (spec 003); unknown kinds pass.
#[wasm_bindgen]
pub fn validate_kind(record: &[u8]) -> Result<(), JsError> {
    let r = Record::from_cbor(record).map_err(js_err)?;
    kinds::validate(&r).map_err(js_err)
}

/// Canonical bytes → JSON interchange form.
#[wasm_bindgen]
pub fn record_to_json(record: &[u8]) -> Result<String, JsError> {
    let r = Record::from_cbor(record).map_err(js_err)?;
    Ok(r.to_json())
}

/// JSON interchange form → canonical bytes (strictness identical to CBOR
/// intake; spec 001 §11).
#[wasm_bindgen]
pub fn record_from_json(json: &str) -> Result<Vec<u8>, JsError> {
    let r = Record::from_json(json).map_err(js_err)?;
    Ok(r.to_canonical_cbor())
}

/// An Ed25519 keypair.
#[wasm_bindgen]
pub struct WasmKeypair {
    inner: Keypair,
}

#[wasm_bindgen]
impl WasmKeypair {
    /// Generate from host randomness.
    #[wasm_bindgen(constructor)]
    pub fn generate() -> Result<WasmKeypair, JsError> {
        Ok(WasmKeypair { inner: Keypair::generate().map_err(js_err)? })
    }

    /// Restore from 32 secret bytes.
    #[wasm_bindgen(js_name = fromSecret)]
    pub fn from_secret(secret: &[u8]) -> Result<WasmKeypair, JsError> {
        let secret = take32(secret, "secret")?;
        Ok(WasmKeypair { inner: Keypair::from_secret_bytes(&secret) })
    }

    /// The 32-byte public key.
    #[wasm_bindgen(js_name = publicKey)]
    pub fn public_key(&self) -> Vec<u8> {
        self.inner.public_key_bytes().to_vec()
    }

    /// The 32 secret bytes. Handle with care.
    pub fn secret(&self) -> Vec<u8> {
        self.inner.secret_bytes().to_vec()
    }
}

/// Build and sign a record. `refs_json` is a JSON array of
/// `[ref_type, "hex:<hash>"]` pairs; `body_json` is the body map in JSON
/// interchange form. Returns canonical bytes.
#[wasm_bindgen(js_name = buildRecord)]
pub fn build_record(
    keypair: &WasmKeypair,
    identity_id: &[u8],
    kind: u32,
    created_at: i64,
    refs_json: &str,
    body_json: &str,
) -> Result<Vec<u8>, JsError> {
    let identity = IdentityId(take32(identity_id, "identity_id")?);
    let refs_value = codec::from_json(refs_json).map_err(js_err)?;
    let refs_arr = refs_value
        .as_array()
        .ok_or_else(|| JsError::new("refs must be a JSON array"))?;
    let mut refs = Vec::with_capacity(refs_arr.len());
    for entry in refs_arr {
        let pair = entry
            .as_array()
            .ok_or_else(|| JsError::new("each ref must be [type, hash]"))?;
        if pair.len() != 2 {
            return Err(JsError::new("each ref must be [type, hash]"));
        }
        let ref_type =
            pair[0].as_u64().ok_or_else(|| JsError::new("ref type must be 0 or 1"))?;
        if ref_type > 1 {
            return Err(JsError::new("ref type must be 0 or 1"));
        }
        let hash = pair[1]
            .as_bytes()
            .ok_or_else(|| JsError::new("ref hash must be hex bytes"))?;
        refs.push(vidmesh_kernel::Ref { ref_type, hash: take32(hash, "ref hash")? });
    }
    let body = codec::from_json(body_json).map_err(js_err)?;
    let record = vidmesh_kernel::RecordBuilder::new(kind as u64)
        .created_at(created_at)
        .refs(refs)
        .body(body)
        .sign_as(&keypair.inner, identity)
        .map_err(js_err)?;
    Ok(record.to_canonical_cbor())
}

/// Create a genesis rotation record (spec 002 §2). `recovery_json` is a
/// JSON array of `[alg, "hex:<key>"]`. Returns canonical bytes; the new
/// identity id is `record_id()` of the result.
#[wasm_bindgen(js_name = genesisRecord)]
pub fn genesis_record(
    keypair: &WasmKeypair,
    recovery_json: &str,
    contest_window: u32,
    created_at: i64,
) -> Result<Vec<u8>, JsError> {
    let recovery = parse_recovery(recovery_json)?;
    let (_, record) =
        Identity::genesis(&keypair.inner, &recovery, contest_window as u64, created_at)
            .map_err(js_err)?;
    Ok(record.to_canonical_cbor())
}

/// Create a rotation record (spec 002 §3). Returns canonical bytes.
#[wasm_bindgen(js_name = rotateRecord)]
#[allow(clippy::too_many_arguments)]
pub fn rotate_record(
    signer: &WasmKeypair,
    identity_id: &[u8],
    prev_rotation_id: &[u8],
    new_key: &[u8],
    new_key_alg: u32,
    recovery_json: &str,
    contest_window: u32,
    created_at: i64,
) -> Result<Vec<u8>, JsError> {
    let recovery = parse_recovery(recovery_json)?;
    let record = Identity::rotate(
        IdentityId(take32(identity_id, "identity_id")?),
        RecordId(take32(prev_rotation_id, "prev_rotation_id")?),
        new_key,
        new_key_alg as u64,
        &recovery,
        contest_window as u64,
        created_at,
        &signer.inner,
    )
    .map_err(js_err)?;
    Ok(record.to_canonical_cbor())
}

fn parse_recovery(json: &str) -> Result<Vec<(u64, Vec<u8>)>, JsError> {
    let value = codec::from_json(json).map_err(js_err)?;
    let arr = value
        .as_array()
        .ok_or_else(|| JsError::new("recovery must be a JSON array"))?;
    let mut out = Vec::with_capacity(arr.len());
    for entry in arr {
        let pair = entry
            .as_array()
            .ok_or_else(|| JsError::new("each recovery entry must be [alg, key]"))?;
        if pair.len() != 2 {
            return Err(JsError::new("each recovery entry must be [alg, key]"));
        }
        let alg = pair[0]
            .as_u64()
            .ok_or_else(|| JsError::new("recovery alg must be a number"))?;
        let key = pair[1]
            .as_bytes()
            .ok_or_else(|| JsError::new("recovery key must be hex bytes"))?
            .to_vec();
        out.push((alg, key));
    }
    Ok(out)
}

/// Verify a rotation chain (spec 002 §4) from an array of canonical
/// record byte buffers. All rotations are treated as just-observed (no
/// finality); returns the state as a JSON object string.
#[wasm_bindgen(js_name = verifyChain)]
pub fn verify_chain(records: js_sys::Array, now: i64) -> Result<String, JsError> {
    let mut parsed = Vec::with_capacity(records.length() as usize);
    for entry in records.iter() {
        let bytes = js_sys::Uint8Array::new(&entry).to_vec();
        parsed.push(Record::from_cbor(&bytes).map_err(js_err)?);
    }
    let state = Identity::verify_chain(&parsed, &|_| None, now).map_err(js_err)?;
    let recovery: Vec<String> = state
        .recovery
        .iter()
        .map(|(alg, key)| format!("[{},\"hex:{}\"]", alg, hex_of(key)))
        .collect();
    Ok(format!(
        "{{\"identity_id\":\"hex:{}\",\"signing_key\":\"hex:{}\",\"key_alg\":{},\
         \"recovery\":[{}],\"contest_window\":{},\"head\":\"hex:{}\",\"depth\":{}}}",
        state.identity_id.to_hex(),
        hex_of(&state.signing_key),
        state.key_alg,
        recovery.join(","),
        state.contest_window,
        state.head.to_hex(),
        state.depth,
    ))
}

fn hex_of(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use core::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// BLAKE3-256 of a complete in-memory blob.
#[wasm_bindgen(js_name = hashBlob)]
pub fn hash_blob(bytes: &[u8]) -> Vec<u8> {
    vidmesh_kernel::blob::hash_blob(bytes).as_bytes().to_vec()
}

/// Sign a rendition derivation statement (spec 004 §3.1). The statement
/// construction lives in the kernel so every runtime signs identical
/// bytes. Returns the 64-byte signature for the manifest's
/// `derivation_sig` field.
#[wasm_bindgen(js_name = signDerivation)]
pub fn sign_derivation(
    keypair: &WasmKeypair,
    original: &[u8],
    rendition: &[u8],
    codec: &str,
    width: u32,
    height: u32,
    bitrate: u32,
) -> Result<Vec<u8>, JsError> {
    let original = vidmesh_kernel::BlobId(take32(original, "original blob id")?);
    let rendition_id = vidmesh_kernel::BlobId(take32(rendition, "rendition blob id")?);
    let statement = kinds::derivation_statement(
        &original,
        &rendition_id,
        codec,
        width as u64,
        height as u64,
        bitrate as u64,
    )
    .map_err(js_err)?;
    let digest = blake3::hash(&statement);
    let mut msg = Vec::with_capacity(kinds::DERIVATION_SIG_PREFIX.len() + 32);
    msg.extend_from_slice(kinds::DERIVATION_SIG_PREFIX);
    msg.extend_from_slice(digest.as_bytes());
    Ok(keypair.inner.sign(&msg).to_vec())
}

/// Verify one chunk against a chunk root (spec 001 §8). `proof` is the
/// concatenation of 32-byte sibling hashes from the prover.
#[wasm_bindgen(js_name = verifyChunk)]
pub fn verify_chunk(
    root: &[u8],
    n_chunks: u32,
    index: u32,
    chunk: &[u8],
    proof: &[u8],
) -> Result<(), JsError> {
    let root = take32(root, "root")?;
    if proof.len() % 32 != 0 {
        return Err(JsError::new("proof must be a concatenation of 32-byte hashes"));
    }
    let siblings: Vec<[u8; 32]> = proof
        .chunks_exact(32)
        .map(|c| {
            let mut a = [0u8; 32];
            a.copy_from_slice(c);
            a
        })
        .collect();
    vidmesh_kernel::blob::verify_chunk(&root, n_chunks as usize, index as usize, chunk, &siblings)
        .map_err(js_err)
}

/// Incremental blob hasher for WHATWG streams: feeds the flat blob hash
/// and the chunk-tree leaves in one pass.
#[wasm_bindgen]
pub struct BlobHasher {
    hasher: blake3::Hasher,
    leaves: Vec<[u8; 32]>,
    pending: Vec<u8>,
    size: u64,
    finished: bool,
    root: Option<[u8; 32]>,
}

impl Default for BlobHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl BlobHasher {
    /// Start a new hasher.
    #[wasm_bindgen(constructor)]
    pub fn new() -> BlobHasher {
        BlobHasher {
            hasher: blake3::Hasher::new(),
            leaves: Vec::new(),
            pending: Vec::new(),
            size: 0,
            finished: false,
            root: None,
        }
    }

    /// Feed the next bytes of the stream.
    pub fn update(&mut self, bytes: &[u8]) -> Result<(), JsError> {
        if self.finished {
            return Err(JsError::new("hasher already finalized"));
        }
        self.hasher.update(bytes);
        self.size += bytes.len() as u64;
        let mut rest = bytes;
        let chunk_size = vidmesh_kernel::blob::CHUNK_SIZE;
        while !rest.is_empty() {
            let space = chunk_size - self.pending.len();
            let take = space.min(rest.len());
            self.pending.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.pending.len() == chunk_size {
                self.leaves.push(vidmesh_kernel::blob::leaf_hash(&self.pending));
                self.pending.clear();
            }
        }
        Ok(())
    }

    /// Finish hashing. After this, read `idHex`, `size`, `nChunks`,
    /// and `chunkRootHex`.
    pub fn finalize(&mut self) -> Result<(), JsError> {
        if self.finished {
            return Err(JsError::new("hasher already finalized"));
        }
        if !self.pending.is_empty() {
            self.leaves.push(vidmesh_kernel::blob::leaf_hash(&self.pending));
            self.pending.clear();
        }
        // Fold leaves to the root with the kernel's node hash
        // (spec 001 §8: pair left-to-right, promote odd nodes).
        let mut level = self.leaves.clone();
        self.root = if level.is_empty() {
            None
        } else {
            while level.len() > 1 {
                let mut next = Vec::with_capacity(level.len().div_ceil(2));
                for pair in level.chunks(2) {
                    match pair {
                        [l, r] => next.push(vidmesh_kernel::blob::node_hash(l, r)),
                        [odd] => next.push(*odd),
                        _ => {}
                    }
                }
                level = next;
            }
            level.first().copied()
        };
        self.finished = true;
        Ok(())
    }

    /// The blob id (lowercase hex). Only valid after `finalize`.
    #[wasm_bindgen(getter, js_name = idHex)]
    pub fn id_hex(&self) -> Result<String, JsError> {
        if !self.finished {
            return Err(JsError::new("call finalize first"));
        }
        Ok(hex_of(self.hasher.clone().finalize().as_bytes()))
    }

    /// Total bytes hashed.
    #[wasm_bindgen(getter)]
    pub fn size(&self) -> f64 {
        self.size as f64
    }

    /// Number of 1 MiB chunks.
    #[wasm_bindgen(getter, js_name = nChunks)]
    pub fn n_chunks(&self) -> u32 {
        self.leaves.len() as u32
    }

    /// The chunk root (lowercase hex), or undefined for an empty blob.
    /// Only valid after `finalize`.
    #[wasm_bindgen(getter, js_name = chunkRootHex)]
    pub fn chunk_root_hex(&self) -> Result<Option<String>, JsError> {
        if !self.finished {
            return Err(JsError::new("call finalize first"));
        }
        Ok(self.root.map(|r| hex_of(&r)))
    }
}

#[cfg(test)]
mod tests {
    // Native-target tests of the pure logic (wasm-bindgen types are
    // exercised by the kernel-ts test suite in Node and a browser).
    use super::*;

    #[test]
    fn blob_hasher_matches_kernel() {
        let data = vec![7u8; vidmesh_kernel::blob::CHUNK_SIZE + 123];
        let mut h = BlobHasher::new();
        h.update(&data[..1000]).unwrap();
        h.update(&data[1000..]).unwrap();
        h.finalize().unwrap();
        assert_eq!(h.id_hex().unwrap(), vidmesh_kernel::blob::hash_blob(&data).to_hex());
        let tree = vidmesh_kernel::ChunkTree::from_bytes(&data);
        assert_eq!(
            h.chunk_root_hex().unwrap(),
            tree.root().map(|r| hex_of(&r))
        );
        assert_eq!(h.n_chunks(), 2);
    }

    #[test]
    fn blob_hasher_empty() {
        let mut h = BlobHasher::new();
        h.finalize().unwrap();
        assert_eq!(h.n_chunks(), 0);
        assert_eq!(h.chunk_root_hex().unwrap(), None);
    }
}
