//! Deterministic random numbers.
//!
//! Every random draw comes from a stream derived from the world seed and stable
//! keys, such as an entity and a purpose. Streams never depend on the order in
//! which threads run, so parallel work stays reproducible. There is no shared
//! global generator.

/// A pseudo-random generator: PCG32 (XSH RR variant).
///
/// Small, fast and statistically sound for simulation. Not cryptographic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rng {
    state: u64,
    increment: u64,
}

/// Multiplier of PCG's underlying linear congruential generator.
const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

impl Rng {
    /// Creates the generator for the stream identified by `seed` and `keys`.
    ///
    /// The same seed and keys always give the same sequence. Different keys give
    /// independent-looking sequences, even for neighbouring values.
    #[must_use]
    pub fn from_stream(seed: u64, keys: &[u64]) -> Self {
        let mut mixed = split_mix(seed);
        for &key in keys {
            mixed = split_mix(mixed ^ key);
        }
        Self::seeded(mixed, split_mix(mixed))
    }

    /// Standard PCG seeding (`pcg32_srandom_r` in the reference implementation):
    /// select the sequence, then advance past the initial state.
    fn seeded(initial_state: u64, sequence: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: (sequence << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(initial_state);
        rng.next_u32();
        rng
    }

    /// The next 32 random bits.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.increment);
        let shifted = ((old >> 18) ^ old) >> 27;
        let rotation = old >> 59;
        truncate_to_u32(shifted).rotate_right(truncate_to_u32(rotation))
    }

    /// A uniformly distributed number in `0..bound`, without modulo bias.
    ///
    /// # Panics
    ///
    /// Panics if `bound` is zero.
    pub fn below(&mut self, bound: u32) -> u32 {
        assert!(bound > 0, "the bound of a random range must be positive");
        // Reject the few values at the bottom of the range that would make some
        // results more likely than others.
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.next_u32();
            if value >= threshold {
                return value % bound;
            }
        }
    }

    /// A uniformly chosen index into a collection of `len` elements.
    ///
    /// # Panics
    ///
    /// Panics if `len` is zero or does not fit in 32 bits.
    pub fn index(&mut self, len: usize) -> usize {
        let bound =
            u32::try_from(len).expect("collections drawn from hold fewer than 2^32 elements");
        usize::try_from(self.below(bound)).expect("supported targets have 64-bit usize")
    }
}

/// Keeps the low 32 bits. PCG's output function takes them by design.
#[expect(
    clippy::cast_possible_truncation,
    reason = "PCG deliberately keeps the low 32 bits"
)]
const fn truncate_to_u32(value: u64) -> u32 {
    value as u32
}

/// The `SplitMix64` finalizer: turns related inputs into unrelated outputs.
const fn split_mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draws(rng: &mut Rng, count: usize) -> Vec<u32> {
        (0..count).map(|_| rng.next_u32()).collect()
    }

    /// Catches any deviation from PCG32, whose statistical quality we rely on.
    /// Expected values are the output of the reference implementation's demo
    /// program (pcg-c-basic, `pcg32_srandom_r(42, 54)`).
    #[test]
    fn matches_the_reference_pcg32() {
        let mut rng = Rng::seeded(42, 54);
        assert_eq!(
            draws(&mut rng, 6),
            [
                0xa15c_02b7,
                0x7b47_f409,
                0xba1d_3330,
                0x83d2_f293,
                0xbfa4_784b,
                0xcbed_606e
            ]
        );
    }

    /// Catches any deviation from `SplitMix64`, which keeps related keys (such
    /// as consecutive entity ids) from producing related streams. Expected
    /// value: the reference algorithm's first output for state 0.
    #[test]
    fn matches_the_reference_split_mix() {
        assert_eq!(split_mix(0), 0xe220_a839_7b1d_cdaf);
    }

    /// Catches streams that ignore their seed or one of their keys, which would
    /// make supposedly independent things (two entities, two purposes) move in
    /// lockstep.
    #[test]
    fn seed_and_every_key_select_a_different_stream() {
        let reference = draws(&mut Rng::from_stream(7, &[1, 2]), 16);
        assert_eq!(reference, draws(&mut Rng::from_stream(7, &[1, 2]), 16));
        assert_ne!(reference, draws(&mut Rng::from_stream(8, &[1, 2]), 16));
        assert_ne!(reference, draws(&mut Rng::from_stream(7, &[0, 2]), 16));
        assert_ne!(reference, draws(&mut Rng::from_stream(7, &[1, 3]), 16));
        assert_ne!(reference, draws(&mut Rng::from_stream(7, &[2, 1]), 16));
    }

    /// Catches off-by-one errors at either end of the range, and a range that
    /// silently loses values.
    #[test]
    fn below_covers_exactly_the_requested_range() {
        let mut rng = Rng::from_stream(42, &[]);
        for bound in [1, 2, 3, 7] {
            let mut seen = vec![false; usize::try_from(bound).expect("small bound")];
            for _ in 0..1_000 {
                let value = rng.below(bound);
                assert!(value < bound, "{value} is outside 0..{bound}");
                seen[usize::try_from(value).expect("small value")] = true;
            }
            assert!(
                seen.iter().all(|&hit| hit),
                "some values in 0..{bound} never came up"
            );
        }
    }
}
