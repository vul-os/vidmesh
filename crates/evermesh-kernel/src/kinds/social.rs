//! Social kinds (spec 003 §5): `comment`, `reaction`, `follow`,
//! `playlist`, `channel`. `profile` (spec 003 §3.2) is grouped here too:
//! it is the identity-group kind closest in shape to `channel` (a
//! display card with a name/title, bio/description, and avatar), and
//! spec 003 does not assign it a dedicated file.

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::{BlobId, IdentityId, RecordId};
use crate::record::{Record, Ref};

use super::content::PaymentPointer;
use super::{
    blob_id_array, blob_id_field, ref_record_id, refs_empty, refs_exact, required_array,
    required_nonempty_text, required_text, text_array, text_field,
};

/// A profile card (spec 003 §3.2): the identity's display name, bio, and
/// discovery metadata. Latest-wins per spec 002 §6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Display name, required, at most 256 bytes.
    pub name: String,
    /// Bio.
    pub about: Option<String>,
    /// Avatar image blob.
    pub avatar: Option<BlobId>,
    /// Payment pointers (spec 010 §1).
    pub payment: Vec<PaymentPointer>,
    /// Relay URLs the identity publishes to.
    pub relays: Vec<String>,
    /// Endpoints seeding the identity's blobs.
    pub seeds: Vec<String>,
    /// Encryption public key `[alg, key]` (spec 008 §4).
    pub enc_key: Option<(u64, Vec<u8>)>,
}

impl Profile {
    /// Parse and validate a `profile` record body (spec 003 §3.2): no
    /// validation beyond schema; refs MUST be empty.
    pub fn parse(record: &Record) -> Result<Profile> {
        refs_empty(record, "profile: refs must be empty")?;
        let body = record.body();
        let name = required_nonempty_text(body, "name", 256, "profile: name required")?;
        let about = text_field(body, "about", usize::MAX, "profile: about must be text")?;
        let avatar = blob_id_field(body, "avatar", "profile: avatar must be 32 bytes")?;
        let payment = match body.map_get("payment") {
            None => Vec::new(),
            Some(v) => {
                let arr = v
                    .as_array()
                    .ok_or(Error::Kind("profile: payment must be an array"))?;
                let mut out = Vec::with_capacity(arr.len());
                for item in arr {
                    out.push(PaymentPointer::parse(item)?);
                }
                out
            }
        };
        let relays = text_array(
            body,
            "relays",
            usize::MAX,
            "profile: relay url must be text",
        )?;
        let seeds = text_array(body, "seeds", usize::MAX, "profile: seed url must be text")?;
        let enc_key = match body.map_get("enc_key") {
            None => None,
            Some(v) => {
                let arr = v
                    .as_array()
                    .ok_or(Error::Kind("profile: enc_key must be [alg, key]"))?;
                if arr.len() != 2 {
                    return Err(Error::Kind("profile: enc_key must have exactly 2 elements"));
                }
                let alg = arr[0]
                    .as_u64()
                    .ok_or(Error::Kind("profile: enc_key alg must be a uint"))?;
                let key = arr[1]
                    .as_bytes()
                    .ok_or(Error::Kind("profile: enc_key key must be bytes"))?
                    .to_vec();
                Some((alg, key))
            }
        };
        Ok(Profile {
            name,
            about,
            avatar,
            payment,
            relays,
            seeds,
            enc_key,
        })
    }

    /// Build the CBOR body for this profile.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(Value::Text("name".into()), Value::Text(self.name.clone()))];
        if let Some(a) = &self.about {
            e.push((Value::Text("about".into()), Value::Text(a.clone())));
        }
        if let Some(avatar) = &self.avatar {
            e.push((
                Value::Text("avatar".into()),
                Value::Bytes(avatar.as_bytes().to_vec()),
            ));
        }
        if !self.payment.is_empty() {
            e.push((
                Value::Text("payment".into()),
                Value::Array(self.payment.iter().map(PaymentPointer::to_value).collect()),
            ));
        }
        if !self.relays.is_empty() {
            e.push((
                Value::Text("relays".into()),
                Value::Array(self.relays.iter().map(|r| Value::Text(r.clone())).collect()),
            ));
        }
        if !self.seeds.is_empty() {
            e.push((
                Value::Text("seeds".into()),
                Value::Array(self.seeds.iter().map(|s| Value::Text(s.clone())).collect()),
            ));
        }
        if let Some((alg, key)) = &self.enc_key {
            e.push((
                Value::Text("enc_key".into()),
                Value::Array(vec![Value::Uint(*alg), Value::Bytes(key.clone())]),
            ));
        }
        Value::Map(e)
    }
}

/// A comment on a manifest or live stream, optionally threaded (spec 003
/// §5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The commented-on record (`refs[0]`): a `manifest` or
    /// `live.manifest` record id.
    pub subject: RecordId,
    /// The parent comment (`refs[1]`), for threading.
    pub parent: Option<RecordId>,
    /// Comment text, non-empty, at most 8192 bytes.
    pub text: String,
    /// Attached media.
    pub media: Vec<BlobId>,
}

impl Comment {
    /// Parse and validate a `comment` record body (spec 003 §5.1).
    ///
    /// Not checked here (needs the parent record): that a present parent
    /// itself references the same subject — see
    /// [`check_comment_thread`].
    pub fn parse(record: &Record) -> Result<Comment> {
        let refs = record.refs();
        if refs.is_empty() || refs.len() > 2 {
            return Err(Error::Kind(
                "comment: refs must have 1 (subject) or 2 (subject, parent) elements",
            ));
        }
        let subject = ref_record_id(&refs[0], "comment: subject ref must be a record ref")?;
        let parent = if refs.len() == 2 {
            Some(ref_record_id(
                &refs[1],
                "comment: parent ref must be a record ref",
            )?)
        } else {
            None
        };
        let body = record.body();
        let text = required_nonempty_text(body, "text", 8192, "comment: text required")?;
        let media = blob_id_array(body, "media", "comment: media must be an array of blob ids")?;
        Ok(Comment {
            subject,
            parent,
            text,
            media,
        })
    }

    /// Build the CBOR body for this comment.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(Value::Text("text".into()), Value::Text(self.text.clone()))];
        if !self.media.is_empty() {
            e.push((
                Value::Text("media".into()),
                Value::Array(
                    self.media
                        .iter()
                        .map(|b| Value::Bytes(b.as_bytes().to_vec()))
                        .collect(),
                ),
            ));
        }
        Value::Map(e)
    }

    /// The refs this comment should carry: `[subject]` or `[subject,
    /// parent]`.
    pub fn refs(&self) -> Vec<Ref> {
        let mut r = vec![Ref::record(self.subject)];
        if let Some(p) = self.parent {
            r.push(Ref::record(p));
        }
        r
    }
}

/// Checks the cross-record half of `comment` thread validity (spec 003
/// §5.1, test vector `comment/parent-subject-mismatch`): when `comment`
/// has a parent, the parent MUST itself reference the same subject.
/// [`Comment::parse`] cannot check this alone — it does not have the
/// parent record's own subject.
pub fn check_comment_thread(comment: &Comment, parent: &Comment) -> Result<()> {
    match comment.parent {
        None => Ok(()),
        Some(_) => {
            if comment.subject == parent.subject {
                Ok(())
            } else {
                Err(Error::Kind("comment/parent-subject-mismatch"))
            }
        }
    }
}

/// A reaction to a target record (spec 003 §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reaction {
    /// The reacted-to record (`refs[0]`).
    pub target: RecordId,
    /// Single emoji grapheme cluster or registered token, at most 32
    /// bytes. Grapheme-cluster validity is not enforced beyond the byte
    /// length (spec 003 §5.2 states only the byte ceiling).
    pub reaction: String,
}

impl Reaction {
    /// Parse and validate a `reaction` record body (spec 003 §5.2).
    pub fn parse(record: &Record) -> Result<Reaction> {
        refs_exact(record, 1, "reaction: refs must be exactly one target ref")?;
        let target = ref_record_id(&record.refs()[0], "reaction: ref must be a record ref")?;
        let body = record.body();
        let reaction = required_text(
            body,
            "reaction",
            32,
            "reaction: reaction required (<=32 bytes)",
        )?;
        Ok(Reaction { target, reaction })
    }

    /// Build the CBOR body for this reaction.
    pub fn to_body(&self) -> Value {
        Value::Map(vec![(
            Value::Text("reaction".into()),
            Value::Text(self.reaction.clone()),
        )])
    }

    /// The refs this record should carry: exactly one, the target.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(self.target)]
    }
}

/// "This identity follows that identity." (spec 003 §5.3)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Follow {
    /// The followed identity, referenced by its genesis rotation record
    /// id (`refs[0]`).
    pub followed: IdentityId,
    /// Optional note.
    pub note: Option<String>,
}

impl Follow {
    /// Parse and validate a `follow` record body (spec 003 §5.3): no
    /// validation beyond schema.
    pub fn parse(record: &Record) -> Result<Follow> {
        refs_exact(
            record,
            1,
            "follow: refs must be exactly one followed-identity ref",
        )?;
        let r = &record.refs()[0];
        if !r.is_record() {
            return Err(Error::Kind("follow: ref must be a record ref"));
        }
        let followed = IdentityId(r.hash);
        let body = record.body();
        let note = text_field(body, "note", usize::MAX, "follow: note must be text")?;
        Ok(Follow { followed, note })
    }

    /// Build the CBOR body for this follow.
    pub fn to_body(&self) -> Value {
        let mut e = Vec::new();
        if let Some(n) = &self.note {
            e.push((Value::Text("note".into()), Value::Text(n.clone())));
        }
        Value::Map(e)
    }

    /// The refs this record should carry: exactly one, the followed
    /// identity's genesis record id.
    pub fn refs(&self) -> Vec<Ref> {
        vec![Ref::record(RecordId(self.followed.0))]
    }
}

/// An ordered list of manifests (spec 003 §5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playlist {
    /// Title, required, at most 512 bytes.
    pub title: String,
    /// Description, at most 16384 bytes.
    pub description: Option<String>,
    /// Manifest record ids, in order. Non-empty.
    pub entries: Vec<RecordId>,
}

impl Playlist {
    /// Parse and validate a `playlist` record body (spec 003 §5.4).
    pub fn parse(record: &Record) -> Result<Playlist> {
        refs_empty(record, "playlist: refs must be empty")?;
        let body = record.body();
        let title = required_nonempty_text(body, "title", 512, "playlist: title required")?;
        let description = text_field(body, "description", 16384, "playlist: description too long")?;
        let entries_v = required_array(body, "entries", "playlist: entries required")?;
        if entries_v.is_empty() {
            return Err(Error::Kind("playlist: entries must be non-empty"));
        }
        let mut entries = Vec::with_capacity(entries_v.len());
        for v in entries_v {
            let b = v
                .as_bytes()
                .ok_or(Error::Kind("playlist: entry must be bytes"))?;
            let a: [u8; 32] = b
                .try_into()
                .map_err(|_| Error::Kind("playlist: entry must be 32 bytes"))?;
            entries.push(RecordId(a));
        }
        Ok(Playlist {
            title,
            description,
            entries,
        })
    }

    /// Build the CBOR body for this playlist.
    pub fn to_body(&self) -> Value {
        let mut e = vec![
            (Value::Text("title".into()), Value::Text(self.title.clone())),
            (
                Value::Text("entries".into()),
                Value::Array(
                    self.entries
                        .iter()
                        .map(|id| Value::Bytes(id.as_bytes().to_vec()))
                        .collect(),
                ),
            ),
        ];
        if let Some(d) = &self.description {
            e.push((Value::Text("description".into()), Value::Text(d.clone())));
        }
        Value::Map(e)
    }
}

/// A named collection an identity publishes manifests into (spec 003
/// §5.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    /// Title, required, at most 512 bytes.
    pub title: String,
    /// Description, at most 16384 bytes.
    pub description: Option<String>,
    /// Avatar image blob.
    pub avatar: Option<BlobId>,
    /// Banner image blob.
    pub banner: Option<BlobId>,
}

impl Channel {
    /// Parse and validate a `channel` record body (spec 003 §5.5): no
    /// validation beyond schema; refs MUST be empty.
    pub fn parse(record: &Record) -> Result<Channel> {
        refs_empty(record, "channel: refs must be empty")?;
        let body = record.body();
        let title = required_nonempty_text(body, "title", 512, "channel: title required")?;
        let description = text_field(body, "description", 16384, "channel: description too long")?;
        let avatar = blob_id_field(body, "avatar", "channel: avatar must be 32 bytes")?;
        let banner = blob_id_field(body, "banner", "channel: banner must be 32 bytes")?;
        Ok(Channel {
            title,
            description,
            avatar,
            banner,
        })
    }

    /// Build the CBOR body for this channel.
    pub fn to_body(&self) -> Value {
        let mut e = vec![(Value::Text("title".into()), Value::Text(self.title.clone()))];
        if let Some(d) = &self.description {
            e.push((Value::Text("description".into()), Value::Text(d.clone())));
        }
        if let Some(a) = &self.avatar {
            e.push((
                Value::Text("avatar".into()),
                Value::Bytes(a.as_bytes().to_vec()),
            ));
        }
        if let Some(b) = &self.banner {
            e.push((
                Value::Text("banner".into()),
                Value::Bytes(b.as_bytes().to_vec()),
            ));
        }
        Value::Map(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Keypair;
    use crate::record::RecordBuilder;

    fn kp() -> Keypair {
        Keypair::from_secret_bytes(&[3u8; 32])
    }

    fn author() -> IdentityId {
        IdentityId([4; 32])
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
    fn profile_round_trip() {
        let p = Profile {
            name: "asha".into(),
            about: Some("field recordings".into()),
            avatar: Some(BlobId([1; 32])),
            payment: vec![PaymentPointer(1, "asha@ln.example.net".into())],
            relays: vec!["wss://relay.example.net/sync".into()],
            seeds: vec!["https://seed.example.net".into()],
            enc_key: Some((1, vec![9u8; 32])),
        };
        let record = sign(super::super::KIND_PROFILE, vec![], p.to_body());
        let back = Profile::parse(&record).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn profile_rejects_nonempty_refs() {
        let body = Value::Map(vec![(Value::Text("name".into()), Value::Text("a".into()))]);
        let record = sign(
            super::super::KIND_PROFILE,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Profile::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn profile_rejects_missing_name() {
        let record = sign(super::super::KIND_PROFILE, vec![], Value::Map(vec![]));
        assert!(matches!(Profile::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn comment_round_trip_with_parent() {
        let c = Comment {
            subject: RecordId([1; 32]),
            parent: Some(RecordId([2; 32])),
            text: "the second movement is extraordinary".into(),
            media: vec![BlobId([3; 32])],
        };
        let record = sign(super::super::KIND_COMMENT, c.refs(), c.to_body());
        let back = Comment::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn comment_round_trip_without_parent() {
        let c = Comment {
            subject: RecordId([1; 32]),
            parent: None,
            text: "nice".into(),
            media: vec![],
        };
        let record = sign(super::super::KIND_COMMENT, c.refs(), c.to_body());
        let back = Comment::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn comment_rejects_empty_refs() {
        let body = Value::Map(vec![(Value::Text("text".into()), Value::Text("hi".into()))]);
        let record = sign(super::super::KIND_COMMENT, vec![], body);
        assert!(matches!(Comment::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn comment_rejects_too_many_refs() {
        let body = Value::Map(vec![(Value::Text("text".into()), Value::Text("hi".into()))]);
        let record = sign(
            super::super::KIND_COMMENT,
            vec![
                Ref::record(RecordId([1; 32])),
                Ref::record(RecordId([2; 32])),
                Ref::record(RecordId([3; 32])),
            ],
            body,
        );
        assert!(matches!(Comment::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn comment_rejects_empty_text() {
        let body = Value::Map(vec![(Value::Text("text".into()), Value::Text("".into()))]);
        let record = sign(
            super::super::KIND_COMMENT,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Comment::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn check_comment_thread_accepts_matching_subject() {
        let subject = RecordId([1; 32]);
        let parent = Comment {
            subject,
            parent: None,
            text: "root".into(),
            media: vec![],
        };
        let reply = Comment {
            subject,
            parent: Some(RecordId([9; 32])),
            text: "reply".into(),
            media: vec![],
        };
        assert!(check_comment_thread(&reply, &parent).is_ok());
    }

    #[test]
    fn check_comment_thread_rejects_parent_subject_mismatch() {
        let parent = Comment {
            subject: RecordId([1; 32]),
            parent: None,
            text: "root".into(),
            media: vec![],
        };
        let reply = Comment {
            subject: RecordId([2; 32]), // different subject than the parent's
            parent: Some(RecordId([9; 32])),
            text: "reply".into(),
            media: vec![],
        };
        assert_eq!(
            check_comment_thread(&reply, &parent),
            Err(Error::Kind("comment/parent-subject-mismatch"))
        );
    }

    #[test]
    fn reaction_round_trip() {
        let r = Reaction {
            target: RecordId([1; 32]),
            reaction: "\u{1F525}".into(),
        };
        let record = sign(super::super::KIND_REACTION, r.refs(), r.to_body());
        let back = Reaction::parse(&record).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn reaction_rejects_too_long() {
        let body = Value::Map(vec![(
            Value::Text("reaction".into()),
            Value::Text("x".repeat(33)),
        )]);
        let record = sign(
            super::super::KIND_REACTION,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Reaction::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn reaction_rejects_wrong_refs_count() {
        let body = Value::Map(vec![(
            Value::Text("reaction".into()),
            Value::Text("x".into()),
        )]);
        let record = sign(super::super::KIND_REACTION, vec![], body);
        assert!(matches!(Reaction::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn follow_round_trip() {
        let f = Follow {
            followed: IdentityId([7; 32]),
            note: Some("great channel".into()),
        };
        let record = sign(super::super::KIND_FOLLOW, f.refs(), f.to_body());
        let back = Follow::parse(&record).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn follow_rejects_blob_ref() {
        let record = sign(
            super::super::KIND_FOLLOW,
            vec![Ref::blob(BlobId([1; 32]))],
            Value::Map(vec![]),
        );
        assert!(matches!(Follow::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn playlist_round_trip() {
        let p = Playlist {
            title: "winter mixes".into(),
            description: Some("a few tracks".into()),
            entries: vec![RecordId([1; 32]), RecordId([2; 32])],
        };
        let record = sign(super::super::KIND_PLAYLIST, vec![], p.to_body());
        let back = Playlist::parse(&record).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn playlist_rejects_empty_entries() {
        let body = Value::Map(vec![
            (Value::Text("title".into()), Value::Text("t".into())),
            (Value::Text("entries".into()), Value::Array(vec![])),
        ]);
        let record = sign(super::super::KIND_PLAYLIST, vec![], body);
        assert!(matches!(Playlist::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn playlist_rejects_nonempty_refs() {
        let body = Value::Map(vec![
            (Value::Text("title".into()), Value::Text("t".into())),
            (
                Value::Text("entries".into()),
                Value::Array(vec![Value::Bytes(vec![1; 32])]),
            ),
        ]);
        let record = sign(
            super::super::KIND_PLAYLIST,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Playlist::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn channel_round_trip() {
        let c = Channel {
            title: "Field Notes".into(),
            description: Some("one river a week".into()),
            avatar: Some(BlobId([1; 32])),
            banner: Some(BlobId([2; 32])),
        };
        let record = sign(super::super::KIND_CHANNEL, vec![], c.to_body());
        let back = Channel::parse(&record).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn channel_rejects_missing_title() {
        let record = sign(super::super::KIND_CHANNEL, vec![], Value::Map(vec![]));
        assert!(matches!(Channel::parse(&record), Err(Error::Kind(_))));
    }

    #[test]
    fn channel_rejects_nonempty_refs() {
        let body = Value::Map(vec![(Value::Text("title".into()), Value::Text("t".into()))]);
        let record = sign(
            super::super::KIND_CHANNEL,
            vec![Ref::record(RecordId([1; 32]))],
            body,
        );
        assert!(matches!(Channel::parse(&record), Err(Error::Kind(_))));
    }
}
