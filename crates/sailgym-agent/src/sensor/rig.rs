//! `rig_state` and `actuator_state` — proprioception (section 05 task 5.3).
//!
//! Both sensors live here because they answer the same question from two ends
//! of the same rope: *what is the boat's own machinery doing right now?* The
//! PRD's `Owns:` list names four sensor files for five sensors, and this is the
//! file the pairing goes in; `docs/v2/progress/05-handoff.md` records it.
//!
//! # `actuator_state` is not optional
//!
//! `δr` is a **state variable** (F3) driven by a rate command (F4.3), so the
//! inner loop is part of the plant. A policy that can see neither the rudder
//! angle nor the command it is currently following is steering an aircraft
//! whose stick position it cannot feel: it cannot tell "the tiller is hard
//! over" from "the tiller is centred and I have just asked for hard over".
//!
//! # Sheet slack, and the one number that is not a coefficient
//!
//! `ℓ(β)` is the geometric rope path from the boom attachment to the block
//! (F6.8), and `L` is the available length. The rope is **slack** when
//! `L > ℓ`, which is exactly F6.8's `e = ℓ − L < 0` and the condition under
//! which `T = 0`. The column reports `(L − ℓ)` normalised by the sheet's own
//! travel `L_max − L_min`, so it is dimensionless and scales with the
//! catalogue rather than with a number chosen here. Every quantity in it comes
//! from `parameters.rs` through `rigging::mainsheet`; this file introduces no
//! literal at all.

use sailgym_physics::rigging::mainsheet::rope_path_length;
use sailgym_physics::rng::Pcg32;

use super::{FieldSpec, Sensor};
use crate::worldview::WorldView;

/// Boom angle, boom rate, sheet length and normalised sheet slack.
#[derive(Clone, Copy, Debug, Default)]
pub struct RigState;

impl RigState {
    pub const ID: &'static str = "rig_state";
    pub const VERSION: u32 = 1;
    pub const WIDTH: usize = 4;
}

impl Sensor for RigState {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn version(&self) -> u32 {
        Self::VERSION
    }

    fn width(&self) -> usize {
        Self::WIDTH
    }

    fn fields(&self) -> Vec<FieldSpec> {
        vec![
            // `beta` is wrapped to (−π, π] (F3).
            FieldSpec::sensed(
                "beta",
                "rad",
                Some(-std::f64::consts::PI),
                Some(std::f64::consts::PI),
            ),
            FieldSpec::sensed("beta_dot", "rad/s", None, None),
            // No declared bound: `L` is clamped to [l_sheet_min, l_sheet_max],
            // which are catalogue values that brief §31 makes live-editable. A
            // bound recorded here would be a copy of a parameter, and a copy
            // goes stale on the first edit (F7).
            FieldSpec::sensed("l_sheet", "m", None, None),
            FieldSpec::sensed("sheet_slack", "1", None, None)
                .normalised("(l_sheet - rope_path(beta)) / (l_sheet_max - l_sheet_min)"),
        ]
    }

    fn sense(&mut self, view: &WorldView, _rng: &mut Pcg32, out: &mut [f64]) {
        assert_eq!(
            out.len(),
            Self::WIDTH,
            "rig_state writes exactly {} scalars",
            Self::WIDTH
        );
        let st = view.st;
        let travel = view.p.sheet.l_sheet_max - view.p.sheet.l_sheet_min;
        let slack = st.l_sheet - rope_path_length(st.beta, view.p);
        out[0] = st.beta;
        out[1] = st.beta_dot;
        out[2] = st.l_sheet;
        out[3] = if travel > 0.0 { slack / travel } else { 0.0 };
    }
}

/// Rudder angle, and the rate command in force.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActuatorState;

impl ActuatorState {
    pub const ID: &'static str = "actuator_state";
    pub const VERSION: u32 = 1;
    pub const WIDTH: usize = 2;
}

impl Sensor for ActuatorState {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn version(&self) -> u32 {
        Self::VERSION
    }

    fn width(&self) -> usize {
        Self::WIDTH
    }

    fn fields(&self) -> Vec<FieldSpec> {
        vec![
            // Bounded by `delta_r_max`, a catalogue value: not copied here,
            // for the same reason `l_sheet` is not.
            FieldSpec::sensed("delta_r", "rad", None, None),
            // This one **is** bounded by the contract rather than by the
            // catalogue: F3 defines the rate command as normalised [−1, 1].
            FieldSpec::sensed("rudder_rate_cmd", "1", Some(-1.0), Some(1.0)),
        ]
    }

    fn sense(&mut self, view: &WorldView, _rng: &mut Pcg32, out: &mut [f64]) {
        assert_eq!(
            out.len(),
            Self::WIDTH,
            "actuator_state writes exactly {} scalars",
            Self::WIDTH
        );
        out[0] = view.st.delta_r;
        out[1] = view.controls.rudder_rate_cmd;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sailgym_physics::parameters::BoatParameters;
    use sailgym_physics::simulation::Simulation;
    use sailgym_physics::state::{BoatState, Controls};
    use sailgym_physics::testkit::{mirror_controls, mirror_state, still_air};

    fn read<S: Sensor>(mut s: S, st: &BoatState, c: &Controls, p: &BoatParameters) -> Vec<f64> {
        let air = still_air();
        let view = WorldView {
            st,
            controls: c,
            p,
            wind: &air,
            guidance: None,
            others: &[],
            t: st.t,
        };
        let mut rng = Pcg32::seed_from_u64(3);
        let mut out = vec![0.0; s.width()];
        s.sense(&view, &mut rng, &mut out);
        out
    }

    #[test]
    fn widths_match_the_declared_layouts() {
        for (w, fields, names, privileged) in [
            (
                RigState.width(),
                RigState.fields().len(),
                RigState.field_names().len(),
                RigState.privileged(),
            ),
            (
                ActuatorState.width(),
                ActuatorState.fields().len(),
                ActuatorState.field_names().len(),
                ActuatorState.privileged(),
            ),
        ] {
            assert_eq!(w, fields);
            assert_eq!(w, names);
            assert!(!privileged);
        }
    }

    /// A fresh simulation is two-blocked and **unloaded** (v2 F18.1b), so the
    /// rope path is exactly the available length and the slack column is zero.
    #[test]
    fn a_two_blocked_sheet_reports_no_slack() {
        let p = BoatParameters::ilca7();
        let st = Simulation::initial_state(&p);
        let out = read(RigState, &st, &Controls::default(), &p);
        assert_eq!(out[2], st.l_sheet);
        assert_eq!(out[3], 0.0, "a two-blocked sheet has no slack");
    }

    /// Easing the sheet without the boom following makes the column positive;
    /// hauling it in past the rope path makes it negative, which is the
    /// loaded branch of F6.8.
    #[test]
    fn slack_is_positive_when_rope_is_paid_out() {
        let p = BoatParameters::ilca7();
        let base = Simulation::initial_state(&p);
        let travel = p.sheet.l_sheet_max - p.sheet.l_sheet_min;

        let eased = BoatState {
            l_sheet: base.l_sheet + 0.5 * travel,
            ..base
        };
        let out = read(RigState, &eased, &Controls::default(), &p);
        assert!((out[3] - 0.5).abs() < 1e-12, "slack {}", out[3]);

        // With the boom swung out the rope path is longer than the available
        // length, so the rope is in tension and the column goes negative.
        let loaded = BoatState { beta: 1.0, ..base };
        let out = read(RigState, &loaded, &Controls::default(), &p);
        assert!(out[3] < 0.0, "a loaded sheet reads negative: {}", out[3]);
        let expect = loaded.l_sheet - rope_path_length(loaded.beta, &p);
        assert!(
            (out[3] * travel - expect).abs() < 1e-12,
            "{} vs {expect}",
            out[3] * travel
        );
    }

    #[test]
    fn the_actuator_reports_the_command_in_force_and_not_only_the_angle() {
        let p = BoatParameters::ilca7();
        let st = BoatState {
            delta_r: 0.3,
            ..BoatState::ZERO
        };
        // Same angle, opposite commands: a sensor that reported only the angle
        // could not tell these apart, and neither could a policy.
        let hard_over = Controls {
            rudder_rate_cmd: 1.0,
            ..Controls::default()
        };
        let coming_back = Controls {
            rudder_rate_cmd: -1.0,
            ..Controls::default()
        };
        assert_eq!(read(ActuatorState, &st, &hard_over, &p), vec![0.3, 1.0]);
        assert_eq!(read(ActuatorState, &st, &coming_back, &p), vec![0.3, -1.0]);
    }

    /// R3: the signed columns flip under the mirror and the lengths do not.
    #[test]
    fn the_rig_and_actuator_mirror() {
        let p = BoatParameters::ilca7();
        let st = BoatState {
            beta: 0.7,
            beta_dot: -0.2,
            delta_r: 0.25,
            l_sheet: 2.6,
            phi: 0.3,
            psi: -0.4,
            u: 2.0,
            ..BoatState::ZERO
        };
        let c = Controls {
            rudder_rate_cmd: 0.6,
            sheet_rate_cmd: -0.2,
            sheet_release: false,
        };
        let a = read(RigState, &st, &c, &p);
        let b = read(RigState, &mirror_state(&st), &mirror_controls(&c), &p);
        assert!((b[0] + a[0]).abs() < 1e-15, "beta");
        assert!((b[1] + a[1]).abs() < 1e-15, "beta_dot");
        assert_eq!(b[2], a[2], "l_sheet has no side");
        assert!((b[3] - a[3]).abs() < 1e-15, "slack has no side");

        let a = read(ActuatorState, &st, &c, &p);
        let b = read(ActuatorState, &mirror_state(&st), &mirror_controls(&c), &p);
        assert!((b[0] + a[0]).abs() < 1e-15, "delta_r");
        assert!((b[1] + a[1]).abs() < 1e-15, "rudder_rate_cmd");
    }

    /// RV31 again: neither sensor emits an absolute coordinate, and moving the
    /// boat does not change either reading.
    #[test]
    fn neither_sensor_depends_on_where_the_boat_is() {
        let p = BoatParameters::ilca7();
        let c = Controls {
            rudder_rate_cmd: -0.4,
            ..Controls::default()
        };
        let here = BoatState {
            beta: 0.5,
            l_sheet: 2.0,
            delta_r: -0.1,
            ..BoatState::ZERO
        };
        let there = BoatState {
            x: 1234.0,
            y: -99.0,
            psi: 2.0,
            ..here
        };
        assert_eq!(
            read(RigState, &here, &c, &p),
            read(RigState, &there, &c, &p)
        );
        assert_eq!(
            read(ActuatorState, &here, &c, &p),
            read(ActuatorState, &there, &c, &p)
        );
        for name in RigState
            .field_names()
            .into_iter()
            .chain(ActuatorState.field_names())
        {
            for forbidden in ["x", "y", "psi", "heading"] {
                assert_ne!(name, forbidden, "`{name}` is an absolute quantity");
            }
        }
    }
}
