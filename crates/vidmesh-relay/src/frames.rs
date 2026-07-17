//! The `/sync` wire frames (spec 006 §1) and their codec.
//!
//! Frames are binary WebSocket messages: one canonically-encoded CBOR
//! array whose first element is a text tag. The kernel's CBOR codec is
//! not exported for this purpose (it exists to decode/encode record
//! envelopes, not arbitrary frames), so this module implements a small,
//! self-contained, canonical-only CBOR reader/writer for exactly the
//! shapes frames need: unsigned ints, byte strings, text strings,
//! booleans, null, arrays, and maps (definite lengths only).
//!
//! The decoder enforces the same canonical-encoding discipline as spec
//! 001 §2 (shortest-form integers, definite lengths, no floats, no
//! tags, sorted-unique map keys) so that a frame has exactly one valid
//! encoding — mirroring the envelope's "reject, never re-canonicalize"
//! rule. It never panics on untrusted input: every fallible step
//! returns a [`FrameError`] instead of indexing out of bounds,
//! unwrapping, or allocating an attacker-controlled amount of memory.

use std::fmt;

use crate::filter::Filter;

/// Everything that can go wrong decoding a frame or the CBOR beneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Ran out of bytes before a value was complete.
    Truncated,
    /// Extra bytes remained after decoding one complete value.
    TrailingBytes,
    /// A length or integer head used more bytes than the shortest
    /// possible encoding (non-canonical).
    NonCanonical(&'static str),
    /// A CBOR major type appeared where a different one was required.
    UnexpectedType(&'static str),
    /// A well-formed but unsupported CBOR construct (indefinite length,
    /// tag, float, negative integer, ...): frames never use these.
    Unsupported(&'static str),
    /// Bytes were not valid UTF-8 where text was required.
    Utf8,
    /// The decoded value tree does not have the shape a frame requires.
    BadShape(&'static str),
    /// The frame's first element is a text tag this relay does not
    /// recognize.
    UnknownFrameTag(String),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::Truncated => write!(f, "truncated frame"),
            FrameError::TrailingBytes => write!(f, "trailing bytes after frame"),
            FrameError::NonCanonical(m) => write!(f, "non-canonical CBOR: {m}"),
            FrameError::UnexpectedType(m) => write!(f, "unexpected CBOR type: {m}"),
            FrameError::Unsupported(m) => write!(f, "unsupported CBOR construct: {m}"),
            FrameError::Utf8 => write!(f, "invalid UTF-8 text"),
            FrameError::BadShape(m) => write!(f, "malformed frame: {m}"),
            FrameError::UnknownFrameTag(t) => write!(f, "unknown frame tag {t:?}"),
        }
    }
}

impl std::error::Error for FrameError {}

// ---------------------------------------------------------------------
// A minimal CBOR value tree, canonical-encode and canonical-decode only.
// ---------------------------------------------------------------------

/// A decoded CBOR value, restricted to the shapes frames use.
///
/// `pub(crate)` so [`crate::filter`] can build/inspect filter maps
/// without a second codec.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Uint(u64),
    Bytes(Vec<u8>),
    Text(String),
    Bool(bool),
    Null,
    Array(Vec<Value>),
    /// Key-value pairs in canonical (sorted) order once encoded; may be
    /// given in any order when constructed for encoding.
    Map(Vec<(Value, Value)>),
}

impl Value {
    pub(crate) fn as_uint(&self) -> Option<u64> {
        match self {
            Value::Uint(v) => Some(*v),
            _ => None,
        }
    }

    pub(crate) fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub(crate) fn as_bytes32(&self) -> Option<[u8; 32]> {
        let b = self.as_bytes()?;
        if b.len() != 32 {
            return None;
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(b);
        Some(out)
    }

    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub(crate) fn as_map(&self) -> Option<&[(Value, Value)]> {
        match self {
            Value::Map(pairs) => Some(pairs),
            _ => None,
        }
    }
}

// -- encoding ------------------------------------------------------------

fn encode_head(buf: &mut Vec<u8>, major: u8, value: u64) {
    let major = major << 5;
    match value {
        0..=23 => buf.push(major | value as u8),
        24..=255 => {
            buf.push(major | 24);
            buf.push(value as u8);
        }
        256..=65_535 => {
            buf.push(major | 25);
            buf.extend_from_slice(&(value as u16).to_be_bytes());
        }
        65_536..=4_294_967_295 => {
            buf.push(major | 26);
            buf.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            buf.push(major | 27);
            buf.extend_from_slice(&value.to_be_bytes());
        }
    }
}

pub(crate) fn encode_value(buf: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Uint(x) => encode_head(buf, 0, *x),
        Value::Bytes(b) => {
            encode_head(buf, 2, b.len() as u64);
            buf.extend_from_slice(b);
        }
        Value::Text(s) => {
            encode_head(buf, 3, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        Value::Bool(false) => buf.push(0xF4),
        Value::Bool(true) => buf.push(0xF5),
        Value::Null => buf.push(0xF6),
        Value::Array(items) => {
            encode_head(buf, 4, items.len() as u64);
            for item in items {
                encode_value(buf, item);
            }
        }
        Value::Map(pairs) => {
            // Canonical order: sort by the bytewise encoding of the key
            // (spec 001 §2 rule 3), applied here to frame maps too.
            let mut encoded: Vec<(Vec<u8>, Vec<u8>)> = pairs
                .iter()
                .map(|(k, v)| {
                    let mut kb = Vec::new();
                    encode_value(&mut kb, k);
                    let mut vb = Vec::new();
                    encode_value(&mut vb, v);
                    (kb, vb)
                })
                .collect();
            encoded.sort_by(|a, b| a.0.cmp(&b.0));
            encode_head(buf, 5, encoded.len() as u64);
            for (k, v) in encoded {
                buf.extend_from_slice(&k);
                buf.extend_from_slice(&v);
            }
        }
    }
}

pub(crate) fn encode_value_to_vec(v: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_value(&mut buf, v);
    buf
}

// -- decoding --------------------------------------------------------------

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn peek_u8(&self) -> Result<u8, FrameError> {
        self.buf.get(self.pos).copied().ok_or(FrameError::Truncated)
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], FrameError> {
        let end = self.pos.checked_add(n).ok_or(FrameError::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(FrameError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    /// Read a length/value field from the low 5 bits of a head byte,
    /// enforcing the shortest-encoding rule (spec 001 §2 rule 2).
    fn read_length(&mut self, additional_info: u8) -> Result<u64, FrameError> {
        match additional_info {
            0..=23 => Ok(additional_info as u64),
            24 => {
                let v = *self.buf.get(self.pos).ok_or(FrameError::Truncated)? as u64;
                self.advance(1);
                if v < 24 {
                    return Err(FrameError::NonCanonical("1-byte length not minimal"));
                }
                Ok(v)
            }
            25 => {
                let b = self.read_bytes(2)?;
                let v = u16::from_be_bytes([b[0], b[1]]) as u64;
                if v <= 255 {
                    return Err(FrameError::NonCanonical("2-byte length not minimal"));
                }
                Ok(v)
            }
            26 => {
                let b = self.read_bytes(4)?;
                let v = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64;
                if v <= 65_535 {
                    return Err(FrameError::NonCanonical("4-byte length not minimal"));
                }
                Ok(v)
            }
            27 => {
                let b = self.read_bytes(8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                let v = u64::from_be_bytes(a);
                if v <= 4_294_967_295 {
                    return Err(FrameError::NonCanonical("8-byte length not minimal"));
                }
                Ok(v)
            }
            28..=30 => Err(FrameError::Unsupported("reserved additional info")),
            31 => Err(FrameError::Unsupported("indefinite length")),
            _ => Err(FrameError::Unsupported("invalid additional info")),
        }
    }

    /// Bound a claimed array/map/string length against the bytes we
    /// actually have left, so a tiny malicious frame cannot force a
    /// huge allocation.
    fn checked_len(&self, len: u64) -> Result<usize, FrameError> {
        if len > self.remaining() as u64 {
            return Err(FrameError::Truncated);
        }
        Ok(len as usize)
    }
}

fn decode_value(r: &mut Reader) -> Result<Value, FrameError> {
    let head = r.peek_u8()?;
    let major = head >> 5;
    let ai = head & 0x1F;
    match major {
        0 => {
            r.advance(1);
            Ok(Value::Uint(r.read_length(ai)?))
        }
        1 => Err(FrameError::Unsupported("negative integers")),
        2 => {
            r.advance(1);
            let raw_len = r.read_length(ai)?;
            let len = r.checked_len(raw_len)?;
            Ok(Value::Bytes(r.read_bytes(len)?.to_vec()))
        }
        3 => {
            r.advance(1);
            let raw_len = r.read_length(ai)?;
            let len = r.checked_len(raw_len)?;
            let bytes = r.read_bytes(len)?;
            let s = std::str::from_utf8(bytes).map_err(|_| FrameError::Utf8)?;
            Ok(Value::Text(s.to_string()))
        }
        4 => {
            r.advance(1);
            let raw_len = r.read_length(ai)?;
            let len = r.checked_len(raw_len)?;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(decode_value(r)?);
            }
            Ok(Value::Array(items))
        }
        5 => {
            r.advance(1);
            let raw_len = r.read_length(ai)?;
            let len = r.checked_len(raw_len)?;
            let mut pairs = Vec::with_capacity(len);
            let mut prev_key_bytes: Option<Vec<u8>> = None;
            for _ in 0..len {
                let key_start = r.pos;
                let k = decode_value(r)?;
                let key_bytes = r.buf[key_start..r.pos].to_vec();
                let v = decode_value(r)?;
                if let Some(prev) = &prev_key_bytes {
                    if key_bytes <= *prev {
                        return Err(FrameError::NonCanonical("map keys not strictly sorted"));
                    }
                }
                prev_key_bytes = Some(key_bytes);
                pairs.push((k, v));
            }
            Ok(Value::Map(pairs))
        }
        6 => Err(FrameError::Unsupported("tags")),
        7 => {
            r.advance(1);
            match ai {
                20 => Ok(Value::Bool(false)),
                21 => Ok(Value::Bool(true)),
                22 => Ok(Value::Null),
                _ => Err(FrameError::Unsupported("simple value or float")),
            }
        }
        _ => Err(FrameError::Unsupported("invalid major type")),
    }
}

pub(crate) fn decode_value_complete(bytes: &[u8]) -> Result<Value, FrameError> {
    let mut r = Reader::new(bytes);
    let v = decode_value(&mut r)?;
    if r.pos != bytes.len() {
        return Err(FrameError::TrailingBytes);
    }
    Ok(v)
}

// ---------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------

/// A frame sent by a client to the relay (spec 006 §1).
#[derive(Debug, Clone, PartialEq)]
pub enum ClientFrame {
    /// `["REQ", sub_id, filter]`
    Req { sub_id: String, filter: Filter },
    /// `["CLOSE", sub_id]`
    Close { sub_id: String },
    /// `["PUB", record, nonce]`
    Pub {
        record_bytes: Vec<u8>,
        nonce: Option<u64>,
    },
}

/// A frame sent by the relay to a client (spec 006 §1).
#[derive(Debug, Clone, PartialEq)]
pub enum RelayFrame {
    /// `["REC", sub_id, seq, record]`
    Rec {
        sub_id: String,
        seq: u64,
        record_bytes: Vec<u8>,
    },
    /// `["EOSE", sub_id]`
    Eose { sub_id: String },
    /// `["OK", id, accepted, reason]`
    Ok {
        id: [u8; 32],
        accepted: bool,
        reason: String,
    },
    /// `["CLOSED", sub_id, reason]`
    Closed { sub_id: String, reason: String },
}

/// `sub_id` MUST be at most this many bytes (spec 006 §1).
pub const MAX_SUB_ID_BYTES: usize = 64;

impl ClientFrame {
    pub fn decode(bytes: &[u8]) -> Result<Self, FrameError> {
        let value = decode_value_complete(bytes)?;
        let arr = value
            .as_array()
            .ok_or(FrameError::BadShape("frame is not an array"))?;
        let tag = arr
            .first()
            .and_then(Value::as_text)
            .ok_or(FrameError::BadShape("frame missing text tag"))?;
        match tag {
            "REQ" => {
                if arr.len() != 3 {
                    return Err(FrameError::BadShape("REQ takes [tag, sub_id, filter]"));
                }
                let sub_id = read_sub_id(&arr[1])?;
                let filter = Filter::from_value(&arr[2])
                    .map_err(|_| FrameError::BadShape("invalid filter"))?;
                Ok(ClientFrame::Req { sub_id, filter })
            }
            "CLOSE" => {
                if arr.len() != 2 {
                    return Err(FrameError::BadShape("CLOSE takes [tag, sub_id]"));
                }
                Ok(ClientFrame::Close {
                    sub_id: read_sub_id(&arr[1])?,
                })
            }
            "PUB" => {
                if arr.len() != 3 {
                    return Err(FrameError::BadShape("PUB takes [tag, record, nonce]"));
                }
                let record_bytes = arr[1]
                    .as_bytes()
                    .ok_or(FrameError::BadShape("PUB record must be bytes"))?
                    .to_vec();
                let nonce = match &arr[2] {
                    Value::Null => None,
                    Value::Uint(n) => Some(*n),
                    _ => return Err(FrameError::BadShape("PUB nonce must be uint or null")),
                };
                Ok(ClientFrame::Pub {
                    record_bytes,
                    nonce,
                })
            }
            other => Err(FrameError::UnknownFrameTag(other.to_string())),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let value = match self {
            ClientFrame::Req { sub_id, filter } => Value::Array(vec![
                Value::Text("REQ".to_string()),
                Value::Text(sub_id.clone()),
                filter.to_value(),
            ]),
            ClientFrame::Close { sub_id } => Value::Array(vec![
                Value::Text("CLOSE".to_string()),
                Value::Text(sub_id.clone()),
            ]),
            ClientFrame::Pub {
                record_bytes,
                nonce,
            } => Value::Array(vec![
                Value::Text("PUB".to_string()),
                Value::Bytes(record_bytes.clone()),
                match nonce {
                    Some(n) => Value::Uint(*n),
                    None => Value::Null,
                },
            ]),
        };
        encode_value_to_vec(&value)
    }
}

impl RelayFrame {
    pub fn decode(bytes: &[u8]) -> Result<Self, FrameError> {
        let value = decode_value_complete(bytes)?;
        let arr = value
            .as_array()
            .ok_or(FrameError::BadShape("frame is not an array"))?;
        let tag = arr
            .first()
            .and_then(Value::as_text)
            .ok_or(FrameError::BadShape("frame missing text tag"))?;
        match tag {
            "REC" => {
                if arr.len() != 4 {
                    return Err(FrameError::BadShape("REC takes [tag, sub_id, seq, record]"));
                }
                let sub_id = read_sub_id(&arr[1])?;
                let seq = arr[2]
                    .as_uint()
                    .ok_or(FrameError::BadShape("REC seq must be uint"))?;
                let record_bytes = arr[3]
                    .as_bytes()
                    .ok_or(FrameError::BadShape("REC record must be bytes"))?
                    .to_vec();
                Ok(RelayFrame::Rec {
                    sub_id,
                    seq,
                    record_bytes,
                })
            }
            "EOSE" => {
                if arr.len() != 2 {
                    return Err(FrameError::BadShape("EOSE takes [tag, sub_id]"));
                }
                Ok(RelayFrame::Eose {
                    sub_id: read_sub_id(&arr[1])?,
                })
            }
            "OK" => {
                if arr.len() != 4 {
                    return Err(FrameError::BadShape("OK takes [tag, id, accepted, reason]"));
                }
                let id = arr[1]
                    .as_bytes32()
                    .ok_or(FrameError::BadShape("OK id must be bytes(32)"))?;
                let accepted = match &arr[2] {
                    Value::Bool(b) => *b,
                    _ => return Err(FrameError::BadShape("OK accepted must be bool")),
                };
                let reason = arr[3]
                    .as_text()
                    .ok_or(FrameError::BadShape("OK reason must be text"))?
                    .to_string();
                Ok(RelayFrame::Ok {
                    id,
                    accepted,
                    reason,
                })
            }
            "CLOSED" => {
                if arr.len() != 3 {
                    return Err(FrameError::BadShape("CLOSED takes [tag, sub_id, reason]"));
                }
                let sub_id = read_sub_id(&arr[1])?;
                let reason = arr[2]
                    .as_text()
                    .ok_or(FrameError::BadShape("CLOSED reason must be text"))?
                    .to_string();
                Ok(RelayFrame::Closed { sub_id, reason })
            }
            other => Err(FrameError::UnknownFrameTag(other.to_string())),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let value = match self {
            RelayFrame::Rec {
                sub_id,
                seq,
                record_bytes,
            } => Value::Array(vec![
                Value::Text("REC".to_string()),
                Value::Text(sub_id.clone()),
                Value::Uint(*seq),
                Value::Bytes(record_bytes.clone()),
            ]),
            RelayFrame::Eose { sub_id } => Value::Array(vec![
                Value::Text("EOSE".to_string()),
                Value::Text(sub_id.clone()),
            ]),
            RelayFrame::Ok {
                id,
                accepted,
                reason,
            } => Value::Array(vec![
                Value::Text("OK".to_string()),
                Value::Bytes(id.to_vec()),
                Value::Bool(*accepted),
                Value::Text(reason.clone()),
            ]),
            RelayFrame::Closed { sub_id, reason } => Value::Array(vec![
                Value::Text("CLOSED".to_string()),
                Value::Text(sub_id.clone()),
                Value::Text(reason.clone()),
            ]),
        };
        encode_value_to_vec(&value)
    }
}

/// Encode a chunk-tree range proof as CBOR `[chunk_index, [sibling
/// hashes]]` (spec 006 §5.2, spec 001 §8). Used by [`crate::blobs`];
/// kept here so the frame codec remains the crate's one CBOR encoder.
pub(crate) fn encode_chunk_proof(chunk_index: u64, siblings: &[[u8; 32]]) -> Vec<u8> {
    let value = Value::Array(vec![
        Value::Uint(chunk_index),
        Value::Array(siblings.iter().map(|h| Value::Bytes(h.to_vec())).collect()),
    ]);
    encode_value_to_vec(&value)
}

fn read_sub_id(v: &Value) -> Result<String, FrameError> {
    let s = v
        .as_text()
        .ok_or(FrameError::BadShape("sub_id must be text"))?;
    if s.len() > MAX_SUB_ID_BYTES {
        return Err(FrameError::BadShape("sub_id exceeds 64 bytes"));
    }
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::Filter;

    #[test]
    fn req_round_trip() {
        let frame = ClientFrame::Req {
            sub_id: "sub1".to_string(),
            filter: Filter {
                kinds: Some(vec![1, 2]),
                authors: Some(vec![[7u8; 32]]),
                refs: None,
                ids: None,
                since: Some(42),
                limit: Some(10),
            },
        };
        let bytes = frame.encode();
        let decoded = ClientFrame::decode(&bytes).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn close_round_trip() {
        let frame = ClientFrame::Close {
            sub_id: "abc".to_string(),
        };
        assert_eq!(ClientFrame::decode(&frame.encode()).unwrap(), frame);
    }

    #[test]
    fn pub_round_trip_with_and_without_nonce() {
        let with_nonce = ClientFrame::Pub {
            record_bytes: vec![1, 2, 3],
            nonce: Some(9999),
        };
        assert_eq!(
            ClientFrame::decode(&with_nonce.encode()).unwrap(),
            with_nonce
        );

        let without_nonce = ClientFrame::Pub {
            record_bytes: vec![],
            nonce: None,
        };
        assert_eq!(
            ClientFrame::decode(&without_nonce.encode()).unwrap(),
            without_nonce
        );
    }

    #[test]
    fn rec_eose_ok_closed_round_trip() {
        let rec = RelayFrame::Rec {
            sub_id: "s".to_string(),
            seq: 7,
            record_bytes: vec![9, 9, 9],
        };
        assert_eq!(RelayFrame::decode(&rec.encode()).unwrap(), rec);

        let eose = RelayFrame::Eose {
            sub_id: "s".to_string(),
        };
        assert_eq!(RelayFrame::decode(&eose.encode()).unwrap(), eose);

        let ok = RelayFrame::Ok {
            id: [3u8; 32],
            accepted: true,
            reason: String::new(),
        };
        assert_eq!(RelayFrame::decode(&ok.encode()).unwrap(), ok);

        let closed = RelayFrame::Closed {
            sub_id: "s".to_string(),
            reason: "rate".to_string(),
        };
        assert_eq!(RelayFrame::decode(&closed.encode()).unwrap(), closed);
    }

    #[test]
    fn unknown_tag_is_reported_not_panicked() {
        let value = Value::Array(vec![Value::Text("NOPE".to_string())]);
        let bytes = encode_value_to_vec(&value);
        match ClientFrame::decode(&bytes) {
            Err(FrameError::UnknownFrameTag(t)) => assert_eq!(t, "NOPE"),
            other => panic!("expected UnknownFrameTag, got {other:?}"),
        }
    }

    #[test]
    fn truncated_input_errors_cleanly() {
        // A REQ frame's head byte claiming 3 array elements but with no
        // payload must error, not panic or read out of bounds.
        let mut buf = Vec::new();
        encode_head(&mut buf, 4, 3);
        assert_eq!(ClientFrame::decode(&buf), Err(FrameError::Truncated));
    }

    #[test]
    fn non_minimal_length_is_rejected() {
        // Encode the text "REQ" length (3) using the 1-byte-length form
        // (major 3, additional info 24) instead of the minimal small
        // immediate form: non-canonical, must be rejected.
        let mut buf = vec![(3u8 << 5) | 24, 3];
        buf.extend_from_slice(b"REQ");
        let mut frame_buf = Vec::new();
        encode_head(&mut frame_buf, 4, 1);
        frame_buf.extend_from_slice(&buf);
        assert_eq!(
            decode_value_complete(&frame_buf),
            Err(FrameError::NonCanonical("1-byte length not minimal"))
        );
    }

    #[test]
    fn unsorted_map_keys_are_rejected() {
        // Build a 2-entry map with keys "b" then "a" (wrong order).
        let mut buf = Vec::new();
        encode_head(&mut buf, 5, 2);
        encode_value(&mut buf, &Value::Text("b".to_string()));
        encode_value(&mut buf, &Value::Uint(1));
        encode_value(&mut buf, &Value::Text("a".to_string()));
        encode_value(&mut buf, &Value::Uint(2));
        assert_eq!(
            decode_value_complete(&buf),
            Err(FrameError::NonCanonical("map keys not strictly sorted"))
        );
    }

    #[test]
    fn chunk_proof_encodes_as_index_and_sibling_array() {
        let bytes = encode_chunk_proof(3, &[[1u8; 32], [2u8; 32]]);
        let value = decode_value_complete(&bytes).unwrap();
        let arr = value.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_uint(), Some(3));
        let siblings = arr[1].as_array().unwrap();
        assert_eq!(siblings.len(), 2);
        assert_eq!(siblings[0].as_bytes32(), Some([1u8; 32]));
        assert_eq!(siblings[1].as_bytes32(), Some([2u8; 32]));
    }

    #[test]
    fn sub_id_over_64_bytes_rejected() {
        let long = "x".repeat(65);
        let frame = ClientFrame::Close { sub_id: long };
        let bytes = frame.encode();
        assert!(matches!(
            ClientFrame::decode(&bytes),
            Err(FrameError::BadShape(_))
        ));
    }
}
