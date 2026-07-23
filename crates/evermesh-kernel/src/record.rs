//! The record envelope: building, encoding, identifying, and verifying
//! records (spec 001 §§1–6).

use crate::codec::{self, Value};
use crate::error::{Error, Result};
use crate::identity::Keypair;
use crate::ids::{IdentityId, RecordId};

/// Registered signature algorithm id for Ed25519 (spec 001 §7).
pub const SIG_ALG_ED25519: u64 = 1;

/// Domain-separation prefix for record signatures (spec 001 §4).
pub const RECORD_SIG_PREFIX: &[u8] = b"evermesh:record:v1";

/// A typed reference to a record or a blob (spec 001 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ref {
    /// 0 = record, 1 = blob.
    pub ref_type: u64,
    /// The referenced record id or blob id.
    pub hash: [u8; 32],
}

impl Ref {
    /// Reference a record.
    pub fn record(id: RecordId) -> Ref {
        Ref {
            ref_type: 0,
            hash: id.0,
        }
    }

    /// Reference a blob.
    pub fn blob(id: crate::ids::BlobId) -> Ref {
        Ref {
            ref_type: 1,
            hash: id.0,
        }
    }

    /// True if this is a record reference.
    pub fn is_record(&self) -> bool {
        self.ref_type == 0
    }

    /// True if this is a blob reference.
    pub fn is_blob(&self) -> bool {
        self.ref_type == 1
    }

    fn to_value(self) -> Value {
        Value::Array(vec![
            Value::Uint(self.ref_type),
            Value::Bytes(self.hash.to_vec()),
        ])
    }

    fn from_value(v: &Value) -> Result<Ref> {
        let arr = v
            .as_array()
            .ok_or(Error::Envelope("ref must be an array"))?;
        if arr.len() != 2 {
            return Err(Error::Envelope("ref must have exactly 2 elements"));
        }
        let ref_type = arr[0]
            .as_u64()
            .ok_or(Error::Envelope("ref type must be a uint"))?;
        if ref_type > 1 {
            return Err(Error::Envelope("unknown ref type"));
        }
        let bytes = arr[1]
            .as_bytes()
            .ok_or(Error::Envelope("ref hash must be bytes"))?;
        let hash: [u8; 32] = bytes
            .try_into()
            .map_err(|_| Error::Envelope("ref hash must be 32 bytes"))?;
        Ok(Ref { ref_type, hash })
    }
}

/// Names the author of a record and carries its verification key
/// (spec 001 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRef {
    /// Stable identity identifier; all-zero in genesis records.
    pub identity_id: IdentityId,
    /// Public key that produced the record's signature, encoded per
    /// the record's `sig_alg`.
    pub signing_key: Vec<u8>,
}

impl IdentityRef {
    fn to_value(&self) -> Value {
        Value::Array(vec![
            Value::Bytes(self.identity_id.0.to_vec()),
            Value::Bytes(self.signing_key.clone()),
        ])
    }

    fn from_value(v: &Value) -> Result<IdentityRef> {
        let arr = v
            .as_array()
            .ok_or(Error::Envelope("author must be an array"))?;
        if arr.len() != 2 {
            return Err(Error::Envelope("author must have exactly 2 elements"));
        }
        let id_bytes = arr[0]
            .as_bytes()
            .ok_or(Error::Envelope("identity id must be bytes"))?;
        let identity_id: [u8; 32] = id_bytes
            .try_into()
            .map_err(|_| Error::Envelope("identity id must be 32 bytes"))?;
        let signing_key = arr[1]
            .as_bytes()
            .ok_or(Error::Envelope("signing key must be bytes"))?
            .to_vec();
        Ok(IdentityRef {
            identity_id: IdentityId(identity_id),
            signing_key,
        })
    }
}

/// A signed, immutable record (spec 001 §1).
///
/// Construct with [`RecordBuilder`]; parse untrusted bytes with
/// [`Record::from_cbor`], which enforces canonical encoding and envelope
/// shape but not the signature — call [`Record::verify`] before trusting
/// attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    kind: u64,
    author: IdentityRef,
    created_at: i64,
    refs: Vec<Ref>,
    body: Value,
    sig_alg: u64,
    sig: Vec<u8>,
}

impl Record {
    /// The record kind (spec 003).
    pub fn kind(&self) -> u64 {
        self.kind
    }

    /// The signing identity reference.
    pub fn author(&self) -> &IdentityRef {
        &self.author
    }

    /// The author's identity id as raw bytes (convenience for indexes).
    pub fn author_identity_id(&self) -> [u8; 32] {
        self.author.identity_id.0
    }

    /// Author-asserted Unix time in seconds. Untrusted (spec 001 §10).
    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    /// The ordered refs.
    pub fn refs(&self) -> &[Ref] {
        &self.refs
    }

    /// Hashes of all refs, in order (convenience for indexes).
    pub fn ref_hashes(&self) -> Vec<[u8; 32]> {
        self.refs.iter().map(|r| r.hash).collect()
    }

    /// The kind-defined body map.
    pub fn body(&self) -> &Value {
        &self.body
    }

    /// The signature algorithm id.
    pub fn sig_alg(&self) -> u64 {
        self.sig_alg
    }

    /// The raw signature bytes.
    pub fn sig(&self) -> &[u8] {
        &self.sig
    }

    fn to_value(&self, with_sig: bool) -> Value {
        let mut entries = vec![
            (Value::Uint(1), Value::Uint(self.kind)),
            (Value::Uint(2), self.author.to_value()),
            (Value::Uint(3), Value::from_i64(self.created_at)),
            (
                Value::Uint(4),
                Value::Array(self.refs.iter().map(|r| r.to_value()).collect()),
            ),
            (Value::Uint(5), self.body.clone()),
            (Value::Uint(6), Value::Uint(self.sig_alg)),
        ];
        if with_sig {
            entries.push((Value::Uint(7), Value::Bytes(self.sig.clone())));
        }
        Value::Map(entries)
    }

    /// The record identifier: BLAKE3-256 over the canonical envelope
    /// without the signature (spec 001 §3).
    pub fn id(&self) -> RecordId {
        // Encoding a builder-validated or decoder-validated record cannot
        // produce duplicate keys, so this cannot fail.
        let bytes = codec::encode_canonical(&self.to_value(false))
            .expect("envelope map has fixed unique keys");
        RecordId(*blake3::hash(&bytes).as_bytes())
    }

    /// Canonical envelope bytes, signature included (the wire form).
    pub fn to_canonical_cbor(&self) -> Vec<u8> {
        codec::encode_canonical(&self.to_value(true)).expect("envelope map has fixed unique keys")
    }

    /// Parse untrusted bytes: strict canonical decode plus envelope shape
    /// (spec 001 §§1–3, rules 1–3 of validity). Does not verify the
    /// signature — call [`Record::verify`].
    pub fn from_cbor(bytes: &[u8]) -> Result<Record> {
        let value = codec::decode_canonical(bytes)?;
        Self::from_value(&value)
    }

    fn from_value(value: &Value) -> Result<Record> {
        let map = value
            .as_map()
            .ok_or(Error::Envelope("record must be a map"))?;
        if map.len() != 7 {
            return Err(Error::Envelope("record must have exactly keys 1-7"));
        }
        // Canonical decoding guarantees ascending unique keys, so a
        // 7-entry map with keys 1..=7 must be exactly [1,2,...,7].
        for (i, (k, _)) in map.iter().enumerate() {
            if k.as_u64() != Some(i as u64 + 1) {
                return Err(Error::Envelope("record must have exactly keys 1-7"));
            }
        }
        let kind = map[0]
            .1
            .as_u64()
            .ok_or(Error::Envelope("kind must be a uint"))?;
        let author = IdentityRef::from_value(&map[1].1)?;
        let created_at = map[2]
            .1
            .as_i64()
            .ok_or(Error::Envelope("created_at must be an int64"))?;
        let refs_arr = map[3]
            .1
            .as_array()
            .ok_or(Error::Envelope("refs must be an array"))?;
        let mut refs = Vec::with_capacity(refs_arr.len());
        for r in refs_arr {
            refs.push(Ref::from_value(r)?);
        }
        let body = map[4].1.clone();
        if body.as_map().is_none() {
            return Err(Error::Envelope("body must be a map"));
        }
        let sig_alg = map[5]
            .1
            .as_u64()
            .ok_or(Error::Envelope("sig_alg must be a uint"))?;
        let sig = map[6]
            .1
            .as_bytes()
            .ok_or(Error::Envelope("sig must be bytes"))?
            .to_vec();
        Ok(Record {
            kind,
            author,
            created_at,
            refs,
            body,
            sig_alg,
            sig,
        })
    }

    /// Verify the signature under `sig_alg` (spec 001 §§3–4). Together
    /// with the strict decoding performed by [`Record::from_cbor`], a
    /// passing record is envelope-valid.
    pub fn verify(&self) -> Result<()> {
        match self.sig_alg {
            SIG_ALG_ED25519 => {
                let key_bytes: [u8; 32] = self
                    .author
                    .signing_key
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::Signature)?;
                let key = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
                    .map_err(|_| Error::Signature)?;
                let sig_bytes: [u8; 64] = self
                    .sig
                    .as_slice()
                    .try_into()
                    .map_err(|_| Error::Signature)?;
                let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
                let mut msg = Vec::with_capacity(RECORD_SIG_PREFIX.len() + 32);
                msg.extend_from_slice(RECORD_SIG_PREFIX);
                msg.extend_from_slice(self.id().as_bytes());
                key.verify_strict(&msg, &sig).map_err(|_| Error::Signature)
            }
            other => Err(Error::UnknownAlgorithm(other)),
        }
    }

    /// JSON interchange form (spec 001 §11). Never the signed form.
    pub fn to_json(&self) -> String {
        codec::to_json(&self.to_value(true))
    }

    /// Parse the JSON interchange form, converting through canonical
    /// CBOR (spec 001 §11).
    pub fn from_json(s: &str) -> Result<Record> {
        let value = codec::from_json(s)?;
        // Round-trip through canonical bytes so JSON-borne records get
        // exactly the same strictness as CBOR-borne ones.
        let bytes = codec::encode_canonical(&value)?;
        Self::from_cbor(&bytes)
    }
}

/// Builder for signing new records.
///
/// ```no_run
/// use evermesh_kernel::{codec::Value, identity::Keypair, record::RecordBuilder};
/// let kp = Keypair::generate().unwrap();
/// let record = RecordBuilder::new(32)
///     .created_at(1_752_710_400)
///     .body(Value::Map(vec![(Value::Text("text".into()), Value::Text("hi".into()))]))
///     .sign_as(&kp, evermesh_kernel::ids::IdentityId::ZERO)
///     .unwrap();
/// assert!(record.verify().is_ok());
/// ```
#[derive(Debug, Clone)]
pub struct RecordBuilder {
    kind: u64,
    created_at: i64,
    refs: Vec<Ref>,
    body: Value,
}

impl RecordBuilder {
    /// Start a record of the given kind.
    pub fn new(kind: u64) -> RecordBuilder {
        RecordBuilder {
            kind,
            created_at: 0,
            refs: Vec::new(),
            body: Value::Map(Vec::new()),
        }
    }

    /// Set the author-asserted creation time (Unix seconds).
    pub fn created_at(mut self, t: i64) -> Self {
        self.created_at = t;
        self
    }

    /// Append one ref.
    pub fn r#ref(mut self, r: Ref) -> Self {
        self.refs.push(r);
        self
    }

    /// Replace the refs list.
    pub fn refs(mut self, refs: Vec<Ref>) -> Self {
        self.refs = refs;
        self
    }

    /// Set the body. Must be a map (spec 001 §1).
    pub fn body(mut self, body: Value) -> Self {
        self.body = body;
        self
    }

    /// Sign as the given identity, producing the finished record.
    ///
    /// Fails if the body is not a map or contains duplicate map keys
    /// anywhere.
    pub fn sign_as(self, keypair: &Keypair, identity: IdentityId) -> Result<Record> {
        if self.body.as_map().is_none() {
            return Err(Error::Envelope("body must be a map"));
        }
        let mut record = Record {
            kind: self.kind,
            author: IdentityRef {
                identity_id: identity,
                signing_key: keypair.public_key_bytes().to_vec(),
            },
            created_at: self.created_at,
            refs: self.refs,
            // Store the body in canonical key order so a freshly built
            // record has the same in-memory shape as one decoded from its
            // canonical bytes (the `Value::Map` order invariant). Ordering
            // only — the signed bytes are unchanged, since encoding sorts
            // regardless.
            body: self.body.into_canonical(),
            sig_alg: SIG_ALG_ED25519,
            sig: Vec::new(),
        };
        // Surface duplicate-key bodies as an error before signing.
        codec::encode_canonical(&record.to_value(false))?;
        let mut msg = Vec::with_capacity(RECORD_SIG_PREFIX.len() + 32);
        msg.extend_from_slice(RECORD_SIG_PREFIX);
        msg.extend_from_slice(record.id().as_bytes());
        record.sig = keypair.sign(&msg).to_vec();
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;

    fn test_keypair() -> Keypair {
        Keypair::from_secret_bytes(&[7u8; 32])
    }

    fn simple_record() -> Record {
        RecordBuilder::new(32)
            .created_at(1_752_710_400)
            .r#ref(Ref {
                ref_type: 0,
                hash: [0xaa; 32],
            })
            .body(Value::Map(vec![(
                Value::Text("text".into()),
                Value::Text("hello".into()),
            )]))
            .sign_as(&test_keypair(), IdentityId([1; 32]))
            .unwrap()
    }

    #[test]
    fn sign_verify_round_trip() {
        let r = simple_record();
        r.verify().unwrap();
        let bytes = r.to_canonical_cbor();
        let back = Record::from_cbor(&bytes).unwrap();
        back.verify().unwrap();
        assert_eq!(back, r);
        assert_eq!(back.id(), r.id());
    }

    #[test]
    fn id_excludes_signature() {
        let r = simple_record();
        let mut tampered = r.clone();
        tampered.sig = vec![0; 64];
        assert_eq!(tampered.id(), r.id());
        assert!(tampered.verify().is_err());
    }

    #[test]
    fn tampered_body_fails() {
        let r = simple_record();
        let mut tampered = r.clone();
        tampered.body = Value::Map(vec![(
            Value::Text("text".into()),
            Value::Text("evil".into()),
        )]);
        assert_ne!(tampered.id(), r.id());
        assert_eq!(tampered.verify(), Err(Error::Signature));
    }

    #[test]
    fn unknown_sig_alg_rejected() {
        let r = simple_record();
        let mut bad = r.clone();
        bad.sig_alg = 99;
        assert_eq!(bad.verify(), Err(Error::UnknownAlgorithm(99)));
    }

    #[test]
    fn envelope_shape_enforced() {
        // 6-entry map (missing sig) must be rejected.
        let r = simple_record();
        let bytes = codec::encode_canonical(&r.to_value(false)).unwrap();
        assert!(matches!(Record::from_cbor(&bytes), Err(Error::Envelope(_))));
    }

    #[test]
    fn non_map_body_rejected() {
        let err = RecordBuilder::new(1)
            .body(Value::Uint(3))
            .sign_as(&test_keypair(), IdentityId::ZERO)
            .unwrap_err();
        assert!(matches!(err, Error::Envelope(_)));
    }

    #[test]
    fn refs_validation() {
        let bad = Value::Array(vec![Value::Uint(2), Value::Bytes(vec![0; 32])]);
        assert!(Ref::from_value(&bad).is_err());
        let short = Value::Array(vec![Value::Uint(0), Value::Bytes(vec![0; 31])]);
        assert!(Ref::from_value(&short).is_err());
    }

    #[test]
    fn json_round_trip() {
        let r = simple_record();
        let json = r.to_json();
        let back = Record::from_json(&json).unwrap();
        assert_eq!(back, r);
        back.verify().unwrap();
    }

    #[test]
    fn negative_created_at_supported() {
        let r = RecordBuilder::new(1)
            .created_at(-86400)
            .sign_as(&test_keypair(), IdentityId::ZERO)
            .unwrap();
        let back = Record::from_cbor(&r.to_canonical_cbor()).unwrap();
        assert_eq!(back.created_at(), -86400);
        back.verify().unwrap();
    }
}
