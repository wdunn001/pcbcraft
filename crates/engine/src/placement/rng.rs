//! The seeded random number generator every placement step shares.
//!
//! Context: global placement breaks equal distance ties with it and the
//! annealer draws every move and every acceptance from it. One tiny generator
//! with a pinned output stream keeps a run reproducible on every platform.
//!
//! Local rules:
//! - The algorithm is SplitMix64, whose published first outputs for seed 0 are
//!   `0xE220A8397B1DCDAF`, `0x6E789E6AA1B965F4` and `0x06C45D188009454F`.
//! - Any seed is valid, including 0, because the state is a counter and not a
//!   shift register that can get stuck on zero.
//! - Draw order is part of the contract. The golden vectors in
//!   `crates/engine/tests/fixtures/placement/` depend on it.
//!
//! Dependencies: none. No clock, no process id and no operating system source.

/// A SplitMix64 generator: 64 bits of state, one add and three mixing steps
/// per draw.
///
/// # Rules
/// - The same seed always gives the same stream, on every target.
/// - `next_f64` has 53 random bits and is in the range 0 (included) to 1 (excluded).
///
/// # Dependencies
/// - crates/engine/src/placement/anneal.rs (draws moves and acceptance)
/// - crates/engine/src/placement/legalize.rs (breaks equal distance ties)
///
/// # Do not
/// - Seed it from the clock or the process.
/// - Add a draw in the middle of a step without updating the golden vectors.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator that starts from `seed`.
    ///
    /// # Rules
    /// - Every `u64` is a valid seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 random bits.
    ///
    /// # Rules
    /// - Advances the state by the golden-ratio constant, then mixes it.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The next number in `[0, 1)` with 53 random bits.
    ///
    /// # Rules
    /// - The top 53 bits of one `next_u64` draw, times 2 to the power -53.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    /// The next whole number in `0..n`; 0 when `n` is 0.
    ///
    /// # Rules
    /// - One `next_u64` draw modulo `n`. The bias is below 2e-16 for any `n`
    ///   under 2000, which is far under what a placement can notice.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_zero_matches_the_published_splitmix64_stream() {
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn seed_one_and_the_float_and_range_helpers_are_pinned() {
        let mut r = Rng::new(1);
        assert_eq!(r.next_u64(), 0x910A_2DEC_8902_5CC1);
        let mut f = Rng::new(1);
        assert!((f.next_f64() - 0.566_561_575_172_280_9).abs() < 1e-15);
        let mut b = Rng::new(1);
        assert_eq!(b.below(8), (0x910A_2DEC_8902_5CC1_u64 % 8) as usize);
        assert_eq!(b.below(0), 0);
    }

    #[test]
    fn two_generators_with_one_seed_agree_and_floats_stay_below_one() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        for _ in 0..1000 {
            let x = a.next_f64();
            assert_eq!(x.to_bits(), b.next_f64().to_bits());
            assert!((0.0..1.0).contains(&x));
        }
    }
}
