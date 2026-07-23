//! # evermesh-kernel
//!
//! The permanent core library of the Evermesh protocol: signed records
//! (canonical CBOR, Ed25519, BLAKE3), identity rotation chains,
//! content-addressed blobs with chunk-tree range verification, typed
//! record kinds, and bundle import/export.
//!
//! Implements `spec/001-kernel.md` and `spec/002-identity.md`; typed kind
//! wrappers implement `spec/003-kinds-registry.md`. The same crate
//! compiles natively and to WASM so one implementation verifies
//! everywhere.
//!
//! Entry points:
//! - [`record::Record::from_cbor`] / [`record::Record::verify`] — parse
//!   and verify untrusted records.
//! - [`record::RecordBuilder`] — create and sign records.
//! - [`identity::Identity`] — genesis, rotation, chain verification.
//! - [`blob`] — blob hashing, chunk trees, range proofs.
//! - [`codec`] — canonical CBOR and JSON interchange.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod blob;
pub mod bundle;
pub mod codec;
#[cfg(feature = "dmtap-pub")]
pub mod dmtap_pub;
pub mod error;
pub mod identity;
pub mod ids;
pub mod kinds;
pub mod record;

pub use blob::ChunkTree;
pub use bundle::{Bundle, ImportResult};
pub use error::{Error, Result};
pub use identity::{Identity, IdentityState, Keypair};
pub use ids::{BlobId, IdentityId, RecordId};
pub use record::{IdentityRef, Record, RecordBuilder, Ref};
