//! Compliance kinds (spec 003 §6.5-6.7): `notice.takedown`,
//! `notice.counter`, `feed.takedown`. A notice obligates no one by
//! protocol; gateways act on notices per their jurisdiction (spec 009
//! §3).

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::RecordId;
use crate::record::{Record, Ref};

use super::{
    ref_record_id, refs_empty, refs_exact, refs_min, required_array, required_nonempty_text,
    required_u64, text_field,
};

/// The claimant identified in a notice (spec 003 §6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimant {
    /// Claimant name, required.
    pub name: String,
    /// Contact information, required.
    pub contact: String,
    /// Optional principal the claimant represents.
    pub on_behalf_of: Option<String>,
}

impl Claimant {
    fn to_value(&self) -> Value {
        let mut e = vec![
            (Value::Text("name".into()), Value::Text(self.name.clone())),
            (
                Value::Text("contact".into()),
                Value::Text(self.contact.clone()),
            ),
        ];
        if let Some(o) = &self.on_behalf_of {
            e.push((Value::Text("on_behalf_of".into()), Value::Text(o.clone())));
        }
        Value::Map(e)
    }

    fn parse(v: &Value) -> Result<Claimant> {
        let name = required_nonempty_text(v, "name", usize::MAX, "claimant: name required")?;
        let contact =
            required_nonempty_text(v, "contact", usize::MAX, "claimant: contact required")?;
        let on_behalf_of = text_field(
            v,
            "on_behalf_of",
            usize::MAX,
            "claimant: on_behalf_of must be text",
        )?;
        Ok(Claimant {
            name,
            contact,
            on_behalf_of,
        })
    }
}

/// A structured legal takedown notice, as a signed record (spec 003
/// §6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeTakedown {
    /// One or more subject refs identifying the material (each `[0,
    /// manifest]` or `[1, blob]`).
    pub subjects: Vec<Ref>,
    /// Legal regime, e.g. `us-dmca-512`.
    pub regime: String,
    /// The claimant.
    pub claimant: Claimant,
    /// The sworn/good-faith statement the regime requires.
    pub statement: String,
    /// Identification of the allegedly infringed work.
    pub work: String,
    /// Natural-person signature line.
    pub signature_name: String,
}

impl NoticeTakedown {
    /// Parse and validate a `notice.takedown` record body (spec 003
    /// §6.5): at least one ref; all required fields non-empty.
    pub fn parse(record: &Record) -> Result<NoticeTakedown> {
        refs_min(
            record,
            1,
            "notice.takedown: at least one subject ref required",
        )?;
        let subjects = record.refs().to_vec();
        let body = record.body();
        let regime = required_nonempty_text(
            body,
            "regime",
            usize::MAX,
            "notice.takedown: regime required",
        )?;
        let claimant_v = body
            .map_get("claimant")
            .ok_or(Error::Kind("notice.takedown: claimant required"))?;
        let claimant = Claimant::parse(claimant_v)?;
        let statement = required_nonempty_text(
            body,
            "statement",
            usize::MAX,
            "notice.takedown: statement required",
        )?;
        let work =
            required_nonempty_text(body, "work", usize::MAX, "notice.takedown: work required")?;
        let signature_name = required_nonempty_text(
            body,
            "signature_name",
            usize::MAX,
            "notice.takedown: signature_name required",
        )?;
        Ok(NoticeTakedown {
            subjects,
            regime,
            claimant,
            statement,
            work,
            signature_name,
        })
    }

    /// Build the CBOR body for this notice.
    pub fn to_body(&self) -> Value {
        Value::Map(vec![
            (
                Value::Text("regime".into()),
                Value::Text(self.regime.clone()),
            ),
            (Value::Text("claimant".into()), self.claimant.to_value()),
            (
                Value::Text("statement".into()),
                Value::Text(self.statement.clone()),
            ),
            (Value::Text("work".into()), Value::Text(self.work.clone())),
            (
                Value::Text("signature_name".into()),
                Value::Text(self.signature_name.clone()),
            ),
        ])
    }

    /// The refs this record should carry: the subjects.
    pub fn refs(&self) -> Vec<Ref> {
        self.subjects.clone()
    }
}

/// A counter-notice (spec 003 §6.6): same schema as `notice.takedown`
/// except `work` is optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeCounter {
    /// The `notice.takedown` record being countered (`refs[0]`).
    pub target: RecordId,
    /// Legal regime, e.g. `us-dmca-512`.
    pub regime: String,
    /// The claimant.
    pub claimant: Claimant,
    /// The sworn/good-faith statement the regime requires.
    pub statement: String,
    /// Identification of the work; optional here (unlike
    /// `notice.takedown`).
    pub work: Option<String>,
    /// Natural-person signature line.
    pub signature_name: String,
}

impl NoticeCounter {
    /// Parse and validate a `notice.counter` record body (spec 003
    /// §6.6): exactly one ref; all required fields non-empty (`work` is
    /// optional).
    pub fn parse(record: &Record) -> Result<NoticeCounter> {
        refs_exact(
            record,
            1,
            "notice.counter: refs must be exactly one target ref",
        )?;
        let target = ref_record_id(
            &record.refs()[0],
            "notice.counter: ref must be a record ref",
        )?;
        let body = record.body();
        let regime = required_nonempty_text(
            body,
            "regime",
            usize::MAX,
            "notice.counter: regime required",
        )?;
        let claimant_v = body
            .map_get("claimant")
            .ok_or(Error::Kind("notice.counter: claimant required"))?;
        let claimant = Claimant::parse(claimant_v)?;
        let statement = required_nonempty_text(
            body,
            "statement",
            usize::MAX,
            "notice.counter: statement required",
        )?;
        let work = text_field(
            body,
            "work",
            usize::MAX,
            "notice.counter: work must be text",
        )?;
        let signature_name = required_nonempty_text(
            body,
            "signature_name",
            usize::MAX,
            "notice.counter: signature_name required",
        )?;
        Ok(NoticeCounter {
            target,
            regime,
            claimant,
            statement,
            work,
            signature_name,
        })
    }

    /// Build the CBOR body for this counter-notice.
    pub fn to_body(&self) -> Value {
        let mut e = vec![
            (
                Value::Text("regime".into()),
                Value::Text(self.regime.clone()),
            ),
            (Value::Text("claimant".into()), self.claimant.to_value()),
            (
                Value::Text("statement".into()),
                Value::Text(self.statement.clone()),
            ),
            (
                Value::Text("signature_name".into()),
                Value::Text(self.signature_name.clone()),
            ),
        ];
        if let Some(w) = &self.work {
            e.push((Value::Text("work".into()), Value::Text(w.clone())));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the target.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.target)]
    }
}

/// One entry in a `feed.takedown` batch's `add` list (spec 003 §6.7):
/// `[ref_type, hash, reason, notice?]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedEntry {
    /// 0 = record, 1 = blob.
    pub ref_type: u64,
    /// The referenced record or blob hash.
    pub hash: [u8; 32],
    /// SHOULD be a stable code (`copyright`, `court-order`, `csam`,
    /// `other`); not enforced to a fixed set.
    pub reason: String,
    /// Optional `notice.takedown` record id backing this entry.
    pub notice: Option<RecordId>,
}

impl FeedEntry {
    fn to_value(&self) -> Value {
        let mut arr = vec![
            Value::Uint(self.ref_type),
            Value::Bytes(self.hash.to_vec()),
            Value::Text(self.reason.clone()),
        ];
        if let Some(n) = &self.notice {
            arr.push(Value::Bytes(n.as_bytes().to_vec()));
        }
        Value::Array(arr)
    }

    fn parse(v: &Value) -> Result<FeedEntry> {
        let arr = v
            .as_array()
            .ok_or(Error::Kind("feed.takedown: add entry must be an array"))?;
        if arr.len() < 3 || arr.len() > 4 {
            return Err(Error::Kind(
                "feed.takedown: add entry must be [ref_type, hash, reason, notice?]",
            ));
        }
        let ref_type = arr[0]
            .as_u64()
            .ok_or(Error::Kind("feed.takedown: entry ref_type must be a uint"))?;
        if ref_type > 1 {
            return Err(Error::Kind("feed.takedown: entry ref_type must be 0 or 1"));
        }
        let hash_bytes = arr[1]
            .as_bytes()
            .ok_or(Error::Kind("feed.takedown: entry hash must be bytes"))?;
        let hash: [u8; 32] = hash_bytes
            .try_into()
            .map_err(|_| Error::Kind("feed.takedown: entry hash must be 32 bytes"))?;
        let reason = arr[2]
            .as_text()
            .ok_or(Error::Kind("feed.takedown: entry reason must be text"))?
            .to_string();
        let notice = match arr.get(3) {
            None => None,
            Some(Value::Null) => None,
            Some(v) => {
                let b = v
                    .as_bytes()
                    .ok_or(Error::Kind("feed.takedown: entry notice must be bytes"))?;
                let a: [u8; 32] = b
                    .try_into()
                    .map_err(|_| Error::Kind("feed.takedown: entry notice must be 32 bytes"))?;
                Some(RecordId(a))
            }
        };
        Ok(FeedEntry {
            ref_type,
            hash,
            reason,
            notice,
        })
    }
}

/// One entry in a `feed.takedown` batch's `remove` list (spec 003 §6.7):
/// `[ref_type, hash]`, reversing a prior `add`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedRemoveEntry {
    /// 0 = record, 1 = blob.
    pub ref_type: u64,
    /// The referenced record or blob hash.
    pub hash: [u8; 32],
}

impl FeedRemoveEntry {
    fn to_value(&self) -> Value {
        Value::Array(vec![
            Value::Uint(self.ref_type),
            Value::Bytes(self.hash.to_vec()),
        ])
    }

    fn parse(v: &Value) -> Result<FeedRemoveEntry> {
        let arr = v
            .as_array()
            .ok_or(Error::Kind("feed.takedown: remove entry must be an array"))?;
        if arr.len() != 2 {
            return Err(Error::Kind(
                "feed.takedown: remove entry must be [ref_type, hash]",
            ));
        }
        let ref_type = arr[0]
            .as_u64()
            .ok_or(Error::Kind("feed.takedown: entry ref_type must be a uint"))?;
        if ref_type > 1 {
            return Err(Error::Kind("feed.takedown: entry ref_type must be 0 or 1"));
        }
        let hash_bytes = arr[1]
            .as_bytes()
            .ok_or(Error::Kind("feed.takedown: entry hash must be bytes"))?;
        let hash: [u8; 32] = hash_bytes
            .try_into()
            .map_err(|_| Error::Kind("feed.takedown: entry hash must be 32 bytes"))?;
        Ok(FeedRemoveEntry { ref_type, hash })
    }
}

/// A subscribable compliance feed batch (spec 003 §6.7, spec 009 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedTakedown {
    /// Feed name, stable per publisher.
    pub feed: String,
    /// Monotonic batch number within the feed.
    pub seq: u64,
    /// Entries to add; MAY be empty but MUST be present.
    pub add: Vec<FeedEntry>,
    /// Entries to remove (reversing a prior add); MAY be empty but MUST
    /// be present.
    pub remove: Vec<FeedRemoveEntry>,
}

impl FeedTakedown {
    /// Parse and validate a `feed.takedown` record body (spec 003 §6.7):
    /// refs MUST be empty; `add`/`remove` MAY be empty but MUST be
    /// present.
    pub fn parse(record: &Record) -> Result<FeedTakedown> {
        refs_empty(record, "feed.takedown: refs must be empty")?;
        let body = record.body();
        let feed =
            required_nonempty_text(body, "feed", usize::MAX, "feed.takedown: feed required")?;
        let seq = required_u64(body, "seq", "feed.takedown: seq required")?;
        let add_v = required_array(body, "add", "feed.takedown: add required (may be empty)")?;
        let mut add = Vec::with_capacity(add_v.len());
        for item in add_v {
            add.push(FeedEntry::parse(item)?);
        }
        let remove_v = required_array(
            body,
            "remove",
            "feed.takedown: remove required (may be empty)",
        )?;
        let mut remove = Vec::with_capacity(remove_v.len());
        for item in remove_v {
            remove.push(FeedRemoveEntry::parse(item)?);
        }
        Ok(FeedTakedown {
            feed,
            seq,
            add,
            remove,
        })
    }

    /// Build the CBOR body for this feed batch.
    pub fn to_body(&self) -> Value {
        Value::Map(vec![
            (Value::Text("feed".into()), Value::Text(self.feed.clone())),
            (Value::Text("seq".into()), Value::Uint(self.seq)),
            (
                Value::Text("add".into()),
                Value::Array(self.add.iter().map(FeedEntry::to_value).collect()),
            ),
            (
                Value::Text("remove".into()),
                Value::Array(self.remove.iter().map(FeedRemoveEntry::to_value).collect()),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;
    use crate::ids::IdentityId;
    use crate::record::RecordBuilder;

    fn kp() -> Keypair {
        Keypair::from_secret_bytes(&[8u8; 32])
    }

    fn author() -> IdentityId {
        IdentityId([2; 32])
    }

    fn sign(kind: u64, refs: Vec<Ref>, body: Value) -> Record {
        RecordBuilder::new(kind)
            .created_at(1)
            .refs(refs)
            .body(body)
            .sign_as(&kp(), author())
            .unwrap()
    }

    fn sample_claimant() -> Claimant {
        Claimant {
            name: "Example Pictures LLC".into(),
            contact: "legal@example.com".into(),
            on_behalf_of: None,
        }
    }

    #[test]
    fn notice_takedown_round_trip() {
        let n = NoticeTakedown {
            subjects: vec![Ref::record(RecordId([1; 32]))],
            regime: "us-dmca-512".into(),
            claimant: sample_claimant(),
            statement: "I have a good faith belief...".into(),
            work: "\"Winter Film\" (2024), reg. PA0002419331".into(),
            signature_name: "J. Doe".into(),
        };
        let record = sign(super::super::KIND_NOTICE_TAKEDOWN, n.refs(), n.to_body());
        let back = NoticeTakedown::parse(&record).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn notice_takedown_rejects_empty_refs() {
        let body = Value::Map(vec![(
            Value::Text("regime".into()),
            Value::Text("us-dmca-512".into()),
        )]);
        let record = sign(super::super::KIND_NOTICE_TAKEDOWN, vec![], body);
        assert!(matches!(
            NoticeTakedown::parse(&record),
            Err(Error::Kind(_))
        ));
    }

    #[test]
    fn notice_takedown_rejects_missing_statement() {
        let body = Value::Map(vec![
            (
                Value::Text("regime".into()),
                Value::Text("us-dmca-512".into()),
            ),
            (Value::Text("claimant".into()), sample_claimant().to_value()),
            (Value::Text("work".into()), Value::Text("work".into())),
            (
                Value::Text("signature_name".into()),
                Value::Text("J. Doe".into()),
            ),
        ]);
        let record = sign(
            super::super::KIND_NOTICE_TAKEDOWN,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(
            NoticeTakedown::parse(&record),
            Err(Error::Kind(_))
        ));
    }

    #[test]
    fn notice_counter_round_trip_without_work() {
        let n = NoticeCounter {
            target: RecordId([1; 32]),
            regime: "us-dmca-512".into(),
            claimant: Claimant {
                name: "A. Creator".into(),
                contact: "a@example.net".into(),
                on_behalf_of: None,
            },
            statement: "I have a good faith belief the material was removed by mistake...".into(),
            work: None,
            signature_name: "A. Creator".into(),
        };
        let record = sign(super::super::KIND_NOTICE_COUNTER, n.refs(), n.to_body());
        let back = NoticeCounter::parse(&record).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn notice_counter_rejects_wrong_refs_count() {
        let body = Value::Map(vec![
            (
                Value::Text("regime".into()),
                Value::Text("us-dmca-512".into()),
            ),
            (Value::Text("claimant".into()), sample_claimant().to_value()),
            (Value::Text("statement".into()), Value::Text("s".into())),
            (
                Value::Text("signature_name".into()),
                Value::Text("A".into()),
            ),
        ]);
        let record = sign(super::super::KIND_NOTICE_COUNTER, vec![], body);
        assert!(matches!(NoticeCounter::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn feed_takedown_round_trip() {
        let f = FeedTakedown {
            feed: "example-org/us".into(),
            seq: 4182,
            add: vec![FeedEntry {
                ref_type: 1,
                hash: [0x0b; 32],
                reason: "copyright".into(),
                notice: Some(RecordId([0x91; 32])),
            }],
            remove: vec![],
        };
        let record = sign(super::super::KIND_FEED_TAKEDOWN, vec![], f.to_body());
        let back = FeedTakedown::parse(&record).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn feed_takedown_add_entry_without_notice_round_trips() {
        let f = FeedTakedown {
            feed: "example-org/us".into(),
            seq: 1,
            add: vec![FeedEntry {
                ref_type: 0,
                hash: [1; 32],
                reason: "court-order".into(),
                notice: None,
            }],
            remove: vec![FeedRemoveEntry {
                ref_type: 1,
                hash: [2; 32],
            }],
        };
        let record = sign(super::super::KIND_FEED_TAKEDOWN, vec![], f.to_body());
        let back = FeedTakedown::parse(&record).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn feed_takedown_allows_empty_add_and_remove() {
        let body = Value::Map(vec![
            (Value::Text("feed".into()), Value::Text("f".into())),
            (Value::Text("seq".into()), Value::Uint(1)),
            (Value::Text("add".into()), Value::Array(vec![])),
            (Value::Text("remove".into()), Value::Array(vec![])),
        ]);
        let record = sign(super::super::KIND_FEED_TAKEDOWN, vec![], body);
        assert!(FeedTakedown::parse(&record).is_ok());
    }

    #[test]
    fn feed_takedown_rejects_missing_add() {
        let body = Value::Map(vec![
            (Value::Text("feed".into()), Value::Text("f".into())),
            (Value::Text("seq".into()), Value::Uint(1)),
            (Value::Text("remove".into()), Value::Array(vec![])),
        ]);
        let record = sign(super::super::KIND_FEED_TAKEDOWN, vec![], body);
        assert!(matches!(FeedTakedown::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn feed_takedown_rejects_nonempty_refs() {
        let body = Value::Map(vec![
            (Value::Text("feed".into()), Value::Text("f".into())),
            (Value::Text("seq".into()), Value::Uint(1)),
            (Value::Text("add".into()), Value::Array(vec![])),
            (Value::Text("remove".into()), Value::Array(vec![])),
        ]);
        let record = sign(
            super::super::KIND_FEED_TAKEDOWN,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(FeedTakedown::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn feed_entry_rejects_bad_ref_type() {
        let v = Value::Array(vec![
            Value::Uint(2),
            Value::Bytes(vec![0; 32]),
            Value::Text("copyright".into()),
        ]);
        assert!(matches!(FeedEntry::parse(&v), Err(Error::Kind(_))));
    }
}
