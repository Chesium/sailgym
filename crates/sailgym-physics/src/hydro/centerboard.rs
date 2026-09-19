//! Centreboard foil load (F6.5), and the flow model both underwater foils
//! share.
//!
//! [`local_flow`] and [`foil_hydro_load`] live here because the board is the
//! simpler of the two surfaces and the rudder is the special case; both are
//! re-exported from `crate::hydro` so `rudder.rs` reaches them by the module
//! path rather than by reaching sideways into this file.

use crate::constants::RHO_WATER;
use crate::dynamics::Load;
use crate::foil::{angle_of_attack, foil_force, FoilParams};
use crate::frames::{rot_x, rot_x_inv};
use crate::parameters::BoatParameters;
use crate::state::BoatState;
use crate::vec::{Vec2, Vec3};

/// What one underwater foil produced, plus the two quantities the debug panel
/// (section 08) and the tests need to see.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FoilLoad {
    pub load: Load,
    /// rad, angle of attack (F5.1).
    pub alpha: f64,
    /// m/s, local water velocity relative to the surface, in `B`, spanwise
    /// component already dropped.
    pub v_local: Vec2,
}

/// Local water velocity relative to the surface, in `B`, spanwise (`z_B`)
/// component dropped. Verbatim from F6.5:
///
/// ```text
/// v_local_H = −[ (u, v, 0) + ω_H × (R_x(φ)·r_b) ]
/// ```
///
/// The water is still, so this is simply minus the velocity of the material
/// point the foil occupies. The `ω × r` term is not decoration: it is the
/// whole of yaw damping, and it is why rudder authority grows with speed and
/// vanishes at rest (brief §14) without any of those being special-cased.
pub fn local_flow(st: &BoatState, r_b: Vec3) -> Vec2 {
    let omega_h = Vec3::new(st.p, 0.0, st.r);
    let r_h = rot_x(st.phi, r_b);
    let v_point_h = Vec3::new(st.u, st.v, 0.0) + omega_h.cross(r_h);
    rot_x_inv(st.phi, -v_point_h).xy()
}

/// One underwater lifting surface, verbatim from F6.5.
///
/// `chord_b` is the chord direction in `B`, leading edge → trailing edge;
/// `r_b` is the mount point relative to the CG, in `B`. The returned [`Load`]
/// is in `B` like every other load (F4.4), so `Generalized::add` applies the
/// F6.4 heel geometry to it and nothing here needs a `cos φ` of its own.
pub fn foil_hydro_load(st: &BoatState, r_b: Vec3, chord_b: Vec2, p: &FoilParams) -> FoilLoad {
    let v_local = local_flow(st, r_b);
    let f = foil_force(v_local, chord_b, RHO_WATER, p);
    FoilLoad {
        load: Load {
            f: Vec3::new(f.x, f.y, 0.0),
            r: r_b,
        },
        alpha: angle_of_attack(v_local, chord_b),
        v_local,
    }
}

/// The centreboard: a fixed foil whose chord runs along the centreline,
/// leading edge forward (F6.5 fixes the chord at `(−1, 0)`).
pub fn centerboard_load(st: &BoatState, p: &BoatParameters) -> FoilLoad {
    foil_hydro_load(st, p.board.pos_b, Vec2::new(-1.0, 0.0), &p.board.section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::mirror_state;

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

    #[test]
    fn zero_flow_zero_lift() {
        let p = params();
        let out = centerboard_load(&BoatState::ZERO, &p);
        assert_eq!(out.load.f, Vec3::ZERO);
        assert_eq!(out.v_local, Vec2::ZERO);
    }

    #[test]
    fn side_force_opposes_leeway() {
        // brief §35 / section AC 5. Drifting to port (`v > 0`) must produce a
        // board force to starboard (`f.y < 0`) — it resists leeway. Never
        // weaken this test.
        let p = params();
        let st = BoatState {
            u: 3.0,
            v: 0.3,
            ..BoatState::ZERO
        };
        let out = centerboard_load(&st, &p);
        assert!(out.load.f.y < 0.0, "board force = {:?}", out.load.f);

        // And the mirror image drifts the other way.
        let out = centerboard_load(&mirror_state(&st), &p);
        assert!(out.load.f.y > 0.0, "board force = {:?}", out.load.f);
    }

    #[test]
    fn v_squared_scaling() {
        // brief §35 velocity scaling, at a fixed leeway angle.
        let p = params();
        let speeds: [f64; 4] = [1.0, 2.0, 4.0, 6.0];
        let leeway = 0.1_f64; // rad
        let magnitude = |speed: f64| {
            let st = BoatState {
                u: speed * leeway.cos(),
                v: speed * leeway.sin(),
                ..BoatState::ZERO
            };
            centerboard_load(&st, &p).load.f.length()
        };
        for w in speeds.windows(2) {
            let (v1, v2) = (w[0], w[1]);
            let want = (v2 / v1).powi(2);
            let got = magnitude(v2) / magnitude(v1);
            assert!(
                (got - want).abs() < 0.02 * want,
                "F({v2})/F({v1}) = {got}, expected {want}"
            );
        }
    }

    #[test]
    fn alpha_equals_leeway_angle() {
        // With `r = p = 0` the board sees the CG velocity alone. The water
        // runs past the boat at `−(u, v)`, and the chord points aft, so
        // F5.1 gives `α = atan2(−v, u)`.
        //
        // **Note for the human (section 04 handoff §2).** `docs/04-hydro.md`
        // writes this criterion as `atan2(v, u)`. That sign cannot be right:
        // it would make a boat drifting to port see a positive `α`, hence
        // (F5.3) lift to *port*, which amplifies leeway and contradicts the
        // PRD's own `side_force_opposes_leeway` — a section-AC-5 test that may
        // never be weakened. The value asserted here is the one F2 + F5 + F6.5
        // produce; the magnitude is the leeway angle either way.
        let p = params();
        for &(u, v) in &[(3.0, 0.3), (3.0, -0.3), (1.0, 0.05), (5.0, 1.0)] {
            let st = BoatState {
                u,
                v,
                ..BoatState::ZERO
            };
            let out = centerboard_load(&st, &p);
            let want = (-v).atan2(u);
            assert!(
                (out.alpha - want).abs() < 1e-9,
                "u = {u}, v = {v}: α = {}, expected {want}",
                out.alpha
            );
            // The magnitude is the leeway angle, on either sign convention.
            assert!((out.alpha.abs() - v.atan2(u).abs()).abs() < 1e-9);
        }
    }

    #[test]
    fn yaw_rate_contributes() {
        // F6.5's `ω × r` term. At `u = 3, v = 0, r = 0.5` the board, mounted
        // forward of the CG, sees a sideways flow of `r · x_board`.
        //
        // Same sign note as `alpha_equals_leeway_angle`: the PRD writes
        // `atan2(r·x_board, u)`; F2 + F5 + F6.5 give its negative.
        let p = params();
        let st = BoatState {
            u: 3.0,
            r: 0.5,
            ..BoatState::ZERO
        };
        let out = centerboard_load(&st, &p);
        assert!(out.alpha != 0.0);
        let want = (-(st.r * p.board.pos_b.x)).atan2(st.u);
        assert!(
            (out.alpha - want).abs() < 1e-9,
            "α = {}, expected {want}",
            out.alpha
        );
    }

    #[test]
    fn mirror_symmetry() {
        let p = params();
        let mut rng = Lcg(0xB0A2_D000_1234_5678);
        for k in 0..50 {
            let st = BoatState {
                psi: rng.range(-3.0, 3.0),
                phi: rng.range(-1.2, 1.2),
                u: rng.range(-4.0, 4.0),
                v: rng.range(-1.5, 1.5),
                r: rng.range(-0.8, 0.8),
                p: rng.range(-0.8, 0.8),
                ..BoatState::ZERO
            };
            let a = centerboard_load(&st, &p);
            let b = centerboard_load(&mirror_state(&st), &p);
            assert!((b.load.f.x - a.load.f.x).abs() < 1e-14, "case {k}: f.x");
            assert!((b.load.f.y + a.load.f.y).abs() < 1e-14, "case {k}: f.y");
            assert!((b.load.f.z - a.load.f.z).abs() < 1e-14, "case {k}: f.z");
            assert!((b.load.r.x - a.load.r.x).abs() < 1e-14, "case {k}: r.x");
            assert!((b.load.r.y + a.load.r.y).abs() < 1e-14, "case {k}: r.y");
            assert!((b.alpha + a.alpha).abs() < 1e-14, "case {k}: alpha");
        }
    }
}
