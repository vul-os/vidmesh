//! Bundle export and import (spec 007): single-file, self-verifying
//! containers of records and blobs.

use std::io::{Read, Write};

use crate::blob::{hash_blob, CHUNK_SIZE};
use crate::codec::{self, Value};
use crate::error::{Error, Result};
use crate::ids::BlobId;
use crate::record::Record;

/// Bundle magic: "EVMS" + format version 1 (spec 007 §1).
pub const MAGIC: [u8; 5] = *b"EVMS\x01";

/// Blobs larger than this are split into `bp` parts (spec 007 §1).
pub const PART_SPLIT_THRESHOLD: usize = 16 << 20;

/// Everything an importer accepted, plus a salvage report.
#[derive(Debug, Default)]
pub struct ImportResult {
    /// Envelope-valid records, deduplicated by id, in bundle order.
    pub records: Vec<Record>,
    /// Hash-verified blobs, deduplicated, in completion order.
    pub blobs: Vec<(BlobId, Vec<u8>)>,
    /// Human-readable reports for every skipped/invalid item.
    pub skipped: Vec<String>,
    /// True if the bundle ended without a well-formed `end` item.
    pub truncated: bool,
}

/// Export and import operations (spec 007).
pub struct Bundle;

impl Bundle {
    /// Write a bundle containing the given records and blobs.
    ///
    /// Callers pass what they selected (spec 007 §3 filtering happens at
    /// a higher layer); every blob is re-hashed and its id checked so an
    /// exporter can never emit a blob it cannot verify.
    pub fn export<W: Write>(
        records: &[Record],
        blobs: &[(BlobId, Vec<u8>)],
        mut out: W,
    ) -> Result<()> {
        let io = |_e: std::io::Error| Error::Io("bundle write failed");
        out.write_all(&MAGIC).map_err(io)?;
        write_item(
            &mut out,
            &Value::Array(vec![Value::Text("hdr".into()), Value::Map(Vec::new())]),
        )?;
        for record in records {
            write_item(
                &mut out,
                &Value::Array(vec![
                    Value::Text("r".into()),
                    Value::Bytes(record.to_canonical_cbor()),
                ]),
            )?;
        }
        let mut distinct: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();
        for (id, content) in blobs {
            if hash_blob(content) != *id {
                return Err(Error::Bundle("exporter blob does not match its id"));
            }
            if !distinct.insert(id.0) {
                continue;
            }
            if content.len() > PART_SPLIT_THRESHOLD {
                for (index, part) in content.chunks(CHUNK_SIZE).enumerate() {
                    write_item(
                        &mut out,
                        &Value::Array(vec![
                            Value::Text("bp".into()),
                            Value::Bytes(id.0.to_vec()),
                            Value::Uint(index as u64),
                            Value::Bytes(part.to_vec()),
                        ]),
                    )?;
                }
            } else {
                write_item(
                    &mut out,
                    &Value::Array(vec![
                        Value::Text("b".into()),
                        Value::Bytes(id.0.to_vec()),
                        Value::Bytes(content.clone()),
                    ]),
                )?;
            }
        }
        write_item(
            &mut out,
            &Value::Array(vec![
                Value::Text("end".into()),
                Value::Uint(records.len() as u64),
                Value::Uint(distinct.len() as u64),
            ]),
        )?;
        Ok(())
    }

    /// Read and verify a bundle (spec 007 §2). Invalid items are skipped
    /// and reported; valid items are salvaged. Memory use is bounded by
    /// the largest single blob, not the bundle.
    pub fn import<R: Read>(reader: R) -> Result<ImportResult> {
        let mut result = ImportResult::default();
        let mut records = Vec::new();
        let mut blobs = Vec::new();
        let truncated = import_streaming(
            reader,
            &mut |record| records.push(record),
            &mut |id, content| blobs.push((id, content)),
            &mut result.skipped,
        )?;
        result.records = records;
        result.blobs = blobs;
        result.truncated = truncated;
        Ok(result)
    }
}

/// Streaming import core: sinks receive each verified record/blob as it
/// completes. Returns whether the bundle was truncated. Errors only on a
/// bad magic or an unreadable stream — item-level problems are reported
/// and skipped (spec 007 §2 salvage rule).
pub fn import_streaming<R: Read>(
    mut reader: R,
    on_record: &mut dyn FnMut(Record),
    on_blob: &mut dyn FnMut(BlobId, Vec<u8>),
    skipped: &mut Vec<String>,
) -> Result<bool> {
    let mut magic = [0u8; 5];
    reader
        .read_exact(&mut magic)
        .map_err(|_| Error::Bundle("missing magic"))?;
    if magic != MAGIC {
        return Err(Error::Bundle("bad magic or unsupported version"));
    }

    let mut seen_records: std::collections::HashSet<[u8; 32]> = Default::default();
    let mut seen_blobs: std::collections::HashSet<[u8; 32]> = Default::default();
    let mut n_records: u64 = 0;
    let mut n_blobs: u64 = 0;
    // In-progress parted blob: (id, expected next index, accumulated bytes).
    let mut pending_parts: Option<([u8; 32], u64, Vec<u8>)> = None;
    let mut first = true;
    let mut ended = false;

    let flush_parts = |pending: &mut Option<([u8; 32], u64, Vec<u8>)>,
                       seen_blobs: &mut std::collections::HashSet<[u8; 32]>,
                       n_blobs: &mut u64,
                       on_blob: &mut dyn FnMut(BlobId, Vec<u8>),
                       skipped: &mut Vec<String>| {
        if let Some((id, _, content)) = pending.take() {
            let blob_id = BlobId(id);
            if hash_blob(&content) == blob_id {
                if seen_blobs.insert(id) {
                    *n_blobs += 1;
                    on_blob(blob_id, content);
                }
            } else {
                skipped.push(format!("parted blob {blob_id} failed hash verification"));
            }
        }
    };

    loop {
        let item_bytes = match read_item(&mut reader) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => break, // clean EOF at an item boundary
            Err(_) => {
                skipped.push("unreadable item; stopping".into());
                break;
            }
        };
        if ended {
            skipped.push("item after end marker".into());
            continue;
        }
        let value = match codec::decode_canonical(&item_bytes) {
            Ok(v) => v,
            Err(e) => {
                skipped.push(format!("undecodable item: {e}"));
                continue;
            }
        };
        let Some(arr) = value.as_array() else {
            skipped.push("item is not an array".into());
            continue;
        };
        let tag = arr.first().and_then(Value::as_text).unwrap_or("");
        if first {
            first = false;
            if tag != "hdr" {
                skipped.push("first item is not hdr".into());
            }
            if tag == "hdr" {
                continue;
            }
        }
        match tag {
            "hdr" => skipped.push("duplicate hdr".into()),
            "r" => {
                flush_parts(
                    &mut pending_parts,
                    &mut seen_blobs,
                    &mut n_blobs,
                    on_blob,
                    skipped,
                );
                let Some(bytes) = arr.get(1).and_then(Value::as_bytes) else {
                    skipped.push("r item without bytes".into());
                    continue;
                };
                match Record::from_cbor(bytes).and_then(|r| r.verify().map(|_| r)) {
                    Ok(record) => {
                        if seen_records.insert(record.id().0) {
                            n_records += 1;
                            on_record(record);
                        }
                    }
                    Err(e) => skipped.push(format!("invalid record: {e}")),
                }
            }
            "b" => {
                flush_parts(
                    &mut pending_parts,
                    &mut seen_blobs,
                    &mut n_blobs,
                    on_blob,
                    skipped,
                );
                let (Some(id), Some(content)) = (
                    arr.get(1).and_then(Value::as_bytes),
                    arr.get(2).and_then(Value::as_bytes),
                ) else {
                    skipped.push("b item malformed".into());
                    continue;
                };
                let Ok(id) = <[u8; 32]>::try_from(id) else {
                    skipped.push("b item id not 32 bytes".into());
                    continue;
                };
                let blob_id = BlobId(id);
                if hash_blob(content) != blob_id {
                    skipped.push(format!("blob {blob_id} failed hash verification"));
                } else if seen_blobs.insert(id) {
                    n_blobs += 1;
                    on_blob(blob_id, content.to_vec());
                }
            }
            "bp" => {
                let (Some(id), Some(index), Some(part)) = (
                    arr.get(1).and_then(Value::as_bytes),
                    arr.get(2).and_then(Value::as_u64),
                    arr.get(3).and_then(Value::as_bytes),
                ) else {
                    skipped.push("bp item malformed".into());
                    continue;
                };
                let Ok(id) = <[u8; 32]>::try_from(id) else {
                    skipped.push("bp item id not 32 bytes".into());
                    continue;
                };
                match &mut pending_parts {
                    Some((cur_id, next, content)) if *cur_id == id && *next == index => {
                        content.extend_from_slice(part);
                        *next += 1;
                        // Any part shorter than CHUNK_SIZE is the final one.
                        if part.len() < CHUNK_SIZE {
                            flush_parts(
                                &mut pending_parts,
                                &mut seen_blobs,
                                &mut n_blobs,
                                on_blob,
                                skipped,
                            );
                        }
                    }
                    Some(_) => {
                        skipped.push("non-contiguous blob part; dropping pending blob".into());
                        flush_parts(
                            &mut pending_parts,
                            &mut seen_blobs,
                            &mut n_blobs,
                            on_blob,
                            skipped,
                        );
                        pending_parts = None;
                    }
                    None if index == 0 => {
                        let mut content = Vec::new();
                        content.extend_from_slice(part);
                        if part.len() < CHUNK_SIZE {
                            pending_parts = Some((id, 1, content));
                            flush_parts(
                                &mut pending_parts,
                                &mut seen_blobs,
                                &mut n_blobs,
                                on_blob,
                                skipped,
                            );
                        } else {
                            pending_parts = Some((id, 1, content));
                        }
                    }
                    None => skipped.push("blob part does not start at index 0".into()),
                }
            }
            "end" => {
                flush_parts(
                    &mut pending_parts,
                    &mut seen_blobs,
                    &mut n_blobs,
                    on_blob,
                    skipped,
                );
                ended = true;
                let (Some(rc), Some(bc)) = (
                    arr.get(1).and_then(Value::as_u64),
                    arr.get(2).and_then(Value::as_u64),
                ) else {
                    skipped.push("end item malformed".into());
                    continue;
                };
                if rc != n_records || bc != n_blobs {
                    skipped.push(format!(
                        "end counts mismatch: declared {rc} records/{bc} blobs, \
                         verified {n_records}/{n_blobs}"
                    ));
                }
            }
            other => skipped.push(format!("unknown item tag {other:?}")),
        }
    }
    flush_parts(
        &mut pending_parts,
        &mut seen_blobs,
        &mut n_blobs,
        on_blob,
        skipped,
    );
    Ok(!ended)
}

fn write_item<W: Write>(out: &mut W, item: &Value) -> Result<()> {
    let bytes = codec::encode_canonical(item)?;
    out.write_all(&bytes)
        .map_err(|_| Error::Io("bundle write failed"))
}

/// Read exactly one complete CBOR item's bytes from a stream, without
/// decoding it. Returns Ok(None) on clean EOF at an item boundary.
///
/// Tracks nesting with an explicit stack of remaining-child counts so
/// memory is bounded by the item size, and enforces the same structural
/// limits as the codec (definite lengths, depth 64) so an attacker
/// cannot force unbounded reads.
fn read_item<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    // Each stack entry is the number of complete items still expected at
    // that nesting level.
    let mut stack: Vec<u64> = vec![1];
    let mut first_byte = true;

    while let Some(remaining) = stack.last_mut() {
        if *remaining == 0 {
            stack.pop();
            continue;
        }
        *remaining -= 1;

        let mut head = [0u8; 1];
        match reader.read_exact(&mut head) {
            Ok(()) => {}
            Err(_) if first_byte => return Ok(None),
            Err(_) => return Err(Error::Bundle("truncated item")),
        }
        first_byte = false;
        buf.push(head[0]);
        let major = head[0] >> 5;
        let additional = head[0] & 0x1f;

        let arg: u64 = match additional {
            0..=23 => additional as u64,
            24..=27 => {
                let n = 1usize << (additional - 24);
                let mut arg_bytes = [0u8; 8];
                reader
                    .read_exact(&mut arg_bytes[..n])
                    .map_err(|_| Error::Bundle("truncated item"))?;
                buf.extend_from_slice(&arg_bytes[..n]);
                let mut v: u64 = 0;
                for b in &arg_bytes[..n] {
                    v = (v << 8) | *b as u64;
                }
                v
            }
            _ => return Err(Error::Bundle("indefinite or reserved length in item")),
        };

        match major {
            0 | 1 | 7 => {}
            2 | 3 => {
                let len = usize::try_from(arg).map_err(|_| Error::Bundle("item too large"))?;
                if len > (64 << 20) {
                    return Err(Error::Bundle("item too large"));
                }
                let start = buf.len();
                buf.resize(start + len, 0);
                reader
                    .read_exact(&mut buf[start..])
                    .map_err(|_| Error::Bundle("truncated item"))?;
            }
            4 => {
                if stack.len() >= 64 {
                    return Err(Error::Bundle("item too deep"));
                }
                stack.push(arg);
            }
            5 => {
                if stack.len() >= 64 {
                    return Err(Error::Bundle("item too deep"));
                }
                let entries = arg.checked_mul(2).ok_or(Error::Bundle("item too large"))?;
                stack.push(entries);
            }
            6 => return Err(Error::Bundle("tag in item")),
            _ => return Err(Error::Bundle("unsupported item")),
        }
    }
    Ok(Some(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Identity, Keypair};

    fn sample_records() -> Vec<Record> {
        let kp = Keypair::from_secret_bytes(&[9u8; 32]);
        let (id, genesis) = Identity::genesis(&kp, &[], 604_800, 100).unwrap();
        let comment = crate::record::RecordBuilder::new(32)
            .created_at(200)
            .r#ref(crate::record::Ref::record(genesis.id()))
            .body(Value::Map(vec![(
                Value::Text("text".into()),
                Value::Text("hello".into()),
            )]))
            .sign_as(&kp, id)
            .unwrap();
        vec![genesis, comment]
    }

    fn roundtrip(records: &[Record], blobs: &[(BlobId, Vec<u8>)]) -> ImportResult {
        let mut out = Vec::new();
        Bundle::export(records, blobs, &mut out).unwrap();
        Bundle::import(&out[..]).unwrap()
    }

    #[test]
    fn export_import_round_trip() {
        let records = sample_records();
        let content = vec![7u8; 1000];
        let blobs = vec![(hash_blob(&content), content)];
        let result = roundtrip(&records, &blobs);
        assert_eq!(result.records, records);
        assert_eq!(result.blobs, blobs);
        assert!(result.skipped.is_empty(), "{:?}", result.skipped);
        assert!(!result.truncated);
    }

    #[test]
    fn large_blob_parts_round_trip() {
        let records = sample_records();
        let content: Vec<u8> = (0..(PART_SPLIT_THRESHOLD + 5))
            .map(|i| (i % 251) as u8)
            .collect();
        let blobs = vec![(hash_blob(&content), content)];
        let result = roundtrip(&records, &blobs);
        assert_eq!(result.blobs.len(), 1);
        assert_eq!(result.blobs[0], blobs[0]);
        assert!(result.skipped.is_empty(), "{:?}", result.skipped);
    }

    #[test]
    fn truncated_bundle_salvages_prefix() {
        let records = sample_records();
        let mut out = Vec::new();
        Bundle::export(&records, &[], &mut out).unwrap();
        out.truncate(out.len() - 3); // cut into the end item
        let result = Bundle::import(&out[..]).unwrap();
        assert_eq!(result.records, records);
        assert!(result.truncated);
    }

    #[test]
    fn corrupted_blob_is_skipped_not_fatal() {
        let records = sample_records();
        let content = vec![1u8; 100];
        let good = vec![2u8; 100];
        let mut blobs = vec![
            (hash_blob(&content), content),
            (hash_blob(&good), good.clone()),
        ];
        blobs[0].1[0] ^= 0xff; // corrupt after hashing
        let mut out = Vec::new();
        // export re-verifies, so build the corrupt bundle by hand:
        let err = Bundle::export(&records, &blobs, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, Error::Bundle(_)));
        // hand-craft: valid export of good blob, then splice a corrupt item
        out.clear();
        Bundle::export(&records, &[(hash_blob(&good), good.clone())], &mut out).unwrap();
        // remove end marker, add corrupt b item + fresh end
        let end_item = codec::encode_canonical(&Value::Array(vec![
            Value::Text("end".into()),
            Value::Uint(records.len() as u64),
            Value::Uint(1),
        ]))
        .unwrap();
        out.truncate(out.len() - end_item.len());
        let corrupt = codec::encode_canonical(&Value::Array(vec![
            Value::Text("b".into()),
            Value::Bytes([0xee; 32].to_vec()),
            Value::Bytes(vec![3u8; 10]),
        ]))
        .unwrap();
        out.extend_from_slice(&corrupt);
        out.extend_from_slice(&end_item);
        let result = Bundle::import(&out[..]).unwrap();
        assert_eq!(result.blobs.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert!(!result.truncated);
    }

    #[test]
    fn import_is_idempotent_under_duplication() {
        let records = sample_records();
        let content = vec![7u8; 64];
        let blobs = vec![(hash_blob(&content), content)];
        let mut out = Vec::new();
        // Two copies of everything: records duplicated in one bundle.
        let doubled: Vec<Record> = records.iter().chain(records.iter()).cloned().collect();
        Bundle::export(&doubled, &blobs, &mut out).unwrap();
        let result = Bundle::import(&out[..]).unwrap();
        assert_eq!(result.records.len(), records.len());
        assert_eq!(result.blobs.len(), 1);
    }

    #[test]
    fn bad_magic_rejected() {
        let err = Bundle::import(&b"NOPE\x01rest"[..]).unwrap_err();
        assert!(matches!(err, Error::Bundle(_)));
    }
}
