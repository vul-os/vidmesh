//! Trust and economics kinds (spec 003 §7): `endorse.gateway`,
//! `receipt`, `attest`.

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::{IdentityId, RecordId};
use crate::record::{Record, Ref};

use super::{
    ref_record_id, refs_empty, refs_exact, required_identity_id, required_nonempty_text,
    required_u64, text_field,
};

fn is_https_origin(url: &str) -> bool {
    const PREFIX: &str = "https://";
    url.starts_with(PREFIX) && url.len() > PREFIX.len()
}

/// A creator's endorsement of a gateway (spec 003 §7.1). Withdrawal is a
/// `retract`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndorseGateway {
    /// Gateway origin, required, an `https://` URL.
    pub url: String,
    /// Optional note.
    pub note: Option<String>,
}

impl EndorseGateway {
    /// Parse and validate an `endorse.gateway` record body (spec 003
    /// §7.1): refs MUST be empty; `url` MUST be an https origin.
    pub fn parse(record: &Record) -> Result<EndorseGateway> {
        refs_empty(record, "endorse.gateway: refs must be empty")?;
        let body = record.body();
        let url = required_nonempty_text(body, "url", usize::MAX, "endorse.gateway: url required")?;
        if !is_https_origin(&url) {
            return Err(Error::Kind("endorse.gateway: url must be an https origin"));
        }
        let note = text_field(
            body,
            "note",
            usize::MAX,
            "endorse.gateway: note must be text",
        )?;
        Ok(EndorseGateway { url, note })
    }

    /// Build the CBOR body for this endorsement.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(Value::Text("url".into()), Value::Text(self.url.clone()))];
        if let Some(n) = &self.note {
            e.push((Value::Text("note".into()), Value::Text(n.clone())));
        }
        Value::Map(e)
    }
}

/// A signed payment statement — the tip/superchat primitive (spec 003
/// §7.2, spec 010 §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// The paid-for record (`refs[0]`): a `manifest` or `live.manifest`
    /// record id.
    pub subject: RecordId,
    /// Amount in minor units (cents, sats, ...); must be > 0.
    pub amount: u64,
    /// ISO 4217 code or rail-native unit (`USD`, `sat`).
    pub currency: String,
    /// Payment pointer type id (spec 010 §1).
    pub rail: u64,
    /// Recipient identity.
    pub payee: IdentityId,
    /// Rail-specific proof (preimage, tx ref).
    pub proof: Option<String>,
    /// Display message.
    pub message: Option<String>,
}

impl Receipt {
    /// Parse and validate a `receipt` record body (spec 003 §7.2):
    /// exactly one subject ref; `amount` > 0.
    pub fn parse(record: &Record) -> Result<Receipt> {
        refs_exact(record, 1, "receipt: refs must be exactly one subject ref")?;
        let subject = ref_record_id(&record.refs()[0], "receipt: ref must be a record ref")?;
        let body = record.body();
        let amount = required_u64(body, "amount", "receipt: amount required")?;
        if amount == 0 {
            return Err(Error::Kind("receipt: amount must be > 0"));
        }
        let currency =
            required_nonempty_text(body, "currency", usize::MAX, "receipt: currency required")?;
        let rail = required_u64(body, "rail", "receipt: rail required")?;
        let payee = required_identity_id(body, "payee", "receipt: payee required (32 bytes)")?;
        let proof = text_field(body, "proof", usize::MAX, "receipt: proof must be text")?;
        let message = text_field(body, "message", usize::MAX, "receipt: message must be text")?;
        Ok(Receipt {
            subject,
            amount,
            currency,
            rail,
            payee,
            proof,
            message,
        })
    }

    /// Build the CBOR body for this receipt.
    pub fn to_body(&self) -> Value {
        let mut e = vec![
            (Value::Text("amount".into()), Value::Uint(self.amount)),
            (
                Value::Text("currency".into()),
                Value::Text(self.currency.clone()),
            ),
            (Value::Text("rail".into()), Value::Uint(self.rail)),
            (
                Value::Text("payee".into()),
                Value::Bytes(self.payee.as_bytes().to_vec()),
            ),
        ];
        if let Some(p) = &self.proof {
            e.push((Value::Text("proof".into()), Value::Text(p.clone())));
        }
        if let Some(m) = &self.message {
            e.push((Value::Text("message".into()), Value::Text(m.clone())));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the subject.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.subject)]
    }
}

/// Portable third-party reputation: "G attests that K reached X." (spec
/// 003 §7.3)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attest {
    /// The subject identity's genesis or record id (`refs[0]`).
    pub subject: RecordId,
    /// Human-readable attestation.
    pub statement: String,
    /// Machine-readable payload, attester-defined.
    pub data: Option<Value>,
}

impl Attest {
    /// Parse and validate an `attest` record body (spec 003 §7.3): no
    /// validation beyond schema.
    pub fn parse(record: &Record) -> Result<Attest> {
        refs_exact(record, 1, "attest: refs must be exactly one subject ref")?;
        let subject = ref_record_id(&record.refs()[0], "attest: ref must be a record ref")?;
        let body = record.body();
        let statement =
            required_nonempty_text(body, "statement", usize::MAX, "attest: statement required")?;
        let data = match body.map_get("data") {
            None => None,
            Some(v) => {
                if v.as_map().is_none() {
                    return Err(Error::Kind("attest: data must be a map"));
                }
                Some(v.clone())
            }
        };
        Ok(Attest {
            subject,
            statement,
            data,
        })
    }

    /// Build the CBOR body for this attestation.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(
            Value::Text("statement".into()),
            Value::Text(self.statement.clone()),
        )];
        if let Some(d) = &self.data {
            e.push((Value::Text("data".into()), d.clone()));
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
        Keypair::from_secret_bytes(&[11u8; 32])
    }

    fn author() -> IdentityId {
        IdentityId([12; 32])
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
    fn endorse_gateway_round_trip() {
        let e = EndorseGateway {
            url: "https://watch.example.net".into(),
            note: None,
        };
        let record = sign(super::super::KIND_ENDORSE_GATEWAY, vec![], e.to_body());
        let back = EndorseGateway::parse(&record).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn endorse_gateway_rejects_non_https() {
        let body = Value::Map(vec![(
            Value::Text("url".into()),
            Value::Text("http://watch.example.net".into()),
        )]);
        let record = sign(super::super::KIND_ENDORSE_GATEWAY, vec![], body);
        assert!(matches!(
            EndorseGateway::parse(&record),
            Err(Error::Kind(_))
        ));
    }

    #[test]
    fn endorse_gateway_rejects_nonempty_refs() {
        let body = Value::Map(vec![(
            Value::Text("url".into()),
            Value::Text("https://a.example".into()),
        )]);
        let record = sign(
            super::super::KIND_ENDORSE_GATEWAY,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(
            EndorseGateway::parse(&record),
            Err(Error::Kind(_))
        ));
    }

    #[test]
    fn receipt_round_trip() {
        let r = Receipt {
            subject: RecordId([1; 32]),
            amount: 21_000,
            currency: "sat".into(),
            rail: 1,
            payee: IdentityId([2; 32]),
            proof: None,
            message: Some("for the river week 12".into()),
        };
        let record = sign(super::super::KIND_RECEIPT, r.refs(), r.to_body());
        let back = Receipt::parse(&record).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn receipt_rejects_zero_amount() {
        let body = Value::Map(vec![
            (Value::Text("amount".into()), Value::Uint(0)),
            (Value::Text("currency".into()), Value::Text("sat".into())),
            (Value::Text("rail".into()), Value::Uint(1)),
            (Value::Text("payee".into()), Value::Bytes(vec![2; 32])),
        ]);
        let record = sign(
            super::super::KIND_RECEIPT,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Receipt::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn receipt_rejects_wrong_refs_count() {
        let body = Value::Map(vec![
            (Value::Text("amount".into()), Value::Uint(1)),
            (Value::Text("currency".into()), Value::Text("sat".into())),
            (Value::Text("rail".into()), Value::Uint(1)),
            (Value::Text("payee".into()), Value::Bytes(vec![2; 32])),
        ]);
        let record = sign(super::super::KIND_RECEIPT, vec![], body);
        assert!(matches!(Receipt::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn attest_round_trip() {
        let a = Attest {
            subject: RecordId([1; 32]),
            statement: "100000 verified views on watch.example.net, 2026-06".into(),
            // Canonical key order (5-byte "value" sorts before 6-byte
            // "metric"): a signed record's nested maps are always canonical
            // once round-tripped, so the expected value must be too.
            data: Some(Value::Map(vec![
                (Value::Text("value".into()), Value::Uint(100_000)),
                (Value::Text("metric".into()), Value::Text("views".into())),
            ])),
        };
        let record = sign(super::super::KIND_ATTEST, a.refs(), a.to_body());
        let back = Attest::parse(&record).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn attest_rejects_wrong_refs_count() {
        let body = Value::Map(vec![(
            Value::Text("statement".into()),
            Value::Text("s".into()),
        )]);
        let record = sign(super::super::KIND_ATTEST, vec![], body);
        assert!(matches!(Attest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn attest_rejects_non_map_data() {
        let body = Value::Map(vec![
            (Value::Text("statement".into()), Value::Text("s".into())),
            (Value::Text("data".into()), Value::Uint(1)),
        ]);
        let record = sign(
            super::super::KIND_ATTEST,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Attest::parse(&record), Err(Error::Kind(_))));
    }
}
