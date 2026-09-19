//! SCAFFOLD — DELETED BY TASK 4.5. NOT PHYSICS.
//!
//! This file exists so that the integrator, the WASM boundary, rendering,
//! the controls and the determinism tests can be built and verified before
//! any real force model lands (R4). Its numbers are chosen only to make the
//! app steerable and bounded. They are not coefficients, they are not tagged,
//! they are not in F7, and nothing may be inferred from them.
//!
//! Section 04 task 4.5 deletes this file in its entirety together with
//! `sim.scaffold_thrust`, and proves the deletion by grep. Do not improve it,
//! do not tune it, and do not write tests that assert its behaviour beyond
//! finiteness and boundedness.

use crate::dynamics::{ForceModel, Generalized};
use crate::parameters::BoatParameters;
use crate::state::{BoatState, Controls};

/// N·s/m — surge drag.
const DRAG_U: f64 = 25.0;
/// N·s/m — sway drag.
const DRAG_V: f64 = 800.0;
/// N·m·s — yaw damping.
const DRAG_R: f64 = 200.0;
/// N·m per radian of rudder per m/s of speed — steering authority.
const YAW_AUTHORITY: f64 = 30.0;

/// The M1 placeholder force model (R4).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScaffoldForces;

impl ForceModel for ScaffoldForces {
    fn generalized(
        &self,
        st: &BoatState,
        _c: &Controls,
        p: &BoatParameters,
        _t: f64,
    ) -> Generalized {
        Generalized {
            x: p.sim.scaffold_thrust - DRAG_U * st.u,
            y: -DRAG_V * st.v,
            // δr > 0 turns the bow to starboard (F2.2), i.e. ψ̇ < 0, so the
            // yaw moment is negative. Authority grows with speed, which is the
            // one qualitative behaviour the M1 app needs from steering.
            n: -YAW_AUTHORITY * st.delta_r * st.u - DRAG_R * st.r,
            // Roll and boom are untouched at M1.
            k: 0.0,
        }
    }

    fn boom_moment(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrator::step;

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

    /// 60 s from a randomised state under a randomised control sequence.
    /// Returns the worst `|u|` and `|r|` seen, or `None` if any step produced
    /// a non-finite field.
    fn episode(rng: &mut Lcg, p: &BoatParameters) -> Option<(f64, f64)> {
        let mut st = BoatState {
            x: rng.range(-50.0, 50.0),
            y: rng.range(-50.0, 50.0),
            psi: rng.range(-3.2, 3.2),
            phi: rng.range(-1.0, 1.0),
            u: rng.range(-3.0, 3.0),
            v: rng.range(-1.0, 1.0),
            r: rng.range(-0.5, 0.5),
            p: rng.range(-0.5, 0.5),
            beta: rng.range(-1.7, 1.7),
            beta_dot: rng.range(-1.0, 1.0),
            delta_r: rng.range(-p.rudder.delta_r_max, p.rudder.delta_r_max),
            l_sheet: rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
            t: 0.0,
        };
        let mut c = Controls::default();
        let mut worst_u: f64 = 0.0;
        let mut worst_r: f64 = 0.0;
        let steps = (60.0 / p.sim.dt).round() as u32;
        for i in 0..steps {
            // Re-roll the controls twice a second.
            if i % 100 == 0 {
                c = Controls {
                    rudder_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_release: rng.unit() < 0.1,
                };
            }
            st = step(&st, &c, p, &ScaffoldForces, p.sim.dt, p.sim.integrator);
            if !st.is_finite() {
                return None;
            }
            worst_u = worst_u.max(st.u.abs());
            worst_r = worst_r.max(st.r.abs());
        }
        Some((worst_u, worst_r))
    }

    #[test]
    fn scaffold_finite() {
        let p = BoatParameters::ilca7();
        let mut rng = Lcg(0xC0FF_EE00_1234_5678);
        for i in 0..20 {
            assert!(
                episode(&mut rng, &p).is_some(),
                "episode {i} went non-finite"
            );
        }
    }

    #[test]
    fn scaffold_bounded() {
        let p = BoatParameters::ilca7();
        let mut rng = Lcg(0xC0FF_EE00_1234_5678);
        for i in 0..20 {
            let (u, r) = episode(&mut rng, &p).expect("finite");
            assert!(u < 20.0, "episode {i}: |u| reached {u}");
            assert!(r < 5.0, "episode {i}: |r| reached {r}");
        }
    }
}
