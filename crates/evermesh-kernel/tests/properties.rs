//! Property-based tests for the kernel (build plan §6):
//!
//! - Canonical-encoding round-trips for arbitrary `Value`s (spec 001 §2).
//! - Canonical-encoding stability (re-encoding a decoded value reproduces
//!   the exact same bytes).
//! - JSON interchange round-trips (spec 001 §11), restricted to what the
//!   codec documents as JSON-losslessly representable.
//! - `decode_canonical` never panics on arbitrary bytes.
//! - Signed `Record` round-trips through canonical CBOR.
//! - Identity rotation chain verification is independent of the order
//!   records are handed to `Identity::verify_chain` (spec 002 §4).
//! - Bundle export/import round-trips arbitrary small records and blobs.

use std::collections::HashSet;

use proptest::prelude::*;
use proptest::test_runner::Config as ProptestConfig;

use evermesh_kernel::codec::{self, Value};
use evermesh_kernel::identity::{AlgKey, Identity};
use evermesh_kernel::record::{Record, RecordBuilder, Ref, SIG_ALG_ED25519};
use evermesh_kernel::{BlobId, IdentityId, Keypair, RecordId};

const WINDOW: u64 = 604_800;

// ---------------------------------------------------------------------
// Bounded recursive `Value` strategies
// ---------------------------------------------------------------------

const MAX_LEN: usize = 64;
const MAX_ITEMS: usize = 8;
const MAX_DEPTH: u32 = 4;

/// Sort a candidate list of map entries by the bytewise order of their
/// canonically encoded keys, dropping later entries whose key encodes
/// identically to one already kept. This is exactly the rule
/// `encode_canonical` applies when writing a map (spec 001 §2 rule 3;
/// see `encode_map` in `codec.rs`, which sorts pairs by encoded-key bytes
/// and errors on duplicates) — doing it here means every generated
/// `Value::Map` is guaranteed encodable, and already in the order
/// `decode_canonical` will hand back, so value equality after a
/// round-trip is exact rather than "equal as a set".
fn canonical_map(mut entries: Vec<(Value, Value)>) -> Value {
    let mut keyed: Vec<(Vec<u8>, Value, Value)> = Vec::with_capacity(entries.len());
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    for (k, v) in entries.drain(..) {
        // Safe: by construction every candidate key was itself built
        // through `canonical_map` (or is a leaf), so it has no duplicate
        // keys anywhere in its own structure and always encodes.
        let kbytes =
            codec::encode_canonical(&k).expect("generated map keys are canonically encodable");
        if seen.insert(kbytes.clone()) {
            keyed.push((kbytes, k, v));
        }
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Map(keyed.into_iter().map(|(_, k, v)| (k, v)).collect())
}

fn arb_text() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..=MAX_LEN).prop_map(|cs| cs.into_iter().collect())
}

fn arb_bytes() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=MAX_LEN)
}

/// A scalar leaf value (no recursion). `max_nint` bounds the `Nint`
/// magnitude so callers can keep it JSON-safe (`<= i64::MAX as u64`) or
/// let it range over all of `u64` for CBOR-only properties.
fn arb_leaf(max_nint: u64) -> BoxedStrategy<Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<u64>().prop_map(Value::Uint),
        (0..=max_nint).prop_map(Value::Nint),
        arb_bytes().prop_map(Value::Bytes),
        arb_text().prop_map(Value::Text),
    ]
    .boxed()
}

/// Full recursive `Value` strategy: depth <= `depth`, collections <= 8
/// elements, byte/text lengths <= 64. Map keys may be any `Value`
/// (arrays and maps included), matching what canonical CBOR permits.
fn arb_value(depth: u32, max_nint: u64) -> BoxedStrategy<Value> {
    let leaf = arb_leaf(max_nint);
    if depth == 0 {
        return leaf;
    }
    let inner = arb_value(depth - 1, max_nint);
    prop_oneof![
        3 => leaf,
        1 => prop::collection::vec(inner.clone(), 0..=MAX_ITEMS).prop_map(Value::Array),
        1 => prop::collection::vec((inner.clone(), inner), 0..=MAX_ITEMS).prop_map(canonical_map),
    ]
    .boxed()
}

/// A map key restricted to the scalar types the JSON mapping actually
/// round-trips. The codec documents (`key_json_string` in `codec.rs`)
/// that `Array`/`Map`/`Bool`/`Null` keys render to a non-reversible
/// `"unsupported-key:..."` placeholder — deliberately, since `to_json`
/// cannot fail — so those key shapes are excluded from the JSON-safe
/// generator.
fn arb_json_key(max_nint: u64) -> BoxedStrategy<Value> {
    prop_oneof![
        any::<u64>().prop_map(Value::Uint),
        (0..=max_nint).prop_map(Value::Nint),
        arb_bytes().prop_map(Value::Bytes),
        arb_text().prop_map(Value::Text),
    ]
    .boxed()
}

/// Recursive `Value` strategy restricted to what `to_json`/`from_json`
/// round-trip losslessly: `Nint` capped at `i64::MAX` (so the magnitude
/// fits the JSON parser's `i64` limit for negative numbers) and map keys
/// restricted to scalars.
fn arb_value_json_safe(depth: u32) -> BoxedStrategy<Value> {
    let max_nint = i64::MAX as u64;
    let leaf = arb_leaf(max_nint);
    if depth == 0 {
        return leaf;
    }
    let inner = arb_value_json_safe(depth - 1);
    prop_oneof![
        3 => leaf,
        1 => prop::collection::vec(inner.clone(), 0..=MAX_ITEMS).prop_map(Value::Array),
        1 => prop::collection::vec((arb_json_key(max_nint), inner), 0..=MAX_ITEMS)
                .prop_map(canonical_map),
    ]
    .boxed()
}

/// A `Value::Map` suitable as a record body (spec 001 §1: the body must
/// be a map). Keys may be any `Value`.
fn arb_body(depth: u32) -> BoxedStrategy<Value> {
    prop::collection::vec(
        (arb_value(depth, u64::MAX), arb_value(depth, u64::MAX)),
        0..=MAX_ITEMS,
    )
    .prop_map(canonical_map)
    .boxed()
}

fn arb_ref() -> impl Strategy<Value = Ref> {
    (
        prop_oneof![Just(0u64), Just(1u64)],
        prop::array::uniform32(any::<u8>()),
    )
        .prop_map(|(ref_type, hash)| Ref { ref_type, hash })
}

// ---------------------------------------------------------------------
// Cheap properties: codec round-trips, default proptest case count.
// ---------------------------------------------------------------------

proptest! {
    /// `encode_canonical` -> `decode_canonical` reproduces the identical
    /// `Value` (spec 001 §2).
    #[test]
    fn value_canonical_round_trip(v in arb_value(MAX_DEPTH, u64::MAX)) {
        let bytes = codec::encode_canonical(&v).unwrap();
        let decoded = codec::decode_canonical(&bytes).unwrap();
        prop_assert_eq!(decoded, v);
    }

    /// Re-encoding a decoded value reproduces byte-identical output:
    /// canonical form is a fixed point, not just a valid encoding.
    #[test]
    fn value_canonical_encoding_is_stable(v in arb_value(MAX_DEPTH, u64::MAX)) {
        let bytes = codec::encode_canonical(&v).unwrap();
        let decoded = codec::decode_canonical(&bytes).unwrap();
        let bytes_again = codec::encode_canonical(&decoded).unwrap();
        prop_assert_eq!(bytes, bytes_again);
    }

    /// `to_json` -> `from_json` round-trips for values within the
    /// documented JSON-safe subset (spec 001 §11): `Nint` magnitude
    /// capped at `i64::MAX`, map keys restricted to scalars.
    #[test]
    fn value_json_round_trip(v in arb_value_json_safe(MAX_DEPTH)) {
        let json = codec::to_json(&v);
        let back = codec::from_json(&json).unwrap();
        prop_assert_eq!(back, v);
    }

    /// `decode_canonical` never panics on arbitrary bytes: it returns
    /// `Ok` or `Err`, nothing else (build plan §6: no panics on
    /// untrusted input).
    #[test]
    fn decode_canonical_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = codec::decode_canonical(&bytes);
    }
}

// ---------------------------------------------------------------------
// Identity chain strategy: genesis + K rotations, mixed signing/recovery
// authorization, plus a shuffled permutation of the same records.
// ---------------------------------------------------------------------

/// Builds `(signing_seeds, recovery_seed, use_recovery, shuffled_indices)`
/// where:
/// - `signing_seeds` has `k + 1` entries: `signing_seeds[0]` seeds the
///   genesis signing key, `signing_seeds[i + 1]` seeds the key installed
///   by rotation `i`.
/// - `use_recovery[i]` selects whether rotation `i` is authorized by the
///   then-current signing key or by the (fixed, always-declared) recovery
///   key.
/// - `shuffled_indices` is a permutation of `0..k+1` (genesis + k
///   rotations), built with `Just` + `prop_shuffle` so every permutation
///   of the chain is exercised.
fn identity_chain_and_shuffle_strategy(
) -> impl Strategy<Value = (Vec<[u8; 32]>, [u8; 32], Vec<bool>, Vec<usize>)> {
    (1usize..5).prop_flat_map(|k| {
        let n = k + 1;
        (
            prop::collection::vec(prop::array::uniform32(any::<u8>()), n),
            prop::array::uniform32(any::<u8>()),
            prop::collection::vec(any::<bool>(), k),
            Just((0..n).collect::<Vec<usize>>()).prop_shuffle(),
        )
    })
}

// ---------------------------------------------------------------------
// Expensive properties: signing, chain verification, bundle export —
// keep the case count moderate.
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// A signed record survives `to_canonical_cbor` -> `from_cbor`
    /// unchanged and still verifies (spec 001 §§1-6).
    #[test]
    fn record_round_trip(
        seed in prop::array::uniform32(any::<u8>()),
        identity_seed in prop::array::uniform32(any::<u8>()),
        kind in any::<u64>(),
        created_at in any::<i64>(),
        refs in prop::collection::vec(arb_ref(), 0..=4),
        body in arb_body(3),
    ) {
        let kp = Keypair::from_secret_bytes(&seed);
        let identity_id = IdentityId(identity_seed);
        let record = RecordBuilder::new(kind)
            .created_at(created_at)
            .refs(refs)
            .body(body)
            .sign_as(&kp, identity_id)
            .unwrap();

        let bytes = record.to_canonical_cbor();
        let back = Record::from_cbor(&bytes).unwrap();
        prop_assert_eq!(&back, &record);
        prop_assert!(back.verify().is_ok());
    }

    /// `Identity::verify_chain` computes the same `IdentityState`
    /// regardless of the order records are supplied in (spec 002 §4):
    /// the chain here is a single unforked line (each rotation
    /// authorized by either the current signing key or the fixed
    /// recovery key), so there is exactly one correct resulting state,
    /// and any permutation of the input records must resolve to it.
    #[test]
    fn identity_merge_order_independence(
        (signing_seeds, recovery_seed, use_recovery, shuffle)
            in identity_chain_and_shuffle_strategy()
    ) {
        let recovery_kp = Keypair::from_secret_bytes(&recovery_seed);
        let recovery_pair: AlgKey = (SIG_ALG_ED25519, recovery_kp.public_key_bytes().to_vec());
        let kps: Vec<Keypair> =
            signing_seeds.iter().map(Keypair::from_secret_bytes).collect();

        let (id, genesis) =
            Identity::genesis(&kps[0], std::slice::from_ref(&recovery_pair), WINDOW, 100).unwrap();
        let mut records = vec![genesis];
        let mut prev_id: RecordId = records[0].id();

        for (i, use_rec) in use_recovery.iter().enumerate() {
            let signer = if *use_rec { &recovery_kp } else { &kps[i] };
            let new_kp = &kps[i + 1];
            let rot = Identity::rotate(
                id,
                prev_id,
                &new_kp.public_key_bytes(),
                SIG_ALG_ED25519,
                std::slice::from_ref(&recovery_pair),
                WINDOW,
                200 + i as i64 * 100,
                signer,
            )
            .unwrap();
            prev_id = rot.id();
            records.push(rot);
        }

        let observed_at = |_: &RecordId| -> Option<i64> { None };
        let now: i64 = 10_000_000_000;

        let baseline = Identity::verify_chain(&records, &observed_at, now).unwrap();

        let permuted_records: Vec<Record> = shuffle.iter().map(|&i| records[i].clone()).collect();
        let permuted = Identity::verify_chain(&permuted_records, &observed_at, now).unwrap();

        prop_assert_eq!(permuted, baseline);
    }

    /// Exporting a bundle of records and blobs and re-importing it
    /// recovers exactly what was exported, with nothing skipped
    /// (spec 007). Blobs are deduplicated by id before comparison,
    /// mirroring what `Bundle::export`/`Bundle::import` do internally,
    /// so an accidental content collision in the generated input can't
    /// spuriously fail the property.
    #[test]
    fn bundle_round_trip(
        record_seeds in prop::collection::vec(prop::array::uniform32(any::<u8>()), 0..=3),
        blob_contents in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..=4096), 0..=3),
    ) {
        // `created_at` is keyed by index so records are pairwise
        // distinct (and therefore have distinct ids) regardless of any
        // coincidental overlap among the random seeds/bodies below —
        // `Bundle::export`'s declared record count is the raw input
        // length, so a duplicate record id would spuriously produce a
        // "counts mismatch" skip entry.
        let records: Vec<Record> = record_seeds
            .iter()
            .enumerate()
            .map(|(i, seed)| {
                let kp = Keypair::from_secret_bytes(seed);
                RecordBuilder::new(32)
                    .created_at(1000 + i as i64)
                    .body(Value::Map(vec![(Value::Text("i".into()), Value::Uint(i as u64))]))
                    .sign_as(&kp, IdentityId::ZERO)
                    .unwrap()
            })
            .collect();

        let mut expected_blobs: Vec<(BlobId, Vec<u8>)> = Vec::new();
        let mut seen_blob_ids: HashSet<[u8; 32]> = HashSet::new();
        for content in &blob_contents {
            let id = evermesh_kernel::blob::hash_blob(content);
            if seen_blob_ids.insert(*id.as_bytes()) {
                expected_blobs.push((id, content.clone()));
            }
        }

        let mut out = Vec::new();
        evermesh_kernel::Bundle::export(&records, &expected_blobs, &mut out).unwrap();
        let result = evermesh_kernel::Bundle::import(&out[..]).unwrap();

        prop_assert_eq!(result.records, records);
        prop_assert_eq!(result.blobs, expected_blobs);
        prop_assert!(result.skipped.is_empty(), "unexpected skips: {:?}", result.skipped);
        prop_assert!(!result.truncated);
    }
}
