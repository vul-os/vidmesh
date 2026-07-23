//! Shared identifier newtypes: record ids, blob ids, identity ids.

use core::fmt;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub [u8; 32]);

        impl $name {
            /// The raw 32 bytes.
            pub fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }

            /// Lowercase hex, the text form defined by spec 001.
            pub fn to_hex(&self) -> String {
                let mut s = String::with_capacity(64);
                for b in self.0 {
                    use fmt::Write;
                    let _ = write!(s, "{b:02x}");
                }
                s
            }

            /// Parse from 64 lowercase (or uppercase) hex characters.
            pub fn from_hex(hex: &str) -> Option<Self> {
                let bytes = hex.as_bytes();
                if bytes.len() != 64 {
                    return None;
                }
                let mut out = [0u8; 32];
                for (i, pair) in bytes.chunks_exact(2).enumerate() {
                    let hi = (pair[0] as char).to_digit(16)?;
                    let lo = (pair[1] as char).to_digit(16)?;
                    out[i] = ((hi << 4) | lo) as u8;
                }
                Some(Self(out))
            }
        }

        impl fmt::Debug for $name {
            fmt_impl!();
        }

        impl fmt::Display for $name {
            fmt_impl!();
        }

        impl AsRef<[u8]> for $name {
            fn as_ref(&self) -> &[u8] {
                &self.0
            }
        }
    };
}

macro_rules! fmt_impl {
    () => {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for b in self.0 {
                write!(f, "{b:02x}")?;
            }
            Ok(())
        }
    };
}

id_type! {
    /// A record identifier: BLAKE3-256 over the canonical envelope
    /// without the signature (spec 001 §3).
    RecordId
}

id_type! {
    /// A blob identifier: BLAKE3-256 over the blob bytes (spec 001 §6).
    /// Text form is `b3-256:<hex>`.
    BlobId
}

id_type! {
    /// An identity identifier: the record id of the identity's genesis
    /// rotation record (spec 002 §2).
    IdentityId
}

impl BlobId {
    /// The `b3-256:<hex>` text form (spec 001 §6).
    pub fn to_uri(&self) -> String {
        format!("b3-256:{}", self.to_hex())
    }

    /// Parse the `b3-256:<hex>` text form.
    pub fn from_uri(s: &str) -> Option<Self> {
        Self::from_hex(s.strip_prefix("b3-256:")?)
    }
}

impl IdentityId {
    /// The zero identity id carried by genesis records (spec 001 §5).
    pub const ZERO: IdentityId = IdentityId([0u8; 32]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let id = RecordId([0xab; 32]);
        assert_eq!(id.to_hex().len(), 64);
        assert_eq!(RecordId::from_hex(&id.to_hex()), Some(id));
    }

    #[test]
    fn blob_uri_round_trip() {
        let id = BlobId([7; 32]);
        assert_eq!(BlobId::from_uri(&id.to_uri()), Some(id));
        assert!(BlobId::from_uri("b2-256:00").is_none());
    }

    #[test]
    fn from_hex_rejects_bad_input() {
        assert!(RecordId::from_hex("zz").is_none());
        assert!(RecordId::from_hex(&"a".repeat(63)).is_none());
        assert!(RecordId::from_hex(&"g".repeat(64)).is_none());
    }
}
