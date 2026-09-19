//! Shared lift/drag foil model (F5).
//!
//! **This is the only lift/drag implementation in the repository.** The sail
//! (section 05), the centreboard and the rudder all call [`foil_force`]
//! unchanged; a second copy of `C_L(α)` anywhere would be the sign-convention
//! drift R3 warns about, and section 04's acceptance criterion 6 greps for it.
//!
//! The coefficients are continuous on the whole of `[−π, π]` (brief §10):
//! attached flow blended into a flat-plate post-stall model. There is no
//! singularity, no division by `sin α`, and **no branch on the sign of `α`**
//! (F5.2) — which is what makes `C_L` exactly odd and `C_D` exactly even, and
//! therefore makes port/starboard mirror symmetry hold bit-for-bit rather
//! than to a tolerance.

use crate::vec::Vec2;

/// The F5.3 section coefficients.
///
/// This is the F7 catalogue's [`crate::parameters::FoilSection`] under the
/// name F5.3 gives it. It is re-exported rather than re-declared: the nine
/// fields would otherwise exist twice, and a parameter declared in two places
/// is exactly how the catalogue drifts (F7). `sail.section`, `board.section`
/// and `rudder.section` are already values of this type, so the model reads
/// the catalogue directly with no conversion step to get wrong.
pub use crate::parameters::FoilSection as FoilParams;

/// Flow-speed threshold below which a velocity is treated as zero (F5.1).
/// Also the degenerate-length cut-off for [`crate::vec::Vec2::normalize`].
pub const EPS_FLOW: f64 = 1e-9; // m/s

/// Hermite smoothstep: `0` below `edge0`, `1` above `edge1`, `C¹` across.
///
/// Used only for the stall blend `s(α)` of F5.2. A degenerate or inverted
/// interval falls back to a step at `edge0`; `FoilSection::validate` rejects
/// a non-positive `stall_blend`, so that path is unreachable for a validated
/// catalogue and exists only to keep the function total.
pub fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    if edge1 <= edge0 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed angle from the flow direction to the chord, verbatim from F5.1.
///
/// `α = 0` means the flow runs leading edge → trailing edge. For a rudder in
/// straight-ahead flow this returns `δr` exactly, which
/// `tests::alpha_equals_rudder_angle` asserts.
pub fn angle_of_attack(v: Vec2, chord: Vec2) -> f64 {
    let f = v.normalize(); // Vec2::ZERO if |v| < EPS_FLOW
    let c = chord.normalize();
    (f.cross(c)).atan2(f.dot(c))
}

/// Lift-curve slope per radian, `C_Lα = 2π / (1 + 2/AR)` (F5.2).
fn lift_curve_slope(p: &FoilParams) -> f64 {
    std::f64::consts::TAU / (1.0 + 2.0 / p.ar)
}

/// The camber hook `α_e = α − α_0·tanh(α / α_b)` (F5.2).
///
/// `α_0 = 0` in v1, so this is the identity. It stays **odd** in `α` because
/// of the `tanh`, which is what lets a cambered sail flip sides without
/// breaking mirror symmetry; `α_0·sign(α)` would be discontinuous and is
/// explicitly forbidden by F5.2.
fn alpha_effective(a: f64, p: &FoilParams) -> f64 {
    a - p.alpha_camber * (a / p.camber_blend).tanh()
}

/// `s(α)` of F5.2: 0 = attached, 1 = fully stalled. Even in `α`.
fn stall_fraction(a: f64, p: &FoilParams) -> f64 {
    smoothstep(p.alpha_stall, p.alpha_stall + p.stall_blend, a.abs())
}

/// `C_L,att(α) = C_Lα · sin α_e` (F5.2).
fn cl_attached(a: f64, p: &FoilParams) -> f64 {
    lift_curve_slope(p) * alpha_effective(a, p).sin()
}

/// Lift coefficient (F5.2). Odd in `α`, bit-for-bit.
pub fn cl(a: f64, p: &FoilParams) -> f64 {
    let s = stall_fraction(a, p);
    let (sin_a, cos_a) = a.sin_cos();
    (1.0 - s) * cl_attached(a, p) + s * (p.cn_max * sin_a * cos_a)
}

/// Drag coefficient (F5.2). Even in `α`, bit-for-bit.
pub fn cd(a: f64, p: &FoilParams) -> f64 {
    let s = stall_fraction(a, p);
    let att = cl_attached(a, p);
    let induced = att * att / (std::f64::consts::PI * p.oswald * p.ar);
    let sin_a = a.sin();
    p.cd0 + (1.0 - s) * induced + s * (p.cn_max * sin_a * sin_a)
}

/// Lift ⟂ flow, drag ∥ flow. Returns the force in the same 2-D frame as `v`
/// and `chord`. Verbatim from F5.3.
///
/// **The lift direction is `(f̂.y, −f̂.x)`, a −90° rotation of the flow
/// direction.** Combined with `C_L > 0` for small `α > 0`, this is the unique
/// choice that makes a rudder at `δr > 0` push its blade to port. A sign error
/// here is the single most likely defect in the whole project (R3);
/// `tests::lift_direction_sign` asserts it directly and must never be
/// weakened.
pub fn foil_force(v: Vec2, chord: Vec2, rho: f64, p: &FoilParams) -> Vec2 {
    let vsq = v.length_squared();
    if vsq < EPS_FLOW * EPS_FLOW {
        return Vec2::ZERO; // zero flow ⇒ exactly zero force
    }
    let q = 0.5 * rho * vsq;
    let a = angle_of_attack(v, chord);
    let f_hat = v.normalize();
    let l_hat = Vec2::new(f_hat.y, -f_hat.x); // rotate the flow direction by −90°
    l_hat * (q * p.area * cl(a, p)) + f_hat * (q * p.area * cd(a, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::RHO_WATER;
    use crate::frames::rudder_chord;
    use crate::parameters::BoatParameters;
    use std::f64::consts::PI;

    fn rudder() -> FoilParams {
        BoatParameters::ilca7().rudder.section
    }

    fn board() -> FoilParams {
        BoatParameters::ilca7().board.section
    }

    fn sail() -> FoilParams {
        BoatParameters::ilca7().sail.section
    }

    /// `n` angles spanning `[−π, π]` inclusive.
    fn sweep(n: usize) -> impl Iterator<Item = f64> {
        (0..n).map(move |i| -PI + 2.0 * PI * (i as f64) / ((n - 1) as f64))
    }

    #[test]
    fn alpha_equals_rudder_angle() {
        // F5.1: for a rudder in straight-ahead flow, α = δr exactly.
        // The water runs from bow to stern past the blade, i.e. along −x_B.
        let flow = Vec2::new(-1.0, 0.0);
        for i in 0..=120 {
            let d = -0.6 + 1.2 * (i as f64) / 120.0;
            let a = angle_of_attack(flow, rudder_chord(d).xy());
            assert!((a - d).abs() < 1e-12, "δr = {d}: α = {a}");
        }
    }

    #[test]
    fn cl_is_odd() {
        // Exactly odd — bit-identical, not to a tolerance. Mirror symmetry
        // (brief §35) is structural and this is where it comes from.
        for p in [&rudder(), &board(), &sail()] {
            for a in sweep(200) {
                assert_eq!(
                    cl(-a, p).to_bits(),
                    (-cl(a, p)).to_bits(),
                    "α = {a}: cl(−α) = {}, −cl(α) = {}",
                    cl(-a, p),
                    -cl(a, p)
                );
            }
        }
    }

    #[test]
    fn cd_is_even() {
        for p in [&rudder(), &board(), &sail()] {
            for a in sweep(200) {
                assert_eq!(
                    cd(-a, p).to_bits(),
                    cd(a, p).to_bits(),
                    "α = {a}: cd(−α) = {}, cd(α) = {}",
                    cd(-a, p),
                    cd(a, p)
                );
            }
        }
    }

    #[test]
    fn cl_zero_at_zero_and_pi() {
        for p in [&rudder(), &board(), &sail()] {
            assert_eq!(cl(0.0, p), 0.0);
            assert!(cl(PI / 2.0, p).abs() < 1e-12);
            assert!(cl(-PI / 2.0, p).abs() < 1e-12);
            assert!(cl(PI, p).abs() < 1e-12);
            assert!(cl(-PI, p).abs() < 1e-12);
        }
    }

    #[test]
    fn cd_endpoints() {
        for p in [&rudder(), &board(), &sail()] {
            assert_eq!(cd(0.0, p), p.cd0);
            for s in [1.0, -1.0] {
                assert!((cd(s * PI / 2.0, p) - (p.cd0 + p.cn_max)).abs() < 1e-12);
                assert!((cd(s * PI, p) - p.cd0).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn cl_slope_at_origin() {
        let h = 1e-6;
        for p in [&rudder(), &board(), &sail()] {
            let slope = (cl(h, p) - cl(-h, p)) / (2.0 * h);
            let want = 2.0 * PI / (1.0 + 2.0 / p.ar);
            assert!(
                (slope - want).abs() < 0.01 * want,
                "dCL/dα = {slope}, expected {want}"
            );
        }
    }

    #[test]
    fn continuity_across_stall() {
        for p in [&rudder(), &board(), &sail()] {
            let lo = p.alpha_stall - 2.0 * p.stall_blend;
            let hi = p.alpha_stall + 2.0 * p.stall_blend;
            let n = ((hi - lo) / 1e-5).round() as usize;
            let (mut prev_l, mut prev_d) = (cl(lo, p), cd(lo, p));
            for i in 1..=n {
                let a = lo + (hi - lo) * (i as f64) / (n as f64);
                let (l, d) = (cl(a, p), cd(a, p));
                assert!((l - prev_l).abs() < 1e-3, "cl step at α = {a}");
                assert!((d - prev_d).abs() < 1e-3, "cd step at α = {a}");
                prev_l = l;
                prev_d = d;
            }
        }
    }

    #[test]
    fn zero_flow_zero_force() {
        let p = rudder();
        for v in [
            Vec2::ZERO,
            Vec2::new(EPS_FLOW / 2.0, 0.0),
            Vec2::new(0.0, -EPS_FLOW / 3.0),
        ] {
            assert_eq!(
                foil_force(v, Vec2::new(-1.0, 0.0), RHO_WATER, &p),
                Vec2::ZERO
            );
        }
    }

    #[test]
    fn lift_direction_sign() {
        // F5.3, and the primary R3 guard. Flow straight aft past a rudder
        // deflected to +0.2 rad: the blade force must point to port (+y).
        let f = foil_force(
            Vec2::new(-1.0, 0.0),
            rudder_chord(0.2).xy(),
            RHO_WATER,
            &rudder(),
        );
        assert!(
            f.y > 0.0,
            "rudder force = {f:?}, expected f.y > 0 (to port)"
        );
        // Mirrored, it must point to starboard by exactly as much.
        let m = foil_force(
            Vec2::new(-1.0, 0.0),
            rudder_chord(-0.2).xy(),
            RHO_WATER,
            &rudder(),
        );
        assert_eq!(m.y, -f.y);
        assert_eq!(m.x, f.x);
    }

    #[test]
    fn finite_over_full_range() {
        let base = rudder();
        let sets = [
            base,
            board(),
            sail(),
            FoilParams { ar: 0.5, ..base },
            FoilParams { ar: 20.0, ..base },
        ];
        for p in &sets {
            for a in sweep(10_000) {
                assert!(cl(a, p).is_finite(), "cl(α = {a}) with AR = {}", p.ar);
                assert!(cd(a, p).is_finite(), "cd(α = {a}) with AR = {}", p.ar);
                let flow = Vec2::new(a.cos(), a.sin()) * 3.0;
                let f = foil_force(flow, Vec2::new(-1.0, 0.0), RHO_WATER, p);
                assert!(f.x.is_finite() && f.y.is_finite(), "force at α = {a}");
            }
        }
    }
}
