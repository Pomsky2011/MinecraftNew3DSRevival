//! `java.util.Random`: a 48-bit linear congruential generator, as specified in its Javadoc.
//! Every release from 1.13 uses it, and before 1.18 it is the only generator world generation has.

const MULTIPLIER: i64 = 0x5_DEEC_E66D;
const ADDEND: i64 = 0xB;
const MASK: i64 = (1 << 48) - 1;

/// `java.util.Random`, giving the same sequence as Java's for the same seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaRandom {
    seed: i64,
}

impl JavaRandom {
    /// `new Random(seed)`.
    pub fn new(seed: i64) -> Self {
        Self {
            seed: scramble(seed),
        }
    }

    /// `setSeed(seed)`. Java's also clears a cached `nextGaussian`, which isn't implemented yet.
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = scramble(seed);
    }

    /// `next(bits)`: the top `bits` bits (1 to 32) of the next state.
    pub fn next(&mut self, bits: u32) -> i32 {
        debug_assert!((1..=32).contains(&bits));
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// `nextInt()`.
    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// `nextInt(bound)`: uniform in `0..bound`.
    ///
    /// # Panics
    /// If `bound` isn't positive, where Java throws `IllegalArgumentException`.
    pub fn next_int_bounded(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive, was {bound}");
        let r = self.next(31);
        let m = bound - 1;
        if bound & m == 0 {
            // A power of two: the high bits, which are the more random ones.
            return ((i64::from(bound) * i64::from(r)) >> 31) as i32;
        }
        // Reject the top partial range so every value is equally likely. The check relies on
        // `u - r + m` overflowing negative, as Java's int arithmetic does.
        let mut u = r;
        loop {
            let r = u % bound;
            if u.wrapping_sub(r).wrapping_add(m) >= 0 {
                return r;
            }
            u = self.next(31);
        }
    }

    /// `nextLong()`. Only 2^48 of the 2^64 values can come out, as in Java.
    pub fn next_long(&mut self) -> i64 {
        let high = i64::from(self.next(32)) << 32;
        high.wrapping_add(i64::from(self.next(32)))
    }

    /// `nextBoolean()`.
    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    /// `nextFloat()`: uniform in `[0, 1)`, a multiple of 2^-24.
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    /// `nextDouble()`: uniform in `[0, 1)`, a multiple of 2^-53.
    pub fn next_double(&mut self) -> f64 {
        let high = i64::from(self.next(26)) << 27;
        let bits = high + i64::from(self.next(27));
        bits as f64 * (1.0 / (1_i64 << 53) as f64)
    }

    // TODO: nextGaussian. Java's uses StrictMath.log (fdlibm), which Rust's f64::ln doesn't
    // promise to match in the last bit; it waits for a StrictMath port (DESIGN.md §5.5).
}

/// The seed as `Random` stores it: XORed with the multiplier, cut to 48 bits.
fn scramble(seed: i64) -> i64 {
    (seed ^ MULTIPLIER) & MASK
}
