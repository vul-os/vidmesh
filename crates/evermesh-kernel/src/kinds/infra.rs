//! Infrastructure and privacy kinds (spec 003 §8): `anchor`, `keygrant`.

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::{BlobId, IdentityId, RecordId};
use crate::record::{Record, Ref};

use super::{
    ref_record_id, refs_empty, refs_exact, required_bytes, required_bytes32, required_identity_id,
    required_nonempty_text, required_u64, text_field,
};

/// Maximum size of an inline anchor proof before it must be moved to a
/// blob (spec 003 §8.1).
const MAX_INLINE_PROOF: usize = 4096;

/// An `anchor` proof (spec 003 §8.1): "bytes or BlobId, inline if <=
/// 4096 bytes else a blob".
///
/// The wire encoding is a single `bytes` field with no separate
/// discriminator, so this crate resolves the two cases by length: a
/// 32-byte value decodes as [`AnchorProof::Blob`] (a `BlobId`
/// pointing at the real proof), anything else decodes as
/// [`AnchorProof::Inline`] if it is at most 4096 bytes. This is a
/// judgment call, not something spec 003 spells out — see this crate's
/// top-level report for the reasoning and its one known edge case (an
/// inline proof that happens to be exactly 32 bytes is indistinguishable
/// from a blob reference).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorProof {
    /// The proof bytes themselves, at most 4096 bytes (and not 32 bytes,
    /// which would be read back as [`AnchorProof::Blob`]).
    Inline(Vec<u8>),
    /// A blob holding a larger system-specific proof.
    Blob(BlobId),
}

impl AnchorProof {
    fn to_bytes(&self) -> Vec<u8> {
        match self {
            AnchorProof::Inline(b) => b.clone(),
            AnchorProof::Blob(id) => id.as_bytes().to_vec(),
        }
    }

    fn parse(raw: Vec<u8>) -> Result<AnchorProof> {
        if raw.len() == 32 {
            match <[u8; 32]>::try_from(raw.as_slice()) {
                Ok(arr) => Ok(AnchorProof::Blob(BlobId(arr))),
                Err(_) => Err(Error::Kind(
                    "anchor: proof must be 32 bytes to parse as a blob id",
                )),
            }
        } else if raw.len() <= MAX_INLINE_PROOF {
            Ok(AnchorProof::Inline(raw))
        } else {
            Err(Error::Kind(
                "anchor: proof too large; use a blob reference (32 bytes)",
            ))
        }
    }
}

/// Commits a batch of observed record ids to an external timestamp
/// system (spec 003 §8.1, spec 001 §10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// Merkle root over the batch (spec 003 §8.1.1).
    pub root: [u8; 32],
    /// Number of ids in the batch.
    pub count: u64,
    /// External system name, e.g. `opentimestamps`.
    pub system: String,
    /// System-specific proof.
    pub proof: AnchorProof,
}

impl Anchor {
    /// Parse and validate an `anchor` record body (spec 003 §8.1): refs
    /// MUST be empty; no validation beyond schema (proofs are evaluated
    /// against the named system by interested verifiers, not by this
    /// crate).
    pub fn parse(record: &Record) -> Result<Anchor> {
        refs_empty(record, "anchor: refs must be empty")?;
        let body = record.body();
        let root = required_bytes32(body, "root", "anchor: root required (32 bytes)")?;
        let count = required_u64(body, "count", "anchor: count required")?;
        let system = required_nonempty_text(body, "system", usize::MAX, "anchor: system required")?;
        let raw = required_bytes(body, "proof", "anchor: proof required")?;
        let proof = AnchorProof::parse(raw)?;
        Ok(Anchor {
            root,
            count,
            system,
            proof,
        })
    }

    /// Build the CBOR body for this anchor.
    pub fn to_body(&self) -> Value {
        Value::Map(vec![
            (Value::Text("root".into()), Value::Bytes(self.root.to_vec())),
            (Value::Text("count".into()), Value::Uint(self.count)),
            (
                Value::Text("system".into()),
                Value::Text(self.system.clone()),
            ),
            (
                Value::Text("proof".into()),
                Value::Bytes(self.proof.to_bytes()),
            ),
        ])
    }
}

/// Wraps a content key to one recipient (spec 003 §8.2, spec 008 §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keygrant {
    /// The encrypted manifest (`refs[0]`).
    pub subject: RecordId,
    /// Recipient identity.
    pub recipient: IdentityId,
    /// Key-wrap algorithm id (spec 008 §4). Not checked against a
    /// registry here — see this crate's top-level report.
    pub wrap_alg: u64,
    /// The wrapped key.
    pub wrapped_key: Vec<u8>,
    /// Optional note.
    pub note: Option<String>,
}

impl Keygrant {
    /// Parse and validate a `keygrant` record body (spec 003 §8.2):
    /// exactly one subject ref.
    pub fn parse(record: &Record) -> Result<Keygrant> {
        refs_exact(record, 1, "keygrant: refs must be exactly one subject ref")?;
        let subject = ref_record_id(&record.refs()[0], "keygrant: ref must be a record ref")?;
        let body = record.body();
        let recipient =
            required_identity_id(body, "recipient", "keygrant: recipient required (32 bytes)")?;
        let wrap_alg = required_u64(body, "wrap_alg", "keygrant: wrap_alg required")?;
        let wrapped_key = required_bytes(body, "wrapped_key", "keygrant: wrapped_key required")?;
        let note = text_field(body, "note", usize::MAX, "keygrant: note must be text")?;
        Ok(Keygrant {
            subject,
            recipient,
            wrap_alg,
            wrapped_key,
            note,
        })
    }

    /// Build the CBOR body for this keygrant.
    pub fn to_body(&self) -> Value {
        let mut e = vec![
            (
                Value::Text("recipient".into()),
                Value::Bytes(self.recipient.as_bytes().to_vec()),
            ),
            (Value::Text("wrap_alg".into()), Value::Uint(self.wrap_alg)),
            (
                Value::Text("wrapped_key".into()),
                Value::Bytes(self.wrapped_key.clone()),
            ),
        ];
        if let Some(n) = &self.note {
            e.push((Value::Text("note".into()), Value::Text(n.clone())));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the subject.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.subject)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;
    use crate::record::RecordBuilder;

    fn kp() -> Keypair {
        Keypair::from_secret_bytes(&[13u8; 32])
    }

    fn author() -> IdentityId {
        IdentityId([14; 32])
    }

    fn sign(kind: u64, refs: Vec<Ref>, body: Value) -> Record {
        RecordBuilder::new(kind)
            .created_at(1)
            .refs(refs)
            .body(body)
            .sign_as(&kp(), author())
            .unwrap()
    }

    #[test]
    fn anchor_round_trip_inline_proof() {
        let a = Anchor {
            root: [0x31; 32],
            count: 18_211,
            system: "opentimestamps".into(),
            proof: AnchorProof::Inline(vec![0x00, 0x63, 0xff]),
        };
        let record = sign(super::super::KIND_ANCHOR, vec![], a.to_body());
        let back = Anchor::parse(&record).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn anchor_round_trip_blob_proof() {
        let a = Anchor {
            root: [0x31; 32],
            count: 1,
            system: "opentimestamps".into(),
            proof: AnchorProof::Blob(BlobId([0x77; 32])),
        };
        let record = sign(super::super::KIND_ANCHOR, vec![], a.to_body());
        let back = Anchor::parse(&record).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn anchor_rejects_oversized_inline_proof() {
        let body = Value::Map(vec![
            (Value::Text("root".into()), Value::Bytes(vec![1; 32])),
            (Value::Text("count".into()), Value::Uint(1)),
            (
                Value::Text("system".into()),
                Value::Text("opentimestamps".into()),
            ),
            (Value::Text("proof".into()), Value::Bytes(vec![0u8; 4097])),
        ]);
        let record = sign(super::super::KIND_ANCHOR, vec![], body);
        assert!(matches!(Anchor::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn anchor_accepts_max_inline_proof_size() {
        let body = Value::Map(vec![
            (Value::Text("root".into()), Value::Bytes(vec![1; 32])),
            (Value::Text("count".into()), Value::Uint(1)),
            (
                Value::Text("system".into()),
                Value::Text("opentimestamps".into()),
            ),
            (Value::Text("proof".into()), Value::Bytes(vec![0u8; 4096])),
        ]);
        let record = sign(super::super::KIND_ANCHOR, vec![], body);
        assert!(Anchor::parse(&record).is_ok());
    }

    #[test]
    fn anchor_rejects_nonempty_refs() {
        let a = Anchor {
            root: [1; 32],
            count: 1,
            system: "opentimestamps".into(),
            proof: AnchorProof::Inline(vec![1, 2, 3]),
        };
        let record = sign(
            super::super::KIND_ANCHOR,
            vec![Ref::record(RecordId([1; 32]))],
            a.to_body(),
        );
        assert!(matches!(Anchor::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn keygrant_round_trip() {
        let k = Keygrant {
            subject: RecordId([1; 32]),
            recipient: IdentityId([2; 32]),
            wrap_alg: 1,
            wrapped_key: vec![0x8e, 0x44],
            note: None,
        };
        let record = sign(super::super::KIND_KEYGRANT, k.refs(), k.to_body());
        let back = Keygrant::parse(&record).unwrap();
        assert_eq!(back, k);
    }

    #[test]
    fn keygrant_rejects_wrong_refs_count() {
        let body = Value::Map(vec![
            (Value::Text("recipient".into()), Value::Bytes(vec![2; 32])),
            (Value::Text("wrap_alg".into()), Value::Uint(1)),
            (Value::Text("wrapped_key".into()), Value::Bytes(vec![1, 2])),
        ]);
        let record = sign(super::super::KIND_KEYGRANT, vec![], body);
        assert!(matches!(Keygrant::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn keygrant_rejects_missing_wrap_alg() {
        let body = Value::Map(vec![
            (Value::Text("recipient".into()), Value::Bytes(vec![2; 32])),
            (Value::Text("wrapped_key".into()), Value::Bytes(vec![1, 2])),
        ]);
        let record = sign(
            super::super::KIND_KEYGRANT,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Keygrant::parse(&record), Err(Error::Kind(_))));
    }
}
