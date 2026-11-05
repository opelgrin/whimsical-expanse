//! Deterministic hashing.
//!
//! The standard library's `HashMap` and `HashSet` use a hasher seeded randomly
//! per process, so their iteration order changes from run to run. The types here
//! use a fixed hasher instead: the same operations give the same order on every
//! run of the same binary.
//!
//! That order is still arbitrary. It changes if insertions are reordered, so
//! when iteration order affects an outcome, prefer `BTreeMap`/`BTreeSet` or sort
//! explicitly. Use these maps for lookups.

use std::hash::{BuildHasherDefault, Hasher};

/// A fast, non-cryptographic hasher without a per-process seed.
///
/// This is the `FxHash` algorithm used inside the Rust compiler. It is not
/// resistant to collision attacks, which is irrelevant here since keys never
/// come from an untrusted source.
#[derive(Debug, Default, Clone, Copy)]
pub struct FixedHasher {
    hash: u64,
}

/// Multiplier of the `FxHash` algorithm.
const MULTIPLIER: u64 = 0x517c_c1b7_2722_0a95;

impl FixedHasher {
    fn add_word(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(MULTIPLIER);
    }
}

impl Hasher for FixedHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add_word(u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut word = [0_u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add_word(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add_word(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.add_word(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add_word(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add_word(value);
    }

    fn write_usize(&mut self, value: usize) {
        // Supported targets are 64-bit, so this conversion never fails.
        self.add_word(u64::try_from(value).expect("usize fits in u64 on supported targets"));
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

/// Builds [`FixedHasher`]s for the collections below.
pub type FixedBuildHasher = BuildHasherDefault<FixedHasher>;

/// A `HashMap` whose iteration order is the same on every run.
#[expect(
    clippy::disallowed_types,
    reason = "this alias is the sanctioned way to use HashMap, with a fixed hasher"
)]
pub type FixedHashMap<K, V> = std::collections::HashMap<K, V, FixedBuildHasher>;

/// A `HashSet` whose iteration order is the same on every run.
#[expect(
    clippy::disallowed_types,
    reason = "this alias is the sanctioned way to use HashSet, with a fixed hasher"
)]
pub type FixedHashSet<T> = std::collections::HashSet<T, FixedBuildHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{BuildHasher, Hash};

    fn hash_of<T: Hash>(value: &T) -> u64 {
        FixedBuildHasher::default().hash_one(value)
    }

    #[test]
    fn equal_values_hash_equally() {
        assert_eq!(hash_of(&"wolf"), hash_of(&String::from("wolf")));
        assert_eq!(hash_of(&(3_u32, -7_i64)), hash_of(&(3_u32, -7_i64)));
    }

    #[test]
    fn different_values_usually_hash_differently() {
        assert_ne!(hash_of(&"wolf"), hash_of(&"deer"));
        assert_ne!(hash_of(&1_u64), hash_of(&2_u64));
    }

    #[test]
    fn iteration_order_is_reproducible() {
        let build = || {
            let mut map = FixedHashMap::default();
            for key in 0..1_000_u32 {
                map.insert(key.wrapping_mul(2_654_435_761), key);
            }
            map.into_iter().collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
    }
}
