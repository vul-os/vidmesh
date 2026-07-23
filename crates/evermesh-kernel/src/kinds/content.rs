//! Content kinds (spec 003 §4): `manifest`, `supersede`, `retract`,
//! `mirror`, `similarity`; plus `delegate` (spec 003 §3.3), grouped here
//! because its one launch capability, `rendition`, exists only to
//! authorize third-party manifest renditions (spec 004 §3).
//!
//! The `manifest` structures (`Media`, `Rendition`, `Caption`,
//! `Encryption`, `Sponsor`, `PaymentPointer`, `Hint`) and the
//! verifiable-derivation machinery implement spec 004 §§1–4.

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::{BlobId, IdentityId, RecordId};
use crate::record::{Record, Ref};

use super::{
    array_field, blob_id_field, bool_or, bytes32_field, bytes_field, i64_field, ref_record_id,
    refs_exact, refs_min, required_blob_id, required_bytes, required_identity_id,
    required_nonempty_text, required_text, required_u64, text_array, text_field, u64_field,
    validate_license, KIND_ROTATION,
};

/// Byte size above which [`Media::chunk_root`] is mandatory (1 MiB;
/// spec 004 §2, spec 001 §8). Reuses [`crate::blob::CHUNK_SIZE`] so the
/// threshold cannot drift from the chunk-tree implementation it refers
/// to.
const CHUNK_SIZE_BYTES: u64 = crate::blob::CHUNK_SIZE as u64;

fn is_null(v: &Value) -> bool {
    *v == Value::Null
}

/// A retrieval hint: `[hint_type, url]` (spec 001 §9). Interpretation of
/// `hint_type` is out of scope for this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint(pub u64, pub String);

impl Hint {
    fn to_value(&self) -> Value {
        Value::Array(vec![Value::Uint(self.0), Value::Text(self.1.clone())])
    }

    fn parse(v: &Value) -> Result<Hint> {
        let arr = v
            .as_array()
            .ok_or(Error::Kind("hint must be [type, url]"))?;
        if arr.len() != 2 {
            return Err(Error::Kind("hint must have exactly 2 elements"));
        }
        let t = arr[0]
            .as_u64()
            .ok_or(Error::Kind("hint type must be a uint"))?;
        let url = arr[1]
            .as_text()
            .ok_or(Error::Kind("hint url must be text"))?
            .to_string();
        Ok(Hint(t, url))
    }
}

/// A payment pointer: `[rail, pointer]` (spec 001 §9, spec 010 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentPointer(pub u64, pub String);

impl PaymentPointer {
    /// Encode as `[rail, pointer]`. Visible crate-wide: `profile` (spec
    /// 003 §3.2) carries payment pointers too.
    pub(crate) fn to_value(&self) -> Value {
        Value::Array(vec![Value::Uint(self.0), Value::Text(self.1.clone())])
    }

    /// Parse from `[rail, pointer]`.
    pub(crate) fn parse(v: &Value) -> Result<PaymentPointer> {
        let arr = v
            .as_array()
            .ok_or(Error::Kind("payment pointer must be [rail, pointer]"))?;
        if arr.len() != 2 {
            return Err(Error::Kind("payment pointer must have exactly 2 elements"));
        }
        let rail = arr[0]
            .as_u64()
            .ok_or(Error::Kind("payment rail must be a uint"))?;
        let pointer = arr[1]
            .as_text()
            .ok_or(Error::Kind("payment pointer must be text"))?
            .to_string();
        Ok(PaymentPointer(rail, pointer))
    }
}

/// A sponsorship segment (spec 010 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sponsor {
    /// Segment start, milliseconds from the start of the video.
    pub start_ms: u64,
    /// Segment end, milliseconds from the start of the video.
    pub end_ms: u64,
    /// Display label for the segment (e.g. "sponsor", "intro").
    pub label: String,
}

impl Sponsor {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            (Value::Text("start_ms".into()), Value::Uint(self.start_ms)),
            (Value::Text("end_ms".into()), Value::Uint(self.end_ms)),
            (Value::Text("label".into()), Value::Text(self.label.clone())),
        ])
    }

    fn parse(v: &Value) -> Result<Sponsor> {
        let start_ms = required_u64(v, "start_ms", "sponsor: start_ms required")?;
        let end_ms = required_u64(v, "end_ms", "sponsor: end_ms required")?;
        let label = required_text(v, "label", usize::MAX, "sponsor: label required")?;
        Ok(Sponsor {
            start_ms,
            end_ms,
            label,
        })
    }
}

/// Content encryption metadata (spec 008 §3); `None`/absent on the
/// manifest means plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encryption {
    /// Registered encryption scheme id (spec 008 §2).
    pub scheme: u64,
    /// Optional human-readable hint identifying which key is needed.
    pub key_hint: Option<String>,
}

impl Encryption {
    fn to_value(&self) -> Value {
        let mut e = vec![(Value::Text("scheme".into()), Value::Uint(self.scheme))];
        if let Some(h) = &self.key_hint {
            e.push((Value::Text("key_hint".into()), Value::Text(h.clone())));
        }
        Value::Map(e)
    }

    fn parse(v: &Value) -> Result<Encryption> {
        let scheme = required_u64(v, "scheme", "encryption: scheme required")?;
        let key_hint = text_field(
            v,
            "key_hint",
            usize::MAX,
            "encryption: key_hint must be text",
        )?;
        Ok(Encryption { scheme, key_hint })
    }
}

/// The source or a derived encoding of a video **or audio** work (DMTAP
/// §24.4.2, superseding spec 004 §2's video-only text).
///
/// `width`/`height` are OPTIONAL and MUST be both present or both absent
/// (DMTAP §24.4.2, VID-16): both present means this encoding carries a
/// video track of those pixel dimensions; both absent means it carries
/// no video track (audio-only — a song, a podcast episode, a radio set,
/// or an audio-only rendition of a work that does have video). There is
/// deliberately no media-kind discriminator field; the presence of
/// `width`/`height` *is* the signal. [`Media::parse`] rejects a map
/// carrying exactly one of them as malformed.
///
/// `wrapped_blob_key` (spec 008 §2.1) is only meaningful when the owning
/// manifest's `encryption` is present, in which case it MUST be exactly
/// 64 bytes; [`Manifest::parse`] enforces that cross-field rule (a
/// single `Media` cannot check it in isolation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Media {
    /// Hash of the encoded file.
    pub blob: BlobId,
    /// Size in bytes.
    pub size: u64,
    /// Chunk-tree root; required when `size` exceeds one chunk (1 MiB).
    pub chunk_root: Option<[u8; 32]>,
    /// RFC 6381 codecs string, e.g. `"av01.0.08M.08"`, or an audio codec
    /// string, e.g. `"opus"`, `"mp4a.40.2"`, `"flac"` (DMTAP §24.4.2).
    pub codec: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Pixel width; present iff this encoding carries a video track.
    /// Both-or-neither with `height` (DMTAP §24.4.2, VID-16).
    pub width: Option<u64>,
    /// Pixel height; present iff this encoding carries a video track.
    /// Both-or-neither with `width` (DMTAP §24.4.2, VID-16).
    pub height: Option<u64>,
    /// The content key wrapped for this blob, when the manifest is
    /// encrypted (spec 008 §2.1); exactly 64 bytes when present.
    pub wrapped_blob_key: Option<Vec<u8>>,
}

impl Media {
    /// This `Media`'s fields as map entries, for splicing into a
    /// dedicated submap (`original`, a `captions[]` entry) or into a
    /// flat rendition map (see [`Rendition::to_value`]).
    fn entries(&self) -> Vec<(Value, Value)> {
        let mut e = vec![
            (
                Value::Text("blob".into()),
                Value::Bytes(self.blob.as_bytes().to_vec()),
            ),
            (Value::Text("size".into()), Value::Uint(self.size)),
            (Value::Text("codec".into()), Value::Text(self.codec.clone())),
            (
                Value::Text("duration".into()),
                Value::Uint(self.duration_ms),
            ),
        ];
        // Both-or-neither (DMTAP §24.4.2): an absent dimension is
        // omitted from the wire entirely, never written as `0` or as
        // CBOR null — `null` is reserved for the signed derivation
        // preimage's fixed-arity encoding (§24.4.4), never for the wire
        // map itself (§18.1.1's "absent optional field is omitted, not
        // null" rule, restated there).
        if let (Some(w), Some(h)) = (self.width, self.height) {
            e.push((Value::Text("width".into()), Value::Uint(w)));
            e.push((Value::Text("height".into()), Value::Uint(h)));
        }
        if let Some(cr) = self.chunk_root {
            e.push((Value::Text("chunk_root".into()), Value::Bytes(cr.to_vec())));
        }
        if let Some(k) = &self.wrapped_blob_key {
            e.push((Value::Text("wrapped_key".into()), Value::Bytes(k.clone())));
        }
        e
    }

    fn to_value(&self) -> Value {
        Value::Map(self.entries())
    }

    /// Parse `Media` fields out of `v`, which must be a map. Extra keys
    /// (e.g. a rendition's `bitrate`/`produced_by`/`derivation_sig`,
    /// spliced into the same map by [`Rendition::parse`]) are ignored,
    /// per spec 003 §2's forward-extensibility convention.
    fn parse(v: &Value) -> Result<Media> {
        let blob = required_blob_id(v, "blob", "media: blob required (32 bytes)")?;
        let size = required_u64(v, "size", "media: size required")?;
        let codec = required_nonempty_text(v, "codec", 256, "media: codec required")?;
        let duration_ms = required_u64(v, "duration", "media: duration required")?;
        // DMTAP §24.4.2, VID-16: width/height are OPTIONAL but MUST be
        // both present or both absent. Exactly one present is malformed
        // and MUST be rejected here, before any signature verification
        // a caller might run over a containing Rendition.
        let width = u64_field(v, "width", "media: width must be a uint")?;
        let height = u64_field(v, "height", "media: height must be a uint")?;
        if width.is_some() != height.is_some() {
            return Err(Error::Kind(
                "media: width and height must be both present or both absent",
            ));
        }
        let chunk_root = bytes32_field(v, "chunk_root", "media: chunk_root must be 32 bytes")?;
        if size > CHUNK_SIZE_BYTES && chunk_root.is_none() {
            return Err(Error::Kind(
                "media: chunk_root required when size exceeds 1 MiB",
            ));
        }
        let wrapped_blob_key = bytes_field(v, "wrapped_key", "media: wrapped_key must be bytes")?;
        Ok(Media {
            blob,
            size,
            chunk_root,
            codec,
            duration_ms,
            width,
            height,
            wrapped_blob_key,
        })
    }
}

/// A verifiable derivation of the manifest's original media (spec 004
/// §3): `Media` plus who transcoded it and their signature over the
/// derivation statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendition {
    /// The derived encoding.
    pub media: Media,
    /// Bitrate in bits/second. Part of the signed derivation statement
    /// (spec 004 §3.1), kept separate from `media` because it is not a
    /// `Media` field.
    pub bitrate: u64,
    /// `(identity, signing key bytes)` of whoever produced this
    /// rendition (spec 004 §3).
    pub produced_by: (IdentityId, Vec<u8>),
    /// Signature over the derivation statement (spec 004 §3.1); verify
    /// with [`verify_derivation`].
    pub derivation_sig: Vec<u8>,
}

impl Rendition {
    fn to_value(&self) -> Value {
        let mut e = self.media.entries();
        e.push((Value::Text("bitrate".into()), Value::Uint(self.bitrate)));
        e.push((
            Value::Text("produced_by".into()),
            Value::Array(vec![
                Value::Bytes(self.produced_by.0.as_bytes().to_vec()),
                Value::Bytes(self.produced_by.1.clone()),
            ]),
        ));
        e.push((
            Value::Text("derivation_sig".into()),
            Value::Bytes(self.derivation_sig.clone()),
        ));
        Value::Map(e)
    }

    fn parse(v: &Value) -> Result<Rendition> {
        let media = Media::parse(v)?;
        let bitrate = required_u64(v, "bitrate", "rendition: bitrate required")?;
        let produced_by_v = v
            .map_get("produced_by")
            .ok_or(Error::Kind("rendition: produced_by required"))?;
        let arr = produced_by_v.as_array().ok_or(Error::Kind(
            "rendition: produced_by must be [identity, key]",
        ))?;
        if arr.len() != 2 {
            return Err(Error::Kind(
                "rendition: produced_by must have exactly 2 elements",
            ));
        }
        let identity_bytes = arr[0]
            .as_bytes()
            .ok_or(Error::Kind("rendition: produced_by identity must be bytes"))?;
        let identity: [u8; 32] = identity_bytes
            .try_into()
            .map_err(|_| Error::Kind("rendition: produced_by identity must be 32 bytes"))?;
        let key = arr[1]
            .as_bytes()
            .ok_or(Error::Kind("rendition: produced_by key must be bytes"))?;
        let derivation_sig =
            required_bytes(v, "derivation_sig", "rendition: derivation_sig required")?;
        Ok(Rendition {
            media,
            bitrate,
            produced_by: (IdentityId(identity), key.to_vec()),
            derivation_sig,
        })
    }
}

/// A caption, lyric, or transcript track (DMTAP §24.4.2, superseding
/// spec 004 §2's subtitle-only text). Lyrics and transcripts are
/// structurally captions and get no object of their own: a lyric track
/// is simply a `Caption` whose `format` is `"lrc"` (or `"vtt"`/`"srt"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caption {
    /// Hash of the caption file.
    pub blob: BlobId,
    /// BCP 47 language tag.
    pub language: String,
    /// Format token — a **decode hint, not an enum**, exactly like
    /// `Media::codec` (DMTAP §24.4.2): `"vtt"`, `"srt"`, and `"lrc"`
    /// (the timed-lyric format) are the tokens in use at launch. A
    /// parser MUST NOT reject a `Caption` (or any other track) for
    /// carrying an unrecognized `format` token — an unrecognized value
    /// is accepted and simply skipped or handed to an external handler
    /// by the caller (VID-18), never treated as malformed.
    pub format: String,
    /// Wrapped content key, required (64 bytes) when the manifest is
    /// encrypted; see [`Media::wrapped_blob_key`].
    pub wrapped_blob_key: Option<Vec<u8>>,
}

impl Caption {
    fn to_value(&self) -> Value {
        let mut e = vec![
            (
                Value::Text("blob".into()),
                Value::Bytes(self.blob.as_bytes().to_vec()),
            ),
            (
                Value::Text("language".into()),
                Value::Text(self.language.clone()),
            ),
            (
                Value::Text("format".into()),
                Value::Text(self.format.clone()),
            ),
        ];
        if let Some(k) = &self.wrapped_blob_key {
            e.push((Value::Text("wrapped_key".into()), Value::Bytes(k.clone())));
        }
        Value::Map(e)
    }

    fn parse(v: &Value) -> Result<Caption> {
        let blob = required_blob_id(v, "blob", "caption: blob required (32 bytes)")?;
        let language = required_nonempty_text(v, "language", 64, "caption: language required")?;
        let format = required_nonempty_text(v, "format", 64, "caption: format required")?;
        let wrapped_blob_key = bytes_field(v, "wrapped_key", "caption: wrapped_key must be bytes")?;
        Ok(Caption {
            blob,
            language,
            format,
            wrapped_blob_key,
        })
    }
}

fn require_wrapped_64(key: &Option<Vec<u8>>, msg: &'static str) -> Result<()> {
    match key {
        Some(k) if k.len() == 64 => Ok(()),
        _ => Err(Error::Kind(msg)),
    }
}

fn parse_items<T>(
    body: &Value,
    key: &str,
    msg: &'static str,
    f: fn(&Value) -> Result<T>,
) -> Result<Vec<T>> {
    match array_field(body, key, msg)? {
        None => Ok(Vec::new()),
        Some(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(f(item)?);
            }
            Ok(out)
        }
    }
}

/// The canonical identity of a video (spec 003 §4.1, spec 004 §§1–4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Title, required, at most 512 bytes.
    pub title: String,
    /// Description, at most 16384 bytes.
    pub description: Option<String>,
    /// At most 32 tags, each at most 64 bytes.
    pub tags: Vec<String>,
    /// BCP 47 language tag.
    pub language: Option<String>,
    /// The source encoding.
    pub original: Media,
    /// Derived encodings.
    pub renditions: Vec<Rendition>,
    /// Caption tracks.
    pub captions: Vec<Caption>,
    /// Thumbnail image blob.
    pub thumbnail: Option<BlobId>,
    /// Encryption metadata; `None` = plaintext.
    pub encryption: Option<Encryption>,
    /// Creator-asserted license (spec 004 §4), 1-64 bytes.
    pub license: String,
    /// Sponsorship segments.
    pub sponsorship: Vec<Sponsor>,
    /// Payment pointers.
    pub payment: Vec<PaymentPointer>,
    /// Retrieval hints for all listed blobs.
    pub hints: Vec<Hint>,
    /// The channel this manifest joins, from `refs[0]` (spec 003 §4.1).
    pub channel: Option<RecordId>,
}

impl Manifest {
    /// Parse and validate a `manifest` record body (spec 003 §4.1, spec
    /// 004 §§1–4).
    ///
    /// Not checked here (needs a second record): that `channel`'s author
    /// equals this manifest's author (spec 003 §4.1); that each
    /// `Rendition::produced_by` is authorized (the manifest's own author,
    /// or an unrevoked/unexpired [`Delegate`] grant — spec 004 §3); the
    /// cryptographic validity of each `derivation_sig` (see
    /// [`verify_derivation`], which callers should run per rendition).
    pub fn parse(record: &Record) -> Result<Manifest> {
        if record.refs().len() > 1 {
            return Err(Error::Kind(
                "manifest: refs must be empty or one channel ref",
            ));
        }
        let channel = match record.refs().first() {
            None => None,
            Some(r) => Some(ref_record_id(
                r,
                "manifest: channel ref must be a record ref",
            )?),
        };
        let body = record.body();
        let title = required_nonempty_text(body, "title", 512, "manifest: title required")?;
        let description = text_field(body, "description", 16384, "manifest: description too long")?;
        let tags = text_array(body, "tags", 64, "manifest: tag too long (<=64 bytes)")?;
        if tags.len() > 32 {
            return Err(Error::Kind("manifest: at most 32 tags"));
        }
        let language = text_field(
            body,
            "language",
            usize::MAX,
            "manifest: language must be text",
        )?;
        let original_v = body
            .map_get("original")
            .ok_or(Error::Kind("manifest: original required"))?;
        let original = Media::parse(original_v)?;
        let renditions = parse_items(
            body,
            "renditions",
            "manifest: renditions must be an array",
            Rendition::parse,
        )?;
        let captions = parse_items(
            body,
            "captions",
            "manifest: captions must be an array",
            Caption::parse,
        )?;
        let thumbnail = blob_id_field(body, "thumbnail", "manifest: thumbnail must be 32 bytes")?;
        let encryption = match body.map_get("encryption") {
            None => None,
            Some(v) if is_null(v) => None,
            Some(v) => Some(Encryption::parse(v)?),
        };
        let license = required_text(body, "license", 4096, "manifest: license required")?;
        validate_license(&license)?;
        let sponsorship = parse_items(
            body,
            "sponsorship",
            "manifest: sponsorship must be an array",
            Sponsor::parse,
        )?;
        let payment = parse_items(
            body,
            "payment",
            "manifest: payment must be an array",
            PaymentPointer::parse,
        )?;
        let hints = parse_items(
            body,
            "hints",
            "manifest: hints must be an array",
            Hint::parse,
        )?;

        if encryption.is_some() {
            require_wrapped_64(
                &original.wrapped_blob_key,
                "manifest: encrypted original must carry a 64-byte wrapped_blob_key",
            )?;
            for r in &renditions {
                require_wrapped_64(
                    &r.media.wrapped_blob_key,
                    "manifest: encrypted rendition must carry a 64-byte wrapped_blob_key",
                )?;
            }
            for c in &captions {
                require_wrapped_64(
                    &c.wrapped_blob_key,
                    "manifest: encrypted caption must carry a 64-byte wrapped_blob_key",
                )?;
            }
        }

        Ok(Manifest {
            title,
            description,
            tags,
            language,
            original,
            renditions,
            captions,
            thumbnail,
            encryption,
            license,
            sponsorship,
            payment,
            hints,
            channel,
        })
    }

    /// Build the CBOR body for this manifest.
    pub fn to_body(&self) -> Value {
        let mut e = vec![
            (Value::Text("title".into()), Value::Text(self.title.clone())),
            (Value::Text("original".into()), self.original.to_value()),
            (
                Value::Text("license".into()),
                Value::Text(self.license.clone()),
            ),
        ];
        if let Some(d) = &self.description {
            e.push((Value::Text("description".into()), Value::Text(d.clone())));
        }
        if !self.tags.is_empty() {
            e.push((
                Value::Text("tags".into()),
                Value::Array(self.tags.iter().map(|t| Value::Text(t.clone())).collect()),
            ));
        }
        if let Some(l) = &self.language {
            e.push((Value::Text("language".into()), Value::Text(l.clone())));
        }
        if !self.renditions.is_empty() {
            e.push((
                Value::Text("renditions".into()),
                Value::Array(self.renditions.iter().map(Rendition::to_value).collect()),
            ));
        }
        if !self.captions.is_empty() {
            e.push((
                Value::Text("captions".into()),
                Value::Array(self.captions.iter().map(Caption::to_value).collect()),
            ));
        }
        if let Some(t) = &self.thumbnail {
            e.push((
                Value::Text("thumbnail".into()),
                Value::Bytes(t.as_bytes().to_vec()),
            ));
        }
        if let Some(enc) = &self.encryption {
            e.push((Value::Text("encryption".into()), enc.to_value()));
        }
        if !self.sponsorship.is_empty() {
            e.push((
                Value::Text("sponsorship".into()),
                Value::Array(self.sponsorship.iter().map(Sponsor::to_value).collect()),
            ));
        }
        if !self.payment.is_empty() {
            e.push((
                Value::Text("payment".into()),
                Value::Array(self.payment.iter().map(PaymentPointer::to_value).collect()),
            ));
        }
        if !self.hints.is_empty() {
            e.push((
                Value::Text("hints".into()),
                Value::Array(self.hints.iter().map(Hint::to_value).collect()),
            ));
        }
        Value::Map(e)
    }

    /// The refs this manifest should carry.
    pub fn refs(&self) -> Vec<Ref> {
        match self.channel {
            Some(id) => vec![Ref::record(id)],
            None => vec![],
        }
    }
}

/// Domain-separation prefix for derivation signatures (spec 004 §3.1).
pub const DERIVATION_SIG_PREFIX: &[u8] = b"evermesh:derivation:v1";

/// Build the derivation statement a rendition producer signs (DMTAP
/// §24.4.4, superseding spec 004 §3.1's video-only text):
/// `det_cbor([derived_from, rendition.blob, codec, width_or_null,
/// height_or_null, bitrate])`.
///
/// This is **always a six-element array**, in this order, whatever the
/// media kind. An absent dimension MUST be encoded as CBOR `null` (the
/// single byte `0xf6`) at its fixed position — never omitted, never a
/// shortened array, never a `0` sentinel (DMTAP §24.4.4, VID-17): `0` is
/// a valid `u32`, so a `0`-sentinel would make "no video track" and
/// "0 × 0 pixels" the same signed statement and let a signature replay
/// between them. Since `width`/`height` were previously REQUIRED, every
/// statement producible under the prior (video-only) text is
/// byte-identical under this one — only the previously unrepresentable
/// audio-only case gains an encoding (§24.17, C-03).
pub fn derivation_statement(
    original: &BlobId,
    rendition: &BlobId,
    codec: &str,
    width: Option<u64>,
    height: Option<u64>,
    bitrate: u64,
) -> Result<Vec<u8>> {
    fn dim(d: Option<u64>) -> Value {
        match d {
            Some(v) => Value::Uint(v),
            None => Value::Null,
        }
    }
    let value = Value::Array(vec![
        Value::Bytes(original.as_bytes().to_vec()),
        Value::Bytes(rendition.as_bytes().to_vec()),
        Value::Text(codec.to_string()),
        dim(width),
        dim(height),
        Value::Uint(bitrate),
    ]);
    crate::codec::encode_canonical(&value)
}

/// Verify a rendition's derivation signature against `original` (DMTAP
/// §24.4.4, superseding spec 004 §3.1): recompute the statement by
/// exactly the fixed-arity rule of [`derivation_statement`] (an absent
/// dimension reconstructs as CBOR `null` at its fixed position), hash
/// it, and check `derivation_sig` under `produced_by`'s key.
///
/// This function reconstructs **one** statement and verifies against
/// it, full stop — it MUST NOT and does not try a shortened array or a
/// `0`-sentinel fallback to make a signature verify (DMTAP §24.4.4): a
/// second acceptable encoding would re-introduce exactly the
/// signature-ambiguity the fixed-arity/`null` rule exists to close. A
/// `Rendition` whose `width`/`height` are not both-present-or-absent is
/// rejected earlier, by [`Media::parse`] (VID-16), before this function
/// ever runs.
///
/// This proves *who* asserts the derivation, not that `produced_by` was
/// actually authorized to produce it for this manifest (that requires
/// checking the manifest's author or an unrevoked/unexpired `delegate`
/// grant against a second record — see [`Manifest::parse`]'s docs) nor
/// transcoding fidelity (spec 004 §3: "a malicious transcoder can sign
/// an unfaithful rendition").
pub fn verify_derivation(rendition: &Rendition, original: &BlobId) -> Result<()> {
    let stmt = derivation_statement(
        original,
        &rendition.media.blob,
        &rendition.media.codec,
        rendition.media.width,
        rendition.media.height,
        rendition.bitrate,
    )?;
    let hash = blake3::hash(&stmt);
    let mut msg = Vec::with_capacity(DERIVATION_SIG_PREFIX.len() + 32);
    msg.extend_from_slice(DERIVATION_SIG_PREFIX);
    msg.extend_from_slice(hash.as_bytes());
    let key_bytes: [u8; 32] = rendition
        .produced_by
        .1
        .as_slice()
        .try_into()
        .map_err(|_| Error::Kind("rendition: produced_by key must be 32 bytes"))?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| Error::Kind("rendition: produced_by key is invalid"))?;
    let sig_bytes: [u8; 64] = rendition
        .derivation_sig
        .as_slice()
        .try_into()
        .map_err(|_| Error::Kind("rendition: derivation_sig must be 64 bytes"))?;
    let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    key.verify_strict(&msg, &sig)
        .map_err(|_| Error::Kind("rendition: derivation signature invalid"))
}

/// A capability grant or revocation (spec 003 §3.3). The one launch
/// capability, `rendition`, authorizes a `Manifest::renditions` entry's
/// `produced_by` (spec 004 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegate {
    /// Identity receiving the capability.
    pub grantee: IdentityId,
    /// Registered capability name.
    pub capability: String,
    /// Unix seconds after which the grant no longer applies; `None` =
    /// until revoked.
    pub expires_at: Option<i64>,
    /// `Some(grant id)` if this record revokes an earlier grant
    /// (`refs[0]`, `revoked = true` in the body); `None` for a grant.
    pub target: Option<RecordId>,
}

impl Delegate {
    /// Parse and validate a `delegate` record body (spec 003 §3.3).
    ///
    /// Not checked here (needs the grant record this revokes): that a
    /// revocation's author equals the grant's author, and that
    /// `grantee`/`capability` match — see [`check_delegate_revocation`].
    pub fn parse(record: &Record) -> Result<Delegate> {
        let body = record.body();
        let grantee =
            required_identity_id(body, "grantee", "delegate: grantee required (32 bytes)")?;
        let capability =
            required_nonempty_text(body, "capability", 256, "delegate: capability required")?;
        let expires_at = i64_field(body, "expires_at", "delegate: expires_at must be an int")?;
        let revoked = bool_or(body, "revoked", false, "delegate: revoked must be a bool")?;
        let target = match (revoked, record.refs().len()) {
            (true, 1) => Some(ref_record_id(
                &record.refs()[0],
                "delegate: revocation ref must be a record ref",
            )?),
            (false, 0) => None,
            _ => {
                return Err(Error::Kind(
                    "delegate: refs must be empty for a grant, or exactly one record ref \
                     for a revocation (with revoked = true)",
                ))
            }
        };
        Ok(Delegate {
            grantee,
            capability,
            expires_at,
            target,
        })
    }

    /// Build the CBOR body for this grant or revocation.
    pub fn to_body(&self) -> Value {
        let mut e = vec![
            (
                Value::Text("grantee".into()),
                Value::Bytes(self.grantee.as_bytes().to_vec()),
            ),
            (
                Value::Text("capability".into()),
                Value::Text(self.capability.clone()),
            ),
        ];
        if let Some(t) = self.expires_at {
            e.push((Value::Text("expires_at".into()), Value::from_i64(t)));
        }
        if self.target.is_some() {
            e.push((Value::Text("revoked".into()), Value::Bool(true)));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: empty for a grant, one record
    /// ref (the grant) for a revocation.
    pub fn refs(&self) -> Vec<Ref> {
        match self.target {
            Some(id) => vec![Ref::record(id)],
            None => vec![],
        }
    }
}

/// Checks the cross-record half of `delegate` revocation validity (spec
/// 003 §3.3): the revocation's author must equal the grant's author, and
/// `grantee`/`capability` must match. [`Delegate::parse`] cannot check
/// this alone — it only sees the revocation record, not the grant it
/// targets.
pub fn check_delegate_revocation(
    revocation_author: [u8; 32],
    revocation: &Delegate,
    grant_author: [u8; 32],
    grant: &Delegate,
) -> Result<()> {
    if revocation_author != grant_author {
        return Err(Error::Kind(
            "delegate: revocation author must equal grant author",
        ));
    }
    if revocation.grantee != grant.grantee || revocation.capability != grant.capability {
        return Err(Error::Kind(
            "delegate: revocation grantee/capability must match the grant",
        ));
    }
    Ok(())
}

/// Replaces an earlier record by the same author (spec 003 §4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supersede {
    /// The record being replaced.
    pub target: RecordId,
    /// Kind of the target record; MUST NOT be `rotation` (chains are
    /// never edited).
    pub target_kind: u64,
    /// Complete replacement body, valid per `target_kind`.
    pub replacement_body: Value,
}

impl Supersede {
    /// Parse and validate a `supersede` record body (spec 003 §4.2).
    ///
    /// Not checked here (needs the target record): that this record's
    /// author equals the target's author, and that `target_kind` equals
    /// the target's actual kind.
    pub fn parse(record: &Record) -> Result<Supersede> {
        refs_exact(record, 1, "supersede: refs must be exactly one target ref")?;
        let target = ref_record_id(&record.refs()[0], "supersede: ref must be a record ref")?;
        let body = record.body();
        let target_kind = required_u64(body, "target_kind", "supersede: target_kind required")?;
        if target_kind == KIND_ROTATION {
            return Err(Error::Kind("supersede/target-rotation"));
        }
        let replacement_body = body
            .map_get("body")
            .ok_or(Error::Kind("supersede: body required"))?
            .clone();
        if replacement_body.as_map().is_none() {
            return Err(Error::Kind("supersede: body must be a map"));
        }
        Ok(Supersede {
            target,
            target_kind,
            replacement_body,
        })
    }

    /// Build the CBOR body for this supersede.
    pub fn to_body(&self) -> Value {
        Value::Map(vec![
            (
                Value::Text("target_kind".into()),
                Value::Uint(self.target_kind),
            ),
            (Value::Text("body".into()), self.replacement_body.clone()),
        ])
    }

    /// The refs this record should carry: exactly one, the target.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.target)]
    }
}

/// Requests withdrawal of an earlier record by the same author (spec 003
/// §4.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retract {
    /// The record being retracted.
    pub target: RecordId,
    /// Optional reason.
    pub reason: Option<String>,
}

impl Retract {
    /// Parse and validate a `retract` record body (spec 003 §4.3).
    ///
    /// Not checked here (needs the target record's kind): that the
    /// target is not a `rotation` — see [`check_retract_target_kind`].
    /// Also not checked: that this record's author equals the target's
    /// author.
    pub fn parse(record: &Record) -> Result<Retract> {
        refs_exact(record, 1, "retract: refs must be exactly one target ref")?;
        let target = ref_record_id(&record.refs()[0], "retract: ref must be a record ref")?;
        let body = record.body();
        let reason = text_field(body, "reason", usize::MAX, "retract: reason must be text")?;
        Ok(Retract { target, reason })
    }

    /// Build the CBOR body for this retract.
    pub fn to_body(&self) -> Value {
        let mut e = Vec::new();
        if let Some(r) = &self.reason {
            e.push((Value::Text("reason".into()), Value::Text(r.clone())));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the target.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.target)]
    }
}

/// Checks the cross-record half of `retract` validity (spec 003 §4.3):
/// the target must not be a `rotation` (chains are never retracted).
/// [`Retract::parse`] cannot check this alone — the target's kind is not
/// part of the retract record's own body.
pub fn check_retract_target_kind(target_kind: u64) -> Result<()> {
    if target_kind == KIND_ROTATION {
        Err(Error::Kind("retract: target must not be a rotation record"))
    } else {
        Ok(())
    }
}

/// "This identity pins these blobs." (spec 003 §4.4)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mirror {
    /// One or more targets: each either a blob ref or a manifest record
    /// ref (pinning every blob it references).
    pub targets: Vec<Ref>,
    /// Where the author serves the pinned blobs.
    pub hints: Vec<Hint>,
}

impl Mirror {
    /// Parse and validate a `mirror` record body (spec 003 §4.4).
    pub fn parse(record: &Record) -> Result<Mirror> {
        refs_min(record, 1, "mirror: at least one ref required")?;
        let targets = record.refs().to_vec();
        let hints = parse_items(
            record.body(),
            "hints",
            "mirror: hints must be an array",
            Hint::parse,
        )?;
        Ok(Mirror { targets, hints })
    }

    /// Build the CBOR body for this mirror.
    pub fn to_body(&self) -> Value {
        let mut e = Vec::new();
        if !self.hints.is_empty() {
            e.push((
                Value::Text("hints".into()),
                Value::Array(self.hints.iter().map(Hint::to_value).collect()),
            ));
        }
        Value::Map(e)
    }

    /// The refs this record should carry.
    pub fn refs(&self) -> Vec<Ref> {
        self.targets.clone()
    }
}

/// Near-duplicate assertion: evidence, never kernel truth (spec 003
/// §4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Similarity {
    /// The candidate ref (`refs[0]`): either a record or blob ref.
    pub candidate: Ref,
    /// The canonical manifest record id (`refs[1]`).
    pub canonical: RecordId,
    /// Algorithm identifier, e.g. `phash-v2`.
    pub method: String,
    /// Similarity in basis points, 0-10000.
    pub score: u64,
}

impl Similarity {
    /// Parse and validate a `similarity` record body (spec 003 §4.5).
    pub fn parse(record: &Record) -> Result<Similarity> {
        refs_exact(record, 2, "similarity: refs must have exactly 2 elements")?;
        let candidate = record.refs()[0];
        let canonical = ref_record_id(
            &record.refs()[1],
            "similarity: refs[1] must be a record ref (canonical manifest)",
        )?;
        let body = record.body();
        let method =
            required_nonempty_text(body, "method", usize::MAX, "similarity: method required")?;
        let score = required_u64(body, "score", "similarity: score required")?;
        if score > 10_000 {
            return Err(Error::Kind("similarity/score-overflow"));
        }
        Ok(Similarity {
            candidate,
            canonical,
            method,
            score,
        })
    }

    /// Build the CBOR body for this similarity assertion.
    pub fn to_body(&self) -> Value {
        Value::Map(vec![
            (
                Value::Text("method".into()),
                Value::Text(self.method.clone()),
            ),
            (Value::Text("score".into()), Value::Uint(self.score)),
        ])
    }

    /// The refs this record should carry: `[candidate, canonical]`.
    pub fn refs(&self) -> Vec<Ref> {
        vec![self.candidate, Ref::record(self.canonical)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;
    use crate::record::RecordBuilder;

    fn kp(seed: u8) -> Keypair {
        Keypair::from_secret_bytes(&[seed; 32])
    }

    fn author() -> IdentityId {
        IdentityId([7; 32])
    }

    fn sign(kind: u64, refs: Vec<Ref>, body: Value) -> Record {
        RecordBuilder::new(kind)
            .created_at(1000)
            .refs(refs)
            .body(body)
            .sign_as(&kp(1), author())
            .unwrap()
    }

    fn small_media(blob_byte: u8) -> Media {
        Media {
            blob: BlobId([blob_byte; 32]),
            size: 1000,
            chunk_root: None,
            codec: "av01.0.08M.08".into(),
            duration_ms: 60_000,
            width: Some(1920),
            height: Some(1080),
            wrapped_blob_key: None,
        }
    }

    /// An audio-only encoding (DMTAP §24.4.2): no video track, so
    /// `width`/`height` are both absent.
    fn small_media_audio(blob_byte: u8) -> Media {
        Media {
            blob: BlobId([blob_byte; 32]),
            size: 1000,
            chunk_root: None,
            codec: "opus".into(),
            duration_ms: 180_000,
            width: None,
            height: None,
            wrapped_blob_key: None,
        }
    }

    fn sample_manifest() -> Manifest {
        Manifest {
            title: "A title".into(),
            description: Some("desc".into()),
            tags: vec!["a".into(), "b".into()],
            language: Some("en".into()),
            original: small_media(1),
            renditions: vec![],
            captions: vec![],
            thumbnail: Some(BlobId([9; 32])),
            encryption: None,
            license: "CC-BY-4.0".into(),
            sponsorship: vec![Sponsor {
                start_ms: 0,
                end_ms: 100,
                label: "sponsor".into(),
            }],
            payment: vec![PaymentPointer(1, "asha@ln.example.net".into())],
            hints: vec![Hint(1, "https://cdn.example.net/".into())],
            channel: None,
        }
    }

    #[test]
    fn manifest_round_trip_plaintext() {
        let m = sample_manifest();
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        let back = Manifest::parse(&record).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn manifest_round_trip_with_channel_and_rendition() {
        let mut m = sample_manifest();
        m.channel = Some(RecordId([3; 32]));
        let rendition = Rendition {
            media: small_media(2),
            bitrate: 2_800_000,
            produced_by: (author(), kp(1).public_key_bytes().to_vec()),
            derivation_sig: vec![0xaa; 64],
        };
        m.renditions.push(rendition);
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        let back = Manifest::parse(&record).unwrap();
        assert_eq!(back, m);
    }

    /// DMTAP §24.4.2: an audio-only work's `original` omits both
    /// `width` and `height`; the wire encoding must not write `0` (or
    /// any placeholder) at either key, and round-tripping must recover
    /// exactly `None`/`None`.
    #[test]
    fn audio_only_media_round_trips_with_dimensions_absent_not_zero() {
        let mut m = sample_manifest();
        m.original = small_media_audio(1);
        let body = m.to_body();

        // The wire map must not carry "width"/"height" keys at all —
        // not even zero-valued ones.
        let original_map = body
            .map_get("original")
            .unwrap()
            .as_map()
            .expect("original must encode as a map");
        assert!(
            original_map
                .iter()
                .all(|(k, _)| k.as_text() != Some("width") && k.as_text() != Some("height")),
            "audio-only Media must omit width/height entirely, never write 0"
        );

        let record = sign(super::super::KIND_MANIFEST, m.refs(), body);
        let back = Manifest::parse(&record).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.original.width, None);
        assert_eq!(back.original.height, None);
    }

    /// DMTAP §24.4.2, VID-16: exactly one of `width`/`height` present is
    /// malformed and MUST be rejected — before any signature
    /// verification (this is a plain parse-time structural check).
    #[test]
    fn media_rejects_exactly_one_of_width_height() {
        let base = small_media(1).to_value();
        let mut only_width = base.as_map().unwrap().to_vec();
        only_width.retain(|(k, _)| k.as_text() != Some("height"));
        let v = Value::Map(only_width);
        assert!(matches!(Media::parse(&v), Err(Error::Kind(_))));

        let base = small_media(1).to_value();
        let mut only_height = base.as_map().unwrap().to_vec();
        only_height.retain(|(k, _)| k.as_text() != Some("width"));
        let v = Value::Map(only_height);
        assert!(matches!(Media::parse(&v), Err(Error::Kind(_))));
    }

    #[test]
    fn manifest_rejects_missing_title() {
        let body = Value::Map(vec![
            (Value::Text("original".into()), small_media(1).to_value()),
            (
                Value::Text("license".into()),
                Value::Text("CC-BY-4.0".into()),
            ),
        ]);
        let record = sign(super::super::KIND_MANIFEST, vec![], body);
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn manifest_rejects_more_than_one_ref() {
        let m = sample_manifest();
        let record = sign(
            super::super::KIND_MANIFEST,
            vec![
                Ref::record(RecordId([1; 32])),
                Ref::record(RecordId([2; 32])),
            ],
            m.to_body(),
        );
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn manifest_rejects_missing_chunk_root_over_1mib() {
        let mut m = sample_manifest();
        m.original.size = CHUNK_SIZE_BYTES + 1;
        m.original.chunk_root = None;
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn manifest_accepts_chunk_root_over_1mib() {
        let mut m = sample_manifest();
        m.original.size = CHUNK_SIZE_BYTES + 1;
        m.original.chunk_root = Some([5; 32]);
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(Manifest::parse(&record).is_ok());
    }

    #[test]
    fn manifest_requires_wrapped_key_when_encrypted() {
        let mut m = sample_manifest();
        m.encryption = Some(Encryption {
            scheme: 1,
            key_hint: None,
        });
        // original.wrapped_blob_key left None: must fail.
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));

        m.original.wrapped_blob_key = Some(vec![0u8; 64]);
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(Manifest::parse(&record).is_ok());
    }

    #[test]
    fn manifest_rejects_wrong_length_wrapped_key() {
        let mut m = sample_manifest();
        m.encryption = Some(Encryption {
            scheme: 1,
            key_hint: None,
        });
        m.original.wrapped_blob_key = Some(vec![0u8; 10]);
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn manifest_rejects_empty_license() {
        let mut m = sample_manifest();
        m.license = String::new();
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn manifest_rejects_oversized_license() {
        let mut m = sample_manifest();
        m.license = "x".repeat(65);
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn manifest_rejects_too_many_tags() {
        let mut m = sample_manifest();
        m.tags = (0..33).map(|i| i.to_string()).collect();
        let record = sign(super::super::KIND_MANIFEST, m.refs(), m.to_body());
        assert!(matches!(Manifest::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn derivation_statement_verifies_with_matching_signature() {
        let producer = kp(2);
        let original = BlobId([1; 32]);
        let media = Media {
            blob: BlobId([2; 32]),
            size: 500,
            chunk_root: None,
            codec: "avc1.640028".into(),
            duration_ms: 60_000,
            width: Some(1280),
            height: Some(720),
            wrapped_blob_key: None,
        };
        let bitrate = 2_800_000;
        let stmt = derivation_statement(
            &original,
            &media.blob,
            &media.codec,
            media.width,
            media.height,
            bitrate,
        )
        .unwrap();
        let hash = blake3::hash(&stmt);
        let mut msg = Vec::new();
        msg.extend_from_slice(DERIVATION_SIG_PREFIX);
        msg.extend_from_slice(hash.as_bytes());
        let sig = producer.sign(&msg);
        let rendition = Rendition {
            media,
            bitrate,
            produced_by: (author(), producer.public_key_bytes().to_vec()),
            derivation_sig: sig.to_vec(),
        };
        assert!(verify_derivation(&rendition, &original).is_ok());
    }

    #[test]
    fn derivation_statement_rejects_tampered_bitrate() {
        let producer = kp(2);
        let original = BlobId([1; 32]);
        let media = Media {
            blob: BlobId([2; 32]),
            size: 500,
            chunk_root: None,
            codec: "avc1.640028".into(),
            duration_ms: 60_000,
            width: Some(1280),
            height: Some(720),
            wrapped_blob_key: None,
        };
        let stmt = derivation_statement(
            &original,
            &media.blob,
            &media.codec,
            media.width,
            media.height,
            1000,
        )
        .unwrap();
        let hash = blake3::hash(&stmt);
        let mut msg = Vec::new();
        msg.extend_from_slice(DERIVATION_SIG_PREFIX);
        msg.extend_from_slice(hash.as_bytes());
        let sig = producer.sign(&msg);
        // Replay the signature onto a different bitrate claim.
        let rendition = Rendition {
            media,
            bitrate: 2_800_000,
            produced_by: (author(), producer.public_key_bytes().to_vec()),
            derivation_sig: sig.to_vec(),
        };
        assert!(verify_derivation(&rendition, &original).is_err());
    }

    /// Byte-identity regression (DMTAP §24.4.4's closing paragraph):
    /// `width`/`height` were previously REQUIRED, so every statement
    /// producible under the old (video-only) text must be byte-identical
    /// under the new optional-dimensions text. This locks in the exact
    /// bytes captured from the pre-change implementation for a
    /// 1280x720 video rendition — a change to these bytes for a
    /// present-dimensions rendition would be a signature-breaking
    /// regression, never an acceptable side effect of the audio work.
    #[test]
    fn derivation_statement_video_bytes_are_byte_identical_to_pre_optional_dimensions() {
        let original = BlobId([1; 32]);
        let rendition = BlobId([2; 32]);
        let stmt = derivation_statement(
            &original,
            &rendition,
            "avc1.640028",
            Some(1280),
            Some(720),
            2_800_000,
        )
        .unwrap();
        let expected_hex = "8658200101010101010101010101010101010101010101010101010101010101010101582002020202020202020202020202020202020202020202020202020202020202026b617663312e3634303032381905001902d01a002ab980";
        let expected: Vec<u8> = (0..expected_hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&expected_hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(
            stmt, expected,
            "video derivation statement bytes must not change under the optional-dimensions text"
        );
    }

    /// DMTAP §24.4.4: an audio-only rendition's derivation statement is
    /// still a six-element array, with CBOR `null` (`0xf6`) at the fixed
    /// `width_or_null`/`height_or_null` positions (array indices 3 and 4,
    /// i.e. the 4th and 5th elements) — never omitted, never `0`.
    #[test]
    fn audio_only_derivation_statement_encodes_null_at_fixed_width_height_positions() {
        let original = BlobId([3; 32]);
        let rendition = BlobId([4; 32]);
        let stmt =
            derivation_statement(&original, &rendition, "opus", None, None, 128_000).unwrap();

        // Independently decode the statement and check the array shape
        // and the two null positions directly, rather than trusting
        // derivation_statement's own internals.
        let decoded = crate::codec::decode_canonical(&stmt).unwrap();
        let arr = decoded.as_array().expect("statement must be an array");
        assert_eq!(arr.len(), 6, "statement must always be a six-element array");
        assert_eq!(arr[3], Value::Null, "width_or_null must be CBOR null");
        assert_eq!(arr[4], Value::Null, "height_or_null must be CBOR null");

        // And the raw bytes must contain 0xf6 (not 0x00, not an absent
        // element) at both positions — verified byte-exactly via a
        // from-scratch expected encoding.
        let expected = crate::codec::encode_canonical(&Value::Array(vec![
            Value::Bytes(original.as_bytes().to_vec()),
            Value::Bytes(rendition.as_bytes().to_vec()),
            Value::Text("opus".into()),
            Value::Null,
            Value::Null,
            Value::Uint(128_000),
        ]))
        .unwrap();
        assert_eq!(stmt, expected);

        let producer = kp(5);
        let media = Media {
            blob: rendition,
            size: 900_000,
            chunk_root: None,
            codec: "opus".into(),
            duration_ms: 180_000,
            width: None,
            height: None,
            wrapped_blob_key: None,
        };
        let hash = blake3::hash(&stmt);
        let mut msg = Vec::new();
        msg.extend_from_slice(DERIVATION_SIG_PREFIX);
        msg.extend_from_slice(hash.as_bytes());
        let sig = producer.sign(&msg);
        let audio_rendition = Rendition {
            media,
            bitrate: 128_000,
            produced_by: (author(), producer.public_key_bytes().to_vec()),
            derivation_sig: sig.to_vec(),
        };
        assert!(verify_derivation(&audio_rendition, &original).is_ok());
    }

    /// DMTAP §24.4.4, VID-17: a verifier MUST NOT accept a shortened
    /// (5-element, dimensions omitted) statement or a `0`-substituted
    /// statement in place of the fixed six-element/`null` encoding —
    /// `verify_derivation` always reconstructs the one canonical
    /// statement and must reject a signature made over either
    /// alternative.
    #[test]
    fn verify_derivation_rejects_shortened_or_zero_substituted_statement() {
        let producer = kp(6);
        let original = BlobId([7; 32]);
        let media = Media {
            blob: BlobId([8; 32]),
            size: 900_000,
            chunk_root: None,
            codec: "opus".into(),
            duration_ms: 180_000,
            width: None,
            height: None,
            wrapped_blob_key: None,
        };
        let bitrate = 128_000u64;

        let sign_over = |value: &Value| -> Vec<u8> {
            let stmt = crate::codec::encode_canonical(value).unwrap();
            let hash = blake3::hash(&stmt);
            let mut msg = Vec::new();
            msg.extend_from_slice(DERIVATION_SIG_PREFIX);
            msg.extend_from_slice(hash.as_bytes());
            producer.sign(&msg).to_vec()
        };

        // Alternative 1: a shortened, 5-element array (dimensions
        // dropped rather than encoded as null).
        let shortened = Value::Array(vec![
            Value::Bytes(original.as_bytes().to_vec()),
            Value::Bytes(media.blob.as_bytes().to_vec()),
            Value::Text(media.codec.clone()),
            Value::Uint(bitrate),
        ]);
        let sig_shortened = sign_over(&shortened);
        let rendition_shortened = Rendition {
            media: media.clone(),
            bitrate,
            produced_by: (author(), producer.public_key_bytes().to_vec()),
            derivation_sig: sig_shortened,
        };
        assert!(verify_derivation(&rendition_shortened, &original).is_err());

        // Alternative 2: a six-element array using a `0` sentinel
        // instead of `null` for the absent dimensions.
        let zero_sub = Value::Array(vec![
            Value::Bytes(original.as_bytes().to_vec()),
            Value::Bytes(media.blob.as_bytes().to_vec()),
            Value::Text(media.codec.clone()),
            Value::Uint(0),
            Value::Uint(0),
            Value::Uint(bitrate),
        ]);
        let sig_zero = sign_over(&zero_sub);
        let rendition_zero = Rendition {
            media,
            bitrate,
            produced_by: (author(), producer.public_key_bytes().to_vec()),
            derivation_sig: sig_zero,
        };
        assert!(verify_derivation(&rendition_zero, &original).is_err());
    }

    #[test]
    fn supersede_round_trip() {
        let s = Supersede {
            target: RecordId([4; 32]),
            target_kind: super::super::KIND_COMMENT,
            replacement_body: Value::Map(vec![(
                Value::Text("text".into()),
                Value::Text("corrected".into()),
            )]),
        };
        let record = sign(super::super::KIND_SUPERSEDE, s.refs(), s.to_body());
        let back = Supersede::parse(&record).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn supersede_rejects_wrong_refs_count() {
        let body = Value::Map(vec![
            (Value::Text("target_kind".into()), Value::Uint(32)),
            (Value::Text("body".into()), Value::Map(vec![])),
        ]);
        let record = sign(super::super::KIND_SUPERSEDE, vec![], body);
        assert!(matches!(Supersede::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn supersede_rejects_target_rotation() {
        let body = Value::Map(vec![
            (
                Value::Text("target_kind".into()),
                Value::Uint(KIND_ROTATION),
            ),
            (Value::Text("body".into()), Value::Map(vec![])),
        ]);
        let record = sign(
            super::super::KIND_SUPERSEDE,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert_eq!(
            Supersede::parse(&record),
            Err(Error::Kind("supersede/target-rotation"))
        );
    }

    #[test]
    fn retract_round_trip_and_target_kind_check() {
        let r = Retract {
            target: RecordId([2; 32]),
            reason: Some("posted in error".into()),
        };
        let record = sign(super::super::KIND_RETRACT, r.refs(), r.to_body());
        let back = Retract::parse(&record).unwrap();
        assert_eq!(back, r);
        assert!(check_retract_target_kind(super::super::KIND_COMMENT).is_ok());
        assert!(check_retract_target_kind(KIND_ROTATION).is_err());
    }

    #[test]
    fn retract_rejects_wrong_refs_count() {
        let record = sign(super::super::KIND_RETRACT, vec![], Value::Map(vec![]));
        assert!(matches!(Retract::parse(&record), Err(Error::Kind(_))));
    }

    /// DMTAP §24.4.2: `"lrc"` (the timed-lyric format) is a valid
    /// `Caption.format` token, alongside `"vtt"`/`"srt"`.
    #[test]
    fn caption_accepts_lrc_format() {
        let c = Caption {
            blob: BlobId([1; 32]),
            language: "en".into(),
            format: "lrc".into(),
            wrapped_blob_key: None,
        };
        let back = Caption::parse(&c.to_value()).unwrap();
        assert_eq!(back, c);
    }

    /// DMTAP §24.4.2, VID-18: `format` is a decode hint, not an enum —
    /// an unrecognized token MUST NOT cause rejection of the caption (or
    /// any other track); a client skips it or hands it to an external
    /// handler. The kernel has no format allow-list, so an unknown token
    /// parses successfully rather than being treated as malformed.
    #[test]
    fn caption_accepts_and_does_not_reject_unknown_format_token() {
        let c = Caption {
            blob: BlobId([2; 32]),
            language: "en".into(),
            format: "a-brand-new-future-format".into(),
            wrapped_blob_key: None,
        };
        let back = Caption::parse(&c.to_value()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn mirror_round_trip() {
        let m = Mirror {
            targets: vec![Ref::record(RecordId([1; 32])), Ref::blob(BlobId([2; 32]))],
            hints: vec![Hint(3, "https://node7.example.org/blob".into())],
        };
        let record = sign(super::super::KIND_MIRROR, m.refs(), m.to_body());
        let back = Mirror::parse(&record).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn mirror_rejects_empty_refs() {
        let record = sign(super::super::KIND_MIRROR, vec![], Value::Map(vec![]));
        assert!(matches!(Mirror::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn similarity_round_trip() {
        let s = Similarity {
            candidate: Ref::blob(BlobId([1; 32])),
            canonical: RecordId([2; 32]),
            method: "phash-v2".into(),
            score: 9700,
        };
        let record = sign(super::super::KIND_SIMILARITY, s.refs(), s.to_body());
        let back = Similarity::parse(&record).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn similarity_rejects_score_overflow() {
        let body = Value::Map(vec![
            (Value::Text("method".into()), Value::Text("phash-v2".into())),
            (Value::Text("score".into()), Value::Uint(10_001)),
        ]);
        let record = sign(
            super::super::KIND_SIMILARITY,
            vec![Ref::blob(BlobId([1; 32])), Ref::record(RecordId([2; 32]))],
            body,
        );
        assert_eq!(
            Similarity::parse(&record),
            Err(Error::Kind("similarity/score-overflow"))
        );
    }

    #[test]
    fn similarity_rejects_wrong_refs_count() {
        let body = Value::Map(vec![
            (Value::Text("method".into()), Value::Text("phash-v2".into())),
            (Value::Text("score".into()), Value::Uint(1)),
        ]);
        let record = sign(
            super::super::KIND_SIMILARITY,
            vec![Ref::blob(BlobId([1; 32]))],
            body,
        );
        assert!(matches!(Similarity::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn similarity_rejects_non_record_second_ref() {
        let body = Value::Map(vec![
            (Value::Text("method".into()), Value::Text("phash-v2".into())),
            (Value::Text("score".into()), Value::Uint(1)),
        ]);
        let record = sign(
            super::super::KIND_SIMILARITY,
            vec![Ref::record(RecordId([1; 32])), Ref::blob(BlobId([2; 32]))],
            body,
        );
        assert!(matches!(Similarity::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn delegate_grant_round_trip() {
        let d = Delegate {
            grantee: IdentityId([9; 32]),
            capability: "rendition".into(),
            expires_at: Some(1_795_000_000),
            target: None,
        };
        let record = sign(super::super::KIND_DELEGATE, d.refs(), d.to_body());
        let back = Delegate::parse(&record).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn delegate_revocation_round_trip_and_cross_check() {
        let grant = Delegate {
            grantee: IdentityId([9; 32]),
            capability: "rendition".into(),
            expires_at: None,
            target: None,
        };
        let grant_record = sign(super::super::KIND_DELEGATE, grant.refs(), grant.to_body());
        let revocation = Delegate {
            grantee: grant.grantee,
            capability: grant.capability.clone(),
            expires_at: None,
            target: Some(grant_record.id()),
        };
        let rev_record = sign(
            super::super::KIND_DELEGATE,
            revocation.refs(),
            revocation.to_body(),
        );
        let back = Delegate::parse(&rev_record).unwrap();
        assert_eq!(back, revocation);

        assert!(check_delegate_revocation(
            rev_record.author_identity_id(),
            &back,
            grant_record.author_identity_id(),
            &grant,
        )
        .is_ok());

        let mismatched_grant = Delegate {
            grantee: IdentityId([1; 32]),
            ..grant
        };
        assert!(check_delegate_revocation(
            rev_record.author_identity_id(),
            &back,
            grant_record.author_identity_id(),
            &mismatched_grant,
        )
        .is_err());
    }

    #[test]
    fn delegate_rejects_revoked_without_ref() {
        let body = Value::Map(vec![
            (Value::Text("grantee".into()), Value::Bytes(vec![9; 32])),
            (
                Value::Text("capability".into()),
                Value::Text("rendition".into()),
            ),
            (Value::Text("revoked".into()), Value::Bool(true)),
        ]);
        let record = sign(super::super::KIND_DELEGATE, vec![], body);
        assert!(matches!(Delegate::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn delegate_rejects_ref_without_revoked_flag() {
        let body = Value::Map(vec![
            (Value::Text("grantee".into()), Value::Bytes(vec![9; 32])),
            (
                Value::Text("capability".into()),
                Value::Text("rendition".into()),
            ),
        ]);
        let record = sign(
            super::super::KIND_DELEGATE,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Delegate::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn ed25519_dalek_signature_import_smoke_test() {
        // Exercises the ed25519_dalek::Signature import path used by
        // verify_derivation, independent of the derivation statement
        // logic above.
        let sig = ed25519_dalek::Signature::from_bytes(&[0u8; 64]);
        assert_eq!(sig.to_bytes().len(), 64);
    }
}
