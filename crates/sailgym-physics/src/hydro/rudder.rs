//! Rudder foil load (F6.5).
//!
//! The same finite lifting surface as the centreboard, mounted aft and with a
//! steerable chord `ĉ_r(δr)` (F2.2). Everything brief §14 asks for —
//! authority growing with speed, stall at large deflections, near-zero
//! authority at rest, yaw damping — falls out of that one substitution; none
//! of it is coded as a special case.

use crate::frames::rudder_chord;
use crate::hydro::{foil_hydro_load, FoilLoad};
use crate::parameters::BoatParameters;
use crate::state::BoatState;

/// The rudder blade, at the deflection the state currently holds.
pub fn rudder_load(st: &BoatState, p: &BoatParameters) -> FoilLoad {
    foil_hydro_load(
        st,
        p.rudder.pos_b,
        rudder_chord(st.delta_r).xy(),
        &p.rudder.section,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::mirror_state;
    use crate::vec::Vec3;

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

    /// Yaw moment about `+z_B` from a rudder load. At `φ = 0` this is exactly
    /// the contribution `Generalized::add` makes to `ΣN` (F6.4).
    fn yaw_moment(out: &FoilLoad) -> f64 {
        out.load.r.x * out.load.f.y - out.load.r.y * out.load.f.x
    }

    fn at(u: f64, delta_r: f64) -> BoatState {
        BoatState {
            u,
            delta_r,
            ..BoatState::ZERO
        }
    }

    #[test]
    fn zero_speed_no_authority() {
        // brief §14: weak rudder authority when nearly stationary.
        let p = params();
        let out = rudder_load(&at(0.01, p.rudder.delta_r_max), &p);
        let n = yaw_moment(&out);
        assert!(n.abs() < 0.1, "yaw moment at 0.01 m/s = {n} N·m");
    }

    #[test]
    fn positive_delta_turns_bow_to_starboard() {
        // F2.2, R3, section AC 5. **The single most important sign test in the
        // project.** `δr > 0` puts side force to port on a blade aft of the
        // CG, so `M_z < 0` and the bow swings to starboard. Never weaken this.
        let p = params();
        let out = rudder_load(&at(3.0, 0.2), &p);
        assert!(out.load.f.y > 0.0, "blade force = {:?}", out.load.f);
        let n = yaw_moment(&out);
        assert!(n < 0.0, "yaw moment = {n} N·m, expected negative");

        // And the mirror image turns the other way.
        let m = rudder_load(&at(3.0, -0.2), &p);
        assert!(yaw_moment(&m) > 0.0);
    }

    #[test]
    fn authority_grows_with_speed() {
        let p = params();
        let speeds: [f64; 3] = [1.0, 2.0, 4.0];
        let n: Vec<f64> = speeds
            .iter()
            .map(|&u| yaw_moment(&rudder_load(&at(u, 0.15), &p)).abs())
            .collect();
        assert!(n[1] > n[0] && n[2] > n[1], "not monotonic: {n:?}");
        for i in 0..2 {
            let want = (speeds[i + 1] / speeds[i]).powi(2);
            let got = n[i + 1] / n[i];
            assert!(
                (got - want).abs() < 0.03 * want,
                "ratio {got} vs V² = {want}"
            );
        }
    }

    #[test]
    fn rudder_stalls() {
        // brief §14: rudder stall at high angles. Sweeping δr from 0 to 0.7
        // rad at 3 m/s, the yaw moment rises, peaks, and falls.
        //
        // The peak looked for is the **first** interior maximum, which is the
        // stall. Past it the F5.2 blend hands over to the flat-plate branch
        // `C_N,max·sin α·cos α`, which climbs again toward its own maximum of
        // `C_N,max/2` at 45°. With the F7 values that flat-plate maximum
        // (0.95) sits slightly above the attached peak (0.884), so the *global*
        // maximum over a 0–0.7 rad sweep is at the far end. That is a property
        // of `C_N,max = 1.9`, an F7 ASSUMED value, and is not a reason to tune
        // it (brief §43): at those angles the blade is a brake, not a rudder,
        // and `delta_r_max` is 0.698 rad in any case. What brief §14 asks for
        // — authority that collapses once the blade stalls — is exactly the
        // first peak and the fall after it, and that is what is asserted.
        let p = params();
        let n = 700;
        let sample = |i: usize| {
            let d = 0.7 * (i as f64) / (n as f64);
            (d, yaw_moment(&rudder_load(&at(3.0, d), &p)).abs())
        };

        // Rise, then the first turn-over.
        let mut i = 1;
        while i <= n && sample(i).1 > sample(i - 1).1 {
            i += 1;
        }
        let (d_peak, m_peak) = sample(i - 1);
        assert!(i > 1 && i <= n, "the moment never turned over");
        assert!(d_peak > 0.0 && d_peak < 0.7, "peak at δr = {d_peak}");

        // And it really falls afterwards, by a margin that is a stall rather
        // than round-off: the trough is well below the peak.
        let trough = (i..=n).map(|k| sample(k).1).fold(f64::INFINITY, f64::min);
        assert!(
            trough < 0.8 * m_peak,
            "moment fell only from {m_peak} to {trough}"
        );

        let a = p.rudder.section.alpha_stall;
        let b = a + 3.0 * p.rudder.section.stall_blend;
        assert!(
            (a..=b).contains(&d_peak),
            "peak at δr = {d_peak}, expected within [{a}, {b}]"
        );
    }

    #[test]
    fn yaw_damping_sign() {
        // brief §14: with the rudder centred, a boat rotating to port is
        // damped by its own rudder.
        let p = params();
        let st = BoatState {
            u: 3.0,
            r: 0.4,
            ..BoatState::ZERO
        };
        let n = yaw_moment(&rudder_load(&st, &p));
        assert!(n < 0.0, "yaw moment = {n} N·m, expected damping");
    }

    #[test]
    fn mirror_symmetry() {
        let p = params();
        let mut rng = Lcg(0x2D0E_7000_9ABC_DEF0);
        for k in 0..50 {
            let st = BoatState {
                psi: rng.range(-3.0, 3.0),
                phi: rng.range(-1.2, 1.2),
                u: rng.range(-4.0, 4.0),
                v: rng.range(-1.5, 1.5),
                r: rng.range(-0.8, 0.8),
                p: rng.range(-0.8, 0.8),
                delta_r: rng.range(-p.rudder.delta_r_max, p.rudder.delta_r_max),
                ..BoatState::ZERO
            };
            let a = rudder_load(&st, &p);
            let b = rudder_load(&mirror_state(&st), &p);
            assert!((b.load.f.x - a.load.f.x).abs() < 1e-14, "case {k}: f.x");
            assert!((b.load.f.y + a.load.f.y).abs() < 1e-14, "case {k}: f.y");
            assert!((b.load.f.z - a.load.f.z).abs() < 1e-14, "case {k}: f.z");
            assert_eq!(b.load.r, Vec3::new(a.load.r.x, -a.load.r.y, a.load.r.z));
            assert!((b.alpha + a.alpha).abs() < 1e-14, "case {k}: alpha");
        }
    }
}
