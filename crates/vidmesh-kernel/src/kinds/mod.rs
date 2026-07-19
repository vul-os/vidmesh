//! Typed record-kind wrappers (spec 003): kind ids, kind name lookup,
//! kind-level validation dispatch, and helpers shared by every kind
//! module.
//!
//! Each submodule owns one registry group and exposes one struct per
//! kind with a `parse(&Record) -> Result<Self>` that enforces every
//! validation rule spec 003 states for that kind, and a `to_body(&self)
//! -> Value` (plus, where refs carry kind-defined meaning, a
//! `refs(&self) -> Vec<Ref>`) so callers can round-trip through
//! [`crate::record::RecordBuilder`].
//!
//! [`validate`] is the single entry point interpreters call after
//! [`crate::record::Record::verify`]: it dispatches on `record.kind()`
//! to the matching wrapper's `parse` and discards the parsed value,
//! surfacing only whether the record is kind-valid. Unknown kinds are
//! kind-valid by definition (spec 003 §1: "A record that is
//! envelope-valid but kind-invalid MUST be ignored by interpreters...";
//! kinds this crate does not know about are simply not interpreted, not
//! rejected).
//!
//! Some validation rules in spec 003 depend on a *second* record (the
//! target of a `supersede`, the parent of a `comment`, the grant a
//! `delegate` revokes...) that `parse` cannot see. Those rules are
//! implemented as separate `check_*` functions next to the relevant
//! struct, documented at each call site; `validate`/`parse` alone are
//! not a complete kind-validity check for those kinds — see each
//! module's doc comments for the exact set.

/// Claim kinds (spec 003 §6.1-6.4): `claim.author`, `claim.license`,
/// `claim.transfer`, `claim.dispute`.
pub mod claims;
/// Compliance kinds (spec 003 §6.5-6.7): `notice.takedown`,
/// `notice.counter`, `feed.takedown`.
pub mod compliance;
/// Content kinds (spec 003 §4) and `delegate` (spec 003 §3.3):
/// `manifest`, `supersede`, `retract`, `mirror`, `similarity`.
pub mod content;
/// Infrastructure and privacy kinds (spec 003 §8): `anchor`, `keygrant`.
pub mod infra;
/// Live kinds (spec 003 §9): `live.manifest`, `live.chat`.
pub mod live;
/// Social kinds (spec 003 §5) and `profile` (spec 003 §3.2): `comment`,
/// `reaction`, `follow`, `playlist`, `channel`.
pub mod social;
/// Trust and economics kinds (spec 003 §7): `endorse.gateway`,
/// `receipt`, `attest`.
pub mod trust;

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::{BlobId, IdentityId, RecordId};
use crate::record::{Record, Ref};

/// Kind id of `rotation` records (spec 003 §3.1). Re-exported from
/// [`crate::identity`], which owns rotation chain parsing and
/// verification; this module only checks the envelope-level refs shape
/// (see [`validate`]).
pub use crate::identity::KIND_ROTATION;

// Flat re-exports of the typed wrappers and the derivation helpers so
// consumers (WASM bindings, relay, conformance) can use `kinds::Manifest`
// etc. without knowing the internal file layout.
pub use claims::{ClaimAuthor, ClaimDispute, ClaimLicense, ClaimTransfer};
pub use compliance::{FeedEntry, FeedTakedown, NoticeCounter, NoticeTakedown};
pub use content::{
    derivation_statement, verify_derivation, Delegate, Manifest, Media, Mirror, Rendition, Retract,
    Similarity, Supersede, DERIVATION_SIG_PREFIX,
};
pub use infra::{Anchor, Keygrant};
pub use live::{LiveChat, LiveManifest};
pub use social::{Channel, Comment, Follow, Playlist, Profile, Reaction};
pub use trust::{Attest, EndorseGateway, Receipt};

/// Kind id of `profile` records (spec 003 §3.2).
pub const KIND_PROFILE: u64 = 2;
/// Kind id of `delegate` records (spec 003 §3.3).
pub const KIND_DELEGATE: u64 = 3;
/// Kind id of `manifest` records (spec 003 §4.1).
pub const KIND_MANIFEST: u64 = 16;
/// Kind id of `supersede` records (spec 003 §4.2).
pub const KIND_SUPERSEDE: u64 = 17;
/// Kind id of `retract` records (spec 003 §4.3).
pub const KIND_RETRACT: u64 = 18;
/// Kind id of `mirror` records (spec 003 §4.4).
pub const KIND_MIRROR: u64 = 19;
/// Kind id of `similarity` records (spec 003 §4.5).
pub const KIND_SIMILARITY: u64 = 20;
/// Kind id of `comment` records (spec 003 §5.1).
pub const KIND_COMMENT: u64 = 32;
/// Kind id of `reaction` records (spec 003 §5.2).
pub const KIND_REACTION: u64 = 33;
/// Kind id of `follow` records (spec 003 §5.3).
pub const KIND_FOLLOW: u64 = 34;
/// Kind id of `playlist` records (spec 003 §5.4).
pub const KIND_PLAYLIST: u64 = 35;
/// Kind id of `channel` records (spec 003 §5.5).
pub const KIND_CHANNEL: u64 = 36;
/// Kind id of `claim.author` records (spec 003 §6.1).
pub const KIND_CLAIM_AUTHOR: u64 = 48;
/// Kind id of `claim.license` records (spec 003 §6.2).
pub const KIND_CLAIM_LICENSE: u64 = 49;
/// Kind id of `claim.transfer` records (spec 003 §6.3).
pub const KIND_CLAIM_TRANSFER: u64 = 50;
/// Kind id of `claim.dispute` records (spec 003 §6.4).
pub const KIND_CLAIM_DISPUTE: u64 = 51;
/// Kind id of `notice.takedown` records (spec 003 §6.5).
pub const KIND_NOTICE_TAKEDOWN: u64 = 64;
/// Kind id of `notice.counter` records (spec 003 §6.6).
pub const KIND_NOTICE_COUNTER: u64 = 65;
/// Kind id of `feed.takedown` records (spec 003 §6.7).
pub const KIND_FEED_TAKEDOWN: u64 = 66;
/// Kind id of `endorse.gateway` records (spec 003 §7.1).
pub const KIND_ENDORSE_GATEWAY: u64 = 80;
/// Kind id of `receipt` records (spec 003 §7.2).
pub const KIND_RECEIPT: u64 = 81;
/// Kind id of `attest` records (spec 003 §7.3).
pub const KIND_ATTEST: u64 = 82;
/// Kind id of `anchor` records (spec 003 §8.1).
pub const KIND_ANCHOR: u64 = 96;
/// Kind id of `keygrant` records (spec 003 §8.2).
pub const KIND_KEYGRANT: u64 = 97;
/// Kind id of `live.manifest` records (spec 003 §9.1).
pub const KIND_LIVE_MANIFEST: u64 = 112;
/// Kind id of `live.chat` records (spec 003 §9.2).
pub const KIND_LIVE_CHAT: u64 = 113;

/// The stable registry name for a kind id (spec 003 §1), or `None` if
/// the id is not a registered launch kind.
pub fn kind_name(kind: u64) -> Option<&'static str> {
    Some(match kind {
        KIND_ROTATION => "rotation",
        KIND_PROFILE => "profile",
        KIND_DELEGATE => "delegate",
        KIND_MANIFEST => "manifest",
        KIND_SUPERSEDE => "supersede",
        KIND_RETRACT => "retract",
        KIND_MIRROR => "mirror",
        KIND_SIMILARITY => "similarity",
        KIND_COMMENT => "comment",
        KIND_REACTION => "reaction",
        KIND_FOLLOW => "follow",
        KIND_PLAYLIST => "playlist",
        KIND_CHANNEL => "channel",
        KIND_CLAIM_AUTHOR => "claim.author",
        KIND_CLAIM_LICENSE => "claim.license",
        KIND_CLAIM_TRANSFER => "claim.transfer",
        KIND_CLAIM_DISPUTE => "claim.dispute",
        KIND_NOTICE_TAKEDOWN => "notice.takedown",
        KIND_NOTICE_COUNTER => "notice.counter",
        KIND_FEED_TAKEDOWN => "feed.takedown",
        KIND_ENDORSE_GATEWAY => "endorse.gateway",
        KIND_RECEIPT => "receipt",
        KIND_ATTEST => "attest",
        KIND_ANCHOR => "anchor",
        KIND_KEYGRANT => "keygrant",
        KIND_LIVE_MANIFEST => "live.manifest",
        KIND_LIVE_CHAT => "live.chat",
        _ => return None,
    })
}

/// The kind id for a registry name (spec 003 §1), or `None` if the name
/// is not a registered launch kind.
pub fn kind_id(name: &str) -> Option<u64> {
    Some(match name {
        "rotation" => KIND_ROTATION,
        "profile" => KIND_PROFILE,
        "delegate" => KIND_DELEGATE,
        "manifest" => KIND_MANIFEST,
        "supersede" => KIND_SUPERSEDE,
        "retract" => KIND_RETRACT,
        "mirror" => KIND_MIRROR,
        "similarity" => KIND_SIMILARITY,
        "comment" => KIND_COMMENT,
        "reaction" => KIND_REACTION,
        "follow" => KIND_FOLLOW,
        "playlist" => KIND_PLAYLIST,
        "channel" => KIND_CHANNEL,
        "claim.author" => KIND_CLAIM_AUTHOR,
        "claim.license" => KIND_CLAIM_LICENSE,
        "claim.transfer" => KIND_CLAIM_TRANSFER,
        "claim.dispute" => KIND_CLAIM_DISPUTE,
        "notice.takedown" => KIND_NOTICE_TAKEDOWN,
        "notice.counter" => KIND_NOTICE_COUNTER,
        "feed.takedown" => KIND_FEED_TAKEDOWN,
        "endorse.gateway" => KIND_ENDORSE_GATEWAY,
        "receipt" => KIND_RECEIPT,
        "attest" => KIND_ATTEST,
        "anchor" => KIND_ANCHOR,
        "keygrant" => KIND_KEYGRANT,
        "live.manifest" => KIND_LIVE_MANIFEST,
        "live.chat" => KIND_LIVE_CHAT,
        _ => return None,
    })
}

/// Kind-level validation dispatch (spec 003). Parses the record's body
/// with the wrapper matching `record.kind()` and discards the result,
/// so `Ok(())` means "kind-valid", not "known and returned". Unknown
/// kinds are `Ok(())`: spec 003 §1 requires interpreters to ignore
/// kind-invalid records but says nothing about kinds they do not
/// recognize at all, and forward-compatibility (the registry grows over
/// time) requires unknown kinds to pass through undisturbed.
///
/// This does not perform envelope validation or signature verification
/// — call [`Record::verify`] first — and it does not perform the
/// cross-record checks documented on individual `check_*` functions
/// (e.g. [`social::check_comment_thread`]), which need a second parsed
/// record that `validate` does not have access to.
pub fn validate(record: &Record) -> Result<()> {
    match record.kind() {
        KIND_ROTATION => validate_rotation_refs(record),
        KIND_PROFILE => social::Profile::parse(record).map(|_| ()),
        KIND_DELEGATE => content::Delegate::parse(record).map(|_| ()),
        KIND_MANIFEST => validate_manifest(record),
        KIND_SUPERSEDE => content::Supersede::parse(record).map(|_| ()),
        KIND_RETRACT => content::Retract::parse(record).map(|_| ()),
        KIND_MIRROR => content::Mirror::parse(record).map(|_| ()),
        KIND_SIMILARITY => content::Similarity::parse(record).map(|_| ()),
        KIND_COMMENT => social::Comment::parse(record).map(|_| ()),
        KIND_REACTION => social::Reaction::parse(record).map(|_| ()),
        KIND_FOLLOW => social::Follow::parse(record).map(|_| ()),
        KIND_PLAYLIST => social::Playlist::parse(record).map(|_| ()),
        KIND_CHANNEL => social::Channel::parse(record).map(|_| ()),
        KIND_CLAIM_AUTHOR => claims::ClaimAuthor::parse(record).map(|_| ()),
        KIND_CLAIM_LICENSE => claims::ClaimLicense::parse(record).map(|_| ()),
        KIND_CLAIM_TRANSFER => claims::ClaimTransfer::parse(record).map(|_| ()),
        KIND_CLAIM_DISPUTE => claims::ClaimDispute::parse(record).map(|_| ()),
        KIND_NOTICE_TAKEDOWN => compliance::NoticeTakedown::parse(record).map(|_| ()),
        KIND_NOTICE_COUNTER => compliance::NoticeCounter::parse(record).map(|_| ()),
        KIND_FEED_TAKEDOWN => compliance::FeedTakedown::parse(record).map(|_| ()),
        KIND_ENDORSE_GATEWAY => trust::EndorseGateway::parse(record).map(|_| ()),
        KIND_RECEIPT => trust::Receipt::parse(record).map(|_| ()),
        KIND_ATTEST => trust::Attest::parse(record).map(|_| ()),
        KIND_ANCHOR => infra::Anchor::parse(record).map(|_| ()),
        KIND_KEYGRANT => infra::Keygrant::parse(record).map(|_| ()),
        KIND_LIVE_MANIFEST => live::LiveManifest::parse(record).map(|_| ()),
        KIND_LIVE_CHAT => live::LiveChat::parse(record).map(|_| ()),
        _ => Ok(()),
    }
}

/// Full kind-level validation of a `manifest` record: the structural parse
/// (`Manifest::parse`) plus the cryptographic check that every rendition's
/// `derivation_sig` verifies against the manifest's own original blob (spec
/// 004 §3.1). `Manifest::parse` deliberately stops at structure — this is
/// the caller it documents as responsible for running [`verify_derivation`]
/// per rendition. (Authorization of `produced_by`, which needs a second
/// record, remains out of scope here, as does transcode fidelity.)
fn validate_manifest(record: &Record) -> Result<()> {
    let manifest = content::Manifest::parse(record)?;
    for rendition in &manifest.renditions {
        content::verify_derivation(rendition, &manifest.original.blob)?;
    }
    Ok(())
}

/// The refs-shape half of `rotation` validity (spec 002 §§2–3, restated
/// by spec 003 §3.1): a genesis record (author identity id all-zero)
/// MUST have empty refs; every other rotation MUST have exactly one
/// record ref (its parent). Chain-level rules — authorization,
/// fork resolution, contest windows — are [`crate::identity::Identity`]'s
/// job, not this crate's; this function only guards the shape
/// [`crate::identity::RotationBody::parse`] itself does not check.
fn validate_rotation_refs(record: &Record) -> Result<()> {
    let is_genesis = record.author().identity_id == IdentityId::ZERO;
    if is_genesis {
        refs_empty(record, "rotation: genesis record must have empty refs")
    } else {
        if record.refs().len() != 1 || !record.refs()[0].is_record() {
            return Err(Error::Kind(
                "rotation: non-genesis record must have exactly one record ref",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------
// Shared parsing helpers (spec 003 §2 conventions).
//
// Every helper below treats a present-but-wrong-typed field the same as
// spec 003 §2 requires: "a missing required field, or a wrong type,
// makes the record kind-invalid." Unknown body keys are never inspected
// here, so they are implicitly ignored, per the same section.
// ---------------------------------------------------------------------

/// Look up a text field, enforcing `max_len` (bytes) when present.
/// `Ok(None)` if the key is absent; `Err` if present with the wrong type
/// or too long.
pub(crate) fn text_field(
    body: &Value,
    key: &str,
    max_len: usize,
    msg: &'static str,
) -> Result<Option<String>> {
    match body.map_get(key) {
        None => Ok(None),
        Some(v) => {
            let s = v.as_text().ok_or(Error::Kind(msg))?;
            if s.len() > max_len {
                return Err(Error::Kind(msg));
            }
            Ok(Some(s.to_string()))
        }
    }
}

/// A required text field: `Err` if absent, wrongly typed, or too long.
pub(crate) fn required_text(
    body: &Value,
    key: &str,
    max_len: usize,
    msg: &'static str,
) -> Result<String> {
    text_field(body, key, max_len, msg)?.ok_or(Error::Kind(msg))
}

/// A required non-empty text field (byte length in `1..=max_len`).
pub(crate) fn required_nonempty_text(
    body: &Value,
    key: &str,
    max_len: usize,
    msg: &'static str,
) -> Result<String> {
    let s = required_text(body, key, max_len, msg)?;
    if s.is_empty() {
        return Err(Error::Kind(msg));
    }
    Ok(s)
}

/// Look up a `u64` field. `Ok(None)` if absent; `Err` if present with
/// the wrong type.
pub(crate) fn u64_field(body: &Value, key: &str, msg: &'static str) -> Result<Option<u64>> {
    match body.map_get(key) {
        None => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or(Error::Kind(msg)),
    }
}

/// A required `u64` field.
pub(crate) fn required_u64(body: &Value, key: &str, msg: &'static str) -> Result<u64> {
    u64_field(body, key, msg)?.ok_or(Error::Kind(msg))
}

/// Look up an `i64` field (spec 003's `int` type). `Ok(None)` if absent;
/// `Err` if present with the wrong type or out of `i64` range.
pub(crate) fn i64_field(body: &Value, key: &str, msg: &'static str) -> Result<Option<i64>> {
    match body.map_get(key) {
        None => Ok(None),
        Some(v) => v.as_i64().map(Some).ok_or(Error::Kind(msg)),
    }
}

/// Look up a `bool` field. `Ok(None)` if absent; `Err` if present with
/// the wrong type.
pub(crate) fn bool_field(body: &Value, key: &str, msg: &'static str) -> Result<Option<bool>> {
    match body.map_get(key) {
        None => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or(Error::Kind(msg)),
    }
}

/// A `bool` field with a default when absent.
pub(crate) fn bool_or(body: &Value, key: &str, default: bool, msg: &'static str) -> Result<bool> {
    Ok(bool_field(body, key, msg)?.unwrap_or(default))
}

/// A required `bool` field.
pub(crate) fn required_bool(body: &Value, key: &str, msg: &'static str) -> Result<bool> {
    bool_field(body, key, msg)?.ok_or(Error::Kind(msg))
}

/// Look up an arbitrary-length bytes field.
pub(crate) fn bytes_field(body: &Value, key: &str, msg: &'static str) -> Result<Option<Vec<u8>>> {
    match body.map_get(key) {
        None => Ok(None),
        Some(v) => v
            .as_bytes()
            .map(|b| b.to_vec())
            .map(Some)
            .ok_or(Error::Kind(msg)),
    }
}

/// A required arbitrary-length bytes field.
pub(crate) fn required_bytes(body: &Value, key: &str, msg: &'static str) -> Result<Vec<u8>> {
    bytes_field(body, key, msg)?.ok_or(Error::Kind(msg))
}

/// Look up a fixed 32-byte field.
pub(crate) fn bytes32_field(
    body: &Value,
    key: &str,
    msg: &'static str,
) -> Result<Option<[u8; 32]>> {
    match body.map_get(key) {
        None => Ok(None),
        Some(v) => {
            let b = v.as_bytes().ok_or(Error::Kind(msg))?;
            let arr: [u8; 32] = b.try_into().map_err(|_| Error::Kind(msg))?;
            Ok(Some(arr))
        }
    }
}

/// A required fixed 32-byte field.
pub(crate) fn required_bytes32(body: &Value, key: &str, msg: &'static str) -> Result<[u8; 32]> {
    bytes32_field(body, key, msg)?.ok_or(Error::Kind(msg))
}

/// Look up a `BlobId` field (32 bytes).
pub(crate) fn blob_id_field(body: &Value, key: &str, msg: &'static str) -> Result<Option<BlobId>> {
    Ok(bytes32_field(body, key, msg)?.map(BlobId))
}

/// A required `BlobId` field.
pub(crate) fn required_blob_id(body: &Value, key: &str, msg: &'static str) -> Result<BlobId> {
    blob_id_field(body, key, msg)?.ok_or(Error::Kind(msg))
}

/// Look up an `IdentityId` field (32 bytes).
pub(crate) fn identity_id_field(
    body: &Value,
    key: &str,
    msg: &'static str,
) -> Result<Option<IdentityId>> {
    Ok(bytes32_field(body, key, msg)?.map(IdentityId))
}

/// A required `IdentityId` field.
pub(crate) fn required_identity_id(
    body: &Value,
    key: &str,
    msg: &'static str,
) -> Result<IdentityId> {
    identity_id_field(body, key, msg)?.ok_or(Error::Kind(msg))
}

/// Look up an array field.
pub(crate) fn array_field<'a>(
    body: &'a Value,
    key: &str,
    msg: &'static str,
) -> Result<Option<&'a [Value]>> {
    match body.map_get(key) {
        None => Ok(None),
        Some(v) => v.as_array().map(Some).ok_or(Error::Kind(msg)),
    }
}

/// A required array field.
pub(crate) fn required_array<'a>(
    body: &'a Value,
    key: &str,
    msg: &'static str,
) -> Result<&'a [Value]> {
    array_field(body, key, msg)?.ok_or(Error::Kind(msg))
}

/// An optional array of `BlobId`s (32-byte entries); `[]` if the key is
/// absent.
pub(crate) fn blob_id_array(body: &Value, key: &str, msg: &'static str) -> Result<Vec<BlobId>> {
    match array_field(body, key, msg)? {
        None => Ok(Vec::new()),
        Some(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                let b = v.as_bytes().ok_or(Error::Kind(msg))?;
                let a: [u8; 32] = b.try_into().map_err(|_| Error::Kind(msg))?;
                out.push(BlobId(a));
            }
            Ok(out)
        }
    }
}

/// An optional array of text strings, each capped at `max_item_len`
/// bytes; `[]` if the key is absent.
pub(crate) fn text_array(
    body: &Value,
    key: &str,
    max_item_len: usize,
    msg: &'static str,
) -> Result<Vec<String>> {
    match array_field(body, key, msg)? {
        None => Ok(Vec::new()),
        Some(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                let s = v.as_text().ok_or(Error::Kind(msg))?;
                if s.len() > max_item_len {
                    return Err(Error::Kind(msg));
                }
                out.push(s.to_string());
            }
            Ok(out)
        }
    }
}

/// `record.refs()` must be empty.
pub(crate) fn refs_empty(record: &Record, msg: &'static str) -> Result<()> {
    if record.refs().is_empty() {
        Ok(())
    } else {
        Err(Error::Kind(msg))
    }
}

/// `record.refs()` must have exactly `n` entries.
pub(crate) fn refs_exact(record: &Record, n: usize, msg: &'static str) -> Result<()> {
    if record.refs().len() == n {
        Ok(())
    } else {
        Err(Error::Kind(msg))
    }
}

/// `record.refs()` must have at least `n` entries.
pub(crate) fn refs_min(record: &Record, n: usize, msg: &'static str) -> Result<()> {
    if record.refs().len() >= n {
        Ok(())
    } else {
        Err(Error::Kind(msg))
    }
}

/// A ref must be a record ref; returns its id.
pub(crate) fn ref_record_id(r: &Ref, msg: &'static str) -> Result<RecordId> {
    if r.is_record() {
        Ok(RecordId(r.hash))
    } else {
        Err(Error::Kind(msg))
    }
}

/// Validate a `license` token (spec 004 §4): non-empty, at most 64
/// bytes. The field is free text by design ("an SPDX license identifier,
/// or one of: ..."; "the field enforces nothing"), so this deliberately
/// does not check the token against the SPDX list or the three Vidmesh
/// values — see this crate's top-level report for the reasoning.
pub(crate) fn validate_license(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 64 {
        Err(Error::Kind("license must be 1-64 bytes"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;
    use crate::record::RecordBuilder;

    fn kp() -> Keypair {
        Keypair::from_secret_bytes(&[42u8; 32])
    }

    #[test]
    fn kind_name_and_id_cover_all_27_kinds() {
        let ids = [
            KIND_ROTATION,
            KIND_PROFILE,
            KIND_DELEGATE,
            KIND_MANIFEST,
            KIND_SUPERSEDE,
            KIND_RETRACT,
            KIND_MIRROR,
            KIND_SIMILARITY,
            KIND_COMMENT,
            KIND_REACTION,
            KIND_FOLLOW,
            KIND_PLAYLIST,
            KIND_CHANNEL,
            KIND_CLAIM_AUTHOR,
            KIND_CLAIM_LICENSE,
            KIND_CLAIM_TRANSFER,
            KIND_CLAIM_DISPUTE,
            KIND_NOTICE_TAKEDOWN,
            KIND_NOTICE_COUNTER,
            KIND_FEED_TAKEDOWN,
            KIND_ENDORSE_GATEWAY,
            KIND_RECEIPT,
            KIND_ATTEST,
            KIND_ANCHOR,
            KIND_KEYGRANT,
            KIND_LIVE_MANIFEST,
            KIND_LIVE_CHAT,
        ];
        assert_eq!(ids.len(), 27);
        for id in ids {
            let name = kind_name(id).unwrap_or_else(|| panic!("no name for kind {id}"));
            assert_eq!(kind_id(name), Some(id), "round trip for {name}");
        }
    }

    #[test]
    fn unknown_kind_name_and_id_are_none() {
        assert_eq!(kind_name(9999), None);
        assert_eq!(kind_id("no-such-kind"), None);
        assert_eq!(kind_name(0), None); // kind 0 is reserved (spec 003 §1)
    }

    #[test]
    fn validate_unknown_kind_is_ok() {
        let record = RecordBuilder::new(9999)
            .created_at(1)
            .sign_as(&kp(), IdentityId::ZERO)
            .unwrap();
        assert!(validate(&record).is_ok());
    }

    #[test]
    fn validate_rotation_genesis_and_non_genesis_refs() {
        let (id, genesis) = crate::identity::Identity::genesis(&kp(), &[], 604_800, 1).unwrap();
        assert!(validate(&genesis).is_ok());

        let rot = crate::identity::Identity::rotate(
            id,
            genesis.id(),
            &kp().public_key_bytes(),
            crate::record::SIG_ALG_ED25519,
            &[],
            604_800,
            2,
            &kp(),
        )
        .unwrap();
        assert!(validate(&rot).is_ok());
    }

    #[test]
    fn validate_rotation_rejects_nonempty_genesis_refs() {
        let bad = RecordBuilder::new(KIND_ROTATION)
            .created_at(1)
            .r#ref(Ref::record(RecordId([9; 32])))
            .body(Value::Map(vec![
                (
                    Value::Text("key".into()),
                    Value::Bytes(kp().public_key_bytes().to_vec()),
                ),
                (Value::Text("key_alg".into()), Value::Uint(1)),
                (Value::Text("recovery".into()), Value::Array(vec![])),
                (Value::Text("contest_window".into()), Value::Uint(0)),
            ]))
            .sign_as(&kp(), IdentityId::ZERO)
            .unwrap();
        assert!(matches!(validate(&bad), Err(Error::Kind(_))));
    }

    #[test]
    fn validate_rotation_rejects_missing_refs_when_non_genesis() {
        let bad = RecordBuilder::new(KIND_ROTATION)
            .created_at(1)
            .body(Value::Map(vec![
                (
                    Value::Text("key".into()),
                    Value::Bytes(kp().public_key_bytes().to_vec()),
                ),
                (Value::Text("key_alg".into()), Value::Uint(1)),
                (Value::Text("recovery".into()), Value::Array(vec![])),
                (Value::Text("contest_window".into()), Value::Uint(0)),
            ]))
            .sign_as(&kp(), IdentityId([1; 32]))
            .unwrap();
        assert!(matches!(validate(&bad), Err(Error::Kind(_))));
    }
}
