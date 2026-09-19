//! Hydrostatic righting — the `GZ` curve of F6.7.
//!
//! ```text
//! GZ(φ)      = c1·sin φ + c2·sin 2φ + c3·sin 3φ
//! K_restore  = −Δ·g·GZ(φ)                          Δ = m_hull + m_sailor
//! ```
//!
//! Three odd harmonics: smooth, `2π`-periodic, mirror-symmetric, and defined
//! for every `φ` including past inversion, which is what brief §17 needs when
//! the boat rolls through `|φ| > 90°`. The human-facing tunables are `GM`,
//! `φ_p`, `GZ_max` and `φ_v`; the coefficients are solved from them once, by
//! [`GzCurve::fit`] or [`GzCurve::from_params`].
//!
//! ## The sign contract
//!
//! `φ > 0` is starboard down (F2). `GZ(φ) > 0` there, so `K_restore < 0`: a
//! moment about `+x_H` that rolls the boat back to port, toward upright. The
//! sign lives in [`righting_moment`] and nowhere else.
//!
//! ## Why every trigonometric identity is expanded by hand
//!
//! [`sines`] and [`cosines`] take one `sin_cos` and build the second and third
//! harmonics from the double- and triple-angle identities. Two properties
//! follow, and both are load-bearing:
//!
//! * Every term of `GZ` carries exactly one factor of `sin φ`. IEEE negation
//!   and multiplication are sign-symmetric and so is addition, so `gz(−φ)` is
//!   the **bit-exact** negation of `gz(φ)` — which is what `gz_is_odd` and,
//!   through it, the port/starboard mirror invariants assert.
//! * `φ` is never an operand of a multiplication anywhere in this module, so
//!   `tests/no_shortcuts.rs::no_linear_righting_spring` can be the plain
//!   source grep the section PRD specifies.

use crate::constants::G;
use crate::parameters::{BoatParameters, ParamError};

/// Interior samples [`GzCurve::fit`] uses to check the shape of a candidate
/// curve. Fine enough to see any wiggle a three-harmonic series can produce.
const SHAPE_SAMPLES: usize = 1024;

/// Below this determinant the F6.7 system is singular — `φ_p` and `φ_v` have
/// collapsed onto each other — and the solve carries no information.
const EPS_DET: f64 = 1e-12;

/// The righting-arm curve of F6.7, as solved coefficients.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GzCurve {
    c1: f64,
    c2: f64,
    c3: f64,
}

/// `(sin φ, sin 2φ, sin 3φ)`, from one `sin_cos`. Odd in `φ` bit for bit; see
/// the module note.
fn sines(phi: f64) -> (f64, f64, f64) {
    let (s, c) = phi.sin_cos();
    (s, 2.0 * s * c, s * (3.0 - 4.0 * s * s))
}

/// `(cos φ, cos 2φ, cos 3φ)`, from one `sin_cos`. Even in `φ`.
fn cosines(phi: f64) -> (f64, f64, f64) {
    let (s, c) = phi.sin_cos();
    (c, 1.0 - 2.0 * s * s, c * (4.0 * c * c - 3.0))
}

/// `3 × 3` determinant, written out. Cramer's rule on a matrix this small is
/// exact enough and allocation-free, and it keeps the solve deterministic
/// (F9): no pivoting means no data-dependent operation order.
fn det3(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

impl GzCurve {
    /// Solve the F6.7 system, with **no** shape validation.
    ///
    /// ```text
    /// c1 +  2c2 +  3c3                              = GM
    /// c1 sin φ_p + c2 sin 2φ_p + c3 sin 3φ_p        = GZ_max
    /// c1 sin φ_v + c2 sin 2φ_v + c3 sin 3φ_v        = 0
    /// ```
    ///
    /// A singular system yields the zero curve — no righting at all — rather
    /// than infinities in the coefficients. That is a degeneracy guard of the
    /// same standing as `foil::EPS_FLOW`: it keeps a hand-edited parameter set
    /// (brief §31 allows live edits, and `BoatParameters::set_path` does not
    /// re-validate) from putting a `NaN` into the state vector. It is
    /// deliberately *visible* — a boat with no righting falls over — rather
    /// than plausible. [`GzCurve::fit`] is the gate that stops such a set
    /// reaching here; section 08's parameter panel must call it.
    fn solve(gm: f64, phi_p: f64, gz_max: f64, phi_v: f64) -> Self {
        let (p1, p2, p3) = sines(phi_p);
        let (v1, v2, v3) = sines(phi_v);
        let a = [[1.0, 2.0, 3.0], [p1, p2, p3], [v1, v2, v3]];
        let d = det3(a);
        if d.abs() < EPS_DET {
            return Self::default();
        }
        let col = |k: usize, b: [f64; 3]| {
            let mut m = a;
            for (row, value) in m.iter_mut().zip(b) {
                row[k] = value;
            }
            det3(m) / d
        };
        let b = [gm, gz_max, 0.0];
        let out = Self {
            c1: col(0, b),
            c2: col(1, b),
            c3: col(2, b),
        };
        if out.c1.is_finite() && out.c2.is_finite() && out.c3.is_finite() {
            out
        } else {
            Self::default()
        }
    }

    /// Solve the F6.7 system and reject a parameter set whose curve is not
    /// physically sensible.
    ///
    /// Rejected:
    ///
    /// * `GM` or `GZ_max` not finite and positive, or `φ_p`, `φ_v` outside
    ///   `0 < φ_p < φ_v < π`;
    /// * a singular system;
    /// * a **sign change before `φ_v`** — `GZ` must stay positive on
    ///   `(0, φ_v)`, so the angle of vanishing stability is the first one
    ///   (F6.7, brief §16);
    /// * a `GZ` that is **not unimodal** on `(0, φ_v)` — it must rise to one
    ///   peak and then fall, with no dip in between.
    ///
    /// ### The unimodality rule, and why it is not strict monotonicity
    ///
    /// F6.7 words the second rule as "reject parameter sets that produce a
    /// non-monotonic `GZ` on `[0, φ_p]`", and in the same sentence states that
    /// "the peak is pinned in value but not exactly in location". **The two
    /// cannot both hold**: a peak anywhere below `φ_p` makes `GZ` fall between
    /// the peak and `φ_p`, which a strict monotonicity test rejects. With the
    /// F7 defaults the fitted peak is at 32.5°, 12.5° below `φ_p = 45°`, so a
    /// strict reading rejects the shipped boat — and task 7.1's
    /// `fit_reproduces_constraints` requires the F7 defaults to fit, while
    /// `gz_rises_then_falls` explicitly tolerates a peak up to 15° away from
    /// `φ_p`.
    ///
    /// Unimodality is the reading under which every one of those holds, and it
    /// still rejects the pathology the clause names: a curve that dips and
    /// climbs again on the way to the peak. The contradiction is recorded in
    /// `docs/progress/07-handoff.md` for human resolution; no convention was
    /// redefined here.
    pub fn fit(gm: f64, phi_p: f64, gz_max: f64, phi_v: f64) -> Result<Self, ParamError> {
        let range = |reason: String, field: &'static str| ParamError::OutOfRange { field, reason };
        if !(gm.is_finite() && gm > 0.0) {
            return Err(range(
                format!("metacentric height must be finite and positive, got {gm}"),
                "stability.gm",
            ));
        }
        if !(gz_max.is_finite() && gz_max > 0.0) {
            return Err(range(
                format!("maximum righting arm must be finite and positive, got {gz_max}"),
                "stability.gz_max",
            ));
        }
        if !(phi_p.is_finite() && phi_p > 0.0) {
            return Err(range(
                format!("require 0 < phi_peak, got {phi_p}"),
                "stability.phi_peak",
            ));
        }
        if !(phi_v.is_finite() && phi_v > phi_p && phi_v < std::f64::consts::PI) {
            return Err(range(
                format!("require phi_peak < phi_vanish < pi, got {phi_v}"),
                "stability.phi_vanish",
            ));
        }

        let curve = Self::solve(gm, phi_p, gz_max, phi_v);
        if curve == Self::default() {
            return Err(range(
                "the F6.7 system is singular at this phi_peak/phi_vanish pair".into(),
                "stability.phi_vanish",
            ));
        }

        // Shape, on the open interval `(0, φ_v)`. The endpoints are pinned by
        // construction: `GZ(0) = 0` identically and `GZ(φ_v) = 0` is the third
        // equation, so sampling them would only measure rounding.
        let mut rises = 0usize;
        let mut falls = 0usize;
        let mut previous = 0.0;
        for i in 1..SHAPE_SAMPLES {
            let phi = phi_v * (i as f64) / (SHAPE_SAMPLES as f64);
            let value = curve.gz(phi);
            if value <= 0.0 {
                return Err(range(
                    format!("GZ changes sign at {phi} rad, before phi_vanish = {phi_v}"),
                    "stability.phi_vanish",
                ));
            }
            if i > 1 {
                // A flat step is neither; only a genuine turn is counted.
                if value > previous && falls > 0 {
                    return Err(range(
                        format!("GZ dips and rises again near {phi} rad; the curve must rise to a single peak and then fall"),
                        "stability.phi_peak",
                    ));
                }
                if value > previous {
                    rises += 1;
                } else if value < previous {
                    falls += 1;
                }
            }
            previous = value;
        }
        if rises == 0 {
            return Err(range(
                "GZ never rises: the curve has no peak below phi_vanish".into(),
                "stability.gz_max",
            ));
        }
        Ok(curve)
    }

    /// The curve the equations of motion use, built from the live parameter
    /// catalogue. Unvalidated by design — see [`GzCurve::solve`].
    pub fn from_params(p: &BoatParameters) -> Self {
        let s = &p.stability;
        Self::solve(s.gm, s.phi_peak, s.gz_max, s.phi_vanish)
    }

    /// `GZ(φ)`, metres. Odd in `φ`, bit for bit.
    pub fn gz(&self, phi: f64) -> f64 {
        let (s1, s2, s3) = sines(phi);
        self.c1 * s1 + self.c2 * s2 + self.c3 * s3
    }

    /// `dGZ/dφ`, metres per radian. `dgz(0)` is `GM` by construction.
    pub fn dgz(&self, phi: f64) -> f64 {
        let (c1, c2, c3) = cosines(phi);
        self.c1 * c1 + 2.0 * self.c2 * c2 + 3.0 * self.c3 * c3
    }

    /// `∫₀^φ GZ(s) ds`, metre-radians — the shape of the roll potential.
    ///
    /// Multiplied by `Δ·g` it is the potential energy stored in heel, which is
    /// what makes `invariants::dissipative_with_roll` a complete energy
    /// account rather than a kinetic one.
    pub fn gz_integral(&self, phi: f64) -> f64 {
        let (c1, c2, c3) = cosines(phi);
        self.c1 * (1.0 - c1) + self.c2 * (1.0 - c2) / 2.0 + self.c3 * (1.0 - c3) / 3.0
    }

    /// `n` evenly spaced `(φ, GZ(φ))` pairs over `[0, π]`, for the parameter
    /// panel and the diagnostics plot (section 08). The curve is odd, so the
    /// negative half is the mirror of this one and is not transmitted.
    pub fn sample(&self, n: usize) -> Vec<(f64, f64)> {
        if n < 2 {
            return Vec::new();
        }
        (0..n)
            .map(|i| {
                let phi = std::f64::consts::PI * (i as f64) / ((n - 1) as f64);
                (phi, self.gz(phi))
            })
            .collect()
    }
}

/// `K_restore = −Δ·g·GZ(φ)` (F6.7, brief §16).
///
/// Positive heel (starboard down) gives a **negative** roll moment, rolling
/// the boat back to port. This one expression carries the whole sign contract;
/// `restoring_moment_sign` is its guard.
pub fn righting_moment(phi: f64, curve: &GzCurve, total_mass: f64) -> f64 {
    -(total_mass * G * curve.gz(phi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    fn default_curve() -> GzCurve {
        let s = params().stability;
        GzCurve::fit(s.gm, s.phi_peak, s.gz_max, s.phi_vanish).expect("the F7 defaults must fit")
    }

    /// A self-consistent set: `GM = 0.55 m` is where the fitted peak lands on
    /// `φ_p` itself for the F7 `GZ_max` and `φ_v`. See `gz_negative_beyond_vanishing`.
    fn consistent_curve() -> (GzCurve, f64) {
        let s = params().stability;
        (
            GzCurve::fit(0.55, s.phi_peak, s.gz_max, s.phi_vanish).expect("well-posed"),
            s.phi_vanish,
        )
    }

    #[test]
    fn fit_reproduces_constraints() {
        let s = params().stability;
        let g = default_curve();
        assert!((g.dgz(0.0) - s.gm).abs() < 1e-9, "dgz(0) = {}", g.dgz(0.0));
        assert!(
            g.gz(s.phi_vanish).abs() < 1e-9,
            "gz(phi_v) = {}",
            g.gz(s.phi_vanish)
        );
        assert!(
            (g.gz(s.phi_peak) - s.gz_max).abs() < 1e-9,
            "gz(phi_p) = {}",
            g.gz(s.phi_peak)
        );
    }

    #[test]
    fn gz_is_odd() {
        // Bit-identical, not merely close: the port/starboard mirror
        // invariants compare whole trajectories with `to_bits`, and a curve
        // that is only approximately odd breaks them in the last places.
        let g = default_curve();
        for i in 0..200 {
            let phi = -PI + 2.0 * PI * (i as f64) / 199.0;
            assert_eq!(
                g.gz(-phi).to_bits(),
                (-g.gz(phi)).to_bits(),
                "phi = {phi}: gz(-phi) = {}, -gz(phi) = {}",
                g.gz(-phi),
                -g.gz(phi)
            );
        }
    }

    #[test]
    fn gz_rises_then_falls() {
        let s = params().stability;
        let g = default_curve();
        let n = 200_000;
        let mut peak = (0.0f64, f64::NEG_INFINITY);
        let mut turns = 0usize;
        let mut previous = (0.0f64, 0.0f64);
        let mut rising = true;
        for i in 1..=n {
            let phi = s.phi_vanish * (i as f64) / (n as f64);
            let value = g.gz(phi);
            if i < n {
                assert!(
                    value > 0.0,
                    "gz({phi}) = {value} is not positive below phi_v"
                );
            }
            if value > peak.1 {
                peak = (phi, value);
            }
            if i > 1 {
                let up = value > previous.1;
                if rising && !up {
                    turns += 1;
                    rising = false;
                } else if !rising && up {
                    turns += 1;
                    rising = true;
                }
            }
            previous = (phi, value);
        }
        assert_eq!(turns, 1, "GZ must have exactly one interior maximum");

        // F6.7 pins the peak's *value*, not its location. Measured offset with
        // the F7 defaults: −12.5°, inside the ±15° the PRD allows but close
        // enough to it that the number is recorded in the section handoff.
        let offset = (peak.0 - s.phi_peak).to_degrees();
        assert!(
            offset.abs() <= 15.0,
            "peak at {:.2}° is {offset:.2}° from phi_peak; the parameter set is poorly posed",
            peak.0.to_degrees()
        );
    }

    #[test]
    fn gz_negative_beyond_vanishing() {
        // brief §16: "possible negative restoring moment after sufficient
        // capsize". This is a property of the *model*, and the PRD does not
        // pin this criterion to the F7 defaults — for good reason, because
        // they do not have it: see `f7_defaults_regain_positive_stability`.
        // `GM = 0.55 m` is the value at which the F7 `GZ_max` and `φ_v` are
        // self-consistent, i.e. the fitted peak lands on `φ_p`.
        let (g, phi_v) = consistent_curve();
        assert!(
            g.gz(phi_v + 0.2) < 0.0,
            "gz(phi_v + 0.2) = {}",
            g.gz(phi_v + 0.2)
        );
    }

    #[test]
    fn f7_defaults_regain_positive_stability() {
        // **A recorded defect in the F7 parameter set, not a property anyone
        // designed.** `GM = 1.00 m` is far too large to be consistent with
        // `GZ_max = 0.30 m` at 45°: a linear curve of that slope would already
        // read 0.79 m there. The three-harmonic fit resolves the conflict by
        // bending over early (peak at 32.5°), grazing zero at `φ_v`, and then
        // climbing back to +0.78 m at 140° — so beyond ≈ 82° of heel the
        // shipped boat is pushed back *upright* instead of further over.
        //
        // The curve does change sign at `φ_v`, as F6.7 requires, but only over
        // a 1.6°-wide window. This test states the measured facts so that the
        // day someone corrects `stability.gm` it fails and points here.
        // Recorded in full in `docs/progress/07-handoff.md`.
        let s = params().stability;
        let g = default_curve();

        let mut minimum = f64::INFINITY;
        for i in 0..=1000 {
            let phi = s.phi_vanish + 0.05 * (i as f64) / 1000.0;
            minimum = minimum.min(g.gz(phi));
        }
        assert!(
            minimum < 0.0,
            "no sign change at phi_v at all: min {minimum}"
        );
        assert!(
            g.gz(s.phi_vanish + 0.2) > 0.0,
            "the defect is fixed; re-read the test"
        );
        assert!(g.gz(2.44) > 0.7, "the defect is fixed; re-read the test");
    }

    #[test]
    fn restoring_moment_sign() {
        // F2/F6.7: `φ > 0` is starboard down, and the moment must roll the
        // boat back to port.
        let p = params();
        let g = default_curve();
        let m = p.total_mass();
        assert!(righting_moment(0.3, &g, m) < 0.0);
        assert!(righting_moment(-0.3, &g, m) > 0.0);
        // And exactly zero upright, with no rounding residue.
        assert_eq!(righting_moment(0.0, &g, m), 0.0);
    }

    #[test]
    fn not_linear_spring() {
        // brief §16 forbids a globally linear restoring spring. The secant
        // stiffness must vary by far more than measurement noise.
        let g = default_curve();
        let near = g.gz(0.1) / 0.1;
        let far = g.gz(0.8) / 0.8;
        let change = (far - near).abs() / near.abs();
        assert!(change > 0.20, "secant GZ/phi changed by only {change:.3}");
    }

    #[test]
    fn fit_rejects_bad_params() {
        assert!(GzCurve::fit(0.1, 1.4, 2.0, 0.5).is_err());
        // The other rejection paths, so the error is clean rather than a
        // silently inverted boat when section 08's panel drags a slider.
        assert!(GzCurve::fit(0.0, 0.785, 0.30, 1.396).is_err(), "GM = 0");
        assert!(
            GzCurve::fit(1.0, 0.785, -0.3, 1.396).is_err(),
            "negative GZ_max"
        );
        assert!(GzCurve::fit(1.0, 0.785, 0.30, 0.785).is_err(), "singular");
        assert!(
            GzCurve::fit(f64::NAN, 0.785, 0.30, 1.396).is_err(),
            "NaN GM"
        );
        assert!(
            GzCurve::fit(1.0, 0.785, 0.30, 4.0).is_err(),
            "phi_v beyond pi"
        );
    }

    #[test]
    fn anchor_value() {
        // The F7 anchor: `Δ·g·GZ_max ≈ 406 N·m` is the whole righting budget
        // of a boat whose sailor never hikes (brief §4, R2).
        let p = params();
        let anchor = p.total_mass() * G * p.stability.gz_max;
        assert!((anchor - 406.0).abs() <= 5.0, "anchor = {anchor} N.m");
    }

    #[test]
    fn sample_spans_the_curve() {
        let g = default_curve();
        let s = g.sample(181);
        assert_eq!(s.len(), 181);
        assert_eq!(s[0].0, 0.0);
        assert!((s[180].0 - PI).abs() < 1e-12);
        for &(phi, value) in &s {
            assert_eq!(value, g.gz(phi));
        }
        assert!(g.sample(1).is_empty());
    }

    #[test]
    fn gz_integral_matches_numeric_quadrature() {
        // The roll potential the energy invariant uses.
        let g = default_curve();
        for target in [-2.0, -0.5, 0.3, 1.4, 2.9] {
            let n = 200_000;
            let mut sum = 0.0;
            for i in 0..n {
                let a = target * (i as f64) / (n as f64);
                let b = target * ((i + 1) as f64) / (n as f64);
                sum += 0.5 * (g.gz(a) + g.gz(b)) * (b - a);
            }
            assert!(
                (g.gz_integral(target) - sum).abs() < 1e-9,
                "phi = {target}: {} vs {sum}",
                g.gz_integral(target)
            );
        }
    }
}
