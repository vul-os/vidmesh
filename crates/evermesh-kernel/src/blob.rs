//! Blob addressing and chunk trees (spec 001 §6, §8).
//!
//! A blob is an opaque byte sequence addressed by the BLAKE3-256 hash of
//! its bytes (§6). Large blobs additionally have a **chunk tree**: a
//! binary Merkle tree over 1 MiB chunks that lets a verifier check any
//! single chunk against a trusted root in `O(log n)` hashes, without the
//! rest of the blob (§8).

use std::io::Read;

use crate::error::{Error, Result};
use crate::ids::BlobId;

/// Chunk size for chunk trees: 1 MiB (spec 001 §8).
pub const CHUNK_SIZE: usize = 1 << 20;

/// Size of the internal buffer used by [`hash_stream`] to read from its
/// reader. Unrelated to `CHUNK_SIZE`: whole-blob hashing has no chunk
/// boundaries, this is purely an I/O granularity.
const READ_BUF_SIZE: usize = 64 * 1024;

/// Hash a complete in-memory blob (BLAKE3-256 of its bytes).
pub fn hash_blob(bytes: &[u8]) -> BlobId {
    BlobId(*blake3::hash(bytes).as_bytes())
}

/// Hash a blob from a reader without loading it; returns (id, size).
///
/// Reads until EOF. I/O errors surface as `Error::Io` rather than a
/// panic.
pub fn hash_stream<R: Read>(mut reader: R) -> Result<(BlobId, u64)> {
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; READ_BUF_SIZE];
    let mut total: u64 = 0;
    loop {
        let n = fill_buf(&mut reader, &mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
        if n < buf.len() {
            // Short read: the reader is exhausted.
            break;
        }
    }
    Ok((BlobId(*hasher.finalize().as_bytes()), total))
}

/// Leaf hash: BLAKE3-256(0x00 || chunk_bytes).
pub fn leaf_hash(chunk: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[0x00]);
    hasher.update(chunk);
    *hasher.finalize().as_bytes()
}

/// Interior hash: BLAKE3-256(0x01 || left || right).
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[0x01]);
    hasher.update(left);
    hasher.update(right);
    *hasher.finalize().as_bytes()
}

/// Read from `reader` until `buf` is full or EOF is reached, looping over
/// short reads. Returns the number of bytes filled; a return value less
/// than `buf.len()` means EOF was reached (0 means EOF with nothing
/// read). Never panics on I/O failure; surfaces it as `Error::Io`.
fn fill_buf<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Error::Io("failed to read from stream")),
        }
    }
    Ok(filled)
}

/// Combine one level of a chunk tree into the next: pair nodes left to
/// right with [`node_hash`]; a level's unpaired final node is promoted
/// unchanged (spec 001 §8 step 3).
fn reduce_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    let mut i = 0;
    while i + 1 < level.len() {
        next.push(node_hash(&level[i], &level[i + 1]));
        i += 2;
    }
    if i < level.len() {
        next.push(level[i]);
    }
    next
}

/// The chunk tree of a blob: its leaf hashes plus total size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkTree {
    leaves: Vec<[u8; 32]>,
    size: u64,
}

impl ChunkTree {
    /// Build by streaming a reader in [`CHUNK_SIZE`] chunks.
    ///
    /// The empty reader (immediate EOF) yields the empty tree (zero
    /// chunks, no root).
    pub fn build<R: Read>(mut reader: R) -> Result<ChunkTree> {
        let mut leaves = Vec::new();
        let mut size: u64 = 0;
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let n = fill_buf(&mut reader, &mut buf)?;
            if n == 0 {
                break;
            }
            leaves.push(leaf_hash(&buf[..n]));
            size += n as u64;
            if n < CHUNK_SIZE {
                break;
            }
        }
        Ok(ChunkTree { leaves, size })
    }

    /// Build from an in-memory blob.
    pub fn from_bytes(bytes: &[u8]) -> ChunkTree {
        let leaves = bytes.chunks(CHUNK_SIZE).map(leaf_hash).collect();
        ChunkTree {
            leaves,
            size: bytes.len() as u64,
        }
    }

    /// Merkle root per spec 001 §8 (odd nodes promoted unchanged).
    /// `None` for the empty blob (zero chunks).
    pub fn root(&self) -> Option<[u8; 32]> {
        if self.leaves.is_empty() {
            return None;
        }
        let mut level = self.leaves.clone();
        while level.len() > 1 {
            level = reduce_level(&level);
        }
        Some(level[0])
    }

    /// Total blob size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Number of chunks (zero for the empty blob).
    pub fn n_chunks(&self) -> usize {
        self.leaves.len()
    }

    /// The leaf hashes, in chunk order.
    pub fn leaves(&self) -> &[[u8; 32]] {
        &self.leaves
    }

    /// Sibling path for chunk `index`, bottom-up; `Err(Error::ChunkProof)`
    /// if index out of range. For a promoted (sibling-less) node at some
    /// level, no hash is emitted at that level — the verifier recomputes
    /// structure from `n_chunks`.
    pub fn prove(&self, index: usize) -> Result<Vec<[u8; 32]>> {
        if index >= self.leaves.len() {
            return Err(Error::ChunkProof("chunk index out of range"));
        }
        let mut proof = Vec::new();
        let mut level = self.leaves.clone();
        let mut cur = index;
        while level.len() > 1 {
            let sibling = cur ^ 1;
            if sibling < level.len() {
                proof.push(level[sibling]);
            }
            cur /= 2;
            level = reduce_level(&level);
        }
        Ok(proof)
    }
}

/// Verify one chunk against a root, given the blob's total chunk count
/// and the sibling path from [`ChunkTree::prove`]. Recomputes the tree
/// structure from `n_chunks`: at each level, if the current node index
/// has a sibling (`sibling_index < level_len`), consumes one proof hash
/// (ordered left/right by index parity); if it is the promoted last
/// node, consumes nothing.
///
/// Errors (`Error::ChunkProof`, static messages): index >= n_chunks,
/// proof too short/long, root mismatch.
pub fn verify_chunk(
    root: &[u8; 32],
    n_chunks: usize,
    index: usize,
    chunk: &[u8],
    proof: &[[u8; 32]],
) -> Result<()> {
    if index >= n_chunks {
        return Err(Error::ChunkProof("chunk index out of range"));
    }
    let mut node = leaf_hash(chunk);
    let mut level_len = n_chunks;
    let mut cur = index;
    let mut used = 0usize;
    while level_len > 1 {
        let sibling = cur ^ 1;
        if sibling < level_len {
            let sib = *proof
                .get(used)
                .ok_or(Error::ChunkProof("proof too short"))?;
            used += 1;
            node = if cur.is_multiple_of(2) {
                node_hash(&node, &sib)
            } else {
                node_hash(&sib, &node)
            };
        }
        cur /= 2;
        level_len = level_len.div_ceil(2);
    }
    if used != proof.len() {
        return Err(Error::ChunkProof("proof too long"));
    }
    if node != *root {
        return Err(Error::ChunkProof("root mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Build a blob of `n_chunks` chunks, all full `CHUNK_SIZE` except
    /// the last, which is `last_len` bytes. Each chunk is filled with a
    /// distinct repeating byte (its index) so that chunks, and thus
    /// their leaf hashes, are pairwise distinct.
    fn make_blob(n_chunks: usize, last_len: usize) -> Vec<u8> {
        assert!(n_chunks > 0);
        assert!(last_len > 0 && last_len <= CHUNK_SIZE);
        let mut out = Vec::with_capacity((n_chunks - 1) * CHUNK_SIZE + last_len);
        for i in 0..n_chunks {
            let len = if i + 1 == n_chunks {
                last_len
            } else {
                CHUNK_SIZE
            };
            out.extend(std::iter::repeat_n(i as u8, len));
        }
        out
    }

    /// Independent reference implementation of the root reduction, used
    /// to cross-check `ChunkTree::root` / `reduce_level`.
    fn reference_root(leaves: &[[u8; 32]]) -> Option<[u8; 32]> {
        if leaves.is_empty() {
            return None;
        }
        let mut level: Vec<[u8; 32]> = leaves.to_vec();
        while level.len() > 1 {
            let mut next = Vec::new();
            for pair in level.chunks(2) {
                if pair.len() == 2 {
                    next.push(node_hash(&pair[0], &pair[1]));
                } else {
                    next.push(pair[0]);
                }
            }
            level = next;
        }
        Some(level[0])
    }

    #[test]
    fn empty_blob_has_no_chunks_and_no_root() {
        let tree = ChunkTree::from_bytes(&[]);
        assert_eq!(tree.n_chunks(), 0);
        assert_eq!(tree.size(), 0);
        assert_eq!(tree.root(), None);
        assert!(tree.leaves().is_empty());

        let tree2 = ChunkTree::build(Cursor::new(Vec::<u8>::new())).unwrap();
        assert_eq!(tree2, tree);
    }

    #[test]
    fn empty_blob_verify_chunk_rejects_any_index() {
        let root = [0u8; 32];
        assert!(verify_chunk(&root, 0, 0, b"x", &[]).is_err());
    }

    #[test]
    fn one_chunk_root_is_the_leaf_hash() {
        let bytes = make_blob(1, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        assert_eq!(tree.n_chunks(), 1);
        assert_eq!(tree.size(), CHUNK_SIZE as u64);
        assert_eq!(tree.root(), Some(leaf_hash(&bytes)));
    }

    #[test]
    fn manual_three_chunk_root_shape() {
        // root = node_hash(node_hash(l0, l1), l2): l2 is promoted
        // unchanged at level 0, then combined with the level-1 pair.
        let bytes = make_blob(3, CHUNK_SIZE);
        let l0 = leaf_hash(&bytes[0..CHUNK_SIZE]);
        let l1 = leaf_hash(&bytes[CHUNK_SIZE..2 * CHUNK_SIZE]);
        let l2 = leaf_hash(&bytes[2 * CHUNK_SIZE..3 * CHUNK_SIZE]);
        let expected = node_hash(&node_hash(&l0, &l1), &l2);

        let tree = ChunkTree::from_bytes(&bytes);
        assert_eq!(tree.leaves(), &[l0, l1, l2]);
        assert_eq!(tree.root(), Some(expected));
    }

    #[test]
    fn known_structure_against_reference_for_several_counts() {
        for &n in &[1usize, 2, 3, 4, 5, 7] {
            let bytes = make_blob(n, CHUNK_SIZE);
            let tree = ChunkTree::from_bytes(&bytes);
            assert_eq!(tree.n_chunks(), n, "n_chunks for count {n}");
            let expected = reference_root(tree.leaves());
            assert_eq!(tree.root(), expected, "root mismatch for count {n}");
        }
    }

    #[test]
    fn known_structure_with_short_final_chunk() {
        for &n in &[1usize, 2, 3, 5, 7] {
            let bytes = make_blob(n, 10);
            let tree = ChunkTree::from_bytes(&bytes);
            assert_eq!(tree.n_chunks(), n);
            assert_eq!(tree.size(), ((n - 1) * CHUNK_SIZE + 10) as u64);
            let expected = reference_root(tree.leaves());
            assert_eq!(
                tree.root(),
                expected,
                "root mismatch for count {n} (short final)"
            );
        }
    }

    #[test]
    fn from_bytes_and_build_agree() {
        let bytes = make_blob(2, 5);
        let via_bytes = ChunkTree::from_bytes(&bytes);
        let via_stream = ChunkTree::build(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(via_bytes, via_stream);
        assert_eq!(via_bytes.root(), via_stream.root());
    }

    #[test]
    fn boundary_exact_chunk_size_is_one_chunk() {
        let bytes = vec![0xabu8; CHUNK_SIZE];
        let tree = ChunkTree::from_bytes(&bytes);
        assert_eq!(tree.n_chunks(), 1);
        assert_eq!(tree.size(), CHUNK_SIZE as u64);

        let tree2 = ChunkTree::build(Cursor::new(bytes)).unwrap();
        assert_eq!(tree, tree2);
    }

    #[test]
    fn boundary_chunk_size_plus_one_is_two_chunks() {
        let bytes = vec![0xcdu8; CHUNK_SIZE + 1];
        let tree = ChunkTree::from_bytes(&bytes);
        assert_eq!(tree.n_chunks(), 2);
        assert_eq!(tree.size(), (CHUNK_SIZE + 1) as u64);
        assert_eq!(tree.leaves()[1], leaf_hash(&bytes[CHUNK_SIZE..]));

        let tree2 = ChunkTree::build(Cursor::new(bytes)).unwrap();
        assert_eq!(tree, tree2);
    }

    #[test]
    fn hash_blob_matches_hash_stream() {
        let bytes = make_blob(2, CHUNK_SIZE / 2);
        let direct = hash_blob(&bytes);
        let (streamed, size) = hash_stream(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(direct, streamed);
        assert_eq!(size, bytes.len() as u64);
    }

    #[test]
    fn hash_blob_matches_hash_stream_for_empty_input() {
        let direct = hash_blob(&[]);
        let (streamed, size) = hash_stream(Cursor::new(Vec::<u8>::new())).unwrap();
        assert_eq!(direct, streamed);
        assert_eq!(size, 0);
    }

    #[test]
    fn prove_and_verify_every_index_for_several_counts() {
        for &n in &[1usize, 2, 3, 4, 5] {
            let bytes = make_blob(n, CHUNK_SIZE);
            let tree = ChunkTree::from_bytes(&bytes);
            let root = tree.root().expect("non-empty tree has a root");
            for index in 0..n {
                let chunk_start = index * CHUNK_SIZE;
                let chunk = &bytes[chunk_start..chunk_start + CHUNK_SIZE];
                let proof = tree.prove(index).expect("valid index");
                verify_chunk(&root, n, index, chunk, &proof)
                    .unwrap_or_else(|e| panic!("verify failed for n={n} index={index}: {e:?}"));
            }
        }
    }

    #[test]
    fn prove_out_of_range_index_errors() {
        let bytes = make_blob(3, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        assert!(tree.prove(3).is_err());
        assert!(tree.prove(usize::MAX).is_err());
    }

    #[test]
    fn verify_rejects_wrong_chunk_bytes() {
        let bytes = make_blob(4, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().unwrap();
        let proof = tree.prove(1).unwrap();
        let mut wrong_chunk = bytes[CHUNK_SIZE..2 * CHUNK_SIZE].to_vec();
        wrong_chunk[0] ^= 0xff;
        assert!(verify_chunk(&root, 4, 1, &wrong_chunk, &proof).is_err());
    }

    #[test]
    fn verify_rejects_wrong_index() {
        let bytes = make_blob(4, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().unwrap();
        let proof = tree.prove(1).unwrap();
        let chunk = &bytes[CHUNK_SIZE..2 * CHUNK_SIZE];
        // Same chunk bytes and proof, but claimed at a different index.
        assert!(verify_chunk(&root, 4, 2, chunk, &proof).is_err());
    }

    #[test]
    fn verify_rejects_truncated_proof() {
        let bytes = make_blob(5, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().unwrap();
        let index = 3;
        let proof = tree.prove(index).unwrap();
        assert!(
            !proof.is_empty(),
            "5-chunk tree should need at least one sibling"
        );
        let truncated = &proof[..proof.len() - 1];
        let chunk_start = index * CHUNK_SIZE;
        let chunk = &bytes[chunk_start..chunk_start + CHUNK_SIZE];
        assert!(verify_chunk(&root, 5, index, chunk, truncated).is_err());
    }

    #[test]
    fn verify_rejects_over_long_proof() {
        let bytes = make_blob(5, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().unwrap();
        let index = 3;
        let mut proof = tree.prove(index).unwrap();
        proof.push([0x42; 32]);
        let chunk_start = index * CHUNK_SIZE;
        let chunk = &bytes[chunk_start..chunk_start + CHUNK_SIZE];
        assert!(verify_chunk(&root, 5, index, chunk, &proof).is_err());
    }

    #[test]
    fn verify_rejects_swapped_sibling_order() {
        let bytes = make_blob(4, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let root = tree.root().unwrap();
        let index = 0;
        let mut proof = tree.prove(index).unwrap();
        assert!(
            proof.len() >= 2,
            "4-chunk tree at index 0 has a two-level path"
        );
        proof.swap(0, 1);
        let chunk = &bytes[0..CHUNK_SIZE];
        assert!(verify_chunk(&root, 4, index, chunk, &proof).is_err());
    }

    #[test]
    fn verify_rejects_wrong_root() {
        let bytes = make_blob(3, CHUNK_SIZE);
        let tree = ChunkTree::from_bytes(&bytes);
        let wrong_root = [0x99; 32];
        let index = 0;
        let proof = tree.prove(index).unwrap();
        let chunk = &bytes[0..CHUNK_SIZE];
        assert!(verify_chunk(&wrong_root, 3, index, chunk, &proof).is_err());
    }

    #[test]
    fn leaf_and_node_hash_are_domain_separated() {
        let a = [0x11; 32];
        let b = [0x22; 32];
        let leaf = leaf_hash(&[0x11; 32]);
        let node = node_hash(&a, &b);
        assert_ne!(leaf, node);
    }
}
