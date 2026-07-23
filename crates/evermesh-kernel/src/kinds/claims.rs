//! Claim kinds (spec 003 §6.1-6.4, spec 005): assertions with
//! provenance, never verified truth. All claim kinds reference the
//! subject manifest as `refs[0]` = `[0, <manifest id>]`.

use crate::error::{Error, Result};
use crate::ids::{BlobId, IdentityId, RecordId};
use crate::record::{Record, Ref};

use super::{
    blob_id_array, ref_record_id, refs_exact, required_identity_id, required_nonempty_text,
    required_text, text_field, validate_license, KIND_CLAIM_AUTHOR, KIND_CLAIM_LICENSE,
    KIND_CLAIM_TRANSFER, KIND_NOTICE_COUNTER, KIND_NOTICE_TAKEDOWN,
};
use crate::codec::Value;

/// An authorship assertion (spec 003 §6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimAuthor {
    /// The subject manifest (`refs[0]`).
    pub subject: RecordId,
    /// Free-text statement.
    pub statement: Option<String>,
    /// Supporting evidence blobs.
    pub evidence: Vec<BlobId>,
}

impl ClaimAuthor {
    /// Parse and validate a `claim.author` record body (spec 003 §6.1).
    pub fn parse(record: &Record) -> Result<ClaimAuthor> {
        refs_exact(
            record,
            1,
            "claim.author: refs must be exactly one subject ref",
        )?;
        let subject = ref_record_id(&record.refs()[0], "claim.author: ref must be a record ref")?;
        let body = record.body();
        let statement = text_field(
            body,
            "statement",
            usize::MAX,
            "claim.author: statement must be text",
        )?;
        let evidence = blob_id_array(
            body,
            "evidence",
            "claim.author: evidence must be an array of blob ids",
        )?;
        Ok(ClaimAuthor {
            subject,
            statement,
            evidence,
        })
    }

    /// Build the CBOR body for this claim.
    pub fn to_body(&self) -> Value {
        let mut e = Vec::new();
        if let Some(s) = &self.statement {
            e.push((Value::Text("statement".into()), Value::Text(s.clone())));
        }
        if !self.evidence.is_empty() {
            e.push((
                Value::Text("evidence".into()),
                Value::Array(
                    self.evidence
                        .iter()
                        .map(|b| Value::Bytes(b.as_bytes().to_vec()))
                        .collect(),
                ),
            ));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the subject.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.subject)]
    }
}

/// A licensing assertion (spec 003 §6.2, spec 004 §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimLicense {
    /// The subject manifest (`refs[0]`).
    pub subject: RecordId,
    /// SPDX id or Evermesh value (spec 004 §4), 1-64 bytes.
    pub license: String,
    /// Free-text terms or pointer.
    pub terms: Option<String>,
}

impl ClaimLicense {
    /// Parse and validate a `claim.license` record body (spec 003 §6.2).
    pub fn parse(record: &Record) -> Result<ClaimLicense> {
        refs_exact(
            record,
            1,
            "claim.license: refs must be exactly one subject ref",
        )?;
        let subject = ref_record_id(&record.refs()[0], "claim.license: ref must be a record ref")?;
        let body = record.body();
        let license = required_text(body, "license", 4096, "claim.license: license required")?;
        validate_license(&license)?;
        let terms = text_field(
            body,
            "terms",
            usize::MAX,
            "claim.license: terms must be text",
        )?;
        Ok(ClaimLicense {
            subject,
            license,
            terms,
        })
    }

    /// Build the CBOR body for this claim.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(
            Value::Text("license".into()),
            Value::Text(self.license.clone()),
        )];
        if let Some(t) = &self.terms {
            e.push((Value::Text("terms".into()), Value::Text(t.clone())));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the subject.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.subject)]
    }
}

/// A rights-transfer assertion (spec 003 §6.3). The author is the
/// assignor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimTransfer {
    /// The subject manifest (`refs[0]`).
    pub subject: RecordId,
    /// Identity receiving the rights.
    pub assignee: IdentityId,
    /// Free-text note.
    pub note: Option<String>,
}

impl ClaimTransfer {
    /// Parse and validate a `claim.transfer` record body (spec 003
    /// §6.3).
    ///
    /// Not checked here (needs the wider claims chain): that the
    /// assignor's own claim position is supported (spec 005 §3).
    pub fn parse(record: &Record) -> Result<ClaimTransfer> {
        refs_exact(
            record,
            1,
            "claim.transfer: refs must be exactly one subject ref",
        )?;
        let subject = ref_record_id(
            &record.refs()[0],
            "claim.transfer: ref must be a record ref",
        )?;
        let body = record.body();
        let assignee = required_identity_id(
            body,
            "assignee",
            "claim.transfer: assignee required (32 bytes)",
        )?;
        let note = text_field(
            body,
            "note",
            usize::MAX,
            "claim.transfer: note must be text",
        )?;
        Ok(ClaimTransfer {
            subject,
            assignee,
            note,
        })
    }

    /// Build the CBOR body for this claim.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(
            Value::Text("assignee".into()),
            Value::Bytes(self.assignee.as_bytes().to_vec()),
        )];
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

/// A dispute of an earlier claim or notice (spec 003 §6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimDispute {
    /// The disputed claim or notice record (`refs[0]`).
    pub target: RecordId,
    /// Statement, required.
    pub statement: String,
    /// Supporting evidence blobs.
    pub evidence: Vec<BlobId>,
}

impl ClaimDispute {
    /// Parse and validate a `claim.dispute` record body (spec 003 §6.4).
    ///
    /// Not checked here (needs the target record's kind): that the
    /// target is kind 48-50 or 64-65 — see
    /// [`check_dispute_target_kind`].
    pub fn parse(record: &Record) -> Result<ClaimDispute> {
        refs_exact(
            record,
            1,
            "claim.dispute: refs must be exactly one target ref",
        )?;
        let target = ref_record_id(&record.refs()[0], "claim.dispute: ref must be a record ref")?;
        let body = record.body();
        let statement = required_nonempty_text(
            body,
            "statement",
            usize::MAX,
            "claim.dispute: statement required",
        )?;
        let evidence = blob_id_array(
            body,
            "evidence",
            "claim.dispute: evidence must be an array of blob ids",
        )?;
        Ok(ClaimDispute {
            target,
            statement,
            evidence,
        })
    }

    /// Build the CBOR body for this dispute.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(
            Value::Text("statement".into()),
            Value::Text(self.statement.clone()),
        )];
        if !self.evidence.is_empty() {
            e.push((
                Value::Text("evidence".into()),
                Value::Array(
                    self.evidence
                        .iter()
                        .map(|b| Value::Bytes(b.as_bytes().to_vec()))
                        .collect(),
                ),
            ));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the target.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.target)]
    }
}

/// Checks the cross-record half of `claim.dispute` validity (spec 003
/// §6.4): the target must be kind `claim.author` (48), `claim.license`
/// (49), `claim.transfer` (50), `notice.takedown` (64), or
/// `notice.counter` (65). [`ClaimDispute::parse`] cannot check this
/// alone — the target's kind is not part of the dispute record's own
/// body.
pub fn check_dispute_target_kind(target_kind: u64) -> Result<()> {
    match target_kind {
        KIND_CLAIM_AUTHOR | KIND_CLAIM_LICENSE | KIND_CLAIM_TRANSFER | KIND_NOTICE_TAKEDOWN
        | KIND_NOTICE_COUNTER => Ok(()),
        _ => Err(Error::Kind(
            "claim.dispute: target must be a claim or notice record",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;
    use crate::record::RecordBuilder;

    fn kp() -> Keypair {
        Keypair::from_secret_bytes(&[5u8; 32])
    }

    fn author() -> IdentityId {
        IdentityId([6; 32])
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
    fn claim_author_round_trip() {
        let c = ClaimAuthor {
            subject: RecordId([1; 32]),
            statement: Some("recorded by me, 2025-11-02".into()),
            evidence: vec![BlobId([2; 32])],
        };
        let record = sign(super::super::KIND_CLAIM_AUTHOR, c.refs(), c.to_body());
        let back = ClaimAuthor::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn claim_author_rejects_wrong_refs_count() {
        let record = sign(super::super::KIND_CLAIM_AUTHOR, vec![], Value::Map(vec![]));
        assert!(matches!(ClaimAuthor::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn claim_license_round_trip() {
        let c = ClaimLicense {
            subject: RecordId([1; 32]),
            license: "CC-BY-4.0".into(),
            terms: None,
        };
        let record = sign(super::super::KIND_CLAIM_LICENSE, c.refs(), c.to_body());
        let back = ClaimLicense::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn claim_license_rejects_empty_license() {
        let body = Value::Map(vec![(
            Value::Text("license".into()),
            Value::Text("".into()),
        )]);
        let record = sign(
            super::super::KIND_CLAIM_LICENSE,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(ClaimLicense::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn claim_transfer_round_trip() {
        let c = ClaimTransfer {
            subject: RecordId([1; 32]),
            assignee: IdentityId([9; 32]),
            note: Some("per agreement of 2026-01-15".into()),
        };
        let record = sign(super::super::KIND_CLAIM_TRANSFER, c.refs(), c.to_body());
        let back = ClaimTransfer::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn claim_transfer_rejects_missing_assignee() {
        let record = sign(
            super::super::KIND_CLAIM_TRANSFER,
            vec![Ref::record(RecordId([1; 32]))],
            Value::Map(vec![]),
        );
        assert!(matches!(ClaimTransfer::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn claim_dispute_round_trip() {
        let c = ClaimDispute {
            target: RecordId([1; 32]),
            statement: "prior publication 2024-06; see archive capture".into(),
            evidence: vec![BlobId([2; 32])],
        };
        let record = sign(super::super::KIND_CLAIM_DISPUTE, c.refs(), c.to_body());
        let back = ClaimDispute::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn claim_dispute_rejects_empty_statement() {
        let body = Value::Map(vec![(
            Value::Text("statement".into()),
            Value::Text("".into()),
        )]);
        let record = sign(
            super::super::KIND_CLAIM_DISPUTE,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(ClaimDispute::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn check_dispute_target_kind_accepts_claims_and_notices() {
        for k in [
            super::super::KIND_CLAIM_AUTHOR,
            super::super::KIND_CLAIM_LICENSE,
            super::super::KIND_CLAIM_TRANSFER,
            super::super::KIND_NOTICE_TAKEDOWN,
            super::super::KIND_NOTICE_COUNTER,
        ] {
            assert!(check_dispute_target_kind(k).is_ok());
        }
    }

    #[test]
    fn check_dispute_target_kind_rejects_other_kinds() {
        assert!(check_dispute_target_kind(super::super::KIND_COMMENT).is_err());
        assert!(check_dispute_target_kind(super::super::KIND_MANIFEST).is_err());
    }
}
