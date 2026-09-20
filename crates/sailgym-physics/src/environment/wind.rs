//! Procedural wind field (`docs/v1/00-foundations.md` F6.1, brief §18).
//!
//! One field, three modes:
//!
//! - [`WindMode::Uniform`] — a constant vector `W0`.
//! - [`WindMode::Spatial`] — `W0` plus a frozen divergence-free perturbation.
//! - [`WindMode::Gust`] — the same perturbation, evolving in time.
//!
//! The perturbation is the curl of a stream function (F6.1):
//!
//! ```text
//! Ψ(p, t) = Σ_{k=1..K} A_k · sin( κ_k · p + ω_k t + ϕ_k )
//! w_pert  = ( ∂Ψ/∂y , −∂Ψ/∂x )
//! A_k     = A0 · |κ_k|^(−pow) / norm
//! ```
//!
//! Taking the curl of a stream function makes the field divergence-free **by
//! construction**, not to within a tolerance: mode `k` contributes
//! `a_k·cos(θ_k)·n̂_k` with `n̂_k ⟂ κ_k`, whose divergence is
//! `a_k·(−sin θ_k)·(κ_k · n̂_k) ≡ 0`. There are no sources or sinks anywhere in
//! the field, at any amplitude.
//!
//! `κ_k`, `ϕ_k` and `ω_k` are drawn once at construction from the wind stream
//! of the seeded RNG ([`crate::rng::STREAM_WIND`]). `K` is fixed, so
//! [`ProceduralWind::sample`] is `O(K)` and allocates nothing; no dense grid is
//! ever stored (brief §18).
//!
//! ## The single-field rule
//!
//! `sample` and `sample_grid` both call the private `sample_inner`, which is
//! the only implementation. The visualization and the physics therefore see
//! literally the same numbers (brief §19, §47), and
//! `tests::grid_matches_point` asserts exact `f32` equality rather than
//! approximate agreement.
//!
//! ## Configuration, not parameters
//!
//! [`WindConfig`] is *environment* configuration — a property of the scenario,
//! not of the boat — so it lives here rather than in the F7 catalogue of
//! `parameters.rs`, which describes the ILCA 7 alone. No value in F7 is
//! duplicated here.

use serde::{Deserialize, Serialize};

use super::{wind_from_bearing, WindField};
use crate::rng::{Pcg32, STREAM_WIND};
use crate::vec::Vec2;

/// Wavenumbers are drawn over `[κ0, MODE_BAND · κ0]`, where
/// `κ0 = 2π / length_scale`. The shortest structure in the field is therefore
/// `length_scale / MODE_BAND` across, which keeps the velocity gradient small
/// enough for the smoothness requirement of brief §18 while still giving the
/// field visible structure at several scales.
const MODE_BAND: f64 = 3.0;

/// RMS perturbation speed per unit `variation`, in units of `speed`.
///
/// `variation` is defined by the bound it implies: the perturbation must stay
/// inside `3 · variation · speed`, which is what `bounded_magnitude` asserts.
/// A sum of `K` modes with independent phases reaches roughly 3.1 × its RMS
/// over a wide sweep, so the RMS is set a little below `variation · speed` to
/// keep the observed peak inside that bound with margin, while leaving the
/// spread large enough to be a visible gust rather than a ripple.
///
/// This is a **definition of the configuration field**, not a physical
/// coefficient: it fixes what the number `variation` means and is not fitted
/// to any measurement (brief §43).
const RMS_PER_VARIATION: f64 = 0.88;

/// Gust frequencies are drawn over `[GUST_SPREAD_LO, GUST_SPREAD_HI] · ω0`,
/// where `ω0 = 2π / time_scale`, so the modes beat against one another instead
/// of pulsing in unison.
const GUST_SPREAD_LO: f64 = 0.5;
const GUST_SPREAD_HI: f64 = 1.5;

const TAU: f64 = std::f64::consts::TAU;
const INV_TAU: f64 = 1.0 / TAU;

/// Cody–Waite split of `2π`: `TAU_HI + TAU_LO + TAU_LO2 = 2π` to 40 digits,
/// with `TAU_HI` carrying only 26 significant bits so that `n · TAU_HI` is
/// **exact** for every `|n| < 2²⁶`.
///
/// Reducing as `θ − n·2π` in one step would lose a digit for every power of
/// ten in `θ`; at the far edge of the domain `finite_over_wide_domain` sweeps
/// (`|x| = 10⁵ m`) that is an error of order `1e-12` rad. The split keeps the
/// reduction accurate to the last bit of the argument instead.
const TAU_HI: f64 = 6.283_185_243_606_567;
const TAU_LO: f64 = 6.357_301_909_411_278e-8;
const TAU_LO2: f64 = 2.547_326_865_404_38e-24;

/// `3 · 2⁵¹` — the classic round-to-nearest-integer magic number for `f64`.
const ROUND_MAGIC: f64 = 6_755_399_441_055_744.0;

/// `cos θ`, evaluated without calling the platform's `libm`.
///
/// **Measured, not assumed.** `sample` is `O(K)` transcendental evaluations,
/// run once per visualization grid node per frame and — from section 05 — once
/// per force evaluation per RK stage, so the kernel is the whole cost of the
/// field. Both variants were built and timed:
///
/// | Build, same harness | `f64::cos` | this kernel |
/// |---|---|---|
/// | wasm32, Chrome, 128×128 grid | 4.1 ms | **2.4 ms** |
/// | native x86-64, 128×128 grid | 2.37 ms | **1.95 ms** |
/// | native x86-64, `sample` throughput | 10.0 M/s | **10.7 M/s** |
///
/// Natively it is worth a few per cent. In **WebAssembly there is no `cos`
/// instruction**: it compiles to a full Payne–Hanek reduction in Rust's own
/// `libm`, and there the kernel is 1.7× faster — the difference between
/// meeting the in-browser grid budget of task 3.3 and missing it. The browser
/// is the deployment target, so the browser decides.
///
/// A second, smaller benefit: `libm`'s `cos` is not bit-identical between
/// platforms or C runtimes. F9 claims bit-identity only for the same build on
/// the same platform, so this is not a correctness fix, but it removes one
/// avoidable source of drift from the section 09 golden trajectories (R7).
///
/// Cody–Waite reduction to `[−π, π]`, a fold onto `[0, π/2]` by
/// `cos(π − x) = −cos x`, then the Taylor series of `cos` truncated after
/// `x²⁰`, whose error on that interval is below `2e-17` — inside the rounding
/// of the result. `tests::wave_matches_cos` asserts agreement with `f64::cos`
/// to `1e-14` over the whole domain the field is sampled on.
///
/// Both seams are continuous: at `|r| = π/2` the two branches give the same
/// `x` and `cos(π/2) = 0`, and at `|r| = π` both give `−1`. No seam can appear
/// in the field's gradient, which `tests::smoothness` would catch.
#[inline(always)]
fn wave(theta: f64) -> f64 {
    // Round to the nearest integer without `round_ties_even`, which is a
    // *library call* on the baseline `x86-64` target this project builds for
    // (no SSE4.1, hence no `roundsd`) and cost more than the rest of the
    // kernel put together. Adding and subtracting 3·2⁵¹ forces the mantissa
    // to drop its fractional bits under the default rounding mode. Rust never
    // reassociates floating point, so the pair cannot be folded away.
    let n = (theta * INV_TAU + ROUND_MAGIC) - ROUND_MAGIC;
    let r = ((theta - n * TAU_HI) - n * TAU_LO) - n * TAU_LO2; // [−π, π]

    let a = r.abs();
    let folded = a > std::f64::consts::FRAC_PI_2;
    let folded_x = std::f64::consts::PI - a;
    // The clamp costs two instructions and makes `wave` total: an absurd
    // argument (beyond 2⁵¹ turns, where the rounding trick stops working)
    // still yields a value in [−1, 1] rather than an overflow to infinity.
    // brief §20 requires numerical stability over the whole world plane.
    let x = if folded { folded_x } else { a }.clamp(0.0, std::f64::consts::FRAC_PI_2);

    // cos x = Σ (−1)ⁿ x²ⁿ / (2n)!, in u = x². Evaluated by Estrin's scheme
    // rather than Horner: eleven Horner steps are a serial chain of eleven
    // dependent multiply-adds, and the sum runs one mode at a time, so the
    // loop ends up latency-bound. Estrin's tree is the same arithmetic with a
    // quarter of the depth, and measures roughly three times faster here.
    let u = x * x;
    let u2 = u * u;
    let u4 = u2 * u2;
    let u8 = u4 * u4;
    let a0 = (1.0 - 0.5 * u) + u2 * (4.166_666_666_666_666_4e-2 - 1.388_888_888_888_889e-3 * u);
    let a1 = (2.480_158_730_158_73e-5 - 2.755_731_922_398_589e-7 * u)
        + u2 * (2.087_675_698_786_81e-9 - 1.147_074_559_772_972_5e-11 * u);
    let a2 = (4.779_477_332_387_385e-14 - 1.561_920_696_858_622_5e-16 * u)
        + u2 * 4.110_317_623_312_165e-19;
    let p = (a0 + u4 * a1) + u8 * a2;
    if folded {
        -p
    } else {
        p
    }
}

/// Which of the three F6.1 modes the field is in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindMode {
    /// A constant vector everywhere, at all times.
    Uniform,
    /// Spatially varying, frozen in time (`ω_k = 0`).
    Spatial,
    /// Spatially varying and evolving (`ω_k ≠ 0`).
    #[default]
    Gust,
}

/// The scenario-level description of a wind field.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindConfig {
    pub mode: WindMode,
    /// m/s. Base speed.
    pub speed: f64,
    /// deg. Meteorological FROM direction, clockwise from north.
    pub bearing_deg: f64,
    /// `0..1`. Gust strength: the perturbation stays within
    /// `3 · variation · speed`. See [`RMS_PER_VARIATION`].
    pub variation: f64,
    /// m. Dominant spatial wavelength.
    pub length_scale: f64,
    /// s. Dominant gust period. `f64::INFINITY` freezes the field.
    pub time_scale: f64,
    /// `K`, the number of Fourier modes.
    pub modes: usize,
    /// `pow` in `A_k = A0·|κ_k|^(−pow)/norm`.
    pub spectral_slope: f64,
}

impl Default for WindConfig {
    /// A moderate, gusty westerly.
    ///
    /// `variation` bounds the perturbation at `3 · variation · speed`, so 0.15
    /// means gusts and lulls inside ±45 % of a 5 m/s breeze, with a standard
    /// deviation of roughly ±9 %.
    /// `speed` is deliberately modest: R2 records that an ILCA with the sailor
    /// fixed amidships is capsize-prone above ≈ 12 kn, so the default must not
    /// be a survival breeze. Section 09 overrides all of this per scenario.
    fn default() -> Self {
        Self {
            mode: WindMode::Gust,
            speed: 5.0,
            bearing_deg: 270.0,
            variation: 0.15,
            length_scale: 120.0,
            time_scale: 25.0,
            modes: 12,
            spectral_slope: 1.5,
        }
    }
}

/// Why a wind configuration was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindConfigError(pub &'static str);

impl std::fmt::Display for WindConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid wind configuration: {}", self.0)
    }
}

impl std::error::Error for WindConfigError {}

impl WindConfig {
    /// Rejects a configuration that would make the field non-finite. An
    /// infinite `time_scale` is explicitly allowed — it is how F6.1 freezes
    /// the temporal component.
    pub fn validate(&self) -> Result<(), WindConfigError> {
        if !self.speed.is_finite() || self.speed < 0.0 {
            return Err(WindConfigError("speed must be finite and non-negative"));
        }
        if !self.bearing_deg.is_finite() {
            return Err(WindConfigError("bearing_deg must be finite"));
        }
        if !self.variation.is_finite() || self.variation < 0.0 {
            return Err(WindConfigError("variation must be finite and non-negative"));
        }
        if !self.length_scale.is_finite() || self.length_scale <= 0.0 {
            return Err(WindConfigError("length_scale must be finite and positive"));
        }
        if self.time_scale.is_nan() || self.time_scale <= 0.0 {
            return Err(WindConfigError("time_scale must be positive"));
        }
        if !self.spectral_slope.is_finite() {
            return Err(WindConfigError("spectral_slope must be finite"));
        }
        Ok(())
    }
}

/// One Fourier mode, with the stream-function amplitude already folded into
/// the velocity amplitude vector.
///
/// `amp` is `a_k · n̂_k`, where `a_k = A_k·|κ_k|` and `n̂_k = (κ_y, −κ_x)/|κ_k|`
/// is the unit vector perpendicular to `κ_k`. Storing the product keeps
/// `sample` to a multiply-add per component and makes the perpendicularity —
/// and hence the zero divergence — a property of the stored data rather than
/// of the sampling loop.
#[derive(Clone, Copy, Debug)]
pub struct WindMode3 {
    k: Vec2,
    amp: Vec2,
    omega: f64,
    phase: f64,
}

/// The F6.1 procedural wind field.
#[derive(Clone, Debug)]
pub struct ProceduralWind {
    base: Vec2,
    modes: Vec<WindMode3>,
    cfg: WindConfig,
}

impl ProceduralWind {
    /// Build the field. All randomness comes from `seed` through the wind
    /// stream, so two fields built from the same `(cfg, seed)` are identical
    /// and adding an RNG consumer elsewhere cannot perturb this one.
    pub fn new(cfg: WindConfig, seed: u64) -> Self {
        let base = wind_from_bearing(cfg.speed, cfg.bearing_deg);

        // Uniform, a zero amplitude, or no modes at all: an empty mode list,
        // which makes `sample` return `base` bit-for-bit rather than
        // `base + 0.0` (which differs from `base` for a negative zero).
        let perturbed = cfg.mode != WindMode::Uniform
            && cfg.modes > 0
            && cfg.variation > 0.0
            && cfg.speed > 0.0;
        if !perturbed {
            return Self {
                base,
                modes: Vec::new(),
                cfg,
            };
        }

        let mut rng = Pcg32::seed_from_u64(seed).stream(STREAM_WIND);
        let k = cfg.modes;
        let k0 = TAU / cfg.length_scale;
        let omega0 = TAU / cfg.time_scale; // exactly 0 for an infinite time scale
        let temporal = cfg.mode == WindMode::Gust;

        // Wavenumber strata, shuffled so that a mode's direction sector does
        // not determine its scale. Without the shuffle the largest-amplitude
        // mode would always lie in the first direction sector, making the
        // field systematically anisotropic in world coordinates.
        let mut stratum: Vec<usize> = (0..k).collect();
        for i in (1..k).rev() {
            let j = (rng.next_f64() * (i + 1) as f64) as usize;
            stratum.swap(i, j);
        }

        // Draw every mode first, then normalise: the normalisation depends on
        // the whole drawn spectrum, and the draw order must not depend on it.
        //
        // Direction and wavenumber are **stratified** rather than drawn
        // independently: mode `i` takes direction sector `i` of `K` and
        // wavenumber octave `stratum[i]` of `K`, each jittered inside its own
        // cell. Independent draws leave the field's statistics badly
        // seed-dependent — clustered directions make the perturbation peaky in
        // one axis and flat in the other — and both `variation_scales` and
        // `bounded_magnitude` are then a lottery on the seed rather than a
        // property of the model.
        let mut modes: Vec<WindMode3> = Vec::with_capacity(k);
        for (i, s) in stratum.iter().enumerate() {
            let direction = TAU * (i as f64 + rng.next_f64()) / k as f64;
            // Log-uniform in |κ| so each octave of scale gets equal weight.
            let octave = (*s as f64 + rng.next_f64()) / k as f64;
            let magnitude = k0 * MODE_BAND.powf(octave);
            let phase = rng.range(0.0, TAU);
            let spread = rng.range(GUST_SPREAD_LO, GUST_SPREAD_HI);
            let (sin_d, cos_d) = direction.sin_cos();
            modes.push(WindMode3 {
                k: Vec2::new(magnitude * cos_d, magnitude * sin_d),
                // Provisional: the shape of the spectrum, scaled below.
                amp: Vec2::new(magnitude.powf(1.0 - cfg.spectral_slope), 0.0),
                omega: if temporal { omega0 * spread } else { 0.0 },
                phase,
            });
        }

        // `A_k = A0·|κ_k|^(−pow)/norm` gives a velocity amplitude
        // `a_k = A_k·|κ_k| = A0·|κ_k|^(1−pow)/norm`. With independent phases
        // the RMS perturbation speed is `sqrt(Σ a_k²/2)`, so choosing
        // `norm = sqrt(Σ |κ_k|^(2−2·pow) / 2)` makes that RMS exactly `A0`.
        let a0 = RMS_PER_VARIATION * cfg.variation * cfg.speed;
        let norm = (modes.iter().map(|m| m.amp.x * m.amp.x).sum::<f64>() / 2.0).sqrt();
        for m in &mut modes {
            let a = a0 * m.amp.x / norm;
            let unit = m.k.length();
            m.amp = Vec2::new(a * m.k.y / unit, -a * m.k.x / unit);
        }

        Self { base, modes, cfg }
    }

    /// The configuration this field was built from.
    pub fn config(&self) -> &WindConfig {
        &self.cfg
    }

    /// The number of Fourier modes actually in use. Zero for a uniform field.
    pub fn mode_count(&self) -> usize {
        self.modes.len()
    }

    /// **The only implementation.** `sample` and `sample_grid` both call it,
    /// which is what makes the displayed field and the simulated field the
    /// same field (brief §19).
    ///
    /// Allocation-free and `O(K)`; asserted by `tests::no_allocation_in_sample`.
    #[inline]
    fn sample_inner(&self, x: f64, y: f64, t: f64) -> Vec2 {
        if self.modes.is_empty() {
            return self.base;
        }
        // Two independent accumulator pairs rather than one. The kernel is
        // latency-bound — a chain of dependent multiply-adds with no FMA on
        // the baseline `x86-64` target — so interleaving two modes per
        // iteration buys about 15 %. The summation order is fixed and explicit
        // (F9.4); it must not be changed casually, because it is part of the
        // bit-exact result.
        let mut acc = [0.0f64; 4];
        let (pairs, tail) = self.modes.as_chunks::<2>();
        for c in pairs {
            let c0 = wave(c[0].k.x * x + c[0].k.y * y + c[0].omega * t + c[0].phase);
            let c1 = wave(c[1].k.x * x + c[1].k.y * y + c[1].omega * t + c[1].phase);
            acc[0] += c[0].amp.x * c0;
            acc[1] += c[0].amp.y * c0;
            acc[2] += c[1].amp.x * c1;
            acc[3] += c[1].amp.y * c1;
        }
        // An odd `K` leaves one mode over.
        for m in tail {
            let c = wave(m.k.x * x + m.k.y * y + m.omega * t + m.phase);
            acc[0] += m.amp.x * c;
            acc[1] += m.amp.y * c;
        }
        Vec2::new(
            self.base.x + (acc[0] + acc[2]),
            self.base.y + (acc[1] + acc[3]),
        )
    }
}

impl WindField for ProceduralWind {
    #[inline]
    fn sample(&self, x: f64, y: f64, t: f64) -> Vec2 {
        self.sample_inner(x, y, t)
    }

    #[allow(clippy::too_many_arguments)] // signature is normative, F6.1
    fn sample_grid(
        &self,
        x0: f64,
        y0: f64,
        dx: f64,
        dy: f64,
        nx: usize,
        ny: usize,
        t: f64,
        out: &mut [f32],
    ) {
        assert_eq!(
            out.len(),
            2 * nx * ny,
            "sample_grid: out.len() must be 2·nx·ny"
        );
        for j in 0..ny {
            let y = y0 + (j as f64) * dy;
            for i in 0..nx {
                let x = x0 + (i as f64) * dx;
                let w = self.sample_inner(x, y, t);
                let at = 2 * (j * nx + i);
                out[at] = w.x as f32;
                out[at + 1] = w.y as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::wind_to_bearing;

    /// A deterministic source of test points. Test-local; nothing here feeds
    /// the field, which draws only from its own seeded stream.
    struct Points(Pcg32);

    impl Points {
        fn new() -> Self {
            Self(Pcg32::seed_from_u64(0xC0FF_EE00))
        }
        fn next(&mut self, span: f64, t_span: f64) -> (f64, f64, f64) {
            (
                self.0.range(-span, span),
                self.0.range(-span, span),
                self.0.range(0.0, t_span),
            )
        }
    }

    fn cfg(mode: WindMode, variation: f64) -> WindConfig {
        WindConfig {
            mode,
            variation,
            ..WindConfig::default()
        }
    }

    #[test]
    fn uniform_is_constant() {
        let c = cfg(WindMode::Uniform, 0.4);
        let w = ProceduralWind::new(c, 12345);
        let expect = wind_from_bearing(c.speed, c.bearing_deg);
        let mut pts = Points::new();
        for _ in 0..1000 {
            let (x, y, t) = pts.next(5000.0, 1000.0);
            let got = w.sample(x, y, t);
            assert_eq!(got.x.to_bits(), expect.x.to_bits());
            assert_eq!(got.y.to_bits(), expect.y.to_bits());
        }
        assert_eq!(w.mode_count(), 0);
    }

    #[test]
    fn deterministic_from_seed() {
        let c = cfg(WindMode::Gust, 0.25);
        let a = ProceduralWind::new(c, 99);
        let b = ProceduralWind::new(c, 99);
        let other = ProceduralWind::new(c, 100);

        let mut pts = Points::new();
        let mut differed = false;
        for _ in 0..1000 {
            let (x, y, t) = pts.next(2000.0, 500.0);
            let wa = a.sample(x, y, t);
            let wb = b.sample(x, y, t);
            assert_eq!(wa.x.to_bits(), wb.x.to_bits());
            assert_eq!(wa.y.to_bits(), wb.y.to_bits());
            let wo = other.sample(x, y, t);
            differed |= wo.x != wa.x || wo.y != wa.y;
        }
        assert!(differed, "a different seed must give a different field");
    }

    #[test]
    fn grid_matches_point() {
        let w = ProceduralWind::new(cfg(WindMode::Gust, 0.3), 7);
        let (nx, ny) = (32usize, 32usize);
        let (x0, y0, dx, dy, t) = (-73.5, 41.25, 6.5, 4.0, 13.75);
        let mut out = vec![0.0f32; 2 * nx * ny];
        w.sample_grid(x0, y0, dx, dy, nx, ny, t, &mut out);

        for j in 0..ny {
            let y = y0 + (j as f64) * dy;
            for i in 0..nx {
                let x = x0 + (i as f64) * dx;
                let p = w.sample(x, y, t);
                let at = 2 * (j * nx + i);
                // Exact equality, no tolerance: this is the single-field
                // guarantee (brief §19, §47).
                assert_eq!(out[at].to_bits(), (p.x as f32).to_bits());
                assert_eq!(out[at + 1].to_bits(), (p.y as f32).to_bits());
            }
        }
    }

    #[test]
    fn divergence_free() {
        let c = cfg(WindMode::Gust, 0.35);
        let w = ProceduralWind::new(c, 4242);
        let h = 1e-4;
        let bound = 1e-6 * c.speed / c.length_scale;
        let mut pts = Points::new();
        let mut worst = 0.0f64;
        for _ in 0..500 {
            let (x, y, t) = pts.next(3000.0, 600.0);
            let dwx = (w.sample(x + h, y, t).x - w.sample(x - h, y, t).x) / (2.0 * h);
            let dwy = (w.sample(x, y + h, t).y - w.sample(x, y - h, t).y) / (2.0 * h);
            worst = worst.max((dwx + dwy).abs());
        }
        assert!(worst < bound, "divergence {worst:e} exceeds {bound:e}");
    }

    /// The velocity gradient stays bounded, so the field is "smooth enough to
    /// avoid numerical artifacts" (brief §18). Frobenius norm of the
    /// finite-difference Jacobian, over the default configuration.
    #[test]
    fn smoothness() {
        let c = WindConfig::default();
        let w = ProceduralWind::new(c, 31337);
        let h = 1e-3;
        let bound = 8.0 * c.speed / c.length_scale;
        let n = 200;
        let span = 2.0 * c.length_scale;
        let mut worst = 0.0f64;
        for j in 0..n {
            let y = -span + 2.0 * span * (j as f64) / (n as f64);
            for i in 0..n {
                let x = -span + 2.0 * span * (i as f64) / (n as f64);
                let ddx = (w.sample(x + h, y, 0.0) - w.sample(x - h, y, 0.0)) / (2.0 * h);
                let ddy = (w.sample(x, y + h, 0.0) - w.sample(x, y - h, 0.0)) / (2.0 * h);
                let frobenius =
                    (ddx.x * ddx.x + ddx.y * ddx.y + ddy.x * ddy.x + ddy.y * ddy.y).sqrt();
                worst = worst.max(frobenius);
            }
        }
        assert!(
            worst <= bound,
            "velocity gradient {worst:e} exceeds {bound:e} s^-1"
        );
    }

    #[test]
    fn variation_scales() {
        // variation = 0 makes Spatial identical to Uniform, bit for bit.
        let flat = ProceduralWind::new(cfg(WindMode::Spatial, 0.0), 11);
        let uniform = ProceduralWind::new(cfg(WindMode::Uniform, 0.0), 11);
        let mut pts = Points::new();
        for _ in 0..500 {
            let (x, y, t) = pts.next(1000.0, 200.0);
            let a = flat.sample(x, y, t);
            let b = uniform.sample(x, y, t);
            assert_eq!(a.x.to_bits(), b.x.to_bits());
            assert_eq!(a.y.to_bits(), b.y.to_bits());
        }

        // variation = 0.3 gives a speed standard deviation in the stated band.
        let c = cfg(WindMode::Spatial, 0.3);
        let w = ProceduralWind::new(c, 2024);
        let n = 200;
        let mut speeds = Vec::with_capacity(n * n);
        for j in 0..n {
            let y = -250.0 + 500.0 * (j as f64) / ((n - 1) as f64);
            for i in 0..n {
                let x = -250.0 + 500.0 * (i as f64) / ((n - 1) as f64);
                speeds.push(w.sample(x, y, 0.0).length());
            }
        }
        let mean = speeds.iter().sum::<f64>() / speeds.len() as f64;
        let var = speeds.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / speeds.len() as f64;
        let sd = var.sqrt();
        assert!(
            (0.15 * c.speed..=0.45 * c.speed).contains(&sd),
            "speed sd {sd} outside [{}, {}]",
            0.15 * c.speed,
            0.45 * c.speed
        );
    }

    #[test]
    fn gust_frozen_when_time_scale_infinite() {
        let c = WindConfig {
            mode: WindMode::Gust,
            variation: 0.3,
            time_scale: f64::INFINITY,
            ..WindConfig::default()
        };
        let w = ProceduralWind::new(c, 5);
        assert!(w.modes.iter().all(|m| m.omega == 0.0));
        let mut pts = Points::new();
        for _ in 0..500 {
            let (x, y, _) = pts.next(1000.0, 1.0);
            let a = w.sample(x, y, 0.0);
            for t in [1.0, 37.5, 1e4] {
                let b = w.sample(x, y, t);
                assert_eq!(a.x.to_bits(), b.x.to_bits());
                assert_eq!(a.y.to_bits(), b.y.to_bits());
            }
        }
    }

    /// `sample_inner` — the routine `sample` and `sample_grid` share — must
    /// not allocate: it runs once per particle-grid node per frame.
    #[test]
    fn no_allocation_in_sample() {
        let src = include_str!("wind.rs");
        let start = src
            .find("fn sample_inner")
            .expect("sample_inner must exist");
        let body = &src[start..];
        let end = body
            .find("\n    }\n")
            .expect("sample_inner must be a closed block");
        let body = &body[..end];
        for needle in ["Vec<", "Vec::", "vec!", "Box", ".collect", "String"] {
            assert!(
                !body.contains(needle),
                "`{needle}` in sample_inner — it must stay allocation-free"
            );
        }
    }

    #[test]
    fn spatial_and_gust_agree_at_t_zero_only_in_shape() {
        // A Spatial field is a Gust field with ω = 0; both must stay finite
        // and centred on the base vector.
        for mode in [WindMode::Spatial, WindMode::Gust] {
            let c = cfg(mode, 0.3);
            let w = ProceduralWind::new(c, 8);
            let (speed, bearing) = wind_to_bearing(w.base);
            assert!((speed - c.speed).abs() < 1e-12);
            assert!((bearing - c.bearing_deg).abs() < 1e-9);
            assert_eq!(w.mode_count(), c.modes);
        }
    }

    /// The hand-rolled kernel agrees with the platform `cos` to well inside
    /// the precision the field is used at, everywhere — including across both
    /// fold seams and far from the origin, where a naive reduction would drift.
    #[test]
    fn wave_matches_cos() {
        let mut worst = 0.0f64;
        let mut worst_at = 0.0f64;
        let mut check = |theta: f64| {
            let e = (wave(theta) - theta.cos()).abs();
            if e > worst {
                worst = e;
                worst_at = theta;
            }
        };
        // Dense sweep over several periods, hitting both seams exactly.
        let n = 200_000;
        for i in 0..=n {
            check(-6.0 * TAU + 12.0 * TAU * (i as f64) / (n as f64));
        }
        for q in [0.0, 0.25, -0.25, 0.5, -0.5, 1.0, -1.0] {
            check(TAU * q);
        }
        // Far field: |x| up to 1e5 m at the shortest wavelength in use.
        let mut rng = Pcg32::seed_from_u64(1);
        for _ in 0..200_000 {
            check(rng.range(-5.0e3, 5.0e3));
        }
        assert!(worst < 1e-14, "wave error {worst:e} at theta {worst_at}");

        // Exact at the origin, and even in theta.
        assert_eq!(wave(0.0), 1.0);
        assert_eq!(wave(TAU * 0.25), wave(-TAU * 0.25));
        assert!((wave(TAU * 0.5) + 1.0).abs() < 1e-15);
    }

    #[test]
    fn config_validation_rejects_the_impossible() {
        assert!(WindConfig::default().validate().is_ok());
        assert!(WindConfig {
            time_scale: f64::INFINITY,
            ..WindConfig::default()
        }
        .validate()
        .is_ok());
        for bad in [
            WindConfig {
                speed: -1.0,
                ..WindConfig::default()
            },
            WindConfig {
                length_scale: 0.0,
                ..WindConfig::default()
            },
            WindConfig {
                time_scale: 0.0,
                ..WindConfig::default()
            },
            WindConfig {
                variation: f64::NAN,
                ..WindConfig::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?} must be rejected");
        }
    }
}
