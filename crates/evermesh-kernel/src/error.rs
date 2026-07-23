//! Kernel error type.

use core::fmt;

/// Every way untrusted input can be rejected by the kernel.
///
/// The kernel never panics on untrusted input; all validation failures
/// surface as a variant of this enum.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Input is not well-formed CBOR (truncated, bad head, trailing bytes…).
    Cbor(&'static str),
    /// Input is well-formed CBOR but violates canonical-encoding rules
    /// (spec 001 §2): non-shortest head, unsorted or duplicate map keys,
    /// indefinite length, float, or tag.
    NonCanonical(&'static str),
    /// The envelope violates spec 001 §1 (missing/unknown keys, wrong types).
    Envelope(&'static str),
    /// The signature does not verify, or the key/signature is malformed.
    Signature,
    /// `sig_alg` (or another algorithm id) is not implemented by this kernel.
    UnknownAlgorithm(u64),
    /// A rotation chain violates spec 002 (bad genesis, unauthorized
    /// rotation, broken parent link…).
    Identity(&'static str),
    /// A chunk-tree proof does not verify (spec 001 §8).
    ChunkProof(&'static str),
    /// A kind-level validation rule failed (spec 003).
    Kind(&'static str),
    /// A bundle item is invalid or the container is malformed (spec 007).
    Bundle(&'static str),
    /// An I/O error occurred while streaming (hashing, bundle export/import).
    Io(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cbor(m) => write!(f, "malformed CBOR: {m}"),
            Error::NonCanonical(m) => write!(f, "non-canonical encoding: {m}"),
            Error::Envelope(m) => write!(f, "invalid envelope: {m}"),
            Error::Signature => write!(f, "signature verification failed"),
            Error::UnknownAlgorithm(id) => write!(f, "unknown algorithm id {id}"),
            Error::Identity(m) => write!(f, "invalid identity chain: {m}"),
            Error::ChunkProof(m) => write!(f, "chunk proof failed: {m}"),
            Error::Kind(m) => write!(f, "kind validation failed: {m}"),
            Error::Bundle(m) => write!(f, "invalid bundle: {m}"),
            Error::Io(m) => write!(f, "i/o error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Kernel result alias.
pub type Result<T> = core::result::Result<T, Error>;
