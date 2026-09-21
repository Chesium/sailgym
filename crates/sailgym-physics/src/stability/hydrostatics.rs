//! Hydrostatic righting — the `GZ` curve of F6.7, as corrected by v2 F18.1a.
//!
//! ```text
//! GZ(φ)      = Σ_{n=1..4} c_n · sin(n φ)
//! K_restore  = −Δ·g·GZ(φ)                          Δ = m_hull + m_sailor
//! ```
//!
//! Four odd harmonics: smooth, `2π`-periodic, mirror-symmetric, and defined for
//! every `φ` including past inversion, which is what brief §17 needs when the
//! boat rolls through `|φ| > 90°`. The human-facing tunables are unchanged —
//! `GM`, `φ_p`, `GZ_max` and `φ_v` — and the coefficients are solved from them
//! once, by [`GzCurve::fit`], [`GzCurve::fit_catalogue`] or
//! [`GzCurve::from_params`].
//!
//! ## Why four and not three (v2 F18.1a, D1)
//!
//! v1's three-harmonic system constrained the slope at the origin, the
//! **value** at `φ_p` and the zero at `φ_v`. It never said `φ_p` was a
//! stationary point, and with the F7 defaults it was not: the measured slope
//! there was `−0.475 m/rad`, the real peak sat 12.5° lower at 18 % above
//! `GZ_max`, and past 81.6° of heel the curve came back **positive**, rising to
//! `+0.785 m` at 142° — a boat pushed upright by the model after it had
//! capsized. The numbers are in `docs/v2/physics-validation.md` §1.
//!
//! The fourth harmonic buys the missing constraint, `GZ'(φ_p) = 0`, and with it
//! the peak's location. Four independent linear constraints need four
//! coefficients; this is the smallest odd series that carries them.
//!
//! ## The sign contract
//!
//! `φ > 0` is starboard down (F2). `GZ(φ) > 0` there, so `K_restore < 0`: a
//! moment about `+x_H` that rolls the boat back to port, toward upright. The
//! sign lives in [`righting_moment`] and nowhere else.
//!
//! ## Why the harmonics are built by recurrence
//!
//! [`sines`] and [`cosines`] take one `sin_cos` and climb with the Chebyshev
//! recurrences `sin nφ = 2 cos φ · sin(n−1)φ − sin(n−2)φ` and
//! `cos nφ = 2 cos φ · cos(n−1)φ − cos(n−2)φ`. Two properties follow, and both
//! are load-bearing:
//!
//! * Every term of `GZ` carries exactly one factor of `sin φ`, and IEEE
//!   negation, multiplication and subtraction are all sign-symmetric, so
//!   `gz(−φ)` is the **bit-exact** negation of `gz(φ)` — which is what
//!   `gz_is_odd` and, through it, the port/starboard mirror invariants assert.
//!   The recurrence preserves it by induction: a difference of two exactly-odd
//!   quantities is exactly odd.
//! * `φ` is never an operand of a multiplication anywhere in this module, so
//!   `tests/no_shortcuts.rs::no_linear_righting_spring` can be the plain
//!   source grep the section PRD specifies.
//!
//! ## One curve, one derivative, one integral
//!
//! [`GzCurve::gz`], [`GzCurve::dgz`] and [`GzCurve::gz_integral`] are the same
//! coefficients, their analytic derivative and their analytic integral. There
//! is no separate energy approximation beside the force, which is what makes
//! `invariants::dissipative_with_roll` a complete account rather than a
//! plausible one.

use serde::{Deserialize, Serialize};

use crate::constants::G;
use crate::parameters::{BoatParameters, ParamError};

/// Number of odd harmonics in the series (v2 F18.1a). Four constraints need
/// four coefficients; nothing outside this module assumes the count.
pub const HARMONICS: usize = 4;

/// Interior samples [`GzCurve::fit`] uses on each of `(0, φ_v)` and
/// `(φ_v, π)`. Fine enough to see any wiggle a four-harmonic series can
/// produce: the series cannot turn more than 4 times on `(0, π)`, and this is
/// three orders of magnitude finer than that.
const SHAPE_SAMPLES: usize = 2048;

/// A pivot smaller than this fraction of the largest entry in the constraint
/// matrix means the F18.1a system is singular — `φ_p` and `φ_v` have collapsed
/// onto each other, or onto the origin — and the solve carries no information.
const EPS_PIVOT: f64 = 1e-12;

/// Rounding headroom on the "no sample exceeds `GZ_max`" test. The peak's
/// value is pinned by construction to within a few ULP of `GZ_max`, so any
/// sample above this is shape, not rounding.
const EPS_PEAK: f64 = 1e-12;

/// The form [`GzRepresentation`] declares, and the only one this build reads.
///
/// A string rather than an enum on purpose: a port that meets an unknown form
/// must be able to say *which* form it did not recognise, and a stale bundle
/// generated under a different representation has to fail loudly rather than
/// be silently reinterpreted as this one.
pub const GZ_FORM: &str = "odd_sine_harmonics";

/// The version of the exported representation's **schema** — the field set of
/// [`GzRepresentation`] and the meaning of [`GZ_FORM`].
///
/// Bumped by hand when a reader written against the previous version would
/// misread the record. It is not the coefficient count: the count travels as
/// data, and no consumer may assume it (v2 F18.1a).
pub const GZ_REPRESENTATION_VERSION: u32 = 1;

/// The solved righting-arm curve, exported as read-only versioned data
/// (section 02 task 2.3, v2 F16.4).
///
/// Enough to evaluate `GZ`, its derivative and its integral under the
/// identified model, and nothing more. Three properties are deliberate:
///
/// * **The coefficient count is data.** v1 had three harmonics and v2 F18.1a
///   has four; a bundle, a port or a manifest that hard-codes either is wrong
///   the next time the representation is corrected. `coefficients.len()` is
///   the count, and [`GzCurve::from_representation`] refuses a length this
///   build cannot evaluate rather than padding or truncating.
/// * **`form` says what the numbers mean.** `odd_sine_harmonics` is
///   `GZ(φ) = Σ_{n=1..N} c_n · sin(n φ)`, with the analytic derivative
///   `Σ n·c_n·cos(n φ)` and the analytic integral
///   `Σ c_n·(1 − cos(n φ))/n`. All three are one set of coefficients, never
///   three approximations.
/// * **No tunable travels.** `GM`, `φ_p`, `GZ_max` and `φ_v` are the *inputs*
///   to the fit, and the fit's shape rules are this crate's business. What a
///   second implementation needs is the solved curve; giving it the tunables
///   instead would make it re-solve a 4×4 system and compare a different
///   number.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GzRepresentation {
    /// [`GZ_REPRESENTATION_VERSION`].
    pub version: u32,
    /// [`GZ_FORM`].
    pub form: String,
    /// `c_1 … c_N`, in ascending harmonic order. `N` is the length.
    pub coefficients: Vec<f64>,
}

/// The righting-arm curve of F6.7 / v2 F18.1a, as solved coefficients.
///
/// Serialisable so that section 02's conformance bundle can carry the resolved
/// curve as data rather than re-deriving it, and without any consumer assuming
/// how many coefficients there are — see [`GzCurve::coefficients`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GzCurve {
    c: [f64; HARMONICS],
}

/// `(sin φ, sin 2φ, …, sin Nφ)`, from one `sin_cos`. Odd in `φ` bit for bit;
/// see the module note.
fn sines(phi: f64) -> [f64; HARMONICS] {
    let (s, c) = phi.sin_cos();
    let mut out = [0.0; HARMONICS];
    out[0] = s;
    if HARMONICS > 1 {
        out[1] = 2.0 * s * c;
    }
    for k in 2..HARMONICS {
        out[k] = 2.0 * c * out[k - 1] - out[k - 2];
    }
    out
}

/// `(cos φ, cos 2φ, …, cos Nφ)`, from one `sin_cos`. Even in `φ` bit for bit.
fn cosines(phi: f64) -> [f64; HARMONICS] {
    let (s, c) = phi.sin_cos();
    let mut out = [0.0; HARMONICS];
    out[0] = c;
    if HARMONICS > 1 {
        out[1] = 1.0 - 2.0 * s * s;
    }
    for k in 2..HARMONICS {
        out[k] = 2.0 * c * out[k - 1] - out[k - 2];
    }
    out
}

/// Gaussian elimination with partial pivoting, on the fixed `HARMONICS` size.
///
/// Cramer's rule was adequate for the 3×3 system; at 4×4 the constraint rows
/// differ in magnitude by more than two decades (`[1, 2, 3, 4]` beside
/// `sin nφ_v`), so pivoting is what keeps the solve accurate. It stays
/// deterministic in F9's sense — this is a pure function of its arguments, and
/// identical arguments take identical pivots on the same build — and it runs
/// once at parameter-build time, never inside a step.
fn solve(
    mut a: [[f64; HARMONICS]; HARMONICS],
    mut b: [f64; HARMONICS],
) -> Option<[f64; HARMONICS]> {
    let scale = a
        .iter()
        .flat_map(|row| row.iter())
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    if scale <= 0.0 || !scale.is_finite() {
        return None;
    }
    for col in 0..HARMONICS {
        let mut pivot = col;
        for row in col + 1..HARMONICS {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        if a[pivot][col].abs() < EPS_PIVOT * scale {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..HARMONICS {
            let factor = a[row][col] / a[col][col];
            let pivot_row = a[col];
            for (target, source) in a[row].iter_mut().zip(pivot_row).skip(col) {
                *target -= factor * source;
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = [0.0; HARMONICS];
    for i in (0..HARMONICS).rev() {
        let mut sum = b[i];
        for k in i + 1..HARMONICS {
            sum -= a[i][k] * x[k];
        }
        x[i] = sum / a[i][i];
    }
    if x.iter().all(|v| v.is_finite()) {
        Some(x)
    } else {
        None
    }
}

impl GzCurve {
    /// Solve the F18.1a system, with **no** shape validation.
    ///
    /// ```text
    /// Σ n·c_n              = GM        slope at the origin
    /// Σ c_n sin(n φ_p)     = GZ_max    the peak's value
    /// Σ n·c_n cos(n φ_p)   = 0         the peak is a stationary point
    /// Σ c_n sin(n φ_v)     = 0         the vanishing angle
    /// ```
    ///
    /// A singular system yields the zero curve — no righting at all — rather
    /// than infinities in the coefficients. That is a degeneracy guard of the
    /// same standing as `foil::EPS_FLOW`: it keeps a hand-edited parameter set
    /// (brief §31 allows live edits, and `BoatParameters::set_path` does not
    /// re-validate) from putting a `NaN` into the state vector. It is
    /// deliberately *visible* — a boat with no righting falls over — rather
    /// than plausible. [`GzCurve::fit`] is the gate that stops such a set
    /// reaching here, and every path that admits parameters from outside the
    /// crate calls it.
    fn solve_unchecked(gm: f64, phi_p: f64, gz_max: f64, phi_v: f64) -> Self {
        let sp = sines(phi_p);
        let cp = cosines(phi_p);
        let sv = sines(phi_v);
        let mut a = [[0.0; HARMONICS]; HARMONICS];
        for (k, ((sp, cp), sv)) in sp.iter().zip(cp.iter()).zip(sv.iter()).enumerate() {
            let n = (k + 1) as f64;
            a[0][k] = n;
            a[1][k] = *sp;
            a[2][k] = n * cp;
            a[3][k] = *sv;
        }
        match solve(a, [gm, gz_max, 0.0, 0.0]) {
            Some(c) => Self { c },
            None => Self::default(),
        }
    }

    /// Solve the F18.1a system and reject a parameter set whose curve is not
    /// physically sensible.
    ///
    /// `gz_limit` is the geometric envelope of rule 6 below — half the hull
    /// beam, supplied by [`GzCurve::fit_catalogue`].
    ///
    /// Rejected, with the offending field named:
    ///
    /// 1. `GM`, `GZ_max` or `gz_limit` not finite and positive, or
    ///    `0 < φ_p < φ_v < π` violated;
    /// 2. a singular system;
    /// 3. `GZ ≤ 0` anywhere on `(0, φ_v)` — the angle of vanishing stability
    ///    must be the **first** zero (F6.7, brief §16);
    /// 4. `GZ` not unimodal on `(0, φ_v)`, or a sample above `GZ_max` — the
    ///    named peak must be the actual maximum;
    /// 5. `GZ ≥ 0` anywhere on `(φ_v, π)` — no positive stability between the
    ///    vanishing angle and inversion;
    /// 6. `|GZ|` above `gz_limit` anywhere on `[0, π]` — a righting arm is the
    ///    horizontal separation of two points inside the hull, so a curve
    ///    outside that envelope describes a boat that does not exist.
    ///
    /// ### Rule 4 closes v1's recorded contradiction
    ///
    /// F6.7 asked for "reject parameter sets that produce a non-monotonic `GZ`
    /// on `[0, φ_p]`" and, in the same breath, "the peak is pinned in value but
    /// not exactly in location". While the location was free the two could not
    /// both hold, and `docs/v1/progress/07-handoff.md` recorded the
    /// contradiction rather than picking a side. F18.1a's fourth constraint
    /// pins the location, so monotonicity on `[0, φ_p]` and unimodality on
    /// `(0, φ_v)` now say the same thing and the contradiction is closed.
    pub fn fit(
        gm: f64,
        phi_p: f64,
        gz_max: f64,
        phi_v: f64,
        gz_limit: f64,
    ) -> Result<Self, ParamError> {
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
        if !(gz_limit.is_finite() && gz_limit > 0.0) {
            return Err(range(
                format!("the righting-arm envelope must be finite and positive, got {gz_limit}"),
                "hull.beam",
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

        let curve = Self::solve_unchecked(gm, phi_p, gz_max, phi_v);
        if curve == Self::default() {
            return Err(range(
                "the F18.1a system is singular at this phi_peak/phi_vanish pair".into(),
                "stability.phi_vanish",
            ));
        }

        // Rules 3 and 4, on the open interval `(0, φ_v)`. The endpoints are
        // pinned by construction — `GZ(0) = 0` identically and `GZ(φ_v) = 0` is
        // the fourth equation — so sampling them would only measure rounding.
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
            if value > gz_max + EPS_PEAK {
                return Err(range(
                    format!(
                        "GZ reaches {value} m at {phi} rad, above gz_max = {gz_max} m at \
                         phi_peak = {phi_p} rad: the named peak is not the maximum"
                    ),
                    "stability.gm",
                ));
            }
            if i > 1 {
                // A flat step is neither; only a genuine turn is counted.
                if value > previous && falls > 0 {
                    return Err(range(
                        format!("GZ dips and rises again near {phi} rad; the curve must rise to a single peak and then fall"),
                        "stability.gm",
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
        if falls == 0 {
            return Err(range(
                "GZ never falls below phi_vanish: the peak is not interior".into(),
                "stability.phi_peak",
            ));
        }

        // Rule 5, on the open interval `(φ_v, π)`. `GZ(π) = 0` for any odd
        // series, so the endpoint is excluded for the same reason as above.
        let span = std::f64::consts::PI - phi_v;
        for i in 1..SHAPE_SAMPLES {
            let phi = phi_v + span * (i as f64) / (SHAPE_SAMPLES as f64);
            if curve.gz(phi) >= 0.0 {
                return Err(range(
                    format!(
                        "GZ is {} m at {phi} rad: the boat regains positive stability between \
                         phi_vanish = {phi_v} rad and inversion",
                        curve.gz(phi)
                    ),
                    "stability.gm",
                ));
            }
        }

        // Rule 6, over the whole supported half-domain `[0, π]`.
        for i in 0..=SHAPE_SAMPLES {
            let phi = std::f64::consts::PI * (i as f64) / (SHAPE_SAMPLES as f64);
            let value = curve.gz(phi);
            if value.abs() > gz_limit {
                return Err(range(
                    format!(
                        "GZ reaches {value} m at {phi} rad, outside the geometric envelope of \
                         {gz_limit} m: a righting arm is a horizontal lever between two points \
                         inside the hull"
                    ),
                    "stability.gm",
                ));
            }
        }

        Ok(curve)
    }

    /// [`GzCurve::fit`] for a whole catalogue: the one entry point every
    /// consumer uses, so the geometric envelope of rule 6 is derived in exactly
    /// one place (v2 F18.1a).
    pub fn fit_catalogue(p: &BoatParameters) -> Result<Self, ParamError> {
        let s = &p.stability;
        Self::fit(
            s.gm,
            s.phi_peak,
            s.gz_max,
            s.phi_vanish,
            Self::gz_envelope(p),
        )
    }

    /// Half the hull beam: the geometric envelope of rule 6.
    pub fn gz_envelope(p: &BoatParameters) -> f64 {
        0.5 * p.hull.beam
    }

    /// The curve the equations of motion use, built from the live parameter
    /// catalogue. Unvalidated by design — see [`GzCurve::solve_unchecked`].
    pub fn from_params(p: &BoatParameters) -> Self {
        let s = &p.stability;
        Self::solve_unchecked(s.gm, s.phi_peak, s.gz_max, s.phi_vanish)
    }

    /// The resolved coefficients, `c_1 … c_N`, for serialisation and for
    /// section 02's conformance bundle. The length is [`HARMONICS`]; no caller
    /// may assume a particular value for it.
    pub fn coefficients(&self) -> [f64; HARMONICS] {
        self.c
    }

    /// The curve as read-only versioned data (section 02 task 2.3).
    ///
    /// The coefficients are copied out, so the exported record cannot be a
    /// window onto a live curve; nothing here re-derives anything.
    pub fn representation(&self) -> GzRepresentation {
        GzRepresentation {
            version: GZ_REPRESENTATION_VERSION,
            form: GZ_FORM.to_string(),
            coefficients: self.c.to_vec(),
        }
    }

    /// Rebuild a curve from [`GzCurve::representation`].
    ///
    /// **No shape validation, deliberately.** `fit` is the gate that decides
    /// whether a *parameter set* describes a boat; this reproduces a curve
    /// that was already admitted, exactly as it was written — including one
    /// generated under a different (earlier or later) set of shape rules,
    /// which is the whole point of a conformance fixture. What it does refuse
    /// is a record it cannot evaluate faithfully: an unknown version, an
    /// unknown form, the wrong number of coefficients, or a non-finite one.
    pub fn from_representation(r: &GzRepresentation) -> Result<Self, ParamError> {
        let reject = |reason: String| ParamError::OutOfRange {
            field: "stability.gz_representation",
            reason,
        };
        if r.version != GZ_REPRESENTATION_VERSION {
            return Err(reject(format!(
                "representation version {} was written by another schema; this build reads {}",
                r.version, GZ_REPRESENTATION_VERSION
            )));
        }
        if r.form != GZ_FORM {
            return Err(reject(format!(
                "unknown righting-arm form `{}`; this build implements `{GZ_FORM}`",
                r.form
            )));
        }
        if r.coefficients.len() != HARMONICS {
            return Err(reject(format!(
                "{} coefficients, but this build's `{GZ_FORM}` has {HARMONICS}. \
                 The count is data and must not be padded or truncated to fit",
                r.coefficients.len()
            )));
        }
        if let Some(bad) = r.coefficients.iter().find(|v| !v.is_finite()) {
            return Err(reject(format!("coefficient {bad} is not finite")));
        }
        let mut c = [0.0; HARMONICS];
        c.copy_from_slice(&r.coefficients);
        Ok(Self { c })
    }

    /// `GZ(φ)`, metres. Odd in `φ`, bit for bit.
    ///
    /// The fold is left to right and its order is fixed (F9.4); an iterator
    /// sum with a different association would change the last bits.
    pub fn gz(&self, phi: f64) -> f64 {
        self.c
            .iter()
            .zip(sines(phi))
            .fold(0.0, |sum, (c, s)| sum + c * s)
    }

    /// `dGZ/dφ`, metres per radian. `dgz(0)` is `GM` by construction.
    pub fn dgz(&self, phi: f64) -> f64 {
        self.c
            .iter()
            .zip(cosines(phi))
            .enumerate()
            .fold(0.0, |sum, (k, (c, cos))| sum + c * ((k + 1) as f64) * cos)
    }

    /// `∫₀^φ GZ(s) ds`, metre-radians — the shape of the roll potential.
    ///
    /// Multiplied by `Δ·g` it is the potential energy stored in heel, which is
    /// what makes `invariants::dissipative_with_roll` a complete energy
    /// account rather than a kinetic one. Analytic, from the same coefficients
    /// as [`GzCurve::gz`].
    pub fn gz_integral(&self, phi: f64) -> f64 {
        self.c
            .iter()
            .zip(cosines(phi))
            .enumerate()
            .fold(0.0, |sum, (k, (c, cos))| {
                sum + c * (1.0 - cos) / ((k + 1) as f64)
            })
    }

    /// `n` evenly spaced `(φ, GZ(φ))` pairs over `[0, π]`, for the parameter
    /// panel and the diagnostics plot. The curve is odd, so the negative half
    /// is the mirror of this one and is not transmitted.
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
        GzCurve::fit_catalogue(&params()).expect("the v2 F18.1c defaults must fit")
    }

    /// The four F18.1a constraints, to the rounding floor.
    #[test]
    fn fit_reproduces_constraints() {
        let s = params().stability;
        let g = default_curve();
        assert_eq!(g.gz(0.0), 0.0, "GZ(0) must be exactly zero");
        assert!((g.dgz(0.0) - s.gm).abs() < 1e-12, "dgz(0) = {}", g.dgz(0.0));
        assert!(
            (g.gz(s.phi_peak) - s.gz_max).abs() < 1e-12,
            "gz(phi_p) = {}",
            g.gz(s.phi_peak)
        );
        // The constraint v1 did not have.
        assert!(
            g.dgz(s.phi_peak).abs() < 1e-12,
            "dgz(phi_p) = {} — phi_peak is not a stationary point",
            g.dgz(s.phi_peak)
        );
        assert!(
            g.gz(s.phi_vanish).abs() < 1e-12,
            "gz(phi_v) = {}",
            g.gz(s.phi_vanish)
        );
        eprintln!("hydrostatics: coefficients {:?}", g.coefficients());
    }

    /// Section 02 task 2.3: a curve rebuilt from its exported record
    /// reproduces `gz`, `dgz` and `gz_integral` **bit for bit** across the
    /// whole supported domain, extrema, roots and inversion included.
    ///
    /// Bit-for-bit rather than to a tolerance, because the two curves are the
    /// same coefficients evaluated by the same code on the same build: any
    /// difference at all would mean the export lost a mantissa bit, and a
    /// conformance fixture built on a lossy export would be comparing ports
    /// against the wrong numbers.
    #[test]
    fn the_exported_representation_rebuilds_the_curve_exactly() {
        let p = params();
        let g = default_curve();
        let r = g.representation();
        assert_eq!(r.version, GZ_REPRESENTATION_VERSION);
        assert_eq!(r.form, GZ_FORM);
        assert_eq!(r.coefficients.len(), HARMONICS);
        assert_eq!(r.coefficients, g.coefficients().to_vec());

        // Through JSON, which is what the bundle ships: a struct copy would
        // not notice a `Serialize` that rounded.
        let text = serde_json::to_string(&r).expect("the record must serialise");
        let read: GzRepresentation = serde_json::from_str(&text).expect("and parse back");
        let back = GzCurve::from_representation(&read).expect("and rebuild");
        assert_eq!(back, g);

        let s = p.stability;
        let mass = p.total_mass();
        // The named places first: the origin, both peaks, both vanishing
        // angles, inversion, and the quarter turns.
        let mut points: Vec<f64> = vec![
            0.0,
            -0.0,
            s.phi_peak,
            -s.phi_peak,
            s.phi_vanish,
            -s.phi_vanish,
            PI,
            -PI,
            PI / 2.0,
            -PI / 2.0,
        ];
        // Then the whole supported domain, densely.
        let n = 20_001;
        for i in 0..n {
            points.push(-PI + 2.0 * PI * (i as f64) / ((n - 1) as f64));
        }
        // And past it, where F18.1a extends the curve by its own periodicity
        // (`state.phi` is unwrapped, F3).
        for i in 0..400 {
            points.push(-4.0 * PI + 8.0 * PI * (i as f64) / 399.0);
        }
        for phi in points {
            assert_eq!(back.gz(phi).to_bits(), g.gz(phi).to_bits(), "gz at {phi}");
            assert_eq!(
                back.dgz(phi).to_bits(),
                g.dgz(phi).to_bits(),
                "dgz at {phi}"
            );
            assert_eq!(
                back.gz_integral(phi).to_bits(),
                g.gz_integral(phi).to_bits(),
                "gz_integral at {phi}"
            );
            assert_eq!(
                righting_moment(phi, &back, mass).to_bits(),
                righting_moment(phi, &g, mass).to_bits(),
                "righting_moment at {phi}"
            );
        }

        // Non-vacuity: the sweep has to have seen the shape it claims to
        // cover, or the bit comparisons above prove nothing.
        assert!(g.gz(s.phi_peak) > 0.0);
        assert!(g.gz(s.phi_vanish + 0.1) < 0.0);
        assert!(g.dgz(PI) > 0.0, "inversion must be a stable equilibrium");
    }

    /// A record this build cannot evaluate faithfully is refused, by name.
    /// Padding or truncating a coefficient list would produce a different
    /// boat and call it the same one.
    #[test]
    fn an_unreadable_representation_is_refused() {
        let good = default_curve().representation();
        let bad = |edit: fn(&mut GzRepresentation)| {
            let mut r = good.clone();
            edit(&mut r);
            r
        };
        let cases = [
            bad(|r| r.version += 1),
            bad(|r| r.form = "cubic_spline".to_string()),
            bad(|r| r.coefficients.push(0.1)),
            bad(|r| {
                r.coefficients.pop();
            }),
            bad(|r| r.coefficients[0] = f64::NAN),
            bad(|r| r.coefficients[1] = f64::INFINITY),
        ];
        for r in cases {
            assert!(
                GzCurve::from_representation(&r).is_err(),
                "accepted an unreadable record: {r:?}"
            );
        }
        assert!(GzCurve::from_representation(&good).is_ok());
    }

    #[test]
    fn gz_is_odd() {
        // Bit-identical, not merely close: the port/starboard mirror
        // invariants compare whole trajectories with `to_bits`, and a curve
        // that is only approximately odd breaks them in the last places.
        // `+0.0` normalises the one place where the bit comparison is about
        // IEEE signed zero rather than about the curve: at `φ = 0` the curve is
        // zero and `−0.0` is not `+0.0` bit for bit, though the two are equal
        // and produce an identical moment. Everywhere else it is the identity.
        let same = |a: f64, b: f64| (a + 0.0).to_bits() == (b + 0.0).to_bits();
        let g = default_curve();
        for i in 0..2001 {
            let phi = -PI + 2.0 * PI * (i as f64) / 2000.0;
            assert!(
                same(g.gz(-phi), -g.gz(phi)),
                "phi = {phi}: gz(-phi) = {}, -gz(phi) = {}",
                g.gz(-phi),
                -g.gz(phi)
            );
            // The derivative and the integral are even, also bit for bit.
            assert!(same(g.dgz(-phi), g.dgz(phi)), "dgz at {phi}");
            assert!(
                same(g.gz_integral(-phi), g.gz_integral(phi)),
                "gz_integral at {phi}"
            );
        }
    }

    /// v2 F18.1a: one root and one maximum below `φ_v`, one minimum above it,
    /// and nothing else on `(0, π]`.
    #[test]
    fn the_supported_domain_has_three_equilibria() {
        let s = params().stability;
        let g = default_curve();
        let n = 2_000_000;
        let refine = |f: &dyn Fn(f64) -> f64, mut a: f64, mut b: f64| {
            for _ in 0..200 {
                let m = 0.5 * (a + b);
                if f(a) * f(m) <= 0.0 {
                    b = m;
                } else {
                    a = m;
                }
            }
            0.5 * (a + b)
        };
        let mut roots = Vec::new();
        let mut stationary = Vec::new();
        let mut previous_gz = g.gz(0.0);
        let mut previous_dgz = g.dgz(0.0);
        for i in 1..=n {
            let phi = PI * (i as f64) / (n as f64);
            let before = PI * ((i - 1) as f64) / (n as f64);
            let value = g.gz(phi);
            let slope = g.dgz(phi);
            if previous_gz.signum() != value.signum() {
                roots.push(refine(&|x| g.gz(x), before, phi));
            }
            if previous_dgz.signum() != slope.signum() {
                stationary.push(refine(&|x| g.dgz(x), before, phi));
            }
            previous_gz = value;
            previous_dgz = slope;
        }
        assert_eq!(
            roots.len(),
            1,
            "GZ must cross zero exactly once on (0, pi]; found {roots:?}"
        );
        assert!(
            (roots[0] - s.phi_vanish).abs() < 1e-9,
            "the only root is at {} rad, not phi_vanish = {} rad",
            roots[0],
            s.phi_vanish
        );
        assert_eq!(
            stationary.len(),
            2,
            "GZ must have exactly two stationary points on (0, pi]; found {stationary:?}"
        );
        assert!(
            (stationary[0] - s.phi_peak).abs() < 1e-9,
            "the maximum is at {} rad, not phi_peak = {} rad",
            stationary[0],
            s.phi_peak
        );
        assert!((g.gz(stationary[0]) - s.gz_max).abs() < 1e-12);
        assert!(
            g.gz(stationary[1]) < 0.0,
            "the second extremum is not a dip"
        );
        eprintln!(
            "hydrostatics: root {:.9} rad; extrema {:.9} rad (GZ {:.9}) and {:.9} rad (GZ {:.9})",
            roots[0],
            stationary[0],
            g.gz(stationary[0]),
            stationary[1],
            g.gz(stationary[1])
        );
        // Inversion is an equilibrium, and a stable one: `GZ' (π) >= 0`.
        assert!(g.gz(PI).abs() < 1e-14, "gz(pi) = {}", g.gz(PI));
        assert!(
            g.dgz(PI) >= 0.0,
            "dgz(pi) = {} — the inverted boat is pushed back upright",
            g.dgz(PI)
        );

        // The negative half, stated rather than inferred. The curve is odd
        // bit for bit (`gz_is_odd`), so every root and extremum mirrors — and
        // the *moment* mirrors with it, which is the statement that matters.
        let m = params().total_mass();
        assert_eq!(g.gz(-roots[0]), -g.gz(roots[0]));
        assert!(
            (g.gz(-stationary[0]) + s.gz_max).abs() < 1e-12,
            "the port-side peak is {}, not -gz_max",
            g.gz(-stationary[0])
        );
        assert!(g.gz(-stationary[1]) > 0.0);
        for phi in [0.1, 0.5, s.phi_peak, 1.2, s.phi_vanish - 0.01] {
            assert!(righting_moment(phi, &g, m) < 0.0, "starboard heel {phi}");
            assert!(righting_moment(-phi, &g, m) > 0.0, "port heel {phi}");
        }
        for phi in [s.phi_vanish + 0.01, 1.6, 2.2, 3.0] {
            assert!(righting_moment(phi, &g, m) > 0.0, "past vanishing {phi}");
            assert!(righting_moment(-phi, &g, m) < 0.0, "past vanishing -{phi}");
        }
    }

    /// The v1 defect, stated as a regression: nowhere between `φ_v` and `π` may
    /// the righting arm be positive.
    #[test]
    fn no_positive_stability_past_vanishing() {
        let s = params().stability;
        let g = default_curve();
        let n = 400_000;
        let mut worst = (0.0_f64, f64::NEG_INFINITY);
        for i in 1..n {
            let phi = s.phi_vanish + (PI - s.phi_vanish) * (i as f64) / (n as f64);
            let value = g.gz(phi);
            if value > worst.1 {
                worst = (phi, value);
            }
        }
        assert!(
            worst.1 < 0.0,
            "GZ is {} m at {} rad ({} deg), past phi_vanish",
            worst.1,
            worst.0,
            worst.0.to_degrees()
        );
        // The v1 three-harmonic curve read +0.036 m at 90 deg and +0.782 m at
        // 140 deg; both are now firmly negative.
        assert!(g.gz(PI / 2.0) < -0.1, "gz(90 deg) = {}", g.gz(PI / 2.0));
        assert!(
            g.gz(140_f64.to_radians()) < -0.3,
            "gz(140 deg) = {}",
            g.gz(140_f64.to_radians())
        );
        eprintln!(
            "hydrostatics: worst GZ past phi_vanish {:.9} m at {:.4} deg",
            worst.1,
            worst.0.to_degrees()
        );
    }

    #[test]
    fn negative_righting_beyond_vanishing() {
        // brief §16: "possible negative restoring moment after sufficient
        // capsize". Unlike v1, this is now a property of the **shipped**
        // parameter set and not of a separately chosen one.
        let p = params();
        let g = default_curve();
        let m = p.total_mass();
        assert!(g.gz(p.stability.phi_vanish + 0.2) < 0.0);
        assert!(righting_moment(p.stability.phi_vanish + 0.2, &g, m) > 0.0);
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

    /// Every rejection path of [`GzCurve::fit`], including the three rules
    /// v2 F18.1a adds.
    #[test]
    fn fit_rejects_bad_params() {
        let limit = GzCurve::gz_envelope(&params());
        let ok = |gm: f64| GzCurve::fit(gm, 0.785, 0.30, 1.396, limit);

        // Rule 1.
        assert!(ok(0.0).is_err(), "GM = 0");
        assert!(ok(f64::NAN).is_err(), "NaN GM");
        assert!(
            GzCurve::fit(0.55, 0.785, -0.3, 1.396, limit).is_err(),
            "negative GZ_max"
        );
        assert!(
            GzCurve::fit(0.55, 0.785, 0.30, 4.0, limit).is_err(),
            "phi_v beyond pi"
        );
        assert!(
            GzCurve::fit(0.55, 1.4, 0.30, 0.5, limit).is_err(),
            "phi_p above phi_v"
        );
        assert!(
            GzCurve::fit(0.55, 0.785, 0.30, 1.396, 0.0).is_err(),
            "zero envelope"
        );
        // Rule 2.
        assert!(
            GzCurve::fit(0.55, 0.785, 0.30, 0.785, limit).is_err(),
            "singular"
        );

        // Rule 4: the F7 `GM = 1.00 m` puts a **minimum** at phi_peak, with
        // maxima at 31.8 deg and 61.2 deg either side of it. This is the
        // measured v1 defect (docs/v2/physics-validation.md §1.5).
        let why = ok(1.0).expect_err("GM = 1.00 m must be rejected");
        eprintln!("hydrostatics: GM = 1.00 rejected — {why}");

        // Rule 5: just below the admissible window the curve comes back
        // positive before inversion.
        assert!(ok(0.52).is_err(), "GM = 0.52 regains positive stability");
        // Rule 6: just above it the negative arm leaves the hull envelope.
        assert!(ok(0.70).is_err(), "GM = 0.70 leaves the geometric envelope");

        // And the shipped value sits inside the window.
        assert!(ok(0.55).is_ok(), "the shipped GM must fit");
    }

    /// The admissible interval measured in `docs/v2/physics-validation.md` §1.5,
    /// asserted so the handoff's number cannot drift from the code.
    #[test]
    fn the_admissible_gm_interval_brackets_the_default() {
        let p = params();
        let limit = GzCurve::gz_envelope(&p);
        let s = p.stability;
        let valid = |gm: f64| GzCurve::fit(gm, s.phi_peak, s.gz_max, s.phi_vanish, limit).is_ok();
        assert!(!valid(0.534), "0.534 must be below the interval");
        assert!(valid(0.535), "0.535 is the measured lower edge");
        assert!(valid(0.561), "0.561 is the measured upper edge");
        assert!(!valid(0.562), "0.562 must be above the interval");
        assert!(
            valid(s.gm) && s.gm > 0.535 && s.gm < 0.561,
            "the shipped GM {} must sit strictly inside [0.535, 0.561]",
            s.gm
        );
    }

    #[test]
    fn anchor_value() {
        // The F7 anchor: `Δ·g·GZ_max ≈ 406 N·m` is the whole righting budget
        // of a boat whose sailor never hikes (brief §4, R2). **Unchanged by
        // v2**: `gz_max` did not move.
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

    /// The shared derivative, against a central difference of the shared `gz`.
    /// Step and bound are explicit: `h = 1e-6` puts the truncation term at
    /// `O(h²·|GZ'''|) ≈ 1e-12` and the cancellation floor at `O(ε/h) ≈ 2e-10`,
    /// so `1.8e-10` is the floor and not a slack tolerance.
    #[test]
    fn dgz_matches_numeric_differentiation() {
        let g = default_curve();
        let h = 1e-6;
        let mut worst = (0.0_f64, 0.0_f64);
        for i in 0..=20_000 {
            let phi = -PI + 2.0 * PI * (i as f64) / 20_000.0;
            let numeric = (g.gz(phi + h) - g.gz(phi - h)) / (2.0 * h);
            let delta = (numeric - g.dgz(phi)).abs();
            if delta > worst.1 {
                worst = (phi, delta);
            }
        }
        assert!(
            worst.1 < 1.8e-10,
            "worst |dgz - central difference| = {:e} at phi = {}",
            worst.1,
            worst.0
        );
        eprintln!(
            "hydrostatics: dgz vs central difference h=1e-6, worst {:e} m/rad",
            worst.1
        );
    }

    /// The shared integral, against the trapezoid rule on the shared `gz`.
    /// `4 × 10⁵` panels put the `O(H²)` trapezoid error near `1e-12`, which is
    /// the bound asserted.
    #[test]
    fn gz_integral_matches_numeric_quadrature() {
        let g = default_curve();
        let mut worst = 0.0_f64;
        for target in [-PI, -2.0, -0.5, 0.3, 1.4, 2.9, PI] {
            let n = 400_000;
            let mut sum = 0.0;
            for i in 0..n {
                let a = target * (i as f64) / (n as f64);
                let b = target * ((i + 1) as f64) / (n as f64);
                sum += 0.5 * (g.gz(a) + g.gz(b)) * (b - a);
            }
            let delta = (g.gz_integral(target) - sum).abs();
            worst = worst.max(delta);
            assert!(
                delta < 1e-11,
                "phi = {target}: {} vs {sum} (|d| = {delta:e})",
                g.gz_integral(target)
            );
        }
        eprintln!("hydrostatics: gz_integral vs trapezoid, worst {worst:e} m.rad");
    }

    /// The roll potential has a minimum upright, a maximum at `φ_v` and a
    /// second minimum inverted — the energy picture of the three equilibria.
    #[test]
    fn roll_potential_shape() {
        let s = params().stability;
        let g = default_curve();
        let barrier = g.gz_integral(s.phi_vanish);
        assert!(barrier > 0.0);
        assert!(
            g.gz_integral(PI) < barrier,
            "the inverted state must sit below the barrier: {} vs {barrier}",
            g.gz_integral(PI)
        );
        assert!(
            g.gz_integral(PI) < 0.0,
            "the inverted state must sit below upright: {}",
            g.gz_integral(PI)
        );
        eprintln!(
            "hydrostatics: roll potential barrier {:.9} m.rad at phi_v, inverted {:.9} m.rad",
            barrier,
            g.gz_integral(PI)
        );
    }

    /// Both sides of the zero curve: `from_params` never fails, and a
    /// catalogue that `fit_catalogue` rejects is exactly the one that produces
    /// it.
    #[test]
    fn from_params_is_total() {
        let mut p = params();
        assert_ne!(GzCurve::from_params(&p), GzCurve::default());
        p.stability.phi_vanish = p.stability.phi_peak;
        assert_eq!(GzCurve::from_params(&p), GzCurve::default());
        assert!(GzCurve::fit_catalogue(&p).is_err());
        // Finite everywhere even then.
        let g = GzCurve::from_params(&p);
        for i in 0..=720 {
            let phi = -PI + 2.0 * PI * (i as f64) / 720.0;
            assert!(g.gz(phi).is_finite() && g.dgz(phi).is_finite());
        }
    }
}
