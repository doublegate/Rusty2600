//! A cheap, deterministic 128-bit checksum over a save-state blob, used to
//! feed GGRS's built-in desync-detection machinery
//! (`GameStateCell::save`'s `checksum` parameter,
//! `SyncTestSession`'s own internal resimulate-and-compare check).
//!
//! Not cryptographic — collision resistance against an adversary is not the
//! goal here, only reliably catching an accidental state divergence between
//! two runs, which a low-quality-but-well-distributed hash already does. Two
//! independent 64-bit hashes (the blob, then the blob with a salt byte
//! appended) combined into one `u128` is cheap and avoids adding a hashing
//! crate dependency for something this small.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Hashes `blob` into a `u128` checksum.
#[must_use]
pub fn checksum128(blob: &[u8]) -> u128 {
    let mut h1 = DefaultHasher::new();
    blob.hash(&mut h1);
    let lo = h1.finish();

    let mut h2 = DefaultHasher::new();
    blob.hash(&mut h2);
    0xA5u8.hash(&mut h2); // a salt byte so h2 isn't just a copy of h1
    let hi = h2.finish();

    (u128::from(hi) << 64) | u128::from(lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_blobs_hash_identically() {
        assert_eq!(checksum128(b"hello"), checksum128(b"hello"));
    }

    #[test]
    fn different_blobs_hash_differently() {
        assert_ne!(checksum128(b"hello"), checksum128(b"world"));
    }
}
