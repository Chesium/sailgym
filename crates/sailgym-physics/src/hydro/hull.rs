//! Reduced empirical hull resistance (F6.6, brief §15).
//!
//! Linear + quadratic damping on each of the four dynamic rigid-body DOF.
//! Every coefficient is a named F7 parameter so that towing-tank or CFD data
//! can replace the whole block later without touching this file's structure
//! (brief §15).
//!
//! **R6 — known limitation.** There is no planing regime and no wave-making
//! hump: `X_uu` is a single quadratic coefficient fitted to nothing. The model
//! is anchored at `u = 2.06 m/s` (4 kn), where it gives ≈ 48 N, and it
//! **over-predicts resistance above roughly 5 m/s** — at `u = 6 m/s` it
//! reports 354 N, of which 92 % is the quadratic term. Accepted for v1 and
//! surfaced in the diagnostics panel (section 08).

use crate::dynamics::Load;
use crate::parameters::BoatParameters;
use crate::state::BoatState;
use crate::vec::Vec3;

/// Hull resistance. The surge/sway force acts at the CG; the yaw and roll
/// moments are moments about the CG and cannot be carried by a [`Load`] with
/// `r = 0`, so they are returned separately and added straight into `ΣN` and
/// `ΣK` (F6.6).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HullLoads {
    /// N, surge and sway at the CG (`r = 0`).
    pub load: Load,
    /// N·m, yaw damping about `+z`.
    pub n_yaw: f64,
    /// N·m, roll damping about `+x`.
    pub k_roll: f64,
}

/// `−(c1·q + c2·q·|q|)` — the F6.6 term for one DOF. Zero velocity gives
/// exactly zero, and the result always opposes `q`.
fn resist(q: f64, linear: f64, quadratic: f64) -> f64 {
    -(linear * q + quadratic * q * q.abs())
}

/// The four F6.6 resistance terms.
pub fn hull_loads(st: &BoatState, p: &BoatParameters) -> HullLoads {
    let r = &p.resistance;
    HullLoads {
        load: Load {
            f: Vec3::new(
                resist(st.u, r.x_u, r.x_uu),
                resist(st.v, r.y_v, r.y_vv),
                0.0,
            ),
            r: Vec3::ZERO,
        },
        n_yaw: resist(st.r, r.n_r, r.n_rr),
        k_roll: resist(st.p, r.k_p, r.k_pp),
    }
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

    /// `a` and `b` have opposite signs, or `a` is exactly zero.
    fn opposes(a: f64, b: f64) -> bool {
        a == 0.0 || a * b < 0.0
    }

    #[test]
    fn drag_opposes_motion() {
        // brief §35 force sign sanity.
        let p = params();
        let mut rng = Lcg(0x8011_0000_ABCD_1234);
        for k in 0..200 {
            let st = BoatState {
                u: rng.range(-8.0, 8.0),
                v: rng.range(-3.0, 3.0),
                r: rng.range(-2.0, 2.0),
                p: rng.range(-2.0, 2.0),
                ..BoatState::ZERO
            };
            let h = hull_loads(&st, &p);
            assert!(opposes(h.load.f.x, st.u), "case {k}: surge");
            assert!(opposes(h.load.f.y, st.v), "case {k}: sway");
            assert!(opposes(h.n_yaw, st.r), "case {k}: yaw");
            assert!(opposes(h.k_roll, st.p), "case {k}: roll");
        }
    }

    #[test]
    fn resistance_anchor() {
        // The F7 sanity anchor: ≈ 48 N at 4 kn. If this fails the resistance
        // parameters changed, and the reason belongs in the handoff note
        // (brief §43).
        let p = params();
        let st = BoatState {
            u: 2.06,
            ..BoatState::ZERO
        };
        let total = -hull_loads(&st, &p).load.f.x;
        assert!((total - 48.0).abs() <= 2.0, "surge resistance = {total} N");
    }

    #[test]
    fn quadratic_dominates_at_speed() {
        // R6: above ≈ 5 m/s the quadratic term carries almost all of the
        // resistance, and there is no planing regime to relieve it.
        let p = params();
        let u = 6.0;
        let linear = p.resistance.x_u * u;
        let quadratic = p.resistance.x_uu * u * u;
        let total = -hull_loads(
            &BoatState {
                u,
                ..BoatState::ZERO
            },
            &p,
        )
        .load
        .f
        .x;
        assert!((total - (linear + quadratic)).abs() < 1e-12);
        assert!(
            quadratic / total > 0.9,
            "quadratic share = {}",
            quadratic / total
        );
    }

    #[test]
    fn zero_velocity_zero_force() {
        let h = hull_loads(&BoatState::ZERO, &params());
        assert_eq!(h.load.f, Vec3::ZERO);
        assert_eq!(h.load.r, Vec3::ZERO);
        assert_eq!(h.n_yaw, 0.0);
        assert_eq!(h.k_roll, 0.0);
    }

    #[test]
    fn mirror_symmetry() {
        let p = params();
        let mut rng = Lcg(0x8011_FFFF_5678_9ABC);
        for k in 0..50 {
            let st = BoatState {
                phi: rng.range(-1.2, 1.2),
                u: rng.range(-5.0, 5.0),
                v: rng.range(-2.0, 2.0),
                r: rng.range(-1.0, 1.0),
                p: rng.range(-1.0, 1.0),
                ..BoatState::ZERO
            };
            let a = hull_loads(&st, &p);
            let b = hull_loads(&mirror_state(&st), &p);
            assert!((b.load.f.x - a.load.f.x).abs() < 1e-14, "case {k}: X");
            assert!((b.load.f.y + a.load.f.y).abs() < 1e-14, "case {k}: Y");
            assert!((b.n_yaw + a.n_yaw).abs() < 1e-14, "case {k}: N");
            assert!((b.k_roll + a.k_roll).abs() < 1e-14, "case {k}: K");
        }
    }
}
