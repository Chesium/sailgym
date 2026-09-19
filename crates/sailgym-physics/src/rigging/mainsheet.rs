//! Unilateral mainsheet tension element (F6.8, brief §11).
//!
//! ## The `max` is the model
//!
//! `T = max(0, k·e + c·ė)` is not a post-hoc clamp on a computed value. There
//! is no branch anywhere in this file of the form "if the sheet is slack, do
//! something different": a slack rope simply produces `e < 0`, therefore a
//! non-positive bracket, therefore zero tension, therefore zero force and zero
//! torque — through the very same expression that carries a loaded rope.
//! `slack_rope_zero_tension` and `damping_can_zero_tension` assert both paths
//! through that single expression.
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
//! T      = max(0, k_sheet·e + c_sheet·ė)
//! F_b    = T · (P_k − P_b)/ℓ
//! M_β    = (P_b − mast_pos_b) × F_b |_z
//! ```
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
    let tension = (p.sheet.k_sheet * extension + p.sheet.c_sheet * extension_rate).max(0.0);
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
