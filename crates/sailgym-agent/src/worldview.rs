//! `WorldView`: the containment boundary (v2 F14.4, section 05 task 5.2).
//!
//! A [`Sensor`](crate::sensor::Sensor) receives one of these and sees
//! everything in it. An [`Agent`](crate::spec::Agent) receives the concatenated
//! observation vector and nothing else. That single structural fact is what
//! makes "privileged information" enforceable by construction rather than by
//! discipline: there is no path from an agent to a `WindField`, and there is
//! nothing to remember not to pass.
//!
//! # What is in it, and what is deliberately not
//!
//! * **`controls`** is here because the actuator inner loop is part of the
//!   plant: a policy that cannot see the rate command in force is flying an
//!   aircraft whose stick position it cannot feel, and `imu` needs it anyway to
//!   evaluate the forces afresh (F14.7).
//! * **`guidance`, not `route`.** The source discussion carried
//!   `course: &Route`. It is not here, and that is the point: F15.5 and RV20
//!   require `signed_cross_track` to be computed in `sailgym-course`'s
//!   `guidance.rs` and **nowhere else**, and the surest way to prevent a sensor
//!   recomputing it is to make the route unreachable from the sensor. A sensor
//!   reports what the course layer already decided.
//! * **`others` is present and empty from day one** (F14.4, the swarm case), so
//!   a second boat is an additive change rather than a refactor. Nothing in
//!   this section reads it; a lidar or a fleet sensor will.
//! * **No obstacles.** `brief.md` S6 is deferred and section 04 shipped no
//!   `Obstacle`, so there is no field standing in for one.
//! * **No `Simulation` and no `ForceBreakdown`.** Trap 1: the cached breakdown
//!   is refreshed once per `advance(n)` call, so an observation built on it
//!   would depend on how the caller chunked its calls. A sensor that needs
//!   accelerations calls `forces::evaluate` itself, at the decision instant
//!   (F14.7).

use sailgym_course::Guidance;
use sailgym_physics::environment::WindField;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::state::{BoatState, Controls};

/// Everything a sensor may see (F14.4).
///
/// Borrowed, never owned: building one is free, so it is built at every
/// decision instant from the live simulation rather than cached.
pub struct WorldView<'a> {
    /// The published F3 state of this boat.
    pub st: &'a BoatState,
    /// The controls in force — the rate commands the inner loop is following.
    pub controls: &'a Controls,
    /// The resolved F7 catalogue. Read for geometry and limits; never written.
    pub p: &'a BoatParameters,
    /// The environment. **Sensors only**: F14.4 puts the boundary here.
    pub wind: &'a dyn WindField,
    /// What the course layer says the boat is being asked to sail, or `None`
    /// when the episode has no route.
    pub guidance: Option<&'a Guidance>,
    /// Other boats. Present and **empty** until the swarm case exists.
    pub others: &'a [BoatState],
    /// s, the simulated time of this decision instant. Never a wall clock.
    pub t: f64,
}

impl WorldView<'_> {
    /// True wind at **this boat's** position, world frame, sampled at `t`.
    ///
    /// The one place a sensor reaches the field. It is here rather than in each
    /// sensor so that "which sensors touched the true wind" is answerable by
    /// reading five call sites, and so that a sensor cannot quietly sample the
    /// field somewhere the boat is not (F14.4: derived guidance can still leak
    /// privileged information, so the leak has to be visible).
    pub fn true_wind_world(&self) -> sailgym_physics::vec::Vec2 {
        self.wind.sample(self.st.x, self.st.y, self.t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sailgym_physics::testkit::uniform_wind;

    #[test]
    fn a_world_view_carries_no_route_and_no_obstacles() {
        // RV20 and RV31 are closed structurally, not by review: the fields a
        // sensor could misuse are absent from the type.
        let src = include_str!("worldview.rs");
        let body = src
            .split_once("pub struct WorldView<'a> {")
            .expect("the struct")
            .1
            .split_once("\n}\n")
            .expect("a closed struct")
            .0;
        for forbidden in [
            "Route",
            "Obstacle",
            "Simulation",
            "ForceBreakdown",
            "Diagnostics",
        ] {
            assert!(
                !body.contains(forbidden),
                "WorldView carries a `{forbidden}`: {body}"
            );
        }
        assert!(body.contains("others"), "the swarm field must stay present");
    }

    #[test]
    fn true_wind_is_sampled_at_the_boat() {
        let wind = uniform_wind(5.0, 0.0); // from the north
        let st = BoatState {
            x: 10.0,
            y: -3.0,
            ..BoatState::ZERO
        };
        let p = BoatParameters::ilca7();
        let c = Controls::default();
        let w = WorldView {
            st: &st,
            controls: &c,
            p: &p,
            wind: &wind,
            guidance: None,
            others: &[],
            t: 1.25,
        };
        let expect = wind.sample(10.0, -3.0, 1.25);
        assert_eq!(w.true_wind_world().x.to_bits(), expect.x.to_bits());
        assert_eq!(w.true_wind_world().y.to_bits(), expect.y.to_bits());
        // Blowing toward the south: F6.1's "from" bearing of 0° is a northerly.
        assert_eq!(w.true_wind_world().x, 0.0);
        assert!((w.true_wind_world().y + 5.0).abs() < 1e-12);
    }
}
