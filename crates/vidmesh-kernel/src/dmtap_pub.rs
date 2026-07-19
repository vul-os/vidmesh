//! DMTAP-PUB §22 interoperability — **additive, phase 1, default-off**.
//!
//! Vidmesh independently arrived at the same substrate shape as DMTAP-PUB §22
//! ("Public Objects"): signed records over an Ed25519 identity, BLAKE3-256
//! content addressing, chunked Merkle-tree blobs, per-author append-only
//! publication, and dumb content-addressed holders. The *shape* matches; the
//! *bytes* do not. `docs/DMTAP-CONVERGENCE.md` records the full mapping.
//!
//! This module is the **§22 path**, sitting **beside** the native vidmesh
//! format rather than replacing it. Nothing here changes a single existing
//! byte: [`crate::record`], [`crate::blob`], [`crate::identity`] and the relay
//! wire format are untouched, and this module does not even compile unless the
//! `dmtap-pub` feature is enabled.
//!
//! # One implementation, not two
//!
//! The §22 object types are **re-exported from `dmtap-core`**, envoir's
//! reference implementation, and are *not* reimplemented here. That is the
//! whole point: a second divergent implementation of §22 would recreate exactly
//! the duplication this convergence exists to remove. What this module adds is
//! only the **bridge** — conversions between vidmesh's kernel types and §22's,
//! and the §24-video-profile framing of a vidmesh record.
//!
//! # What the bridge does and does not claim
//!
//! Conversions here are **representational, not cryptographic**. Converting a
//! vidmesh [`crate::BlobId`] into a §22 [`ContentId`] re-frames an address that
//! was computed by vidmesh's rules; it does **not** make a vidmesh chunk-tree
//! root into a valid `PubManifest.id`, because the two trees hash different
//! things (see [`self::divergence`]). A §22 object is only §22-valid if it was
//! *built* by the §22 constructors in this module.

use dmtap_core::id::MH_BLAKE3_256;

pub use dmtap_core::cbor::Cv;
pub use dmtap_core::id::ContentId;
pub use dmtap_core::identity::IdentityKey;
/// The §22 object model, re-exported verbatim from envoir's `dmtap-core`.
///
/// These are the *same types the reference implementation uses* — `PubAnnounce`
/// (§22.3), `PubManifest` (§22.2), `FeedHead`/`FeedEntry` (§22.4), the
/// anti-rollback check (§22.4.2) and the `ERR_PUB_*` error registry (§22.10).
/// Vidmesh deliberately owns none of this code.
pub use dmtap_core::pubobj::{
    check_anti_rollback, check_supersede, chunk_hash, pub_manifest_root, verify_chunk,
    verify_feed_chain, verify_feed_chain_to_head, FeedEntry, FeedFollower, FeedHead, PubAnnounce,
    PubError, PubManifest, RollbackDecision, ServePolicy, PUB_ANNOUNCE_DS, PUB_FEED_DS,
    PUB_MANIFEST_DS, PUB_V0,
};
pub use dmtap_core::suite::Suite;

use crate::blob::CHUNK_SIZE;
use crate::identity::Keypair;
use crate::ids::{BlobId, RecordId};
use crate::record::Record;

// ── Address bridging (multihash prefix) ──────────────────────────────────────
//
// Vidmesh addresses are bare 32 bytes, rendered `b3-256:<hex>` in text form.
// §22 addresses are `0x1e ‖ <32 bytes>` — a multihash prefix that buys hash
// agility (§18.1.5). The digests are the same width and the same algorithm, so
// the mapping is a pure prefix add/strip.

/// Frame a bare vidmesh 32-byte digest as a §22 multihash [`ContentId`]
/// (`0x1e ‖ digest`).
pub fn content_id_from_digest(digest: &[u8; 32]) -> ContentId {
    let mut v = Vec::with_capacity(33);
    v.push(MH_BLAKE3_256);
    v.extend_from_slice(digest);
    ContentId(v)
}

/// Strip a §22 [`ContentId`] back to a bare 32-byte digest, or `None` if it is
/// not a 33-byte BLAKE3-256 multihash (a SHA-2 or future-suite address has no
/// vidmesh representation and MUST NOT be silently truncated).
pub fn digest_from_content_id(id: &ContentId) -> Option<[u8; 32]> {
    let b = id.as_bytes();
    if b.len() != 33 || b[0] != MH_BLAKE3_256 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b[1..]);
    Some(out)
}

/// Frame a vidmesh [`BlobId`] as a §22 [`ContentId`].
///
/// Representational only: the resulting address is `0x1e ‖ BLAKE3(blob bytes)`,
/// which is a valid §22 *whole-object* address, but it is **not** a
/// `PubManifest.id` — that is a Merkle root over chunk addresses, not a hash of
/// the blob. Use [`pub_manifest_for_bytes`] for the manifest address.
pub fn blob_id_to_content_id(id: &BlobId) -> ContentId {
    content_id_from_digest(id.as_bytes())
}

/// Frame a vidmesh [`RecordId`] as a §22 [`ContentId`].
///
/// Representational only: a vidmesh record id hashes the envelope **without**
/// the signature, whereas a §22 `announce_id` hashes the **complete signed**
/// object (§18.9.4). The two therefore identify the same logical thing under
/// incompatible rules — this conversion is for referencing vidmesh objects from
/// §22 `meta`, never for claiming a §22 anchor.
pub fn record_id_to_content_id(id: &RecordId) -> ContentId {
    content_id_from_digest(id.as_bytes())
}

/// Recover a vidmesh [`BlobId`] from a §22 [`ContentId`], if it is a
/// BLAKE3-256 multihash.
pub fn content_id_to_blob_id(id: &ContentId) -> Option<BlobId> {
    digest_from_content_id(id).map(BlobId)
}

// ── Identity bridging ────────────────────────────────────────────────────────

/// Reinterpret a vidmesh [`Keypair`] as a §22 [`IdentityKey`].
///
/// **The key material is already compatible.** Both are Ed25519 keys built from
/// 32 secret seed bytes (RFC 8032), so this round-trips exactly and the public
/// key is bit-identical on both sides — see the `identity_key_material_is_shared`
/// test. Only the *signing preimages* differ, which is precisely why the
/// convergence is tractable: an existing vidmesh author can publish §22 objects
/// under the identity they already have, without a key migration.
pub fn keypair_to_identity_key(kp: &Keypair) -> IdentityKey {
    IdentityKey::from_seed(&kp.secret_bytes())
}

// ── Blob bridging (§22.2) ────────────────────────────────────────────────────

/// The §22.2.2 plaintext chunk addresses for a blob, using vidmesh's
/// [`CHUNK_SIZE`] (1 MiB) split: `h_i = 0x1e ‖ BLAKE3-256(plaintext_i)`.
///
/// Note what is hashed: the **chunk bytes**, producing an *address*. §22's tree
/// then hashes those addresses. Vidmesh's tree hashes the chunk bytes directly
/// into the leaf. Same chunking, different tree input.
pub fn pub_chunk_hashes(bytes: &[u8]) -> Vec<ContentId> {
    if bytes.is_empty() {
        return Vec::new();
    }
    bytes.chunks(CHUNK_SIZE).map(chunk_hash).collect()
}

/// Build a §22.2.1 [`PubManifest`] over an in-memory blob, chunked at
/// vidmesh's 1 MiB [`CHUNK_SIZE`].
///
/// Returns `None` for the empty blob: §22 requires `n ≥ 1` chunks, whereas
/// vidmesh's [`crate::ChunkTree`] tolerates the empty tree with no root. That
/// asymmetry is a real (if small) migration item — see phase 2 in
/// `docs/DMTAP-CONVERGENCE.md`.
pub fn pub_manifest_for_bytes(bytes: &[u8]) -> Option<PubManifest> {
    let chunks = pub_chunk_hashes(bytes);
    if chunks.is_empty() {
        return None;
    }
    Some(PubManifest::new(
        bytes.len() as u64,
        CHUNK_SIZE as u32,
        chunks,
        Suite::Classical,
    ))
}

/// Demonstrations that the two chunk trees are the **same shape but different
/// values** — kept as callable code, not prose, so the claim stays true.
pub mod divergence {
    use super::*;
    use crate::blob::ChunkTree;

    /// Both trees reduce an `n`-leaf list to a root with the *same tree
    /// structure* for every `n` (vidmesh's odd-node promotion and §22's RFC-6962
    /// largest-power-of-two split agree structurally), but the **root values
    /// differ** because the leaves hash different bytes under different domain
    /// separation.
    ///
    /// Returns `(vidmesh_root, pub_root)` for a blob, or `None` if empty. These
    /// are asserted **unequal** in this module's tests: proofs and roots are
    /// **not wire-interchangeable** between the two substrates.
    pub fn roots_for(bytes: &[u8]) -> Option<([u8; 32], ContentId)> {
        let native = ChunkTree::from_bytes(bytes).root()?;
        let pubm = pub_manifest_for_bytes(bytes)?;
        Some((native, pubm.id))
    }

    /// Per-chunk: what vidmesh **stored** versus what §22 **needs**, for one chunk.
    ///
    /// Returns `(vidmesh_leaf, pub_chunk_address)` where
    /// `vidmesh_leaf = BLAKE3(0x00 ‖ chunk)` ([`crate::blob::leaf_hash`]) and
    /// `pub_chunk_address = 0x1e ‖ BLAKE3(chunk)` ([`chunk_hash`]).
    ///
    /// # Why this exists — a spec erratum
    ///
    /// DMTAP §24.14 item 4 states that on migration "the **chunk leaf hashes are
    /// identical** (bare-chunk BLAKE3) … so re-derivation is a **tree recompute
    /// over the existing chunk hashes**, not a re-read of the media bytes."
    ///
    /// **That is not true of vidmesh's format.** Vidmesh folds a `0x00` leaf tag
    /// *inside* the hash, so what it persisted per chunk is `BLAKE3(0x00 ‖ chunk)`,
    /// while §22's `h_i` digest is `BLAKE3(chunk)` — different preimages, different
    /// values, asserted unequal by this module's tests.
    ///
    /// The operational consequence is significant and belongs in any phase-2 plan:
    /// migrating a blob to §22 requires **re-reading and re-hashing every stored
    /// media byte**, not a cheap metadata-only tree recompute over retained
    /// digests. See `docs/DMTAP-CONVERGENCE.md`.
    pub fn chunk_digests_for(chunk: &[u8]) -> ([u8; 32], ContentId) {
        (crate::blob::leaf_hash(chunk), chunk_hash(chunk))
    }
}

// ── Record → PubAnnounce bridging (§22.3 / §24) ──────────────────────────────

/// `meta` key under which a bridged announce carries the originating vidmesh
/// record's kind id (spec 003 registry). §22 `meta` is text-keyed and
/// profile-defined (§22.3.1), exactly the slot §23 uses for `"artifact"`; the
/// §24 video profile is where these keys become normative.
pub const META_KEY_VIDMESH_KIND: &str = "vidmesh.kind";

/// `meta` key carrying the originating vidmesh record id, so a bridged announce
/// remains traceable to the native record it was derived from.
pub const META_KEY_VIDMESH_RECORD: &str = "vidmesh.record";

/// Build the §24-profile `meta` block describing a native vidmesh [`Record`].
///
/// This is the "kinds registry stops being a wire concept and becomes a `meta`
/// schema" step from the convergence doc, in its minimal phase-1 form: the kind
/// id and the source record id travel as metadata. Translating each of the 27
/// kind *bodies* into normative §24 schemas is phase 2 work — deliberately not
/// attempted here.
pub fn meta_for_record(record: &Record) -> Vec<(String, Cv)> {
    vec![
        (META_KEY_VIDMESH_KIND.to_string(), Cv::U64(record.kind())),
        (
            META_KEY_VIDMESH_RECORD.to_string(),
            Cv::Bytes(record_id_to_content_id(&record.id()).as_bytes().to_vec()),
        ),
    ]
}

/// Build and sign a §22.3 [`PubAnnounce`] that publishes `roots` under the
/// identity `kp`, carrying `meta`.
///
/// The announce is signed with `DMTAP-PUB-v0/announce ‖ 0x00 ‖ det_cbor(∖sig)`
/// by `dmtap-core` itself — vidmesh does not construct the preimage.
pub fn build_announce(
    kp: &Keypair,
    roots: Vec<ContentId>,
    meta: Vec<(String, Cv)>,
    ts: u64,
    supersedes: Option<ContentId>,
) -> PubAnnounce {
    let key = keypair_to_identity_key(kp);
    let pk = key.public();
    let mut a = PubAnnounce {
        v: PUB_V0,
        suite: Suite::Classical,
        publisher: pk.clone(),
        roots,
        meta,
        supersedes,
        ts,
        signer: pk,
        sig: Vec::new(),
    };
    a.sign(&key);
    a
}

/// Bridge a native vidmesh [`Record`] and its blob to a signed §22 announce:
/// the blob becomes a [`PubManifest`], the record becomes §24 `meta`.
///
/// Returns `None` for an empty blob (§22 requires `n ≥ 1`).
pub fn announce_for_record_blob(
    kp: &Keypair,
    record: &Record,
    blob_bytes: &[u8],
    ts: u64,
) -> Option<(PubManifest, PubAnnounce)> {
    let manifest = pub_manifest_for_bytes(blob_bytes)?;
    let announce = build_announce(
        kp,
        vec![manifest.id.clone()],
        meta_for_record(record),
        ts,
        None,
    );
    Some((manifest, announce))
}

// ── Feed construction (§22.4) — the primitive vidmesh does not have ──────────

/// Build the genesis [`FeedEntry`] (`seq = 0`, no `prev`) for an announce.
pub fn feed_genesis(announce: &PubAnnounce, ts: u64) -> FeedEntry {
    FeedEntry {
        seq: 0,
        announce: announce.announce_id(),
        prev: None,
        ts,
    }
}

/// Append a [`FeedEntry`] after `prev`, hash-chaining it (`seq + 1`,
/// `prev = prev.entry_id()`).
pub fn feed_append(prev: &FeedEntry, announce: &PubAnnounce, ts: u64) -> FeedEntry {
    FeedEntry {
        seq: prev.seq + 1,
        announce: announce.announce_id(),
        prev: Some(prev.entry_id()),
        ts,
    }
}

/// Build and sign the [`FeedHead`] committing to `tip`.
///
/// **This has no vidmesh counterpart.** Vidmesh has no signed feed head at all,
/// so §22's mandatory anti-rollback and equivocation detection (§22.4.2) is
/// *adopted here, not translated* — there was nothing to translate. Signing the
/// head authenticates every entry transitively reachable via the `prev` chain.
pub fn build_feed_head(kp: &Keypair, tip: &FeedEntry, ts: u64) -> FeedHead {
    let key = keypair_to_identity_key(kp);
    let pk = key.public();
    let mut h = FeedHead {
        v: PUB_V0,
        suite: Suite::Classical,
        publisher: pk.clone(),
        seq: tip.seq,
        tip: tip.entry_id(),
        ts,
        signer: pk,
        sig: Vec::new(),
    };
    h.sign(&key);
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::ChunkTree;

    fn kp() -> Keypair {
        Keypair::from_secret_bytes(&[7u8; 32])
    }

    #[test]
    fn identity_key_material_is_shared() {
        // The convergence's cheapest win: the same Ed25519 seed yields the same
        // public key on both sides, so no key migration is needed.
        let k = kp();
        let ik = keypair_to_identity_key(&k);
        assert_eq!(ik.public(), k.public_key_bytes().to_vec());
    }

    /// Pins the §24.14-item-4 erratum: vidmesh's retained per-chunk digest is
    /// NOT the digest §22 needs, so migration cannot skip re-reading the media.
    #[test]
    fn stored_vidmesh_leaf_is_not_the_pub_chunk_address() {
        // This exact chunk is the one behind the frozen vector
        // `pub_manifest_single_chunk`, so the §22 side is corpus-anchored.
        let chunk = b"dmtap-pub: one published chunk";
        let (vid_leaf, pub_addr) = divergence::chunk_digests_for(chunk);

        // The §22 address matches the frozen conformance vector byte-for-byte.
        assert_eq!(
            hex_of(pub_addr.as_bytes()),
            "1e458cd8409c3b46d1e59eebedaab232ae9054e51d2cc01e3a0ef7447017301eaf",
            "pub chunk address must match the frozen §22 vector"
        );

        // ...and what vidmesh stored is a different digest entirely.
        assert_ne!(
            vid_leaf,
            *<&[u8; 32]>::try_from(&pub_addr.as_bytes()[1..]).unwrap(),
            "§24.14 item 4 claims these are identical; they are not — \
             migration MUST re-read media bytes, not recompute over stored digests"
        );
    }

    fn hex_of(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn content_id_prefix_round_trips() {
        let b = BlobId([0x5a; 32]);
        let cid = blob_id_to_content_id(&b);
        assert_eq!(cid.as_bytes().len(), 33);
        assert_eq!(cid.as_bytes()[0], MH_BLAKE3_256);
        assert_eq!(content_id_to_blob_id(&cid), Some(b));
    }

    #[test]
    fn non_blake3_content_id_has_no_vidmesh_form() {
        // A future SHA-2 address must not be silently truncated into a BlobId.
        let mut raw = vec![0x12u8];
        raw.extend_from_slice(&[0u8; 32]);
        assert_eq!(content_id_to_blob_id(&ContentId(raw)), None);
    }

    /// THE load-bearing negative result: same tree shape, different values.
    #[test]
    fn native_and_pub_roots_differ_for_every_chunk_count() {
        for n in 1usize..=9 {
            // Small synthetic "chunks" are fine here: both trees are driven by
            // the same chunking, and we are comparing values, not sizes.
            let bytes: Vec<u8> = (0..n).map(|i| i as u8).collect();
            let (native, pubroot) = divergence::roots_for(&bytes).unwrap();
            let native_framed = content_id_from_digest(&native);
            assert_ne!(
                native_framed, pubroot,
                "vidmesh and §22 roots must NOT coincide (n={n}); \
                 if they ever do, the proofs would be silently interchangeable"
            );
        }
    }

    #[test]
    fn empty_blob_has_no_pub_manifest() {
        assert!(pub_manifest_for_bytes(&[]).is_none());
        // ...while vidmesh tolerates the empty tree. A phase-2 migration item.
        assert_eq!(ChunkTree::from_bytes(&[]).root(), None);
    }

    #[test]
    fn pub_manifest_self_verifies_and_round_trips() {
        let bytes = vec![0xa5u8; 4096];
        let m = pub_manifest_for_bytes(&bytes).unwrap();
        m.verify().unwrap();
        let decoded = PubManifest::from_det_cbor(&m.det_cbor()).unwrap();
        assert_eq!(decoded, m);
    }

    #[test]
    fn announce_signs_verifies_and_round_trips() {
        let bytes = vec![1u8, 2, 3, 4];
        let m = pub_manifest_for_bytes(&bytes).unwrap();
        let a = build_announce(
            &kp(),
            vec![m.id.clone()],
            Vec::new(),
            1_700_000_000_000,
            None,
        );
        a.verify(&a.announce_id()).unwrap();
        let decoded = PubAnnounce::from_det_cbor(&a.det_cbor()).unwrap();
        assert_eq!(decoded, a);
    }

    #[test]
    fn announce_rejects_tampered_meta() {
        let bytes = vec![9u8; 64];
        let m = pub_manifest_for_bytes(&bytes).unwrap();
        let mut a = build_announce(&kp(), vec![m.id], Vec::new(), 42, None);
        let good_id = a.announce_id();
        a.meta.push(("injected".to_string(), Cv::U64(1)));
        // The id moves with the bytes, and the signature no longer covers them.
        assert!(a.verify(&good_id).is_err());
        assert!(a.verify(&a.announce_id()).is_err());
    }

    #[test]
    fn feed_chain_builds_and_verifies_against_signed_head() {
        let k = kp();
        let bytes = vec![3u8; 32];
        let m = pub_manifest_for_bytes(&bytes).unwrap();
        let a0 = build_announce(&k, vec![m.id.clone()], Vec::new(), 1, None);
        let a1 = build_announce(&k, vec![m.id], Vec::new(), 2, None);

        let e0 = feed_genesis(&a0, 1);
        let e1 = feed_append(&e0, &a1, 2);
        let entries = vec![e0, e1.clone()];

        verify_feed_chain(&entries).unwrap();
        let head = build_feed_head(&k, &e1, 3);
        head.verify().unwrap();
        verify_feed_chain_to_head(&entries, &head).unwrap();
    }

    #[test]
    fn anti_rollback_rejects_a_lower_seq_head() {
        // The capability vidmesh has no counterpart for at all.
        let k = kp();
        let bytes = vec![4u8; 16];
        let m = pub_manifest_for_bytes(&bytes).unwrap();
        let a0 = build_announce(&k, vec![m.id.clone()], Vec::new(), 1, None);
        let a1 = build_announce(&k, vec![m.id], Vec::new(), 2, None);
        let e0 = feed_genesis(&a0, 1);
        let e1 = feed_append(&e0, &a1, 2);

        let old = build_feed_head(&k, &e0, 1);
        let new = build_feed_head(&k, &e1, 2);

        let mut f = FeedFollower::new(k.public_key_bytes().to_vec());
        assert_eq!(
            f.accept(&new, &[e0.clone(), e1]).unwrap(),
            RollbackDecision::AcceptNew
        );
        // Serving an older head after a newer one is a rollback attempt (0x0907).
        assert_eq!(f.accept(&old, &[e0]), Err(PubError::FeedRollback));
    }

    #[test]
    fn record_meta_carries_kind_and_record_id() {
        use crate::record::RecordBuilder;
        let k = kp();
        let rec = RecordBuilder::new(1)
            .created_at(0)
            .sign_as(&k, crate::ids::IdentityId::ZERO)
            .expect("builds");
        let meta = meta_for_record(&rec);
        assert_eq!(meta.len(), 2);
        assert_eq!(meta[0].0, META_KEY_VIDMESH_KIND);
        assert_eq!(meta[1].0, META_KEY_VIDMESH_RECORD);
    }
}
