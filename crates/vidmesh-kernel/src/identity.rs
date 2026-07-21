//! Keypairs and the identity rotation log (spec 002).

use crate::codec::Value;
use crate::error::{Error, Result};
use crate::ids::{IdentityId, RecordId};
use crate::record::{Record, RecordBuilder, Ref, SIG_ALG_ED25519};

/// Kind id of `rotation` records (spec 003 §1).
pub const KIND_ROTATION: u64 = 1;

/// An Ed25519 signing keypair.
pub struct Keypair {
    signing: ed25519_dalek::SigningKey,
}

impl Keypair {
    /// Generate from the operating system RNG.
    pub fn generate() -> Result<Keypair> {
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret).map_err(|_| Error::Io("rng unavailable"))?;
        Ok(Keypair {
            signing: ed25519_dalek::SigningKey::from_bytes(&secret),
        })
    }

    /// Deterministic construction from 32 secret bytes (tests, storage).
    pub fn from_secret_bytes(secret: &[u8; 32]) -> Keypair {
        Keypair {
            signing: ed25519_dalek::SigningKey::from_bytes(secret),
        }
    }

    /// The 32 secret bytes. Handle with care.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// The 32-byte Ed25519 public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// Sign a message, returning the 64-byte signature.
    pub fn sign(&self, msg: &[u8]) -> [u8; 64] {
        use ed25519_dalek::Signer;
        self.signing.sign(msg).to_bytes()
    }
}

/// A `(key_alg, public_key)` pair, as used for recovery keys.
pub type AlgKey = (u64, Vec<u8>);

/// Parsed body of a rotation record (spec 002 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationBody {
    /// New signing public key.
    pub key: Vec<u8>,
    /// Algorithm of `key`.
    pub key_alg: u64,
    /// Declared recovery keys.
    pub recovery: Vec<AlgKey>,
    /// Contest window in seconds; 0 disables recovery precedence.
    pub contest_window: u64,
}

impl RotationBody {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            (
                Value::Text("contest_window".into()),
                Value::Uint(self.contest_window),
            ),
            (Value::Text("key".into()), Value::Bytes(self.key.clone())),
            (Value::Text("key_alg".into()), Value::Uint(self.key_alg)),
            (
                Value::Text("recovery".into()),
                Value::Array(
                    self.recovery
                        .iter()
                        .map(|(alg, key)| {
                            Value::Array(vec![Value::Uint(*alg), Value::Bytes(key.clone())])
                        })
                        .collect(),
                ),
            ),
        ])
    }

    /// Parse and validate the body of a rotation record.
    pub fn parse(record: &Record) -> Result<RotationBody> {
        if record.kind() != KIND_ROTATION {
            return Err(Error::Identity("not a rotation record"));
        }
        let body = record.body();
        let key = body
            .map_get("key")
            .and_then(Value::as_bytes)
            .ok_or(Error::Identity("rotation body missing key"))?
            .to_vec();
        let key_alg = body
            .map_get("key_alg")
            .and_then(Value::as_u64)
            .ok_or(Error::Identity("rotation body missing key_alg"))?;
        let contest_window = body
            .map_get("contest_window")
            .and_then(Value::as_u64)
            .ok_or(Error::Identity("rotation body missing contest_window"))?;
        let recovery_v = body
            .map_get("recovery")
            .and_then(Value::as_array)
            .ok_or(Error::Identity("rotation body missing recovery"))?;
        let mut recovery = Vec::with_capacity(recovery_v.len());
        for entry in recovery_v {
            let pair = entry
                .as_array()
                .ok_or(Error::Identity("recovery entry must be array"))?;
            if pair.len() != 2 {
                return Err(Error::Identity("recovery entry must be [alg, key]"));
            }
            let alg = pair[0]
                .as_u64()
                .ok_or(Error::Identity("recovery alg must be uint"))?;
            let key = pair[1]
                .as_bytes()
                .ok_or(Error::Identity("recovery key must be bytes"))?
                .to_vec();
            recovery.push((alg, key));
        }
        Ok(RotationBody {
            key,
            key_alg,
            recovery,
            contest_window,
        })
    }
}

/// The verified current state of an identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityState {
    /// The identity's stable identifier.
    pub identity_id: IdentityId,
    /// Current signing key and its algorithm.
    pub signing_key: Vec<u8>,
    /// Algorithm of `signing_key`.
    pub key_alg: u64,
    /// Current recovery key set.
    pub recovery: Vec<AlgKey>,
    /// Current contest window (seconds).
    pub contest_window: u64,
    /// Record id of the chain head (latest applied rotation).
    pub head: RecordId,
    /// Number of rotations applied after genesis.
    pub depth: u64,
}

impl IdentityState {
    /// True if `key` (with `alg`) is the current signing key.
    pub fn is_signing_key(&self, alg: u64, key: &[u8]) -> bool {
        self.key_alg == alg && self.signing_key == key
    }

    /// True if `key` (with `alg`) is a current recovery key.
    pub fn is_recovery_key(&self, alg: u64, key: &[u8]) -> bool {
        self.recovery.iter().any(|(a, k)| *a == alg && k == key)
    }
}

/// Identity operations: genesis, rotation, chain verification.
pub struct Identity;

impl Identity {
    /// Create a new identity (spec 002 §2). Returns the identifier and
    /// the genesis rotation record to publish.
    pub fn genesis(
        keypair: &Keypair,
        recovery: &[AlgKey],
        contest_window: u64,
        created_at: i64,
    ) -> Result<(IdentityId, Record)> {
        let body = RotationBody {
            key: keypair.public_key_bytes().to_vec(),
            key_alg: SIG_ALG_ED25519,
            recovery: recovery.to_vec(),
            contest_window,
        };
        let record = RecordBuilder::new(KIND_ROTATION)
            .created_at(created_at)
            .body(body.to_value())
            .sign_as(keypair, IdentityId::ZERO)?;
        let id = IdentityId(record.id().0);
        Ok((id, record))
    }

    /// Build a rotation record (spec 002 §3). `signer` must hold the
    /// current signing key or a current recovery key; this function does
    /// not check authorization (the chain does, at verification time).
    #[allow(clippy::too_many_arguments)]
    pub fn rotate(
        identity: IdentityId,
        prev: RecordId,
        new_key: &[u8],
        new_key_alg: u64,
        recovery: &[AlgKey],
        contest_window: u64,
        created_at: i64,
        signer: &Keypair,
    ) -> Result<Record> {
        let body = RotationBody {
            key: new_key.to_vec(),
            key_alg: new_key_alg,
            recovery: recovery.to_vec(),
            contest_window,
        };
        RecordBuilder::new(KIND_ROTATION)
            .created_at(created_at)
            .r#ref(Ref::record(prev))
            .body(body.to_value())
            .sign_as(signer, identity)
    }

    /// Verify a rotation chain and compute the current state
    /// (spec 002 §4).
    ///
    /// `records` is any set of records; non-rotations and rotations for
    /// other identities are ignored. `observed_at` reports when this
    /// verifier first saw a record (`None` = treat as just observed,
    /// i.e. not final); `now` is the verifier's current clock. Both are
    /// verifier-local and drive only contest-window finality.
    pub fn verify_chain(
        records: &[Record],
        observed_at: &dyn Fn(&RecordId) -> Option<i64>,
        now: i64,
    ) -> Result<IdentityState> {
        // 1. Locate exactly one valid genesis.
        let mut genesis: Option<(&Record, RotationBody)> = None;
        for r in records {
            if r.kind() != KIND_ROTATION || !r.refs().is_empty() {
                continue;
            }
            if r.author().identity_id != IdentityId::ZERO {
                continue;
            }
            let body = match RotationBody::parse(r) {
                Ok(b) => b,
                Err(_) => continue,
            };
            if r.author().signing_key != body.key || r.sig_alg() != body.key_alg {
                continue;
            }
            if r.verify().is_err() {
                continue;
            }
            if genesis.is_some() {
                return Err(Error::Identity("multiple genesis records in input"));
            }
            genesis = Some((r, body));
        }
        let (genesis, genesis_body) = genesis.ok_or(Error::Identity("no valid genesis"))?;
        let identity_id = IdentityId(genesis.id().0);

        // 2. Index candidate rotations by parent.
        let mut by_parent: std::collections::HashMap<[u8; 32], Vec<&Record>> =
            std::collections::HashMap::new();
        for r in records {
            if r.kind() != KIND_ROTATION || r.refs().len() != 1 {
                continue;
            }
            let parent = r.refs()[0];
            if !parent.is_record() {
                continue;
            }
            if r.author().identity_id != identity_id {
                continue;
            }
            if r.verify().is_err() {
                continue;
            }
            by_parent.entry(parent.hash).or_default().push(r);
        }

        // 3. Walk from genesis, resolving forks per spec 002 §4.
        let mut state = IdentityState {
            identity_id,
            signing_key: genesis_body.key,
            key_alg: genesis_body.key_alg,
            recovery: genesis_body.recovery,
            contest_window: genesis_body.contest_window,
            head: genesis.id(),
            depth: 0,
        };
        loop {
            let Some(children) = by_parent.get(&state.head.0) else {
                return Ok(state);
            };
            // Authorization classes under the state at the parent.
            let mut signing: Vec<(&Record, RotationBody)> = Vec::new();
            let mut recovery: Vec<(&Record, RotationBody)> = Vec::new();
            for child in children {
                let Ok(body) = RotationBody::parse(child) else {
                    continue;
                };
                let a = child.author();
                if state.is_signing_key(child.sig_alg(), &a.signing_key) {
                    signing.push((child, body));
                } else if state.is_recovery_key(child.sig_alg(), &a.signing_key) {
                    recovery.push((child, body));
                }
            }
            let is_final = |r: &Record| -> bool {
                match observed_at(&r.id()) {
                    // `contest_window` is a wire-decoded `u64` (spec 002
                    // §1) and can legitimately be as large as
                    // `u64::MAX` — it is not attacker-*bounded* the way
                    // a fixed-width guard would reject, it is
                    // attacker/author-*chosen*. Comparing via
                    // `state.contest_window as i64` would silently wrap
                    // any value `>= 2^63` into a *negative* `i64` (Rust's
                    // `as` cast reinterprets bits rather than erroring),
                    // making `now.saturating_sub(seen) > negative` true
                    // for essentially any `seen`. That flips a huge
                    // contest window — presumably chosen to mean "give
                    // recovery a very long time to contest" — into an
                    // window of effectively zero, finalizing a rogue
                    // signing rotation instantly and defeating the exact
                    // protection contest_window exists to provide. Do
                    // the comparison in `u64`, the domain
                    // `contest_window` is actually typed in; an elapsed
                    // time that doesn't fit in `u64` (i.e. `seen > now`,
                    // only possible with clock skew) cannot exceed any
                    // window and so is treated as not final.
                    Some(seen) => match u64::try_from(now.saturating_sub(seen)) {
                        Ok(elapsed) => elapsed > state.contest_window,
                        Err(_) => false,
                    },
                    None => false,
                }
            };
            let lowest = |v: &[(&Record, RotationBody)]| -> Option<(RecordId, RotationBody)> {
                v.iter()
                    .map(|(r, b)| (r.id(), b.clone()))
                    .min_by(|a, b| a.0 .0.cmp(&b.0 .0))
            };
            let chosen: Option<(RecordId, RotationBody)> = if state.contest_window == 0 {
                // Recovery precedence disabled: lowest id among all.
                let mut all = signing.clone();
                all.extend(recovery.iter().cloned());
                lowest(&all)
            } else {
                let final_signing: Vec<(&Record, RotationBody)> = signing
                    .iter()
                    .filter(|(r, _)| is_final(r))
                    .cloned()
                    .collect();
                if !final_signing.is_empty() {
                    // Finalized signing rotations cannot be displaced.
                    lowest(&final_signing)
                } else if !recovery.is_empty() {
                    // Theft recovery: recovery beats provisional signing.
                    lowest(&recovery)
                } else {
                    lowest(&signing)
                }
            };
            match chosen {
                None => return Ok(state),
                Some((id, body)) => {
                    state.signing_key = body.key;
                    state.key_alg = body.key_alg;
                    state.recovery = body.recovery;
                    state.contest_window = body.contest_window;
                    state.head = id;
                    state.depth += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kp(seed: u8) -> Keypair {
        Keypair::from_secret_bytes(&[seed; 32])
    }

    fn recovery_of(k: &Keypair) -> AlgKey {
        (SIG_ALG_ED25519, k.public_key_bytes().to_vec())
    }

    const WINDOW: u64 = 604_800;

    /// observed_at treating everything as just seen (nothing final).
    fn seen_now(_: &RecordId) -> Option<i64> {
        None
    }

    #[test]
    fn genesis_verifies_and_derives_id() {
        let user = kp(1);
        let (id, genesis) = Identity::genesis(&user, &[], WINDOW, 100).unwrap();
        genesis.verify().unwrap();
        assert_eq!(id.0, genesis.id().0);
        let state = Identity::verify_chain(&[genesis], &seen_now, 1000).unwrap();
        assert_eq!(state.identity_id, id);
        assert_eq!(state.signing_key, user.public_key_bytes().to_vec());
        assert_eq!(state.depth, 0);
    }

    #[test]
    fn signing_key_rotation_advances_state() {
        let old = kp(1);
        let new = kp(2);
        let (id, genesis) = Identity::genesis(&old, &[], WINDOW, 100).unwrap();
        let rot = Identity::rotate(
            id,
            genesis.id(),
            &new.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            200,
            &old,
        )
        .unwrap();
        let state = Identity::verify_chain(&[genesis, rot.clone()], &seen_now, 1000).unwrap();
        assert_eq!(state.signing_key, new.public_key_bytes().to_vec());
        assert_eq!(state.head, rot.id());
        assert_eq!(state.depth, 1);
    }

    #[test]
    fn unauthorized_rotation_ignored() {
        let user = kp(1);
        let attacker = kp(9);
        let (id, genesis) = Identity::genesis(&user, &[], WINDOW, 100).unwrap();
        let rogue = Identity::rotate(
            id,
            genesis.id(),
            &attacker.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            200,
            &attacker,
        )
        .unwrap();
        let state = Identity::verify_chain(&[genesis, rogue], &seen_now, 1000).unwrap();
        assert_eq!(state.depth, 0);
        assert_eq!(state.signing_key, user.public_key_bytes().to_vec());
    }

    #[test]
    fn recovery_beats_provisional_thief() {
        // Thief steals the signing key and rotates; owner forks from the
        // same parent with the recovery key. Recovery wins while the
        // thief's rotation is not final.
        let owner_signing = kp(1);
        let owner_recovery = kp(2);
        let thief = kp(3);
        let owner_new = kp(4);
        let (id, genesis) =
            Identity::genesis(&owner_signing, &[recovery_of(&owner_recovery)], WINDOW, 100)
                .unwrap();
        let thief_rot = Identity::rotate(
            id,
            genesis.id(),
            &thief.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            200,
            &owner_signing, // stolen key
        )
        .unwrap();
        let owner_rot = Identity::rotate(
            id,
            genesis.id(),
            &owner_new.public_key_bytes(),
            SIG_ALG_ED25519,
            &[recovery_of(&owner_recovery)],
            WINDOW,
            300,
            &owner_recovery,
        )
        .unwrap();
        // Deeper thief branch must still lose.
        let thief_rot2 = Identity::rotate(
            id,
            thief_rot.id(),
            &kp(5).public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            400,
            &thief,
        )
        .unwrap();
        let records = vec![genesis, thief_rot, thief_rot2, owner_rot.clone()];
        let state = Identity::verify_chain(&records, &seen_now, 1000).unwrap();
        assert_eq!(state.head, owner_rot.id());
        assert_eq!(state.signing_key, owner_new.public_key_bytes().to_vec());
    }

    #[test]
    fn final_signing_rotation_resists_recovery_fork() {
        // A legitimate signing rotation observed longer than the window
        // ago cannot be displaced by a later recovery fork.
        let signing = kp(1);
        let recovery = kp(2);
        let new_signing = kp(3);
        let recovery_new = kp(4);
        let (id, genesis) =
            Identity::genesis(&signing, &[recovery_of(&recovery)], WINDOW, 100).unwrap();
        let legit = Identity::rotate(
            id,
            genesis.id(),
            &new_signing.public_key_bytes(),
            SIG_ALG_ED25519,
            &[recovery_of(&recovery)],
            WINDOW,
            200,
            &signing,
        )
        .unwrap();
        let late_recovery = Identity::rotate(
            id,
            genesis.id(),
            &recovery_new.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            300,
            &recovery,
        )
        .unwrap();
        let legit_id = legit.id();
        let observed = move |rid: &RecordId| -> Option<i64> {
            if *rid == legit_id {
                Some(0) // seen long ago
            } else {
                None
            }
        };
        let now = (WINDOW as i64) + 10;
        let records = vec![genesis, legit.clone(), late_recovery];
        let state = Identity::verify_chain(&records, &observed, now).unwrap();
        assert_eq!(state.head, legit.id());
    }

    #[test]
    fn same_class_fork_resolves_by_lowest_id() {
        let signing = kp(1);
        let a = kp(2);
        let b = kp(3);
        let (id, genesis) = Identity::genesis(&signing, &[], WINDOW, 100).unwrap();
        let rot_a = Identity::rotate(
            id,
            genesis.id(),
            &a.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            200,
            &signing,
        )
        .unwrap();
        let rot_b = Identity::rotate(
            id,
            genesis.id(),
            &b.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            201,
            &signing,
        )
        .unwrap();
        let expected = if rot_a.id().0 < rot_b.id().0 {
            rot_a.id()
        } else {
            rot_b.id()
        };
        let state = Identity::verify_chain(&[genesis, rot_a, rot_b], &seen_now, 1000).unwrap();
        assert_eq!(state.head, expected);
    }

    #[test]
    fn huge_contest_window_does_not_wrap_negative_and_disable_recovery() {
        // Regression for the `contest_window as i64` cast hazard: `u64`'s
        // upper half (>= 2^63) reinterprets as a *negative* `i64` under
        // `as`, which would make `is_final` return `true` for a rotation
        // observed only moments ago — the opposite of what a large
        // contest window means. Same shape as
        // `recovery_beats_provisional_thief`, but with
        // `contest_window = u64::MAX` and the thief's forged rotation
        // reported as having just been seen (`elapsed == 0`).
        let owner_signing = kp(1);
        let owner_recovery = kp(2);
        let thief = kp(3);
        let owner_new = kp(4);
        let huge_window = u64::MAX;
        let (id, genesis) = Identity::genesis(
            &owner_signing,
            &[recovery_of(&owner_recovery)],
            huge_window,
            100,
        )
        .unwrap();
        let thief_rot = Identity::rotate(
            id,
            genesis.id(),
            &thief.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            huge_window,
            200,
            &owner_signing, // stolen key
        )
        .unwrap();
        let owner_rot = Identity::rotate(
            id,
            genesis.id(),
            &owner_new.public_key_bytes(),
            SIG_ALG_ED25519,
            &[recovery_of(&owner_recovery)],
            huge_window,
            300,
            &owner_recovery,
        )
        .unwrap();
        let thief_id = thief_rot.id();
        // The thief's rotation was observed at exactly `now`: elapsed is
        // 0, nowhere near "final" under a window this size. Under the
        // wrapped cast, `huge_window as i64` is negative and `0 > that`
        // is true, so the buggy code finalizes it immediately anyway.
        let observed = move |rid: &RecordId| -> Option<i64> {
            if *rid == thief_id {
                Some(1000)
            } else {
                None
            }
        };
        let records = vec![genesis, thief_rot, owner_rot.clone()];
        let state = Identity::verify_chain(&records, &observed, 1000).unwrap();
        assert_eq!(
            state.head,
            owner_rot.id(),
            "recovery must still beat a just-observed provisional signing rotation \
             even when contest_window is astronomically large"
        );
        assert_eq!(state.signing_key, owner_new.public_key_bytes().to_vec());
    }

    #[test]
    fn merge_order_independence() {
        let signing = kp(1);
        let recovery = kp(2);
        let n1 = kp(3);
        let n2 = kp(4);
        let (id, genesis) =
            Identity::genesis(&signing, &[recovery_of(&recovery)], WINDOW, 100).unwrap();
        let r1 = Identity::rotate(
            id,
            genesis.id(),
            &n1.public_key_bytes(),
            SIG_ALG_ED25519,
            &[recovery_of(&recovery)],
            WINDOW,
            200,
            &signing,
        )
        .unwrap();
        let r2 = Identity::rotate(
            id,
            r1.id(),
            &n2.public_key_bytes(),
            SIG_ALG_ED25519,
            &[],
            WINDOW,
            300,
            &n1,
        )
        .unwrap();
        let mut records = vec![genesis, r1, r2];
        let baseline = Identity::verify_chain(&records, &seen_now, 1000).unwrap();
        // All 6 permutations of 3 records give the same state.
        for _ in 0..3 {
            records.rotate_left(1);
            assert_eq!(
                Identity::verify_chain(&records, &seen_now, 1000).unwrap(),
                baseline
            );
            let mut swapped = records.clone();
            swapped.swap(0, 1);
            assert_eq!(
                Identity::verify_chain(&swapped, &seen_now, 1000).unwrap(),
                baseline
            );
        }
    }
}
