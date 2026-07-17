//! Deterministic vector generator for the Vidmesh conformance suite
//! (build plan §11). Writes the full vector tree under
//! `tools/conformance/vectors/`.
//!
//! Determinism rules (enforced throughout this file, never relaxed):
//! * every keypair comes from a fixed secret-byte seed
//!   (`Keypair::from_secret_bytes`), never `Keypair::generate()`;
//! * every `created_at` is a fixed constant, never the wall clock;
//! * every blob is synthesized from a formula (byte pattern), never
//!   read from disk or randomly generated;
//! * output files are written in a stable, sorted order with
//!   pretty-printed, sorted-key JSON (`Vector::to_pretty_json`), so
//!   regenerating the suite produces a clean diff.
//!
//! This binary constructs every kind record with plain
//! [`RecordBuilder`] + [`Value`] bodies built directly from spec
//! 003/004's schemas, not via `vidmesh_kernel::kinds` (that module is
//! being written in parallel and is not part of the compiled crate
//! yet — see the top-level report).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use vidmesh_kernel::blob::{hash_blob, verify_chunk, CHUNK_SIZE};
use vidmesh_kernel::bundle::MAGIC as BUNDLE_MAGIC;
use vidmesh_kernel::codec::{self, Value};
use vidmesh_kernel::identity::{AlgKey, Identity, IdentityState, Keypair};
use vidmesh_kernel::ids::{BlobId, IdentityId, RecordId};
use vidmesh_kernel::record::{Record, RecordBuilder, Ref};

use vidmesh_conformance::vectors::{
    error_class, BundleExpected, IdentityExpected, Layer, Vector, VectorData,
};

// ---------------------------------------------------------------------
// Fixed, deterministic building blocks.
// ---------------------------------------------------------------------

const CREATED_AT: i64 = 1_700_000_000;
const CONTEST_WINDOW: u64 = 604_800;

/// Placeholder record ids used as ref targets. These do not need to
/// resolve to any record present in these fixtures: every kind-specific
/// check exercised here is checkable from a single record's own shape
/// (schema, refs count/type, field constraints), never a cross-record
/// lookup — those (e.g. `comment/parent-subject-mismatch`,
/// `retract`'s "target is not a rotation") are two-record semantic
/// rules and are intentionally not represented as `record-invalid`
/// vectors here (see the top-level report).
const TARGET_MANIFEST: RecordId = RecordId([0xaa; 32]);
const TARGET_CHANNEL: RecordId = RecordId([0xbb; 32]);
const TARGET_PARENT_COMMENT: RecordId = RecordId([0xcc; 32]);
const TARGET_NOTICE: RecordId = RecordId([0xdd; 32]);
const TARGET_GRANT: RecordId = RecordId([0xee; 32]);
const BLOB_A: BlobId = BlobId([0x01; 32]);
const BLOB_B: BlobId = BlobId([0x02; 32]);

fn kp(seed: u8) -> Keypair {
    Keypair::from_secret_bytes(&[seed; 32])
}

fn t(s: &str) -> Value {
    Value::Text(s.to_string())
}
fn u(n: u64) -> Value {
    Value::Uint(n)
}
fn by(bytes: &[u8]) -> Value {
    Value::Bytes(bytes.to_vec())
}
fn map(entries: Vec<(&str, Value)>) -> Value {
    Value::Map(entries.into_iter().map(|(k, v)| (t(k), v)).collect())
}

fn build(
    kind: u64,
    refs: Vec<Ref>,
    body: Value,
    signer: &Keypair,
    identity: IdentityId,
    created_at: i64,
) -> Record {
    RecordBuilder::new(kind)
        .created_at(created_at)
        .refs(refs)
        .body(body)
        .sign_as(signer, identity)
        .expect("fixture body is always a well-formed map")
}

fn out_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vectors")
}

fn write_vector(v: &Vector) {
    let dir = out_root().join(&v.group);
    fs::create_dir_all(&dir).expect("mkdir vectors/<group>");
    let path = dir.join(format!("{}.json", v.name));
    let mut json = v.to_pretty_json().expect("vector must serialize");
    json.push('\n');
    fs::write(&path, json).expect("write vector file");
}

// ---------------------------------------------------------------------
// Vector constructors.
// ---------------------------------------------------------------------

fn valid_vector(group: &str, name: &str, description: &str, record: &Record) -> Vector {
    let json: serde_json::Value =
        serde_json::from_str(&record.to_json()).expect("record.to_json() is valid JSON");
    Vector {
        group: group.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        data: VectorData::RecordValid {
            cbor_hex: hex::encode(record.to_canonical_cbor()),
            expected_id_hex: record.id().to_hex(),
            json,
        },
    }
}

fn invalid_vector(
    group: &str,
    name: &str,
    description: &str,
    cbor: Vec<u8>,
    expected_error: &str,
    layer: Layer,
) -> Vector {
    Vector {
        group: group.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        data: VectorData::RecordInvalid {
            cbor_hex: hex::encode(cbor),
            expected_error: expected_error.to_string(),
            layer,
        },
    }
}

// ---------------------------------------------------------------------
// Generic envelope mutations (used for the "3 mechanical mutations" on
// every kind, and for the richer envelope/ group).
// ---------------------------------------------------------------------

/// Decode `bytes` as a canonical envelope map, hand the 7 (key, value)
/// entries to `f`, and re-encode. Used for edits that must remain
/// canonical (everything except the deliberately non-canonical and
/// float mutations, which operate on raw bytes directly).
fn edit_envelope(bytes: &[u8], f: impl FnOnce(&mut Vec<(Value, Value)>)) -> Vec<u8> {
    let value = codec::decode_canonical(bytes).expect("fixture bytes are canonical");
    let mut entries = value.as_map().expect("envelope is a map").to_vec();
    f(&mut entries);
    codec::encode_canonical(&Value::Map(entries)).expect("edited envelope re-encodes")
}

/// Flip a bit in the signature (envelope key 7). Signature verification
/// must fail; the record still decodes and its id is unchanged (the id
/// excludes the signature, spec 001 §3).
fn flip_sig_byte(bytes: &[u8]) -> Vec<u8> {
    edit_envelope(bytes, |entries| {
        for (k, v) in entries.iter_mut() {
            if k.as_u64() == Some(7) {
                if let Value::Bytes(b) = v {
                    b[0] ^= 0xff;
                }
            }
        }
    })
}

/// Set `sig_alg` (envelope key 6) to an unregistered algorithm id.
fn set_unknown_sig_alg(bytes: &[u8], new_alg: u64) -> Vec<u8> {
    edit_envelope(bytes, |entries| {
        for (k, v) in entries.iter_mut() {
            if k.as_u64() == Some(6) {
                *v = Value::Uint(new_alg);
            }
        }
    })
}

/// Append an 8th envelope key. The envelope MUST contain exactly keys
/// 1-7 (spec 001 §1); this is also used as the generic "envelope
/// tamper" mutation applied to every kind.
fn add_envelope_key(bytes: &[u8]) -> Vec<u8> {
    edit_envelope(bytes, |entries| {
        entries.push((Value::Uint(8), Value::Uint(0)))
    })
}

fn truncate_last_byte(bytes: &[u8]) -> Vec<u8> {
    bytes[..bytes.len() - 1].to_vec()
}

/// Re-encode the CBOR integer head at `offset` using the next-larger
/// argument form than strictly necessary (e.g. a value that fits inline
/// gets the 1-byte-argument form instead). This is non-canonical per
/// spec 001 §2 rule 2 regardless of the value's magnitude, so it works
/// uniformly for every record: applied at offset 2 (right after the
/// fixed 1-byte 7-entry map header and the fixed 1-byte key-1 encoding,
/// both invariant across every record this generator builds), it always
/// widens the `kind` field's value.
fn widen_head_at(bytes: &[u8], offset: usize) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let head = out[offset];
    let major = head >> 5;
    let low = head & 0x1f;
    let (arg, arg_len): (u64, usize) = match low {
        0..=23 => (low as u64, 0),
        24 => (out[offset + 1] as u64, 1),
        25 => (
            u16::from_be_bytes([out[offset + 1], out[offset + 2]]) as u64,
            2,
        ),
        26 => (
            u32::from_be_bytes(out[offset + 1..offset + 5].try_into().unwrap()) as u64,
            4,
        ),
        27 => (
            u64::from_be_bytes(out[offset + 1..offset + 9].try_into().unwrap()),
            8,
        ),
        _ => panic!("reserved additional info at offset {offset}"),
    };
    let (new_low, new_arg): (u8, Vec<u8>) = match low {
        0..=23 => (24, vec![arg as u8]),
        24 => (25, (arg as u16).to_be_bytes().to_vec()),
        25 => (26, (arg as u32).to_be_bytes().to_vec()),
        26 => (27, arg.to_be_bytes().to_vec()),
        _ => panic!("cannot widen an already-8-byte argument"),
    };
    out[offset] = (major << 5) | new_low;
    out.splice(offset + 1..offset + 1 + arg_len, new_arg);
    out
}

fn make_non_canonical(bytes: &[u8]) -> Vec<u8> {
    widen_head_at(bytes, 2)
}

/// Splice `replacement` in place of the unique occurrence of `needle`.
/// Panics if `needle` does not appear exactly once — a self-check that
/// the fixture text chosen for [`inject_float_in_body`] really is
/// unique in the record's bytes (author key / signature bytes are
/// effectively random and won't collide with a chosen ASCII marker).
fn replace_unique(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let hits: Vec<usize> = (0..=haystack.len().saturating_sub(needle.len()))
        .filter(|&i| &haystack[i..i + needle.len()] == needle)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one occurrence of the marker text"
    );
    let pos = hits[0];
    let mut out = Vec::with_capacity(haystack.len() - needle.len() + replacement.len());
    out.extend_from_slice(&haystack[..pos]);
    out.extend_from_slice(replacement);
    out.extend_from_slice(&haystack[pos + needle.len()..]);
    out
}

/// Replace a body text value with a CBOR float32 (`0.0`). Floats MUST
/// NOT appear anywhere in a record (spec 001 §2 rule 4); the codec's
/// `Value` enum has no float variant at all, so this mutation works at
/// the raw byte level rather than through the `Value` type.
fn inject_float_in_body(bytes: &[u8], marker_text: &str) -> Vec<u8> {
    let needle = codec::encode_canonical(&Value::Text(marker_text.to_string())).unwrap();
    let replacement: [u8; 5] = [0xfa, 0x00, 0x00, 0x00, 0x00];
    replace_unique(bytes, &needle, &replacement)
}

/// The three mechanical invalid mutations applied to every kind
/// (build plan §11 / task brief): bad signature, non-canonical
/// encoding, envelope tamper (an added 8th envelope key).
fn mechanical_invalids(group: &str, valid: &Record) -> Vec<Vector> {
    let bytes = valid.to_canonical_cbor();
    vec![
        invalid_vector(
            group,
            "bad-sig",
            "Signature byte flipped; Record::verify must reject it (id is unaffected, the id excludes sig).",
            flip_sig_byte(&bytes),
            error_class::SIGNATURE,
            Layer::Envelope,
        ),
        invalid_vector(
            group,
            "non-canonical",
            "The `kind` envelope field's integer argument is re-encoded in a longer-than-shortest CBOR form.",
            make_non_canonical(&bytes),
            error_class::NON_CANONICAL,
            Layer::Envelope,
        ),
        invalid_vector(
            group,
            "envelope-tamper",
            "An 8th envelope key is appended; the envelope MUST contain exactly keys 1-7 (spec 001 §1).",
            add_envelope_key(&bytes),
            error_class::ENVELOPE,
            Layer::Envelope,
        ),
    ]
}

fn add_kind_group(vectors: &mut Vec<Vector>, group: &str, valid: &Record, valid_desc: &str) {
    vectors.push(valid_vector(group, "valid-basic", valid_desc, valid));
    vectors.extend(mechanical_invalids(group, valid));
}

fn kind_invalid(
    vectors: &mut Vec<Vector>,
    group: &str,
    name: &str,
    desc: &str,
    record: &Record,
    expected_error: &str,
) {
    vectors.push(invalid_vector(
        group,
        name,
        desc,
        record.to_canonical_cbor(),
        expected_error,
        Layer::Kind,
    ));
}

// ---------------------------------------------------------------------
// spec 004 §3.1 derivation statement (implemented here, not imported
// from `vidmesh_kernel::kinds`, per the task brief).
// ---------------------------------------------------------------------

const DERIVATION_SIG_PREFIX: &[u8] = b"vidmesh:derivation:v1";

fn derivation_statement(
    original: BlobId,
    rendition: BlobId,
    codec_str: &str,
    width: u64,
    height: u64,
    bitrate: u64,
) -> Vec<u8> {
    let value = Value::Array(vec![
        by(original.as_bytes()),
        by(rendition.as_bytes()),
        t(codec_str),
        u(width),
        u(height),
        u(bitrate),
    ]);
    codec::encode_canonical(&value).unwrap()
}

/// Sign a derivation statement (spec 004 §3.1). Reuses
/// `vidmesh_kernel::blob::hash_blob` for the BLAKE3-256 hashing step —
/// legitimate since both are literally "BLAKE3-256 of these bytes";
/// this crate deliberately does not add a direct `blake3` dependency.
fn sign_derivation(producer: &Keypair, stmt: &[u8]) -> Vec<u8> {
    let stmt_hash = hash_blob(stmt);
    let mut msg = Vec::with_capacity(DERIVATION_SIG_PREFIX.len() + 32);
    msg.extend_from_slice(DERIVATION_SIG_PREFIX);
    msg.extend_from_slice(stmt_hash.as_bytes());
    producer.sign(&msg).to_vec()
}

fn media_value(
    blob: BlobId,
    size: u64,
    chunk_root: Option<[u8; 32]>,
    codec_str: &str,
    duration_ms: u64,
    width: u64,
    height: u64,
) -> Vec<(&'static str, Value)> {
    let mut e = vec![
        ("blob", by(blob.as_bytes())),
        ("size", u(size)),
        ("codec", t(codec_str)),
        ("duration", u(duration_ms)),
        ("width", u(width)),
        ("height", u(height)),
    ];
    if let Some(cr) = chunk_root {
        e.push(("chunk_root", by(&cr)));
    }
    e
}

// ---------------------------------------------------------------------
// identity/ group helpers.
// ---------------------------------------------------------------------

fn identity_expected_from_state(s: &IdentityState) -> IdentityExpected {
    IdentityExpected {
        head_hex: s.head.to_hex(),
        signing_key_hex: hex::encode(&s.signing_key),
        depth: s.depth,
    }
}

fn identity_chain_vector(
    group: &str,
    name: &str,
    description: &str,
    records: &[Record],
    observed: BTreeMap<String, i64>,
    now: i64,
) -> Vector {
    let observed_at = |id: &RecordId| observed.get(&id.to_hex()).copied();
    let result = Identity::verify_chain(records, &observed_at, now);
    let (expected, expected_error) = match result {
        Ok(state) => (Some(identity_expected_from_state(&state)), None),
        Err(e) => (None, Some(error_class::classify(&e).to_string())),
    };
    Vector {
        group: group.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        data: VectorData::IdentityChain {
            records_hex: records
                .iter()
                .map(|r| hex::encode(r.to_canonical_cbor()))
                .collect(),
            now,
            observed,
            expected,
            expected_error,
        },
    }
}

// ---------------------------------------------------------------------
// main
// ---------------------------------------------------------------------

fn main() {
    let mut vectors: Vec<Vector> = Vec::new();

    build_envelope_group(&mut vectors);
    build_kind_groups(&mut vectors);
    build_identity_group(&mut vectors);
    build_chunktree_group(&mut vectors);
    build_bundle_group(&mut vectors);
    build_json_group(&mut vectors);

    // Clean the output tree first so stale vectors from a previous
    // generator run never linger.
    let root = out_root();
    if root.is_dir() {
        fs::remove_dir_all(&root).expect("clear vectors/ before regenerating");
    }
    fs::create_dir_all(&root).expect("create vectors/");

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for v in &vectors {
        *counts.entry(v.group.clone()).or_insert(0) += 1;
        write_vector(v);
    }

    println!("wrote {} vectors under {}", vectors.len(), root.display());
    for (group, count) in &counts {
        println!("  {group:<24} {count}");
    }
}

// =======================================================================
// envelope/
// =======================================================================

fn build_envelope_group(vectors: &mut Vec<Vector>) {
    let group = "envelope";
    let signer = kp(1);
    let identity = IdentityId([0x11; 32]);
    let body_text = "hello, vidmesh";
    let valid = build(
        32, // comment
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![("text", t(body_text))]),
        &signer,
        identity,
        CREATED_AT,
    );
    let bytes = valid.to_canonical_cbor();

    vectors.push(valid_vector(
        group,
        "valid-comment",
        "A plain, envelope-valid `comment` record (spec 001 §§1-4).",
        &valid,
    ));
    vectors.push(invalid_vector(
        group,
        "bad-sig",
        "Signature byte flipped.",
        flip_sig_byte(&bytes),
        error_class::SIGNATURE,
        Layer::Envelope,
    ));
    vectors.push(invalid_vector(
        group,
        "non-canonical",
        "The `kind` field's integer argument uses a longer-than-shortest CBOR form (spec 001 §2 rule 2).",
        make_non_canonical(&bytes),
        error_class::NON_CANONICAL,
        Layer::Envelope,
    ));
    vectors.push(invalid_vector(
        group,
        "truncated",
        "The last byte of an otherwise-valid record is missing.",
        truncate_last_byte(&bytes),
        error_class::CBOR,
        Layer::Envelope,
    ));
    vectors.push(invalid_vector(
        group,
        "unknown-envelope-key",
        "An 8th envelope key is present; the envelope MUST contain exactly keys 1-7 (spec 001 §1).",
        add_envelope_key(&bytes),
        error_class::ENVELOPE,
        Layer::Envelope,
    ));
    vectors.push(invalid_vector(
        group,
        "unknown-sig-alg",
        "`sig_alg` is set to 99, an unregistered algorithm id (spec 001 §7).",
        set_unknown_sig_alg(&bytes, 99),
        error_class::UNKNOWN_ALGORITHM,
        Layer::Envelope,
    ));
    vectors.push(invalid_vector(
        group,
        "float-in-body",
        "The comment's `text` body value is replaced by a CBOR float32; floats MUST NOT appear anywhere in a record (spec 001 §2 rule 4).",
        inject_float_in_body(&bytes, body_text),
        error_class::NON_CANONICAL,
        Layer::Envelope,
    ));
}

// =======================================================================
// kinds/<kind>/
// =======================================================================

fn build_kind_groups(vectors: &mut Vec<Vector>) {
    let creator_kp = kp(10);
    let (creator_id, creator_genesis) =
        Identity::genesis(&creator_kp, &[], CONTEST_WINDOW, CREATED_AT).unwrap();
    let assignee = IdentityId([0x77; 32]);
    let grantee = IdentityId([0x22; 32]);

    // --- rotation (1) ---------------------------------------------------
    // Deeper rotation-chain invalids (bad genesis shapes, fork
    // resolution...) live under identity/, per spec 002's own test
    // vectors section; this group just proves rotation is one of the
    // kinds the mechanical 3 apply to, using the genesis record above.
    add_kind_group(
        vectors,
        "kinds/rotation",
        &creator_genesis,
        "A genesis `rotation` record (spec 002 §2, spec 003 §3.1).",
    );

    // --- profile (2) ------------------------------------------------------
    let profile_body = map(vec![
        ("name", t("asha")),
        ("about", t("field recordings")),
        (
            "relays",
            Value::Array(vec![t("wss://relay.example.net/sync")]),
        ),
        (
            "payment",
            Value::Array(vec![Value::Array(vec![u(1), t("asha@ln.example.net")])]),
        ),
    ]);
    let profile = build(2, vec![], profile_body, &creator_kp, creator_id, CREATED_AT);
    add_kind_group(
        vectors,
        "kinds/profile",
        &profile,
        "A `profile` record (spec 003 §3.2).",
    );
    kind_invalid(
        vectors,
        "kinds/profile",
        "nonempty-refs",
        "profile refs MUST be empty (spec 003 §3.2).",
        &build(
            2,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![("name", t("asha"))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- delegate (3) -------------------------------------------------------
    let delegate_grant_body = map(vec![
        ("grantee", by(&grantee.0)),
        ("capability", t("rendition")),
        ("expires_at", Value::from_i64(1_795_000_000)),
    ]);
    let delegate_grant = build(
        3,
        vec![],
        delegate_grant_body,
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/delegate",
        &delegate_grant,
        "A `delegate` grant record (spec 003 §3.3).",
    );
    kind_invalid(
        vectors,
        "kinds/delegate",
        "revoked-without-ref",
        "`revoked: true` with no ref to the grant it revokes.",
        &build(
            3,
            vec![],
            map(vec![
                ("grantee", by(&grantee.0)),
                ("capability", t("rendition")),
                ("revoked", Value::Bool(true)),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/delegate",
        "ref-without-revoked-flag",
        "A ref to a grant record without `revoked: true` in the body.",
        &build(
            3,
            vec![Ref::record(TARGET_GRANT)],
            map(vec![
                ("grantee", by(&grantee.0)),
                ("capability", t("rendition")),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- manifest (16) -------------------------------------------------------
    let original_blob = BlobId([0x30; 32]);
    let rendition_blob = BlobId([0x31; 32]);
    let stmt = derivation_statement(
        original_blob,
        rendition_blob,
        "avc1.640028",
        1280,
        720,
        2_800_000,
    );
    let derivation_sig = sign_derivation(&creator_kp, &stmt);
    let mut rendition_entries = media_value(
        rendition_blob,
        402_653_184,
        None,
        "avc1.640028",
        1_841_000,
        1280,
        720,
    );
    rendition_entries.push(("bitrate", u(2_800_000)));
    rendition_entries.push((
        "produced_by",
        Value::Array(vec![by(&creator_id.0), by(&creator_kp.public_key_bytes())]),
    ));
    rendition_entries.push(("derivation_sig", by(&derivation_sig)));
    let manifest_body_valid = map(vec![
        ("title", t("One River a Week -- 12: Breede")),
        ("language", t("en")),
        (
            "original",
            map(media_value(
                original_blob,
                128_974_848,
                None,
                "av01.0.08M.08",
                1_841_000,
                3840,
                2160,
            )),
        ),
        ("renditions", Value::Array(vec![map(rendition_entries)])),
        ("license", t("CC-BY-4.0")),
    ]);
    let manifest_valid = build(
        16,
        vec![],
        manifest_body_valid,
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/manifest",
        &manifest_valid,
        "A `manifest` record with a verifiable-derivation rendition (spec 004 §§1-3): the rendition's `derivation_sig` is a real Ed25519 signature over the spec 004 §3.1 statement.",
    );

    fn simple_manifest_body(
        title: Option<&str>,
        original_size: u64,
        include_chunk_root: bool,
    ) -> Value {
        let mut e = Vec::new();
        if let Some(title) = title {
            e.push(("title", t(title)));
        }
        let chunk_root = if include_chunk_root {
            Some([0x55; 32])
        } else {
            None
        };
        e.push((
            "original",
            map(media_value(
                BlobId([0x30; 32]),
                original_size,
                chunk_root,
                "av01.0.08M.08",
                1000,
                640,
                360,
            )),
        ));
        e.push(("license", t("CC-BY-4.0")));
        map(e)
    }

    kind_invalid(
        vectors,
        "kinds/manifest",
        "missing-title",
        "`title` is required (spec 004 §1).",
        &build(
            16,
            vec![],
            simple_manifest_body(None, 1000, false),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/manifest",
        "missing-chunk-root-over-1mib",
        "`original.size` exceeds 1 MiB but `chunk_root` is absent (spec 004 §2, spec 001 §8).",
        &build(
            16,
            vec![],
            simple_manifest_body(Some("big"), CHUNK_SIZE as u64 + 1, false),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/manifest",
        "too-many-refs",
        "manifest refs MAY contain at most one channel ref (spec 003 §4.1).",
        &build(
            16,
            vec![Ref::record(TARGET_CHANNEL), Ref::record(TARGET_MANIFEST)],
            simple_manifest_body(Some("x"), 1000, false),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    {
        let mut bad_rendition =
            media_value(rendition_blob, 1000, None, "avc1.640028", 1000, 1280, 720);
        bad_rendition.push(("bitrate", u(2_800_000)));
        bad_rendition.push((
            "produced_by",
            Value::Array(vec![by(&creator_id.0), by(&creator_kp.public_key_bytes())]),
        ));
        bad_rendition.push(("derivation_sig", by(&[0xffu8; 64])));
        let body = map(vec![
            ("title", t("x")),
            (
                "original",
                map(media_value(
                    original_blob,
                    1000,
                    None,
                    "av01.0.08M.08",
                    1000,
                    640,
                    360,
                )),
            ),
            ("renditions", Value::Array(vec![map(bad_rendition)])),
            ("license", t("CC-BY-4.0")),
        ]);
        kind_invalid(
            vectors,
            "kinds/manifest",
            "bad-derivation-sig",
            "The rendition's `derivation_sig` is 64 garbage bytes; it must fail cryptographic verification against the spec 004 §3.1 statement (checked beyond plain schema parsing).",
            &build(16, vec![], body, &creator_kp, creator_id, CREATED_AT),
            error_class::KIND,
        );
    }

    // --- supersede (17) -------------------------------------------------------
    let supersede_body = map(vec![
        ("target_kind", u(32)),
        (
            "body",
            map(vec![("text", t("corrected: it was 1971, not 1972"))]),
        ),
    ]);
    let supersede = build(
        17,
        vec![Ref::record(TARGET_PARENT_COMMENT)],
        supersede_body,
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/supersede",
        &supersede,
        "A `supersede` record (spec 003 §4.2).",
    );
    kind_invalid(
        vectors,
        "kinds/supersede",
        "wrong-refs-count",
        "supersede refs MUST be exactly one target ref.",
        &build(
            17,
            vec![],
            map(vec![("target_kind", u(32)), ("body", map(vec![]))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/supersede",
        "target-rotation",
        "`target_kind` MUST NOT be `rotation`; chains are never edited (spec 003 §4.2).",
        &build(
            17,
            vec![Ref::record(TARGET_PARENT_COMMENT)],
            map(vec![("target_kind", u(1)), ("body", map(vec![]))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- retract (18) -------------------------------------------------------
    let retract = build(
        18,
        vec![Ref::record(TARGET_PARENT_COMMENT)],
        map(vec![("reason", t("posted in error"))]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/retract",
        &retract,
        "A `retract` record (spec 003 §4.3).",
    );
    kind_invalid(
        vectors,
        "kinds/retract",
        "wrong-refs-count",
        "retract refs MUST be exactly one target ref.",
        &build(18, vec![], map(vec![]), &creator_kp, creator_id, CREATED_AT),
        error_class::KIND,
    );

    // --- mirror (19) -------------------------------------------------------
    let mirror = build(
        19,
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![(
            "hints",
            Value::Array(vec![Value::Array(vec![
                u(3),
                t("https://node7.example.org/blob"),
            ])]),
        )]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/mirror",
        &mirror,
        "A `mirror` record (spec 003 §4.4).",
    );
    kind_invalid(
        vectors,
        "kinds/mirror",
        "empty-refs",
        "mirror refs MUST have at least one target.",
        &build(19, vec![], map(vec![]), &creator_kp, creator_id, CREATED_AT),
        error_class::KIND,
    );

    // --- similarity (20) -------------------------------------------------------
    let similarity = build(
        20,
        vec![Ref::blob(BLOB_A), Ref::record(TARGET_MANIFEST)],
        map(vec![("method", t("phash-v2")), ("score", u(9700))]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/similarity",
        &similarity,
        "A `similarity` record (spec 003 §4.5).",
    );
    kind_invalid(
        vectors,
        "kinds/similarity",
        "score-overflow",
        "similarity/score-overflow: `score` exceeds 10000 basis points (spec 003 §4.5).",
        &build(
            20,
            vec![Ref::blob(BLOB_A), Ref::record(TARGET_MANIFEST)],
            map(vec![("method", t("phash-v2")), ("score", u(10_001))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/similarity",
        "wrong-refs-count",
        "similarity refs MUST have exactly 2 elements.",
        &build(
            20,
            vec![Ref::blob(BLOB_A)],
            map(vec![("method", t("phash-v2")), ("score", u(1))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- comment (32) -------------------------------------------------------
    let comment = build(
        32,
        vec![
            Ref::record(TARGET_MANIFEST),
            Ref::record(TARGET_PARENT_COMMENT),
        ],
        map(vec![("text", t("the second movement is extraordinary"))]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/comment",
        &comment,
        "A `comment` record, threaded off a parent (spec 003 §5.1).",
    );
    kind_invalid(
        vectors,
        "kinds/comment",
        "empty-text",
        "`text` MUST be non-empty (spec 003 §5.1).",
        &build(
            32,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![("text", t(""))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- reaction (33) -------------------------------------------------------
    let reaction = build(
        33,
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![("reaction", t("\u{1F525}"))]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/reaction",
        &reaction,
        "A `reaction` record (spec 003 §5.2).",
    );
    kind_invalid(
        vectors,
        "kinds/reaction",
        "reaction-too-long",
        "`reaction` MUST be at most 32 bytes (spec 003 §5.2).",
        &build(
            33,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![("reaction", t(&"x".repeat(33)))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- follow (34) -------------------------------------------------------
    let follow = build(
        34,
        vec![Ref::record(RecordId([0x77; 32]))],
        map(vec![]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/follow",
        &follow,
        "A `follow` record (spec 003 §5.3); the ref is the followed identity's genesis record id.",
    );

    // --- playlist (35) -------------------------------------------------------
    let playlist = build(
        35,
        vec![],
        map(vec![
            ("title", t("winter mixes")),
            (
                "entries",
                Value::Array(vec![by(&[0xaa; 32]), by(&[0xbb; 32])]),
            ),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/playlist",
        &playlist,
        "A `playlist` record (spec 003 §5.4).",
    );
    kind_invalid(
        vectors,
        "kinds/playlist",
        "empty-entries",
        "`entries` MUST be non-empty (spec 003 §5.4).",
        &build(
            35,
            vec![],
            map(vec![
                ("title", t("empty")),
                ("entries", Value::Array(vec![])),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/playlist",
        "nonempty-refs",
        "playlist refs MUST be empty (spec 003 §5.4).",
        &build(
            35,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![
                ("title", t("x")),
                ("entries", Value::Array(vec![by(&[0xaa; 32])])),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- channel (36) -------------------------------------------------------
    let channel = build(
        36,
        vec![],
        map(vec![
            ("title", t("Field Notes")),
            ("description", t("one river a week")),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/channel",
        &channel,
        "A `channel` record (spec 003 §5.5).",
    );
    kind_invalid(
        vectors,
        "kinds/channel",
        "nonempty-refs",
        "channel refs MUST be empty (spec 003 §5.5).",
        &build(
            36,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![("title", t("x"))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- claim.author (48) -------------------------------------------------------
    let claim_author = build(
        48,
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![
            ("statement", t("recorded by me, 2025-11-02")),
            ("evidence", Value::Array(vec![by(BLOB_A.as_bytes())])),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/claim.author",
        &claim_author,
        "A `claim.author` record (spec 003 §6.1).",
    );
    kind_invalid(
        vectors,
        "kinds/claim.author",
        "wrong-refs-count",
        "claim.author refs MUST be exactly one (the subject manifest).",
        &build(
            48,
            vec![Ref::record(TARGET_MANIFEST), Ref::record(TARGET_CHANNEL)],
            map(vec![]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- claim.license (49) -------------------------------------------------------
    let claim_license = build(
        49,
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![("license", t("CC-BY-4.0"))]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/claim.license",
        &claim_license,
        "A `claim.license` record (spec 003 §6.2).",
    );
    kind_invalid(
        vectors,
        "kinds/claim.license",
        "missing-license",
        "`license` is required (spec 003 §6.2).",
        &build(
            49,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- claim.transfer (50) -------------------------------------------------------
    let claim_transfer = build(
        50,
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![
            ("assignee", by(&assignee.0)),
            ("note", t("per agreement of 2026-01-15")),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/claim.transfer",
        &claim_transfer,
        "A `claim.transfer` record (spec 003 §6.3).",
    );
    kind_invalid(
        vectors,
        "kinds/claim.transfer",
        "missing-assignee",
        "`assignee` is required (spec 003 §6.3).",
        &build(
            50,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![("note", t("x"))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- claim.dispute (51) -------------------------------------------------------
    let claim_dispute = build(
        51,
        vec![Ref::record(TARGET_NOTICE)],
        map(vec![
            (
                "statement",
                t("prior publication 2024-06; see archive capture"),
            ),
            ("evidence", Value::Array(vec![by(BLOB_B.as_bytes())])),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/claim.dispute",
        &claim_dispute,
        "A `claim.dispute` record (spec 003 §6.4).",
    );
    kind_invalid(
        vectors,
        "kinds/claim.dispute",
        "missing-statement",
        "`statement` is required (spec 003 §6.4).",
        &build(
            51,
            vec![Ref::record(TARGET_NOTICE)],
            map(vec![]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- notice.takedown (64) -------------------------------------------------------
    fn claimant(name: &str, contact: &str) -> Value {
        map(vec![("name", t(name)), ("contact", t(contact))])
    }
    let notice_takedown_body = map(vec![
        ("regime", t("us-dmca-512")),
        (
            "claimant",
            claimant("Example Pictures LLC", "legal@example.com"),
        ),
        ("statement", t("I have a good faith belief...")),
        ("work", t("\"Winter Film\" (2024), reg. PA0002419331")),
        ("signature_name", t("J. Doe")),
    ]);
    let notice_takedown = build(
        64,
        vec![Ref::record(TARGET_MANIFEST)],
        notice_takedown_body,
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/notice.takedown",
        &notice_takedown,
        "A `notice.takedown` record (spec 003 §6.5).",
    );
    kind_invalid(
        vectors,
        "kinds/notice.takedown",
        "empty-refs",
        "notice.takedown refs MUST have at least one subject.",
        &build(
            64,
            vec![],
            map(vec![
                ("regime", t("us-dmca-512")),
                ("claimant", claimant("x", "y")),
                ("statement", t("s")),
                ("work", t("w")),
                ("signature_name", t("n")),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/notice.takedown",
        "missing-claimant",
        "`claimant` is required (spec 003 §6.5).",
        &build(
            64,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![
                ("regime", t("us-dmca-512")),
                ("statement", t("s")),
                ("work", t("w")),
                ("signature_name", t("n")),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- notice.counter (65) -------------------------------------------------------
    let notice_counter = build(
        65,
        vec![Ref::record(TARGET_NOTICE)],
        map(vec![
            ("regime", t("us-dmca-512")),
            ("claimant", claimant("A. Creator", "a@example.net")),
            ("statement", t("I have a good faith belief the material was removed as a result of mistake or misidentification...")),
            ("signature_name", t("A. Creator")),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/notice.counter",
        &notice_counter,
        "A `notice.counter` record (spec 003 §6.6).",
    );
    kind_invalid(
        vectors,
        "kinds/notice.counter",
        "wrong-refs-count",
        "notice.counter refs MUST be exactly one (the notice it counters).",
        &build(
            65,
            vec![Ref::record(TARGET_NOTICE), Ref::record(TARGET_MANIFEST)],
            map(vec![
                ("regime", t("us-dmca-512")),
                ("claimant", claimant("x", "y")),
                ("statement", t("s")),
                ("signature_name", t("n")),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- feed.takedown (66) -------------------------------------------------------
    let feed_takedown = build(
        66,
        vec![],
        map(vec![
            ("feed", t("example-org/us")),
            ("seq", u(4182)),
            (
                "add",
                Value::Array(vec![Value::Array(vec![
                    u(1),
                    by(BLOB_A.as_bytes()),
                    t("copyright"),
                    by(&TARGET_NOTICE.0),
                ])]),
            ),
            ("remove", Value::Array(vec![])),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/feed.takedown",
        &feed_takedown,
        "A `feed.takedown` batch record (spec 003 §6.7).",
    );
    kind_invalid(
        vectors,
        "kinds/feed.takedown",
        "nonempty-refs",
        "feed.takedown refs MUST be empty; subjects live in the body (spec 003 §6.7).",
        &build(
            66,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![
                ("feed", t("f")),
                ("seq", u(1)),
                ("add", Value::Array(vec![])),
                ("remove", Value::Array(vec![])),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- endorse.gateway (80) -------------------------------------------------------
    let endorse = build(
        80,
        vec![],
        map(vec![("url", t("https://watch.example.net"))]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/endorse.gateway",
        &endorse,
        "An `endorse.gateway` record (spec 003 §7.1).",
    );
    kind_invalid(
        vectors,
        "kinds/endorse.gateway",
        "non-https-url",
        "`url` MUST be an https origin (spec 003 §7.1).",
        &build(
            80,
            vec![],
            map(vec![("url", t("http://insecure.example.net"))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/endorse.gateway",
        "nonempty-refs",
        "endorse.gateway refs MUST be empty (spec 003 §7.1).",
        &build(
            80,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![("url", t("https://watch.example.net"))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- receipt (81) -------------------------------------------------------
    let receipt = build(
        81,
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![
            ("amount", u(21_000)),
            ("currency", t("sat")),
            ("rail", u(1)),
            ("payee", by(&grantee.0)),
            ("message", t("for the river week 12")),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/receipt",
        &receipt,
        "A `receipt` record (spec 003 §7.2).",
    );
    kind_invalid(
        vectors,
        "kinds/receipt",
        "amount-zero",
        "`amount` MUST be greater than zero (spec 003 §7.2).",
        &build(
            81,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![
                ("amount", u(0)),
                ("currency", t("sat")),
                ("rail", u(1)),
                ("payee", by(&grantee.0)),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- attest (82) -------------------------------------------------------
    let attest = build(
        82,
        vec![Ref::record(RecordId([0x77; 32]))],
        map(vec![
            (
                "statement",
                t("100000 verified views on watch.example.net, 2026-06"),
            ),
            (
                "data",
                map(vec![("metric", t("views")), ("value", u(100_000))]),
            ),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/attest",
        &attest,
        "An `attest` record (spec 003 §7.3).",
    );
    kind_invalid(
        vectors,
        "kinds/attest",
        "wrong-refs-count",
        "attest refs MUST be exactly one (the subject identity/record).",
        &build(
            82,
            vec![],
            map(vec![("statement", t("x"))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- anchor (96) -------------------------------------------------------
    let anchor = build(
        96,
        vec![],
        map(vec![
            ("root", by(&[0x31; 32])),
            ("count", u(18_211)),
            ("system", t("opentimestamps")),
            ("proof", by(&[0xab; 16])),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/anchor",
        &anchor,
        "An `anchor` record (spec 003 §8.1).",
    );
    kind_invalid(
        vectors,
        "kinds/anchor",
        "nonempty-refs",
        "anchor refs MUST be empty; the batch is under the root, not enumerated (spec 003 §8.1).",
        &build(
            96,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![
                ("root", by(&[0x31; 32])),
                ("count", u(1)),
                ("system", t("opentimestamps")),
                ("proof", by(&[0xab; 4])),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- keygrant (97) -------------------------------------------------------
    let keygrant = build(
        97,
        vec![Ref::record(TARGET_MANIFEST)],
        map(vec![
            ("recipient", by(&grantee.0)),
            ("wrap_alg", u(1)),
            ("wrapped_key", by(&[0xcd; 48])),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/keygrant",
        &keygrant,
        "A `keygrant` record (spec 003 §8.2, spec 008 §3).",
    );
    kind_invalid(
        vectors,
        "kinds/keygrant",
        "missing-wrapped-key",
        "`wrapped_key` is required (spec 003 §8.2).",
        &build(
            97,
            vec![Ref::record(TARGET_MANIFEST)],
            map(vec![("recipient", by(&grantee.0)), ("wrap_alg", u(1))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- live.manifest (112) -------------------------------------------------------
    let live_first = build(
        112,
        vec![],
        map(vec![
            ("title", t("Live")),
            ("seq", u(0)),
            (
                "segments",
                Value::Array(vec![Value::Array(vec![by(BLOB_A.as_bytes()), u(2000)])]),
            ),
            ("final", Value::Bool(false)),
        ]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/live.manifest",
        &live_first,
        "The first `live.manifest` record of a stream (spec 003 §9.1).",
    );
    kind_invalid(
        vectors,
        "kinds/live.manifest",
        "later-record-empty-refs",
        "A later live.manifest record (seq > 0) MUST carry exactly one ref to the stream's first record; this one has empty refs (spec 003 §9.1, spec 004 §6).",
        &build(
            112,
            vec![],
            map(vec![
                ("seq", u(1)),
                ("segments", Value::Array(vec![Value::Array(vec![by(BLOB_B.as_bytes()), u(2000)])])),
                ("final", Value::Bool(false)),
            ]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );

    // --- live.chat (113) -------------------------------------------------------
    let stream_id = live_first.id();
    let live_chat = build(
        113,
        vec![Ref::record(stream_id)],
        map(vec![("text", t("gg \u{1F525}"))]),
        &creator_kp,
        creator_id,
        CREATED_AT,
    );
    add_kind_group(
        vectors,
        "kinds/live.chat",
        &live_chat,
        "A `live.chat` record (spec 003 §9.2).",
    );
    kind_invalid(
        vectors,
        "kinds/live.chat",
        "text-too-long",
        "`text` MUST be at most 2048 bytes (spec 003 §9.2).",
        &build(
            113,
            vec![Ref::record(stream_id)],
            map(vec![("text", t(&"x".repeat(2049)))]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
    kind_invalid(
        vectors,
        "kinds/live.chat",
        "missing-text",
        "`text` is required (spec 003 §9.2).",
        &build(
            113,
            vec![Ref::record(stream_id)],
            map(vec![]),
            &creator_kp,
            creator_id,
            CREATED_AT,
        ),
        error_class::KIND,
    );
}

// =======================================================================
// identity/
// =======================================================================

fn build_identity_group(vectors: &mut Vec<Vector>) {
    let group = "identity";
    let window = CONTEST_WINDOW;

    // -- genesis-valid --------------------------------------------------
    let owner = kp(20);
    let recovery = kp(21);
    let recovery_key: AlgKey = (1, recovery.public_key_bytes().to_vec());
    let (_, genesis) = Identity::genesis(
        &owner,
        std::slice::from_ref(&recovery_key),
        window,
        CREATED_AT,
    )
    .unwrap();
    vectors.push(identity_chain_vector(
        group,
        "genesis-valid",
        "A lone genesis rotation record verifies to depth 0 (spec 002 §2).",
        std::slice::from_ref(&genesis),
        BTreeMap::new(),
        CREATED_AT + 1000,
    ));

    // -- genesis-invalid-nonzero-id --------------------------------------
    let stray = kp(22);
    let stray_body = map(vec![
        ("key", by(&stray.public_key_bytes())),
        ("key_alg", u(1)),
        ("recovery", Value::Array(vec![])),
        ("contest_window", u(window)),
    ]);
    let nonzero_id_genesis = build(
        1,
        vec![],
        stray_body,
        &stray,
        IdentityId([9; 32]),
        CREATED_AT,
    );
    vectors.push(identity_chain_vector(
        group,
        "genesis-invalid-nonzero-id",
        "A rotation record with empty refs but `author.identity_id != 0` is not a valid genesis (spec 002 §2); no genesis means no chain.",
        &[nonzero_id_genesis],
        BTreeMap::new(),
        CREATED_AT + 1000,
    ));

    // -- genesis-invalid-key-mismatch -------------------------------------
    let body_key_holder = kp(23);
    let signer_mismatch = kp(24);
    let mismatch_body = map(vec![
        ("key", by(&body_key_holder.public_key_bytes())),
        ("key_alg", u(1)),
        ("recovery", Value::Array(vec![])),
        ("contest_window", u(window)),
    ]);
    // Signed by a different key than the one the body declares: the
    // genesis record MUST be signed by `body.key` (spec 002 §2).
    let mismatch_genesis = build(
        1,
        vec![],
        mismatch_body,
        &signer_mismatch,
        IdentityId::ZERO,
        CREATED_AT,
    );
    vectors.push(identity_chain_vector(
        group,
        "genesis-invalid-key-mismatch",
        "`author.signing_key` does not equal `body.key`; the genesis record MUST be self-signed by the key it declares (spec 002 §2).",
        &[mismatch_genesis],
        BTreeMap::new(),
        CREATED_AT + 1000,
    ));

    // -- rotate-signing ---------------------------------------------------
    {
        let old = kp(25);
        let new = kp(26);
        let (id, gen) = Identity::genesis(&old, &[], window, CREATED_AT).unwrap();
        let rot = Identity::rotate(
            id,
            gen.id(),
            &new.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 100,
            &old,
        )
        .unwrap();
        vectors.push(identity_chain_vector(
            group,
            "rotate-signing",
            "A signing-key-authorized rotation advances the chain state (spec 002 §3).",
            &[gen, rot],
            BTreeMap::new(),
            CREATED_AT + 1000,
        ));
    }

    // -- rotate-recovery ---------------------------------------------------
    {
        let signing = kp(27);
        let recov = kp(28);
        let new_signing = kp(29);
        let (id, gen) = Identity::genesis(
            &signing,
            &[(1, recov.public_key_bytes().to_vec())],
            window,
            CREATED_AT,
        )
        .unwrap();
        let rot = Identity::rotate(
            id,
            gen.id(),
            &new_signing.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 100,
            &recov,
        )
        .unwrap();
        vectors.push(identity_chain_vector(
            group,
            "rotate-recovery",
            "A recovery-key-authorized rotation advances the chain state (spec 002 §3): the recovery key, not the signing key, produced this rotation.",
            &[gen, rot],
            BTreeMap::new(),
            CREATED_AT + 1000,
        ));
    }

    // -- rotate-alg-migration ---------------------------------------------
    // Only Ed25519 (sig_alg 1) is a registered algorithm at launch (spec
    // 001 §7); there is no second algorithm to migrate *to* yet. This
    // vector exercises the *shape* of the migration path (a rotation
    // whose body explicitly carries `key_alg`, the field the mechanism
    // hinges on) rather than an actual cross-algorithm rotation, which is
    // not yet constructible against a one-algorithm registry. See the
    // top-level report.
    {
        let old = kp(30);
        let new = kp(31);
        let (id, gen) = Identity::genesis(&old, &[], window, CREATED_AT).unwrap();
        let rot = Identity::rotate(
            id,
            gen.id(),
            &new.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 100,
            &old,
        )
        .unwrap();
        vectors.push(identity_chain_vector(
            group,
            "rotate-alg-field-present",
            "Exercises the `key_alg`-carrying rotation shape that is the crypto-agility migration path (spec 001 §7, spec 002 §3); only Ed25519 (id 1) is registered at launch so this is not yet a real cross-algorithm migration.",
            &[gen, rot],
            BTreeMap::new(),
            CREATED_AT + 1000,
        ));
    }

    // -- fork-recovery-precedence ------------------------------------------
    {
        let owner_signing = kp(1);
        let owner_recovery = kp(2);
        let thief = kp(3);
        let owner_new = kp(4);
        let (id, gen) = Identity::genesis(
            &owner_signing,
            &[(1, owner_recovery.public_key_bytes().to_vec())],
            window,
            CREATED_AT,
        )
        .unwrap();
        let thief_rot = Identity::rotate(
            id,
            gen.id(),
            &thief.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 100,
            &owner_signing,
        )
        .unwrap();
        let owner_rot = Identity::rotate(
            id,
            gen.id(),
            &owner_new.public_key_bytes(),
            1,
            &[(1, owner_recovery.public_key_bytes().to_vec())],
            window,
            CREATED_AT + 200,
            &owner_recovery,
        )
        .unwrap();
        let thief_rot2 = Identity::rotate(
            id,
            thief_rot.id(),
            &kp(5).public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 300,
            &thief,
        )
        .unwrap();
        vectors.push(identity_chain_vector(
            group,
            "fork-recovery-precedence",
            "A stolen-key rotation (thief, deeper branch) and an owner recovery rotation fork from genesis; while the thief's branch is not final, recovery wins (spec 002 §4a).",
            &[gen, thief_rot, thief_rot2, owner_rot],
            BTreeMap::new(),
            CREATED_AT + 1000,
        ));
    }

    // -- fork-final-signing -------------------------------------------------
    {
        let signing = kp(6);
        let recov = kp(7);
        let new_signing = kp(8);
        let recovery_new = kp(9);
        let (id, gen) = Identity::genesis(
            &signing,
            &[(1, recov.public_key_bytes().to_vec())],
            window,
            CREATED_AT,
        )
        .unwrap();
        let legit = Identity::rotate(
            id,
            gen.id(),
            &new_signing.public_key_bytes(),
            1,
            &[(1, recov.public_key_bytes().to_vec())],
            window,
            CREATED_AT + 100,
            &signing,
        )
        .unwrap();
        let late_recovery = Identity::rotate(
            id,
            gen.id(),
            &recovery_new.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 200,
            &recov,
        )
        .unwrap();
        let mut observed = BTreeMap::new();
        observed.insert(legit.id().to_hex(), CREATED_AT); // observed long ago
        let now = CREATED_AT + window as i64 + 10;
        vectors.push(identity_chain_vector(
            group,
            "fork-final-signing",
            "A signing-key rotation observed longer than the contest window ago becomes final and resists a later recovery fork (spec 002 §4.1).",
            &[gen, legit, late_recovery],
            observed,
            now,
        ));
    }

    // -- fork-same-class-tiebreak --------------------------------------------
    {
        let signing = kp(40);
        let a = kp(41);
        let b = kp(42);
        let (id, gen) = Identity::genesis(&signing, &[], window, CREATED_AT).unwrap();
        let rot_a = Identity::rotate(
            id,
            gen.id(),
            &a.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 100,
            &signing,
        )
        .unwrap();
        let rot_b = Identity::rotate(
            id,
            gen.id(),
            &b.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 101,
            &signing,
        )
        .unwrap();
        vectors.push(identity_chain_vector(
            group,
            "fork-same-class-tiebreak",
            "Two signing-key-authorized children of the same parent: the bytewise-lower record id wins (spec 002 §4b), fully deterministic given the record set.",
            &[gen, rot_a, rot_b],
            BTreeMap::new(),
            CREATED_AT + 1000,
        ));
    }

    // -- chain-order-merge (three arrival orders, identical result) ---------
    {
        let signing = kp(50);
        let recov = kp(51);
        let n1 = kp(52);
        let n2 = kp(53);
        let (id, gen) = Identity::genesis(
            &signing,
            &[(1, recov.public_key_bytes().to_vec())],
            window,
            CREATED_AT,
        )
        .unwrap();
        let r1 = Identity::rotate(
            id,
            gen.id(),
            &n1.public_key_bytes(),
            1,
            &[(1, recov.public_key_bytes().to_vec())],
            window,
            CREATED_AT + 100,
            &signing,
        )
        .unwrap();
        let r2 = Identity::rotate(
            id,
            r1.id(),
            &n2.public_key_bytes(),
            1,
            &[],
            window,
            CREATED_AT + 200,
            &n1,
        )
        .unwrap();
        let orders: [[Record; 3]; 3] = [
            [gen.clone(), r1.clone(), r2.clone()],
            [r1.clone(), gen.clone(), r2.clone()],
            [r2.clone(), r1.clone(), gen.clone()],
        ];
        for (i, records) in orders.iter().enumerate() {
            vectors.push(identity_chain_vector(
                group,
                &format!("chain-order-merge-{}", i + 1),
                "The same 3 records merged in a different arrival order MUST produce the identical resulting chain state (spec 002 Decisions; Principle 8, build plan §6 merge-order-independence).",
                records,
                BTreeMap::new(),
                CREATED_AT + 1000,
            ));
        }
    }
}

// =======================================================================
// chunktree/
// =======================================================================

fn synth_blob(n_chunks: usize, last_chunk_len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..n_chunks {
        let len = if i + 1 == n_chunks {
            last_chunk_len
        } else {
            CHUNK_SIZE
        };
        out.extend(std::iter::repeat_n((i % 251) as u8, len));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn chunk_proof_vector(
    group: &str,
    name: &str,
    description: &str,
    n_chunks: usize,
    last_chunk_len: usize,
    chunk_index: usize,
    proof: &[[u8; 32]],
    root: [u8; 32],
    valid: bool,
) -> Vector {
    Vector {
        group: group.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        data: VectorData::ChunkProof {
            n_chunks: n_chunks as u64,
            last_chunk_len: last_chunk_len as u64,
            chunk_index: chunk_index as u64,
            proof_hex: proof.iter().map(hex::encode).collect(),
            root_hex: hex::encode(root),
            valid,
        },
    }
}

fn build_chunktree_group(vectors: &mut Vec<Vector>) {
    use vidmesh_kernel::blob::ChunkTree;
    let group = "chunktree";

    // 0 chunks: the empty blob has no chunk tree; any index is out of
    // range, so verify_chunk MUST fail regardless of the (unused) root.
    vectors.push(chunk_proof_vector(
        group,
        "empty-blob-any-index-invalid",
        "The empty blob has zero chunks and no chunk root (spec 001 §8); any index is out of range.",
        0,
        0,
        0,
        &[],
        [0u8; 32],
        false,
    ));

    // 1, 2, 3, 5 chunks: valid proofs at several indexes (task: 0, 1, 2,
    // 3, and 5-chunk synthetic blobs).
    let sizes: [(usize, usize); 4] = [(1, CHUNK_SIZE), (2, 123_456), (3, CHUNK_SIZE), (5, 500_000)];
    for (n_chunks, last_len) in sizes {
        let bytes = synth_blob(n_chunks, last_len);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().expect("non-empty tree has a root");
        for index in 0..n_chunks {
            let proof = tree.prove(index).expect("valid index");
            // Self-check against the kernel's own verifier before
            // shipping the fixture: a generator bug here would corrupt
            // every downstream implementation's ground truth.
            let chunk_start = index * CHUNK_SIZE;
            let chunk_end = if index + 1 == n_chunks {
                bytes.len()
            } else {
                chunk_start + CHUNK_SIZE
            };
            verify_chunk(
                &root,
                n_chunks,
                index,
                &bytes[chunk_start..chunk_end],
                &proof,
            )
            .expect("generator-produced proof must verify");
            vectors.push(chunk_proof_vector(
                group,
                &format!("valid-{n_chunks}-chunks-index-{index}"),
                &format!("A valid range proof for chunk {index} of {n_chunks} (last chunk length {last_len} bytes)."),
                n_chunks,
                last_len,
                index,
                &proof,
                root,
                true,
            ));
        }
    }

    // Invalid: wrong sibling (a proof hash is tampered).
    {
        let n_chunks = 4;
        let bytes = synth_blob(n_chunks, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().unwrap();
        let mut proof = tree.prove(0).unwrap();
        assert!(
            proof.len() >= 2,
            "a 4-chunk tree at index 0 has a 2-level path"
        );
        proof[0][0] ^= 0xff;
        vectors.push(chunk_proof_vector(
            group,
            "invalid-wrong-sibling",
            "One sibling hash in an otherwise-correct proof is tampered; verify_chunk MUST reject it.",
            n_chunks,
            CHUNK_SIZE,
            0,
            &proof,
            root,
            false,
        ));
    }

    // Invalid: wrong index (a correct proof/chunk pairing for index 1,
    // claimed at index 2; the runner reconstructs the chunk bytes for
    // the *claimed* index from the synthesis formula, so this exercises
    // exactly the intended mismatch).
    {
        let n_chunks = 4;
        let bytes = synth_blob(n_chunks, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().unwrap();
        let proof = tree.prove(1).unwrap();
        vectors.push(chunk_proof_vector(
            group,
            "invalid-wrong-index",
            "A proof valid for chunk 1 is checked against chunk 2's bytes; verify_chunk MUST reject the mismatch.",
            n_chunks,
            CHUNK_SIZE,
            2,
            &proof,
            root,
            false,
        ));
    }
}

// =======================================================================
// bundle/
// =======================================================================

fn bundle_item(items: Vec<Value>) -> Vec<u8> {
    codec::encode_canonical(&Value::Array(items)).unwrap()
}

fn build_bundle_group(vectors: &mut Vec<Vector>) {
    let group = "bundle";
    let author = kp(60);
    let (identity, genesis) = Identity::genesis(&author, &[], CONTEST_WINDOW, CREATED_AT).unwrap();
    let comment = build(
        32,
        vec![Ref::record(genesis.id())],
        map(vec![("text", t("hello"))]),
        &author,
        identity,
        CREATED_AT + 10,
    );
    let records = [genesis, comment];

    let small_blob: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    let small_id = hash_blob(&small_blob);

    // A 3 MiB (plus a short remainder) blob, hand-split into 1 MiB `bp`
    // parts. Note: `vidmesh_kernel::bundle::Bundle::export` only
    // auto-splits blobs over `PART_SPLIT_THRESHOLD` (16 MiB, spec 007
    // §1's stated floor); this fixture is hand-assembled at the item
    // level to exercise the `bp` import path with a much smaller blob,
    // which the spec permits (it sets a lower bound, not an exact
    // threshold) — see the top-level report.
    let parted_len = 3 * CHUNK_SIZE + 12_345;
    let parted_blob: Vec<u8> = (0..parted_len).map(|i| (i % 251) as u8).collect();
    let parted_id = hash_blob(&parted_blob);

    fn assemble(records: &[Record], small: (BlobId, &[u8]), parted: (BlobId, &[u8])) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&BUNDLE_MAGIC);
        out.extend(bundle_item(vec![t("hdr"), Value::Map(vec![])]));
        for r in records {
            out.extend(bundle_item(vec![t("r"), by(&r.to_canonical_cbor())]));
        }
        out.extend(bundle_item(vec![
            t("b"),
            by(small.0.as_bytes()),
            by(small.1),
        ]));
        for (i, part) in parted.1.chunks(CHUNK_SIZE).enumerate() {
            out.extend(bundle_item(vec![
                t("bp"),
                by(parted.0.as_bytes()),
                u(i as u64),
                by(part),
            ]));
        }
        out.extend(bundle_item(vec![t("end"), u(records.len() as u64), u(2)]));
        out
    }

    let full = assemble(
        &records,
        (small_id, small_blob.as_slice()),
        (parted_id, parted_blob.as_slice()),
    );
    vectors.push(Vector {
        group: group.to_string(),
        name: "roundtrip".to_string(),
        description: "2 records + a small blob + a 3 MiB+ blob hand-split into 1 MiB `bp` parts; a correct importer recovers everything with nothing skipped (spec 007 §§1-2).".to_string(),
        data: VectorData::Bundle {
            bundle_hex: hex::encode(&full),
            expected: Some(BundleExpected {
                record_ids: records.iter().map(|r| r.id().to_hex()).collect(),
                blob_ids: vec![small_id.to_hex(), parted_id.to_hex()],
                skipped_count: 0,
                truncated: false,
            }),
            expected_error: None,
        },
    });

    // Truncated: cut the last 3 bytes, landing inside the final `end`
    // item. Both records and both blobs were already read/flushed
    // before the truncation point; the truncated `end` item itself
    // becomes one "unreadable item; stopping" skip entry.
    let mut truncated = full.clone();
    truncated.truncate(truncated.len() - 3);
    vectors.push(Vector {
        group: group.to_string(),
        name: "truncated".to_string(),
        description: "The bundle is cut 3 bytes short, inside the final `end` item; the importer MUST salvage every already-complete record and blob and report the truncation (spec 007 §2).".to_string(),
        data: VectorData::Bundle {
            bundle_hex: hex::encode(&truncated),
            expected: Some(BundleExpected {
                record_ids: records.iter().map(|r| r.id().to_hex()).collect(),
                blob_ids: vec![small_id.to_hex(), parted_id.to_hex()],
                skipped_count: 1,
                truncated: true,
            }),
            expected_error: None,
        },
    });

    // Corrupted blob item: a second, hash-mismatched "b" item is spliced
    // in before `end` (whose declared counts reflect only the blobs
    // actually accepted, matching how a real exporter would report a
    // salvage in progress).
    {
        let mut out = Vec::new();
        out.extend_from_slice(&BUNDLE_MAGIC);
        out.extend(bundle_item(vec![t("hdr"), Value::Map(vec![])]));
        for r in &records {
            out.extend(bundle_item(vec![t("r"), by(&r.to_canonical_cbor())]));
        }
        out.extend(bundle_item(vec![
            t("b"),
            by(small_id.as_bytes()),
            by(&small_blob),
        ]));
        // Corrupt: declared id does not match the content's real hash.
        out.extend(bundle_item(vec![t("b"), by(&[0xee; 32]), by(&[3u8; 10])]));
        out.extend(bundle_item(vec![t("end"), u(records.len() as u64), u(1)]));
        vectors.push(Vector {
            group: group.to_string(),
            name: "corrupted-blob-item".to_string(),
            description: "One `b` item's declared id does not match its content's hash; the importer MUST skip only that item and salvage everything else (spec 007 §2).".to_string(),
            data: VectorData::Bundle {
                bundle_hex: hex::encode(&out),
                expected: Some(BundleExpected {
                    record_ids: records.iter().map(|r| r.id().to_hex()).collect(),
                    blob_ids: vec![small_id.to_hex()],
                    skipped_count: 1,
                    truncated: false,
                }),
                expected_error: None,
            },
        });
    }

    // Bad magic: not a container-level salvage, a hard rejection.
    vectors.push(Vector {
        group: group.to_string(),
        name: "bad-magic".to_string(),
        description: "The 5-byte magic is wrong; `Bundle::import` MUST return an error rather than attempt any salvage (spec 007 §2 step 1).".to_string(),
        data: VectorData::Bundle {
            bundle_hex: hex::encode(b"NOPE\x01rest-of-file-does-not-matter"),
            expected: None,
            expected_error: Some(error_class::BUNDLE.to_string()),
        },
    });
}

// =======================================================================
// json/
// =======================================================================

fn build_json_group(vectors: &mut Vec<Vector>) {
    let _group = "json";

    fn json_valid(name: &str, description: &str, value: Value) -> Vector {
        let cbor =
            codec::encode_canonical(&value).expect("fixture value must be canonically encodable");
        Vector {
            group: "json".to_string(),
            name: name.to_string(),
            description: description.to_string(),
            data: VectorData::JsonRoundtrip {
                json: codec::to_json(&value),
                expected_cbor_hex: Some(hex::encode(cbor)),
                expected_error: None,
            },
        }
    }
    fn json_invalid(
        name: &str,
        description: &str,
        json_text: &str,
        expected_error: &str,
    ) -> Vector {
        Vector {
            group: "json".to_string(),
            name: name.to_string(),
            description: description.to_string(),
            data: VectorData::JsonRoundtrip {
                json: json_text.to_string(),
                expected_cbor_hex: None,
                expected_error: Some(expected_error.to_string()),
            },
        }
    }

    // Integer map keys (envelope-shaped map), bytes, and a full record's
    // JSON form (also exercised directly by kinds/comment's own
    // record-valid vector; included again here as a plain json/ fixture
    // per spec 001's own test-vectors wording).
    let record_shaped = Value::Map(vec![
        (Value::Uint(1), Value::Uint(32)),
        (
            Value::Uint(2),
            Value::Array(vec![
                Value::Bytes(vec![0u8; 32]),
                Value::Bytes(vec![1u8; 32]),
            ]),
        ),
        (Value::Uint(3), Value::from_i64(CREATED_AT)),
        (Value::Uint(4), Value::Array(vec![])),
        (
            Value::Uint(5),
            Value::Map(vec![(Value::Text("text".into()), Value::Text("hi".into()))]),
        ),
        (Value::Uint(6), Value::Uint(1)),
        (Value::Uint(7), Value::Bytes(vec![2u8; 64])),
    ]);
    vectors.push(json_valid(
        "record-shaped-integer-keys-and-bytes",
        "A record-envelope-shaped map: integer keys 1-7 and byte-string values render as `hex:<hex>` (spec 001 §11).",
        record_shaped,
    ));

    vectors.push(json_valid(
        "hex-prefix-escape",
        "A text value that itself starts with `hex:` is escaped with a `txt:` prefix so it is not confused with a byte string on decode (spec 001 §11, kernel codec module docs).",
        Value::Map(vec![(Value::Text("k".into()), Value::Text("hex:deadbeef".into()))]),
    ));
    vectors.push(json_valid(
        "txt-prefix-escape",
        "A text value that itself starts with `txt:` is double-escaped the same way.",
        Value::Map(vec![(
            Value::Text("k".into()),
            Value::Text("txt:something".into()),
        )]),
    ));
    vectors.push(json_valid(
        "integer-lookalike-text-key",
        "A text map key whose content is itself a bare decimal integer (`\"5\"`) is escaped with `txt:` so it cannot collide with a genuine integer key 5 on decode.",
        Value::Map(vec![(Value::Text("5".into()), Value::Bool(true))]),
    ));
    vectors.push(json_valid(
        "negative-integer-key",
        "A genuine negative integer map key round-trips through its decimal rendering.",
        Value::Map(vec![(Value::from_i64(-3), Value::Null)]),
    ));

    // Invalid: float, exponent, duplicate key (spec 001 §11's own "MUST
    // fail" example, plus the closing sentence of 001's test vectors:
    // "including a JSON record that MUST fail (non-round-trippable)").
    vectors.push(json_invalid(
        "float-value",
        "A fractional JSON number; spec 001 §2 rule 4 forbids floating point anywhere in a record, and the codec supports only integers.",
        "{\"1\":1.0}",
        error_class::CBOR,
    ));
    vectors.push(json_invalid(
        "exponent-notation",
        "Exponential notation is not a supported integer literal.",
        "{\"1\":1e5}",
        error_class::CBOR,
    ));
    vectors.push(json_invalid(
        "duplicate-object-key",
        "The same object key appears twice; canonical map keys MUST be unique (spec 001 §2 rule 3).",
        "{\"a\":1,\"a\":2}",
        error_class::CBOR,
    ));
    vectors.push(json_invalid(
        "leading-zero-number",
        "`01` is not a canonical integer literal (a leading zero implies non-shortest form).",
        "01",
        error_class::CBOR,
    ));
}
