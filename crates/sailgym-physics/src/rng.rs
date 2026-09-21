//! Deterministic PCG32, implemented in-crate (`docs/v1/00-foundations.md` F9.2).
//!
//! F9.2 forbids depending on an external RNG whose algorithm may change across
//! versions: a silent algorithm change would invalidate every golden
//! trajectory (R7) with no visible cause. The generator here is the published
//! PCG-XSH-RR 64/32 variant, and [`tests::pcg32_known_vector`] pins its output
//! forever. **If that test ever fails, the change is the defect — do not
//! update the expected vector.**
//!
//! ## Streams
//!
//! Every procedural consumer draws from its own *named stream* rather than
//! from a shared parent generator. Without that, adding one consumer in a
//! later section shifts every subsequent draw and silently changes existing
//! scenarios — a regression with no physical cause. [`Pcg32::stream`] takes
//! `&self`, so deriving a stream never advances the parent either.
//!
//! The stream labels are allocated once, here:
//!
//! | Label | Consumer | Section |
//! |---|---|---|
//! | [`STREAM_WIND`] | procedural wind field | 03 |
//! | [`STREAM_SCENARIO`] | scenario randomisation | reserved |
//! | [`STREAM_NOISE`] | sensor / disturbance noise | reserved |
//! | [`STREAM_AGENT`] | agent decisions, and the per-sensor substreams below them | v2 05 |

/// Stream label of the procedural wind field (`environment::wind`).
pub const STREAM_WIND: u64 = 1;
/// Reserved for scenario randomisation (section 09).
pub const STREAM_SCENARIO: u64 = 2;
/// Reserved for any later noise source.
pub const STREAM_NOISE: u64 = 3;
/// Agent decisions (v2 `docs/v2/00-foundations.md` F14.8, section 05).
///
/// `sailgym-agent` derives this stream from the episode's root generator and
/// derives one substream per sensor below it, keyed by the sensor's own id.
/// That is what lets a jittery agent — or a noisy sensor — be added without
/// shifting the wind field by a single bit, which is the whole point of the
/// table above.
pub const STREAM_AGENT: u64 = 4;

/// The PCG multiplier, from the reference implementation.
const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

/// Stream selector used by [`Pcg32::seed_from_u64`], from the reference
/// implementation's default initialiser. Any odd increment gives a valid
/// stream; this one is fixed so the sequence is reproducible.
const DEFAULT_SEQUENCE: u64 = 0xda3e_39cb_94b9_5bdb;

/// PCG-XSH-RR 64/32.
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    /// Seed the default stream.
    pub fn seed_from_u64(seed: u64) -> Self {
        Self::from_seed_and_sequence(seed, DEFAULT_SEQUENCE)
    }

    /// The reference `pcg32_srandom_r`: zero the state, set the (always odd)
    /// increment from the sequence selector, then mix the seed in.
    fn from_seed_and_sequence(seed: u64, sequence: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (sequence << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// Next 32 bits.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
        // XSH: xorshift the high bits down, then RR: rotate by the top 5 bits.
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in `[0, 1)`, with the full 53 bits of an `f64` mantissa.
    pub fn next_f64(&mut self) -> f64 {
        let hi = u64::from(self.next_u32() >> 5); // 27 bits
        let lo = u64::from(self.next_u32() >> 6); // 26 bits
        ((hi << 26) | lo) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }

    /// Derive an independent stream.
    ///
    /// Takes `&self`: deriving a stream must not advance the parent, or the
    /// isolation it exists to provide would be lost the moment two consumers
    /// were derived in a different order.
    pub fn stream(&self, label: u64) -> Pcg32 {
        Self::from_seed_and_sequence(self.state, label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the algorithm forever (R7).
    ///
    /// Two vectors, doing two different jobs:
    ///
    /// 1. `REFERENCE` is the output of the published PCG demo program for
    ///    `pcg32_srandom_r(&rng, 42, 54)`. It checks this implementation
    ///    against an **external** reference rather than against itself, so a
    ///    transcription error in the shift or rotate counts cannot pass.
    /// 2. `DEFAULT_STREAM` is what `seed_from_u64(42)` produces here. It pins
    ///    the default sequence selector as well as the algorithm.
    ///
    /// **If this test ever fails, the change is the defect.** Do not update
    /// either array: doing so silently invalidates every golden trajectory.
    #[test]
    fn pcg32_known_vector() {
        const REFERENCE: [u32; 8] = [
            0xa15c_02b7,
            0x7b47_f409,
            0xba1d_3330,
            0x83d2_f293,
            0xbfa4_784b,
            0xcbed_606e,
            0xbfc6_a3ad,
            0x812f_ff6d,
        ];
        let mut rng = Pcg32::from_seed_and_sequence(42, 54);
        let got: [u32; 8] = std::array::from_fn(|_| rng.next_u32());
        assert_eq!(got, REFERENCE, "the PCG32 algorithm changed — see F9.2");

        const DEFAULT_STREAM: [u32; 8] = [
            0x7130_66ea,
            0x3c7a_0d56,
            0xf424_216a,
            0x25c8_9145,
            0x43e7_ef3e,
            0x90cf_f60c,
            0x5232_0591,
            0x53df_bcb8,
        ];
        let mut rng = Pcg32::seed_from_u64(42);
        let got: [u32; 8] = std::array::from_fn(|_| rng.next_u32());
        assert_eq!(got, DEFAULT_STREAM, "the default stream changed");
    }

    #[test]
    fn pcg32_uniformity() {
        let mut rng = Pcg32::seed_from_u64(42);
        let n = 100_000;
        let mut sum = 0.0;
        for _ in 0..n {
            let u = rng.next_f64();
            assert!((0.0..1.0).contains(&u), "next_f64 out of [0, 1): {u}");
            sum += u;
        }
        let mean = sum / f64::from(n);
        assert!((0.495..=0.505).contains(&mean), "mean {mean} off centre");
    }

    #[test]
    fn streams_independent() {
        let parent = Pcg32::seed_from_u64(7);

        let draw = |rng: &mut Pcg32| -> Vec<u32> { (0..16).map(|_| rng.next_u32()).collect() };

        let a = draw(&mut parent.stream(1));
        let b = draw(&mut parent.stream(2));
        assert_ne!(a, b, "stream(1) and stream(2) must differ");

        // Reproducible from the same parent, and deriving one stream does not
        // disturb the parent or any other stream.
        let a_again = draw(&mut parent.stream(1));
        assert_eq!(a, a_again);
        let b_again = draw(&mut parent.stream(2));
        assert_eq!(b, b_again);
    }

    #[test]
    fn range_is_within_bounds_and_ordered() {
        let mut rng = Pcg32::seed_from_u64(3);
        for _ in 0..10_000 {
            let v = rng.range(-2.0, 5.0);
            assert!((-2.0..5.0).contains(&v), "range out of bounds: {v}");
        }
    }

    #[test]
    fn stream_labels_are_distinct() {
        // Every label against every other, so adding one cannot collide with
        // an existing one and be noticed only as a reproducibility bug.
        let labels = [
            ("STREAM_WIND", STREAM_WIND),
            ("STREAM_SCENARIO", STREAM_SCENARIO),
            ("STREAM_NOISE", STREAM_NOISE),
            ("STREAM_AGENT", STREAM_AGENT),
        ];
        for (i, (a_name, a)) in labels.iter().enumerate() {
            for (b_name, b) in labels.iter().skip(i + 1) {
                assert_ne!(a, b, "{a_name} and {b_name} are the same stream");
            }
        }

        // …and distinct labels really do give distinct sequences, which is the
        // property the table is claiming (F9.2).
        let parent = Pcg32::seed_from_u64(1234);
        let draws: Vec<Vec<u32>> = labels
            .iter()
            .map(|(_, label)| {
                let mut s = parent.stream(*label);
                (0..8).map(|_| s.next_u32()).collect()
            })
            .collect();
        for (i, a) in draws.iter().enumerate() {
            for b in draws.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }
}
