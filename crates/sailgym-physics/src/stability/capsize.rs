//! Capsize reporting (F6.10) — **informational only**.
//!
//! `capsized` is an *output*. It is written here, once per completed step,
//! from `Simulation::advance`, and read by the UI. Nothing in `forces/`,
//! `aero/`, `hydro/`, `rigging/` or `dynamics.rs` reads it, and no force or
//! moment anywhere branches on it (brief §7, §17, §46). The simulation keeps
//! integrating through and past a capsize; `tests/no_shortcuts.rs` is the
//! mechanical audit that says so.
//!
//! Deferred, per F6.10 and brief §17: sail immersion, mast and sail water
//! drag, flooding, the sailor leaving the boat, the righting procedure.

use crate::parameters::BoatParameters;
use crate::state::BoatState;

/// What the UI is told about a capsize. Reported, never acted on.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct CapsizeState {
    /// `|φ|` has exceeded `phi_capsize` continuously for `t_capsize`.
    pub capsized: bool,
    /// s. The simulation time at which the *current* capsize's threshold
    /// crossing happened; `0.0` when the boat is not over the threshold.
    pub since: f64,
    /// rad. The largest `|φ|` seen since the last reset. Never cleared by a
    /// recovery — it is the episode's high-water mark.
    pub max_heel: f64,
    /// The pending crossing, while the timer runs. Private, and excluded from
    /// the serialised record: the public surface is exactly the three fields
    /// above (F6.10, task 7.4).
    #[serde(skip)]
    crossed_at: Option<f64>,
}

impl CapsizeState {
    /// Fold one completed step into the report.
    ///
    /// Called from `Simulation::advance` and **never** from `derivative`,
    /// which takes `&BoatState` and must stay a pure function of it: a
    /// multi-stage integrator evaluates the derivative at points that are not
    /// states the simulation ever visits, so a timer driven from there would
    /// depend on the integrator rather than on the trajectory.
    ///
    /// The flag clears when the boat comes back inside the threshold. It
    /// describes the boat *now*, which is what brief §29's "capsize state"
    /// readout means; `max_heel` is what remembers the episode.
    pub fn update(&mut self, phi: f64, t: f64, p: &BoatParameters) {
        let heel = phi.abs();
        if heel > self.max_heel {
            self.max_heel = heel;
        }
        if heel > p.stability.phi_capsize {
            let crossed = *self.crossed_at.get_or_insert(t);
            if t - crossed >= p.stability.t_capsize {
                self.capsized = true;
                self.since = crossed;
            }
        } else {
            self.crossed_at = None;
            self.capsized = false;
            self.since = 0.0;
        }
    }

    /// Fold a whole state in, for callers that already hold one.
    pub fn observe(&mut self, st: &BoatState, p: &BoatParameters) {
        self.update(st.phi, st.t, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::wind::{WindConfig, WindMode};
    use crate::parameters::BoatParameters;
    use crate::simulation::Simulation;
    use crate::stability::hydrostatics::GzCurve;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn params() -> BoatParameters {
        BoatParameters::ilca7()
    }

    /// Feed a held heel angle to the report at the simulation timestep.
    fn hold(
        state: &mut CapsizeState,
        heel: f64,
        from: f64,
        seconds: f64,
        p: &BoatParameters,
    ) -> f64 {
        let mut t = from;
        let steps = (seconds / p.sim.dt).round() as u32;
        for _ in 0..steps {
            t += p.sim.dt;
            state.update(heel, t, p);
        }
        t
    }

    #[test]
    fn not_capsized_below_threshold() {
        let p = params();
        let mut c = CapsizeState::default();
        hold(&mut c, p.stability.phi_capsize - 1e-6, 0.0, 10.0, &p);
        assert!(!c.capsized);
        assert_eq!(c.since, 0.0);
        // The high-water mark is still recorded.
        assert!(c.max_heel > 0.0);
    }

    #[test]
    fn requires_duration() {
        let p = params();
        let mut c = CapsizeState::default();
        let over = p.stability.phi_capsize + 0.1;
        let t = hold(&mut c, over, 0.0, p.stability.t_capsize / 2.0, &p);
        assert!(!c.capsized, "fired after only half the duration");
        hold(&mut c, 0.0, t, 5.0, &p);
        assert!(!c.capsized);
        assert_eq!(c.since, 0.0);
        // A fresh excursion starts a fresh timer, rather than resuming.
        let t = hold(&mut c, over, 5.0 + t, p.stability.t_capsize / 2.0, &p);
        assert!(!c.capsized, "the two half-excursions were added together");
        let _ = t;
    }

    #[test]
    fn sets_after_duration() {
        let p = params();
        let mut c = CapsizeState::default();
        // 3 s below the threshold first, so `since` is a real crossing time
        // rather than the start of the episode.
        let crossing = hold(&mut c, 0.5, 0.0, 3.0, &p);
        hold(&mut c, PI / 2.0, crossing, p.stability.t_capsize + 0.05, &p);
        assert!(c.capsized);
        assert!(
            (c.since - crossing).abs() <= p.sim.dt,
            "since = {}, crossing = {crossing}",
            c.since
        );
        assert!((c.max_heel - PI / 2.0).abs() < 1e-12);
    }

    /// A beam reach in a squall, sheeted hard in and never eased.
    fn squall(speed: f64) -> Simulation {
        let p = params();
        let mut sim = Simulation::new(p, 7);
        sim.set_wind(WindConfig {
            mode: WindMode::Uniform,
            speed,
            bearing_deg: 0.0,
            ..Default::default()
        });
        sim.reset(Simulation::initial_state(&p), 7);
        sim
    }

    #[test]
    fn simulation_continues_past_90_degrees() {
        // brief §17's explicit requirement: the boat must be able to pass
        // dynamically through `|φ| > 90°`, and the simulation must keep
        // running afterwards rather than terminating or clamping.
        let p = params();
        let mut sim = squall(20.0);
        let mut crossed = None;
        for _ in 0..(30.0 / p.sim.dt) as u32 {
            sim.advance(1);
            if crossed.is_none() && sim.state().phi.abs() > FRAC_PI_2 {
                crossed = Some(sim.state().t);
            }
        }
        let crossed = crossed.expect("never reached 90 degrees of heel");

        // A further 30 s past the crossing, still integrating.
        let before = sim.state().t;
        for _ in 0..(30.0 / p.sim.dt) as u32 {
            sim.advance(1);
            assert!(
                sim.state().is_finite(),
                "state went non-finite: {:?}",
                sim.state()
            );
        }
        assert!(sim.state().t > before + 29.0);
        assert!(sim.capsize().capsized, "the report never fired");
        assert!(
            sim.capsize().max_heel > FRAC_PI_2,
            "crossed at {crossed} but max_heel is {}",
            sim.capsize().max_heel
        );
    }

    #[test]
    fn passes_through_inversion() {
        // Driven past `φ = π` by an initial condition plus roll rate. `φ` is
        // never wrapped (F3), `GZ` stays finite, and its sign stays the one
        // the odd, 2π-periodic series gives — which is what lets the boat roll
        // on through instead of sticking at inversion.
        let p = params();
        let curve = GzCurve::from_params(&p);
        let mut sim = squall(0.0);
        sim.reset(
            BoatState {
                phi: 3.0,
                p: 4.0,
                ..Simulation::initial_state(&p)
            },
            7,
        );

        let mut beyond_pi = false;
        for _ in 0..(20.0 / p.sim.dt) as u32 {
            sim.advance(1);
            let phi = sim.state().phi;
            assert!(sim.state().is_finite());
            assert!(curve.gz(phi).is_finite());
            if phi > PI {
                beyond_pi = true;
                // Never wrapped into (−π, π].
                assert!(sim.state().phi > PI);
            }
        }
        assert!(beyond_pi, "never reached inversion: {}", sim.state().phi);

        // The sign contract, stated on both sides of inversion. `GZ` is odd
        // and 2π-periodic, and under v2 F18.1a it is **negative** on the whole
        // of `(φ_v, π)`: rule 5 forbids the boat regaining positive stability
        // between the vanishing angle and inversion. So `φ = π` is a *stable*
        // equilibrium — a turtled dinghy stays turtled — where v1's curve made
        // it unstable and rolled the boat back upright from 140° of heel.
        // Righting a capsized boat stays deferred; this asserts the sign, not
        // a recovery model.
        assert!(curve.gz(PI - 0.05) < 0.0);
        // `PI` is not exactly π in binary, so `GZ` there is the rounding of
        // zero rather than zero itself.
        assert!(curve.gz(PI).abs() < 1e-15, "gz(pi) = {}", curve.gz(PI));
        assert!(curve.gz(PI + 0.05) > 0.0);
        for phi in [3.3, 4.0, 5.5, -3.3, -4.9] {
            assert!(
                (curve.gz(phi) - curve.gz(phi - std::f64::consts::TAU)).abs() < 1e-12,
                "not 2*pi-periodic at {phi}"
            );
        }
    }

    #[test]
    fn capsize_flag_not_read_by_physics() {
        // F6.10, brief §7/§17/§46: `capsized` is an output. Nothing that
        // produces a force or a moment may see it.
        for (name, src) in [
            ("forces/mod.rs", include_str!("../forces/mod.rs")),
            ("aero/apparent.rs", include_str!("../aero/apparent.rs")),
            ("aero/sail.rs", include_str!("../aero/sail.rs")),
            ("hydro/hull.rs", include_str!("../hydro/hull.rs")),
            (
                "hydro/centerboard.rs",
                include_str!("../hydro/centerboard.rs"),
            ),
            ("hydro/rudder.rs", include_str!("../hydro/rudder.rs")),
            ("rigging/boom.rs", include_str!("../rigging/boom.rs")),
            (
                "rigging/mainsheet.rs",
                include_str!("../rigging/mainsheet.rs"),
            ),
            ("dynamics.rs", include_str!("../dynamics.rs")),
        ] {
            // Built at run time so this test cannot match its own needle.
            let needle: String = ["cap", "sized"].concat();
            assert!(
                !src.contains(needle.as_str()),
                "{name} mentions the capsize flag; it is an output (F6.10)"
            );
        }
    }

    #[test]
    fn derivative_does_not_take_capsize_state() {
        // `update` runs from `Simulation::advance`, once per completed step,
        // and never from `derivative` — which must stay a pure function of
        // `&BoatState`, or a multi-stage integrator would feed the report
        // states the boat never occupies.
        let dynamics = include_str!("../dynamics.rs");
        let signature = dynamics
            .split("pub fn derivative(")
            .nth(1)
            .expect("derivative is declared in dynamics.rs");
        let args = signature.split(')').next().expect("argument list");
        assert!(args.contains("st: &BoatState"));
        assert!(!args.contains("CapsizeState"));
        assert!(!args.contains("&mut"));

        let simulation = include_str!("../simulation.rs");
        assert!(
            simulation.contains("self.capsize.observe("),
            "Simulation::advance must be the one caller of the capsize report"
        );
    }
}
