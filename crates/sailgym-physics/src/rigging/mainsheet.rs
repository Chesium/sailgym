//! Unilateral mainsheet tension element (F6.8, brief §11), as corrected by
//! v2 F18.1b.
//!
//! ## Slack is a state of the rope, not a sign of an expression
//!
//! v1 wrote `T = max(0, k·e + c·ė)` and argued that a slack rope produces
//! `e < 0` and therefore a non-positive bracket. **It does not.** A large
//! positive `ė` carries the bracket above zero while the rope is visibly slack:
//! measured, a rope hanging 0.5 m loose pulls with 5 kN, and the boom rates of
//! a gybe reach that regime through `ė ⊇ (dℓ/dβ)·β̇` alone. The numbers are in
//! `docs/v2/physics-validation.md` §3.
//!
//! The law is therefore stated on the rope's state:
//!
//! ```text
//! T = if e > 0 { max(0, k_sheet·e + c_sheet·ė) } else { 0 }
//! ```
//!
//! **The boundary is `e > 0`, strictly.** The slack set `{e ≤ 0}` is closed and
//! carries `T = 0` on all of it — `e = 0` included, for either sign of `ė` and
//! for `|ė|` arbitrarily large. Stiffness and damping are both properties of
//! *stretched* rope; an element at its natural length transmits nothing. Taking
//! the slack set closed also makes `T` a function of state alone at the
//! boundary: under the alternative (`e ≥ 0` taut) the tension at `e = 0` would
//! depend on the sign of `ė`, which is a worse object to integrate and a worse
//! one to port.
//!
//! The `max` is retained and still load-bearing. It is what stops a **taut**
//! rope from pushing when it is eased faster than it is stretched, which is the
//! `sheet_release` case; `damping_can_zero_tension` is its guard, and
//! `slack_rope_zero_tension` and `slack_is_zero_under_any_rate` are the guards
//! on the branch above it.
//!
//! `T` is discontinuous at take-up: `T → c_sheet·ė` as `e → 0⁺` with `ė > 0`,
//! against `T = 0` at `e = 0`. That is the contact-impact discontinuity every
//! unilateral spring–damper has, and it is the price of the correction — the
//! old law was continuous and wrong. Convergence across it is measured with
//! event-aware checks in `tests/convergence.rs`, never with a smooth-order
//! assertion.
//!
//! ## Geometry, verbatim from F6.8
//!
//! ```text
//! P_b(β) = mast_pos_b + d_sheet · b̂(β) + (0, 0, z_boom)
//! P_k    = block_pos_b
//! ℓ(β)   = |P_b(β) − P_k|
//! e      = ℓ − L                       L = state.l_sheet
//! dℓ/dβ  = ((P_b − P_k) · dP_b/dβ) / ℓ,  dP_b/dβ = d_sheet · (sin β, −cos β, 0)
//! ė      = (dℓ/dβ)·β̇ − L̇
//! F_b    = T · (P_k − P_b)/ℓ
//! M_β    = (P_b − mast_pos_b) × F_b |_z
//! ```
//!
//! ## The shortest path the rig can take (v2 F18.1b)
//!
//! With `a = mast.x − block.x`, `b = mast.y − block.y`,
//! `h = mast.z + z_boom − block.z` and `R = √(a² + b²)`:
//!
//! ```text
//! ℓ(β)² = a² + b² + h² + d_sheet² − 2·d_sheet·R·cos(β − atan2(b, a))
//! β_min = atan2(b, a)                     for d_sheet > 0
//! ℓ_min = ℓ(β_min) = √( h² + (R − d_sheet)² )
//! ```
//!
//! [`min_rope_path`] is the single definition, and it returns
//! `rope_path_length(β_min, p)` rather than the closed form, so the bound and
//! the path the physics evaluates cannot disagree by a rounding step.
//! `BoatParameters::validate` requires `ℓ_min ≤ l_sheet_min`: v1 shipped
//! `l_sheet_min = 0.90 m` against `ℓ_min = 1.0404 m`, a permanent 2.81 kN
//! preload at the one boom angle where `dℓ/dβ = 0` and the element has no
//! damping at all.
//!
//! The boom angle is **never** assigned here; tension produces torque through
//! geometry alone (brief §11). `no_boom_angle_assignment` greps this file.
//!
//! ## Both ends of the rope are returned
//!
//! `F_b` acts on the boom at `P_b`; its reaction `−F_b` acts on the hull at the
//! block `P_k` (F6.8). Dropping the reaction would silently violate Newton's
//! third law, so both `Load`s leave this module and both are summed in
//! `forces::evaluate`.

use crate::dynamics::Load;
use crate::frames::{boom_dir, boom_dir_dbeta};
use crate::parameters::BoatParameters;
use crate::state::BoatState;
use crate::vec::Vec3;

/// Rope paths shorter than this are treated as degenerate: the unit vector
/// along the rope is undefined there, so the element produces nothing rather
/// than a `NaN`. With the F7 geometry `ℓ ≥ 1.04 m`, so this never fires for
/// the shipped boat; it exists so that a hand-edited `block_pos_b` cannot
/// poison the whole state vector. Same role as `foil::EPS_FLOW`.
const EPS_ROPE: f64 = 1e-9;

/// One complete evaluation of the sheet element.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SheetOutput {
    /// N, `≥ 0` always (brief §11).
    pub tension: f64,
    /// N·m, boom moment about `+z_B` at the mast.
    pub m_beta: f64,
    /// Force on the boom, applied at `P_b`, in `B`.
    pub boom_load: Load,
    /// Reaction `−F_b` on the hull, applied at `P_k`, in `B`.
    pub hull_load: Load,
    /// m, `ℓ(β)` — the geometric rope path length.
    pub rope_length: f64,
    /// m, `e = ℓ − L`. Negative when the rope is slack.
    pub extension: f64,
}

/// `P_b(β)`, the boom attachment point in `B` (F6.8).
pub fn boom_attach_point(beta: f64, p: &BoatParameters) -> Vec3 {
    p.sail.mast_pos_b + boom_dir(beta) * p.sheet.d_sheet + Vec3::new(0.0, 0.0, p.sheet.z_boom)
}

/// `ℓ(β)`, the geometric rope path length from the boom to the block (F6.8).
pub fn rope_path_length(beta: f64, p: &BoatParameters) -> f64 {
    (boom_attach_point(beta, p) - p.sheet.block_pos_b).length()
}

/// `dℓ/dβ`, analytic (F6.8). `drope_dbeta_analytic_matches_numeric` checks it
/// against a central difference of [`rope_path_length`].
pub fn drope_dbeta(beta: f64, p: &BoatParameters) -> f64 {
    let d = boom_attach_point(beta, p) - p.sheet.block_pos_b;
    let length = d.length();
    if length < EPS_ROPE {
        return 0.0;
    }
    d.dot(boom_dir_dbeta(beta) * p.sheet.d_sheet) / length
}

/// The shortest rope path over all boom angles, and the boom angle at which it
/// occurs (v2 F18.1b).
///
/// Returned as `rope_path_length(β_min, p)` rather than as the closed form, so
/// that the bound `BoatParameters::validate` enforces and the path the physics
/// evaluates are the **same** floating-point evaluation. `min_is_the_minimum`
/// checks it against a dense sweep.
pub fn min_rope_path(p: &BoatParameters) -> (f64, f64) {
    let a = p.sail.mast_pos_b.x - p.sheet.block_pos_b.x;
    let b = p.sail.mast_pos_b.y - p.sheet.block_pos_b.y;
    // ℓ² is `const − 2·d_sheet·R·cos(β − atan2(b, a))`, so a positive `d_sheet`
    // is shortest where the cosine is `+1` and a negative one where it is `−1`.
    let beta = if p.sheet.d_sheet >= 0.0 {
        b.atan2(a)
    } else {
        (-b).atan2(-a)
    };
    let beta = crate::frames::wrap_pi(beta);
    (rope_path_length(beta, p), beta)
}

/// The F6.8 element, evaluated at `(st, L̇)`.
///
/// `l_sheet_dot` is the commanded payout rate and must be the **same** value
/// the integrator uses for `L̇` in this derivative evaluation, or the damping
/// term is inconsistent across RK2 stages. It is passed in rather than
/// recomputed from `Controls` here, so that the caller cannot disagree with
/// itself.
pub fn sheet_output(st: &BoatState, l_sheet_dot: f64, p: &BoatParameters) -> SheetOutput {
    let attach = boom_attach_point(st.beta, p);
    let to_block = p.sheet.block_pos_b - attach;
    let rope_length = to_block.length();
    let extension = rope_length - st.l_sheet;
    if rope_length < EPS_ROPE {
        return SheetOutput {
            rope_length,
            extension,
            ..SheetOutput::default()
        };
    }
    // ė = (dℓ/dβ)·β̇ − L̇. The payout rate enters with a minus sign: easing
    // lengthens the available rope and therefore relaxes the extension.
    let extension_rate = drope_dbeta(st.beta, p) * st.beta_dot - l_sheet_dot;
    // v2 F18.1b. The taut branch is the **open** half-line `e > 0`; the slack
    // set `{e <= 0}` carries zero tension for every extension rate, `e == 0`
    // included. The `max` inside it is what stops a taut rope from pushing
    // while it is eased faster than it is stretched.
    let tension = if extension > 0.0 {
        (p.sheet.k_sheet * extension + p.sheet.c_sheet * extension_rate).max(0.0)
    } else {
        0.0
    };
    // Along the rope, toward the block: a rope pulls, it never pushes.
    let f = to_block * (tension / rope_length);
    SheetOutput {
        tension,
        m_beta: (attach - p.sail.mast_pos_b).cross(f).z,
        boom_load: Load { f, r: attach },
        hull_load: Load {
            f: -f,
            r: p.sheet.block_pos_b,
        },
        rope_length,
        extension,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::mirror_state;
    use std::f64::consts::PI;

    /// Deterministic test-local generator; see the note in `state.rs`.
    struct Lcg(u64);

    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }

        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + self.unit() * (hi - lo)
        }
    }

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    fn state(beta: f64, beta_dot: f64, l_sheet: f64) -> BoatState {
        BoatState {
            beta,
            beta_dot,
            l_sheet,
            ..BoatState::ZERO
        }
    }

    #[test]
    fn tension_never_negative() {
        // brief §11's fundamental requirement, brief §35's sheet unilateral
        // constraint: 100 000 randomised combinations, extremes included.
        let p = params();
        let mut rng = Lcg(0x5EE7_0000_1111_2222);
        for case in 0..100_000 {
            let extreme = case % 7 == 0;
            let (beta, beta_dot, l_sheet, l_sheet_dot) = if extreme {
                (
                    rng.range(-8.0, 8.0),
                    rng.range(-500.0, 500.0),
                    rng.range(-10.0, 100.0),
                    rng.range(-500.0, 500.0),
                )
            } else {
                (
                    rng.range(-PI, PI),
                    rng.range(-20.0, 20.0),
                    rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
                    rng.range(-p.sheet.sheet_haul_rate, p.sheet.sheet_release_rate),
                )
            };
            let out = sheet_output(&state(beta, beta_dot, l_sheet), l_sheet_dot, &p);
            assert!(
                out.tension >= 0.0,
                "case {case}: T = {} at beta={beta}, beta_dot={beta_dot}, \
                 L={l_sheet}, L_dot={l_sheet_dot}",
                out.tension
            );
        }
    }

    #[test]
    fn slack_rope_zero_tension() {
        let p = params();
        for beta in [-1.2, -0.4, 0.0, 0.4, 1.2] {
            let l_sheet = rope_path_length(beta, &p) + 0.5;
            let out = sheet_output(&state(beta, 0.0, l_sheet), 0.0, &p);
            assert!(out.extension < -0.01);
            assert_eq!(out.tension, 0.0);
            assert_eq!(out.m_beta, 0.0);
            assert_eq!(out.boom_load.f, Vec3::ZERO);
            assert_eq!(out.hull_load.f, Vec3::ZERO);
        }
    }

    /// **The v1 defect, as a regression.** `max(0, k·e + c·ė)` gives a slack
    /// rope positive tension whenever `ė` is large enough: 500 N at 0.05 m of
    /// slack, 5 kN at 0.5 m. Both terms are reachable inside the model — the
    /// sheet command supplies `|L̇| ≤ 6 m/s` and the boom supplies
    /// `(dℓ/dβ)·β̇` with `|dℓ/dβ| ≤ d_sheet`. Under v2 F18.1b every one of
    /// these is exactly zero.
    #[test]
    fn slack_is_zero_under_any_rate() {
        let p = params();
        let mut checked = 0usize;
        for beta in [-2.0, -1.2, -0.4, 0.0, 0.4, 1.2, 2.0] {
            let length = rope_path_length(beta, &p);
            for slack in [1e-12, 1e-6, 0.01, 0.05, 0.2, 0.5, 2.0] {
                for beta_dot in [-500.0, -20.0, -1.0, 0.0, 1.0, 20.0, 500.0] {
                    for l_sheet_dot in [-500.0, -50.0, -5.0, 0.0, 5.0, 50.0, 500.0] {
                        let st = state(beta, beta_dot, length + slack);
                        let out = sheet_output(&st, l_sheet_dot, &p);
                        assert!(out.extension <= 0.0);
                        assert_eq!(
                            out.tension, 0.0,
                            "slack {slack} m at beta {beta}, beta_dot {beta_dot}, \
                             L_dot {l_sheet_dot}: T = {}",
                            out.tension
                        );
                        assert_eq!(out.m_beta, 0.0);
                        assert_eq!(out.boom_load.f, Vec3::ZERO);
                        assert_eq!(out.hull_load.f, Vec3::ZERO);
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 7 * 7 * 7 * 7);
        // The old law, written out, would have produced these. Stated as a
        // number so the regression cannot be mistaken for a tautology: at
        // 0.5 m of slack and 50 m/s of extension rate `max(0, k·e + c·ė)`
        // is 5 kN, and both terms are reachable (module note).
        let e = -0.5;
        let edot = 50.0;
        let old_law = (p.sheet.k_sheet * e + p.sheet.c_sheet * edot).max(0.0);
        assert!(old_law > 4.9e3, "the witness is {old_law} N");
    }

    /// The exact `e == 0` boundary of v2 F18.1b: the slack set is **closed**.
    #[test]
    fn zero_extension_is_slack() {
        let p = params();
        for beta in [-1.2, -0.4, 0.0, 0.4, 1.2] {
            let length = rope_path_length(beta, &p);
            for beta_dot in [-20.0, -1.0, 0.0, 1.0, 20.0] {
                for l_sheet_dot in [-6.0, 0.0, 6.0] {
                    // `l_sheet == ℓ(β)` exactly, so `e` is exactly zero.
                    let out = sheet_output(&state(beta, beta_dot, length), l_sheet_dot, &p);
                    assert_eq!(out.extension, 0.0, "beta {beta}: e is not exactly zero");
                    assert_eq!(
                        out.tension, 0.0,
                        "beta {beta}, beta_dot {beta_dot}, L_dot {l_sheet_dot}"
                    );
                    assert_eq!(out.m_beta, 0.0);
                }
            }
        }
        // One ULP either side: below is slack, above is taut and loaded.
        let beta = 0.6;
        let length = rope_path_length(beta, &p);
        let just_slack = f64::from_bits(length.to_bits() + 1);
        let just_taut = f64::from_bits(length.to_bits() - 1);
        assert_eq!(
            sheet_output(&state(beta, 0.0, just_slack), 0.0, &p).tension,
            0.0
        );
        assert!(sheet_output(&state(beta, 0.0, just_taut), 0.0, &p).tension > 0.0);
    }

    /// A taut rope never pushes: the force is along `P_k − P_b` with a
    /// non-negative magnitude, at every extension and every rate.
    #[test]
    fn taut_never_compresses() {
        let p = params();
        let mut rng = Lcg(0x7A17_1234_5678_9ABC);
        for _ in 0..20_000 {
            let beta = rng.range(-PI, PI);
            let length = rope_path_length(beta, &p);
            let st = state(beta, rng.range(-50.0, 50.0), length - rng.range(1e-9, 0.5));
            let out = sheet_output(&st, rng.range(-10.0, 10.0), &p);
            assert!(out.extension > 0.0);
            assert!(out.tension >= 0.0);
            // The force points from the boom attachment toward the block.
            let along = p.sheet.block_pos_b - boom_attach_point(beta, &p);
            assert!(
                out.boom_load.f.dot(along) >= 0.0,
                "beta {beta}: the rope pushed, f.along = {}",
                out.boom_load.f.dot(along)
            );
        }
    }

    /// v2 F18.1b: the analytic minimum is the minimum, and the F7 geometry's
    /// `l_sheet_min` sits exactly on it.
    #[test]
    fn min_is_the_minimum() {
        let p = params();
        let (l_min, beta_min) = min_rope_path(&p);
        assert_eq!(beta_min, 0.0, "the F7 block is aft on the centreline");
        assert_eq!(l_min, rope_path_length(beta_min, &p));
        let n = 2_000_000;
        for i in 0..=n {
            let beta = -PI + 2.0 * PI * (i as f64) / (n as f64);
            let l = rope_path_length(beta, &p);
            assert!(
                l >= l_min,
                "beta {beta}: l = {l} is below min_rope_path = {l_min}"
            );
        }
        // The closed form, as a second opinion on the same number.
        let a = p.sail.mast_pos_b.x - p.sheet.block_pos_b.x;
        let b = p.sail.mast_pos_b.y - p.sheet.block_pos_b.y;
        let h = p.sail.mast_pos_b.z + p.sheet.z_boom - p.sheet.block_pos_b.z;
        let r = (a * a + b * b).sqrt();
        let closed = (h * h + (r - p.sheet.d_sheet).powi(2)).sqrt();
        assert!((closed - l_min).abs() < 1e-12, "{closed} vs {l_min}");
        // And the shipped stop is exactly it — zero preload, not 2.81 kN.
        assert_eq!(p.sheet.l_sheet_min, l_min);
        let out = sheet_output(&state(beta_min, 0.0, p.sheet.l_sheet_min), 0.0, &p);
        assert_eq!(out.extension, 0.0);
        assert_eq!(out.tension, 0.0);
        eprintln!("mainsheet: l_min = {l_min:?} at beta = {beta_min}");
    }

    /// An off-centreline block moves the minimum off `β = 0`, and
    /// [`min_rope_path`] still finds it.
    #[test]
    fn min_follows_the_geometry() {
        let mut p = params();
        p.sheet.block_pos_b.y = 0.35;
        let (l_min, beta_min) = min_rope_path(&p);
        assert!(
            beta_min.abs() > 1e-3,
            "the minimum did not move: {beta_min}"
        );
        let n = 400_000;
        for i in 0..=n {
            let beta = -PI + 2.0 * PI * (i as f64) / (n as f64);
            assert!(rope_path_length(beta, &p) >= l_min);
        }
    }

    /// Degenerate geometry produces finite output rather than a `NaN`, at zero
    /// flow and at the `EPS_ROPE` guard.
    #[test]
    fn finite_at_degenerate_geometry() {
        let mut p = params();
        // Block coincident with the boom attachment at beta = 0.
        p.sheet.block_pos_b = boom_attach_point(0.0, &p);
        let out = sheet_output(&state(0.0, 0.0, 0.0), 0.0, &p);
        assert!(out.rope_length < EPS_ROPE);
        assert_eq!(out.tension, 0.0);
        assert_eq!(out.m_beta, 0.0);
        assert_eq!(out.boom_load.f, Vec3::ZERO);
        assert_eq!(drope_dbeta(0.0, &p), 0.0);
        let (l_min, beta_min) = min_rope_path(&p);
        assert!(l_min.is_finite() && beta_min.is_finite());
    }

    #[test]
    fn taut_rope_positive_tension() {
        let p = params();
        for beta in [-1.2, -0.4, 0.0, 0.4, 1.2] {
            let length = rope_path_length(beta, &p);
            let l_sheet = length - 0.1;
            let out = sheet_output(&state(beta, 0.0, l_sheet), 0.0, &p);
            let expected = p.sheet.k_sheet * (length - l_sheet);
            assert!(out.tension > 0.0);
            assert!(
                (out.tension - expected).abs() < 1e-9,
                "beta {beta}: {} vs {expected}",
                out.tension
            );
        }
    }

    #[test]
    fn damping_can_zero_tension() {
        // Taut rope, but eased fast enough that k·e + c·ė < 0. The `max` is
        // the only thing standing between that and a pushing rope.
        let p = params();
        let beta = -0.9;
        let length = rope_path_length(beta, &p);
        let st = state(beta, 0.0, length - 0.01);
        let quiet = sheet_output(&st, 0.0, &p);
        assert!(quiet.tension > 0.0);
        let released = sheet_output(&st, p.sheet.sheet_release_rate, &p);
        assert!(
            p.sheet.k_sheet * quiet.extension - p.sheet.c_sheet * p.sheet.sheet_release_rate < 0.0
        );
        assert_eq!(released.tension, 0.0);
        assert_eq!(released.m_beta, 0.0);
        assert_eq!(released.boom_load.f, Vec3::ZERO);
    }

    #[test]
    fn drope_dbeta_analytic_matches_numeric() {
        let p = params();
        let h = 1e-6;
        for i in 0..=320 {
            let beta = -1.6 + 3.2 * (i as f64) / 320.0;
            let numeric =
                (rope_path_length(beta + h, &p) - rope_path_length(beta - h, &p)) / (2.0 * h);
            let analytic = drope_dbeta(beta, &p);
            assert!(
                (numeric - analytic).abs() < 1e-7,
                "beta {beta}: analytic {analytic} vs numeric {numeric}"
            );
        }
    }

    #[test]
    fn sheet_torque_restores_toward_centreline() {
        // The core geometric behaviour: a taut sheet pulls the boom back
        // toward beta = 0, from either side, with no branch on the side.
        let p = params();
        let port = state(-0.8, 0.0, rope_path_length(-0.8, &p) - 0.1);
        let starboard = state(0.8, 0.0, rope_path_length(0.8, &p) - 0.1);
        let port_out = sheet_output(&port, 0.0, &p);
        let starboard_out = sheet_output(&starboard, 0.0, &p);
        assert!(port_out.tension > 0.0 && starboard_out.tension > 0.0);
        assert!(port_out.m_beta > 0.0, "{}", port_out.m_beta);
        assert!(starboard_out.m_beta < 0.0, "{}", starboard_out.m_beta);
    }

    #[test]
    fn newton_third_law() {
        let p = params();
        let mut rng = Lcg(0x3AD0_4444_5555_6666);
        for _ in 0..1000 {
            let beta = rng.range(-PI, PI);
            let st = state(
                beta,
                rng.range(-5.0, 5.0),
                rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
            );
            let out = sheet_output(&st, rng.range(-2.0, 6.0), &p);
            assert_eq!(out.boom_load.f, -out.hull_load.f);
            assert_eq!(out.hull_load.r, p.sheet.block_pos_b);
            assert_eq!(out.boom_load.r, boom_attach_point(beta, &p));
        }
    }

    #[test]
    fn no_boom_angle_assignment() {
        // brief §11: tension produces torque, it does not set an angle. The
        // needle is built at run time so this test cannot match itself.
        let needle = format!(".{}", "beta");
        let source = include_str!("mainsheet.rs");
        let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        for (i, _) in compact.match_indices(&needle) {
            let tail = &compact[i + needle.len()..];
            let tail = tail.strip_prefix("_dot").unwrap_or(tail);
            let assignment = (tail.starts_with('=') && !tail.starts_with("=="))
                || ["+=", "-=", "*=", "/="]
                    .iter()
                    .any(|op| tail.starts_with(op));
            assert!(!assignment, "mainsheet.rs assigns a boom angle");
        }
    }

    #[test]
    fn mirror_symmetry() {
        let p = params();
        let mut rng = Lcg(0x1717_7777_8888_9999);
        for _ in 0..1000 {
            let st = state(
                rng.range(-PI, PI),
                rng.range(-5.0, 5.0),
                rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
            );
            // The sheet command is a length rate: it has no side, so it is not
            // mirrored (see `testkit::mirror_controls`).
            let l_sheet_dot = rng.range(-2.0, 6.0);
            let direct = sheet_output(&st, l_sheet_dot, &p);
            let mirrored = sheet_output(&mirror_state(&st), l_sheet_dot, &p);
            assert!((mirrored.tension - direct.tension).abs() < 1e-13);
            assert!((mirrored.rope_length - direct.rope_length).abs() < 1e-13);
            assert!((mirrored.m_beta + direct.m_beta).abs() < 1e-13);
            assert!((mirrored.boom_load.f.y + direct.boom_load.f.y).abs() < 1e-13);
            assert!((mirrored.hull_load.f.y + direct.hull_load.f.y).abs() < 1e-13);
            assert!((mirrored.boom_load.f.x - direct.boom_load.f.x).abs() < 1e-13);
            assert!((mirrored.boom_load.f.z - direct.boom_load.f.z).abs() < 1e-13);
        }
    }

    #[test]
    fn finite_at_extremes() {
        let p = params();
        for l_sheet in [p.sheet.l_sheet_min, p.sheet.l_sheet_max] {
            for i in 0..=720 {
                let beta = -PI + 2.0 * PI * (i as f64) / 720.0;
                for beta_dot in [-20.0, -1.0, 0.0, 1.0, 20.0] {
                    for l_sheet_dot in [-p.sheet.sheet_haul_rate, 0.0, p.sheet.sheet_release_rate] {
                        let out = sheet_output(&state(beta, beta_dot, l_sheet), l_sheet_dot, &p);
                        assert!(out.tension.is_finite() && out.m_beta.is_finite());
                        assert!(out.boom_load.f.length().is_finite());
                        assert!(out.hull_load.f.length().is_finite());
                        assert!(out.rope_length.is_finite() && out.extension.is_finite());
                        assert!(
                            out.tension < 1e6,
                            "beta {beta}, L {l_sheet}: T = {}",
                            out.tension
                        );
                    }
                }
            }
        }
    }
}
