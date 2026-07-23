//! Live kinds (spec 003 §9, spec 004 §6): `live.manifest`, `live.chat`.

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::{BlobId, RecordId};
use crate::record::{Record, Ref};

use super::{
    ref_record_id, refs_exact, required_array, required_bool, required_nonempty_text, required_u64,
    text_field,
};

/// One segment batch entry in a `live.manifest` (spec 003 §9.1):
/// `[blob, duration_ms]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSegment {
    /// The segment blob.
    pub blob: BlobId,
    /// Segment duration in milliseconds.
    pub duration_ms: u64,
}

/// A rolling signed manifest of live segments (spec 003 §9.1, spec 004
/// §6).
///
/// `stream` is `None` for the first record of a stream (whose own
/// record id then *is* the stream id) and `Some(first record id)` for
/// every later record in the same stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveManifest {
    /// `None` for the stream's first record; `Some(first record id)`
    /// otherwise (from `refs[0]`).
    pub stream: Option<RecordId>,
    /// Required in the first record of a stream; optional afterwards.
    pub title: Option<String>,
    /// 0 for the stream's first record, then +1. Gaps are tolerated.
    pub seq: u64,
    /// Segment batch, in order.
    pub segments: Vec<LiveSegment>,
    /// `true` closes the stream.
    pub is_final: bool,
}

impl LiveManifest {
    /// Parse and validate a `live.manifest` record body (spec 003 §9.1):
    /// the first record of a stream has empty refs and a required
    /// `title`; every later record has exactly one ref (the stream id)
    /// and an optional `title`.
    pub fn parse(record: &Record) -> Result<LiveManifest> {
        let refs = record.refs();
        let stream =
            match refs.len() {
                0 => None,
                1 => Some(ref_record_id(
                    &refs[0],
                    "live.manifest: ref must be a record ref",
                )?),
                _ => return Err(Error::Kind(
                    "live.manifest: refs must be empty (first record) or exactly one stream ref",
                )),
            };
        let body = record.body();
        let title = text_field(body, "title", 512, "live.manifest: title too long")?;
        if stream.is_none() && title.is_none() {
            return Err(Error::Kind(
                "live.manifest: title required in the first record of a stream",
            ));
        }
        let seq = required_u64(body, "seq", "live.manifest: seq required")?;
        let segments_v = required_array(body, "segments", "live.manifest: segments required")?;
        let mut segments = Vec::with_capacity(segments_v.len());
        for v in segments_v {
            let arr = v.as_array().ok_or(Error::Kind(
                "live.manifest: segment must be [blob, duration_ms]",
            ))?;
            if arr.len() != 2 {
                return Err(Error::Kind(
                    "live.manifest: segment must have exactly 2 elements",
                ));
            }
            let blob_bytes = arr[0]
                .as_bytes()
                .ok_or(Error::Kind("live.manifest: segment blob must be bytes"))?;
            let blob: [u8; 32] = blob_bytes
                .try_into()
                .map_err(|_| Error::Kind("live.manifest: segment blob must be 32 bytes"))?;
            let duration_ms = arr[1].as_u64().ok_or(Error::Kind(
                "live.manifest: segment duration_ms must be a uint",
            ))?;
            segments.push(LiveSegment {
                blob: BlobId(blob),
                duration_ms,
            });
        }
        let is_final = required_bool(body, "final", "live.manifest: final required")?;
        Ok(LiveManifest {
            stream,
            title,
            seq,
            segments,
            is_final,
        })
    }

    /// Build the CBOR body for this live manifest record.
    pub fn to_body(&self) -> Value {
        let mut e = Vec::new();
        if let Some(t) = &self.title {
            e.push((Value::Text("title".into()), Value::Text(t.clone())));
        }
        e.push((Value::Text("seq".into()), Value::Uint(self.seq)));
        e.push((
            Value::Text("segments".into()),
            Value::Array(
                self.segments
                    .iter()
                    .map(|s| {
                        Value::Array(vec![
                            Value::Bytes(s.blob.as_bytes().to_vec()),
                            Value::Uint(s.duration_ms),
                        ])
                    })
                    .collect(),
            ),
        ));
        e.push((Value::Text("final".into()), Value::Bool(self.is_final)));
        Value::Map(e)
    }

    /// The refs this record should carry: empty for the stream's first
    /// record, one record ref (the stream id) otherwise.
    pub fn refs(&self) -> Vec<Ref> {
        match self.stream {
            Some(id) => vec![Ref::record(id)],
            None => vec![],
        }
    }
}

/// A live chat message (spec 003 §9.2). Ephemeral in spirit; relays MAY
/// expire these aggressively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveChat {
    /// The stream id (the first `live.manifest` record id of the
    /// stream), `refs[0]`.
    pub stream: RecordId,
    /// Chat text, non-empty, at most 2048 bytes.
    pub text: String,
}

impl LiveChat {
    /// Parse and validate a `live.chat` record body (spec 003 §9.2):
    /// exactly one stream ref; `text` non-empty, at most 2048 bytes.
    pub fn parse(record: &Record) -> Result<LiveChat> {
        refs_exact(record, 1, "live.chat: refs must be exactly one stream ref")?;
        let stream = ref_record_id(&record.refs()[0], "live.chat: ref must be a record ref")?;
        let body = record.body();
        let text = required_nonempty_text(
            body,
            "text",
            2048,
            "live.chat: text required (<=2048 bytes)",
        )?;
        Ok(LiveChat { stream, text })
    }

    /// Build the CBOR body for this chat message.
    pub fn to_body(&self) -> Value {
        Value::Map(vec![(
            Value::Text("text".into()),
            Value::Text(self.text.clone()),
        )])
    }

    /// The refs this record should carry: exactly one, the stream id.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.stream)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;
    use crate::ids::IdentityId;
    use crate::record::RecordBuilder;

    fn kp() -> Keypair {
        Keypair::from_secret_bytes(&[15u8; 32])
    }

    fn author() -> IdentityId {
        IdentityId([16; 32])
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
    fn live_manifest_stream_open_append_final_triple() {
        let open = LiveManifest {
            stream: None,
            title: Some("Field Notes live".into()),
            seq: 0,
            segments: vec![LiveSegment {
                blob: BlobId([1; 32]),
                duration_ms: 2000,
            }],
            is_final: false,
        };
        let open_record = sign(
            super::super::KIND_LIVE_MANIFEST,
            open.refs(),
            open.to_body(),
        );
        let back_open = LiveManifest::parse(&open_record).unwrap();
        assert_eq!(back_open, open);

        let stream_id = open_record.id();
        let append = LiveManifest {
            stream: Some(stream_id),
            title: None,
            seq: 1,
            segments: vec![LiveSegment {
                blob: BlobId([2; 32]),
                duration_ms: 2000,
            }],
            is_final: false,
        };
        let append_record = sign(
            super::super::KIND_LIVE_MANIFEST,
            append.refs(),
            append.to_body(),
        );
        assert_eq!(LiveManifest::parse(&append_record).unwrap(), append);

        let close = LiveManifest {
            stream: Some(stream_id),
            title: None,
            seq: 2,
            segments: vec![],
            is_final: true,
        };
        let close_record = sign(
            super::super::KIND_LIVE_MANIFEST,
            close.refs(),
            close.to_body(),
        );
        assert_eq!(LiveManifest::parse(&close_record).unwrap(), close);
    }

    #[test]
    fn live_manifest_rejects_empty_refs_title_missing() {
        let body = Value::Map(vec![
            (Value::Text("seq".into()), Value::Uint(0)),
            (Value::Text("segments".into()), Value::Array(vec![])),
            (Value::Text("final".into()), Value::Bool(false)),
        ]);
        let record = sign(super::super::KIND_LIVE_MANIFEST, vec![], body);
        assert!(matches!(LiveManifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn live_manifest_refs_shape_is_the_only_first_record_signal() {
        // Spec 003 §9.1 defines "first record of a stream" purely by
        // refs shape (empty), not by an out-of-band notion of sequence
        // position — so a record with a high `seq` but empty refs is a
        // (perhaps unusual, but fully valid) new stream's first record
        // as long as it carries the otherwise-required title. This is
        // the one live.manifest rule spec 004's "later record with
        // empty refs" test vector exercises, and it is fully checkable
        // from the record alone (no second record needed).
        let body = Value::Map(vec![
            (
                Value::Text("title".into()),
                Value::Text("unusual but valid".into()),
            ),
            (Value::Text("seq".into()), Value::Uint(5)),
            (Value::Text("segments".into()), Value::Array(vec![])),
            (Value::Text("final".into()), Value::Bool(false)),
        ]);
        let record = sign(super::super::KIND_LIVE_MANIFEST, vec![], body);
        assert!(LiveManifest::parse(&record).is_ok());

        // The same shape without a title is invalid: empty refs means
        // "first record", and first records require a title.
        let body_no_title = Value::Map(vec![
            (Value::Text("seq".into()), Value::Uint(5)),
            (Value::Text("segments".into()), Value::Array(vec![])),
            (Value::Text("final".into()), Value::Bool(false)),
        ]);
        let record_no_title = sign(super::super::KIND_LIVE_MANIFEST, vec![], body_no_title);
        assert!(matches!(
            LiveManifest::parse(&record_no_title),
            Err(Error::Kind(_))
        ));
    }

    #[test]
    fn live_manifest_rejects_too_many_refs() {
        let body = Value::Map(vec![
            (Value::Text("seq".into()), Value::Uint(1)),
            (Value::Text("segments".into()), Value::Array(vec![])),
            (Value::Text("final".into()), Value::Bool(false)),
        ]);
        let record = sign(
            super::super::KIND_LIVE_MANIFEST,
            vec![
                Ref::record(RecordId([1; 32])),
                Ref::record(RecordId([2; 32])),
            ],
            body,
        );
        assert!(matches!(LiveManifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn live_chat_round_trip() {
        let c = LiveChat {
            stream: RecordId([1; 32]),
            text: "gg \u{1F525}".into(),
        };
        let record = sign(super::super::KIND_LIVE_CHAT, c.refs(), c.to_body());
        let back = LiveChat::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn live_chat_rejects_empty_text() {
        let body = Value::Map(vec![(Value::Text("text".into()), Value::Text("".into()))]);
        let record = sign(
            super::super::KIND_LIVE_CHAT,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(LiveChat::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn live_chat_rejects_oversized_text() {
        let body = Value::Map(vec![(
            Value::Text("text".into()),
            Value::Text("x".repeat(2049)),
        )]);
        let record = sign(
            super::super::KIND_LIVE_CHAT,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(LiveChat::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn live_chat_rejects_wrong_refs_count() {
        let body = Value::Map(vec![(Value::Text("text".into()), Value::Text("hi".into()))]);
        let record = sign(super::super::KIND_LIVE_CHAT, vec![], body);
        assert!(matches!(LiveChat::parse(&record), Err(Error::Kind(_))));
    }
}
