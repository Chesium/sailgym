//! Fixed-step integration (F7 `integrator`, brief §21).
//!
//! [`Integrator::Rk2Midpoint`] is the default. [`Integrator::Rk4`] exists only
//! as the reference method for the section 10 convergence study; it is never
//! the default and is not exposed in the UI.

use serde::{Deserialize, Serialize};

use crate::dynamics::{derivative, ForceModel};
use crate::parameters::BoatParameters;
use crate::state::{BoatState, Controls};

/// Available integration methods (brief §21).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Integrator {
    /// Cheapest stable option; kept for comparison.
    SemiImplicitEuler,
    /// The default (brief §21).
    #[default]
    Rk2Midpoint,
    /// Convergence reference only (section 10).
    Rk4,
}

/// Saturate the actuator states into their admissible boxes (F4.3).
///
/// Applied after **every** stage of every method, not only to the finished
/// step: a stage that overshot the rudder stop would evaluate the rudder
/// hydrodynamics at an angle the boat cannot reach. Together with the strict
/// rate limiter in `dynamics::limit_rate`, this makes a saturated actuator
/// land on exactly its limit for any `dt`.
fn clamp_actuators(st: &mut BoatState, p: &BoatParameters) {
    st.delta_r = st
        .delta_r
        .clamp(-p.rudder.delta_r_max, p.rudder.delta_r_max);
    st.l_sheet = st.l_sheet.clamp(p.sheet.l_sheet_min, p.sheet.l_sheet_max);
}

/// One stage: `base + h · d`, saturated.
fn stage(base: &BoatState, h: f64, d: &crate::state::StateDot, p: &BoatParameters) -> BoatState {
    let mut s = base.axpy(h, d);
    clamp_actuators(&mut s, p);
    s
}

/// One fixed step of `dt`.
///
/// Angle wrapping is applied **once**, after the step completes: wrapping a
/// midpoint stage would put a discontinuity inside the step.
pub fn step(
    st: &BoatState,
    c: &Controls,
    p: &BoatParameters,
    fm: &dyn ForceModel,
    dt: f64,
    method: Integrator,
) -> BoatState {
    let t = st.t;
    let mut out = match method {
        Integrator::SemiImplicitEuler => {
            let d = derivative(st, c, p, fm, t);
            // Velocities and actuators first, then positions from the updated
            // velocities — that is what makes the method symplectic-ish and
            // stable for oscillatory rigging loads.
            let mut next = *st;
            next.u += dt * d.u;
            next.v += dt * d.v;
            next.r += dt * d.r;
            next.p += dt * d.p;
            next.beta_dot += dt * d.beta_dot;
            next.delta_r += dt * d.delta_r;
            next.l_sheet += dt * d.l_sheet;
            clamp_actuators(&mut next, p);

            let (sin_psi, cos_psi) = st.psi.sin_cos();
            next.x += dt * (next.u * cos_psi - next.v * sin_psi);
            next.y += dt * (next.u * sin_psi + next.v * cos_psi);
            next.psi += dt * next.r;
            next.phi += dt * next.p;
            next.beta += dt * next.beta_dot;
            next.t += dt;
            next
        }
        Integrator::Rk2Midpoint => {
            let k1 = derivative(st, c, p, fm, t);
            let mid = stage(st, dt * 0.5, &k1, p);
            let k2 = derivative(&mid, c, p, fm, t + dt * 0.5);
            stage(st, dt, &k2, p)
        }
        Integrator::Rk4 => {
            let k1 = derivative(st, c, p, fm, t);
            let s2 = stage(st, dt * 0.5, &k1, p);
            let k2 = derivative(&s2, c, p, fm, t + dt * 0.5);
            let s3 = stage(st, dt * 0.5, &k2, p);
            let k3 = derivative(&s3, c, p, fm, t + dt * 0.5);
            let s4 = stage(st, dt, &k3, p);
            let k4 = derivative(&s4, c, p, fm, t + dt);

            // Weighted mean of the four slopes, then one stage of full dt.
            let mut mean = k1;
            let sixth = 1.0 / 6.0;
            macro_rules! blend {
                ($($f:ident),* $(,)?) => {
                    $( mean.$f = sixth * (k1.$f + 2.0 * k2.$f + 2.0 * k3.$f + k4.$f); )*
                };
            }
            blend!(x, y, psi, phi, u, v, r, p, beta, beta_dot, delta_r, l_sheet, t);
            stage(st, dt, &mean, p)
        }
    };
    out.wrap_angles();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamics::Generalized;

    /// Returns nothing at all. Used for the rest-equilibrium check.
    struct Zero;

    impl ForceModel for Zero {
        fn generalized(
            &self,
            _: &BoatState,
            _: &Controls,
            _: &BoatParameters,
            _: f64,
        ) -> Generalized {
            Generalized::default()
        }
        fn boom_moment(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> f64 {
            0.0
        }
    }

    /// `ΣX = −λ · m_x · u`, so that `u̇ = −λ u` exactly (F4.2 with `v = r = 0`).
    struct Decay(f64);

    impl ForceModel for Decay {
        fn generalized(
            &self,
            st: &BoatState,
            _: &Controls,
            p: &BoatParameters,
            _: f64,
        ) -> Generalized {
            let m_x = p.total_mass() + p.inertia.a_x;
            Generalized {
                x: -self.0 * m_x * st.u,
                ..Generalized::default()
            }
        }
        fn boom_moment(&self, _: &BoatState, _: &Controls, _: &BoatParameters, _: f64) -> f64 {
            0.0
        }
    }

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    /// Worst absolute error in `u` against `u0·e^(−λt)` over one second.
    fn decay_max_error(dt: f64, lambda: f64, method: Integrator) -> f64 {
        let p = params();
        let fm = Decay(lambda);
        let c = Controls::default();
        let u0 = 5.0;
        let mut st = BoatState {
            u: u0,
            l_sheet: 2.0,
            ..BoatState::ZERO
        };
        let steps = (1.0 / dt).round() as u32;
        let mut worst: f64 = 0.0;
        for _ in 0..steps {
            st = step(&st, &c, &p, &fm, dt, method);
            let exact = u0 * (-lambda * st.t).exp();
            worst = worst.max((st.u - exact).abs());
        }
        worst
    }

    #[test]
    fn rk2_matches_analytic_decay() {
        let lambda = 2.0;
        let coarse = decay_max_error(0.05, lambda, Integrator::Rk2Midpoint);
        let fine = decay_max_error(0.025, lambda, Integrator::Rk2Midpoint);
        assert!(coarse > 0.0 && fine > 0.0, "errors must be measurable");
        let ratio = coarse / fine;
        assert!(
            (3.6..=4.4).contains(&ratio),
            "RK2 is second order: expected a ratio in [3.6, 4.4], got {ratio} \
             (coarse {coarse:e}, fine {fine:e})"
        );
    }

    #[test]
    fn rk4_is_more_accurate_than_rk2() {
        // Rk4 is the section 10 convergence reference; prove it is wired up.
        let lambda = 2.0;
        let rk2 = decay_max_error(0.05, lambda, Integrator::Rk2Midpoint);
        let rk4 = decay_max_error(0.05, lambda, Integrator::Rk4);
        assert!(rk4 < rk2 * 1e-2, "rk4 {rk4:e} vs rk2 {rk2:e}");
    }

    #[test]
    fn clamp_in_derivative() {
        // A saturated rudder command must reach the same `delta_r` at `dt` and
        // `dt/2`. This only holds because the rate limit and the saturation
        // are applied inside every integration stage (F4.3).
        let p = params();
        let c = Controls {
            rudder_rate_cmd: 1.0,
            ..Controls::default()
        };
        let run = |dt: f64| {
            let mut st = BoatState {
                l_sheet: 2.0,
                ..BoatState::ZERO
            };
            let steps = (2.0 / dt).round() as u32;
            for _ in 0..steps {
                st = step(&st, &c, &p, &Zero, dt, Integrator::Rk2Midpoint);
            }
            st.delta_r
        };
        let coarse = run(0.005);
        let fine = run(0.0025);
        assert!(
            (coarse - fine).abs() < 1e-9,
            "delta_r differs with dt: {coarse} vs {fine}"
        );
        assert!((coarse - p.rudder.delta_r_max).abs() < 1e-12);
    }

    #[test]
    fn sheet_length_saturates_at_both_ends() {
        let p = params();
        let run = |cmd: f64, release: bool, dt: f64| {
            let c = Controls {
                sheet_rate_cmd: cmd,
                sheet_release: release,
                ..Controls::default()
            };
            let mut st = BoatState {
                l_sheet: 2.0,
                ..BoatState::ZERO
            };
            for _ in 0..(4.0 / dt).round() as u32 {
                st = step(&st, &c, &p, &Zero, dt, Integrator::Rk2Midpoint);
            }
            st.l_sheet
        };
        assert_eq!(run(-1.0, false, 0.005), p.sheet.l_sheet_min);
        assert_eq!(run(-1.0, false, 0.0025), p.sheet.l_sheet_min);
        assert_eq!(run(1.0, false, 0.005), p.sheet.l_sheet_max);
        assert_eq!(run(0.0, true, 0.005), p.sheet.l_sheet_max);
    }

    #[test]
    fn zero_force_zero_motion() {
        // brief §35 rest equilibrium: no forces, no velocity, neutral
        // controls. Every field but the clock is bit-identical after 10 000
        // steps; `t` is the simulation clock, whose derivative is 1 by
        // definition (F3), so it is excluded by construction, not by
        // weakening.
        let p = params();
        let c = Controls::default();
        let start = BoatState {
            x: 12.0,
            y: -7.0,
            psi: 0.6,
            phi: 0.0,
            l_sheet: 2.0,
            ..BoatState::ZERO
        };
        let mut st = start;
        for _ in 0..10_000 {
            st = step(&st, &c, &p, &Zero, p.sim.dt, Integrator::Rk2Midpoint);
        }
        let a = start.to_array();
        let b = st.to_array();
        for i in 0..a.len() - 1 {
            assert_eq!(
                a[i].to_bits(),
                b[i].to_bits(),
                "field {} moved: {} -> {}",
                crate::state::STATE_FIELDS[i],
                a[i],
                b[i]
            );
        }
        assert!(st.t > 0.0);
    }

    #[test]
    fn advance_n_equals_n_advance_1() {
        // F9.7 at the integrator level: `step` is a pure function of its
        // arguments, so batching cannot change the answer. The `Simulation`
        // level counterpart lives in `tests/determinism.rs`.
        let p = params();
        let c = Controls {
            rudder_rate_cmd: 0.7,
            sheet_rate_cmd: -0.3,
            ..Controls::default()
        };
        let start = BoatState {
            u: 2.0,
            v: 0.1,
            r: 0.05,
            l_sheet: 2.0,
            ..BoatState::ZERO
        };

        let advance = |from: &BoatState, n: u32| {
            let mut st = *from;
            for _ in 0..n {
                st = step(&st, &c, &p, &Decay(0.3), p.sim.dt, p.sim.integrator);
            }
            st
        };

        let batched = advance(&start, 1000);
        let single = {
            let mut st = start;
            for _ in 0..1000 {
                st = advance(&st, 1);
            }
            st
        };
        for (i, (x, y)) in batched
            .to_array()
            .iter()
            .zip(single.to_array().iter())
            .enumerate()
        {
            assert_eq!(
                x.to_bits(),
                y.to_bits(),
                "field {} differs",
                crate::state::STATE_FIELDS[i]
            );
        }
    }

    #[test]
    fn wrapping_happens_once_per_step() {
        let p = params();
        let mut st = BoatState {
            psi: std::f64::consts::PI - 1e-3,
            r: 10.0,
            phi: 4.0,
            l_sheet: 2.0,
            ..BoatState::ZERO
        };
        st = step(
            &st,
            &Controls::default(),
            &p,
            &Zero,
            0.01,
            Integrator::Rk2Midpoint,
        );
        assert!(st.psi > -std::f64::consts::PI && st.psi <= std::f64::consts::PI);
        // phi is never wrapped (F3, brief §17).
        assert_eq!(st.phi, 4.0);
    }

    #[test]
    fn every_method_holds_rest_equilibrium() {
        let p = params();
        for method in [
            Integrator::SemiImplicitEuler,
            Integrator::Rk2Midpoint,
            Integrator::Rk4,
        ] {
            let start = BoatState {
                l_sheet: 2.0,
                ..BoatState::ZERO
            };
            let mut st = start;
            for _ in 0..1000 {
                st = step(&st, &Controls::default(), &p, &Zero, p.sim.dt, method);
            }
            assert_eq!(st.u, 0.0, "{method:?}");
            assert_eq!(st.x, 0.0, "{method:?}");
            assert_eq!(st.psi, 0.0, "{method:?}");
        }
    }
}
