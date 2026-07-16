//! # vidmesh-kernel
//!
//! The permanent core library of the Vidmesh protocol: signed records
//! (canonical CBOR, Ed25519, BLAKE3), identity rotation chains,
//! content-addressed blobs with chunk-tree range verification, typed record
//! kinds, and bundle import/export.
//!
//! Phase 2 of the build plan implements this crate against the spec in
//! `spec/`. Until then it is an empty scaffold.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_compiles() {
        // Replaced by real unit tests and conformance vectors in Phase 2.
    }
}
