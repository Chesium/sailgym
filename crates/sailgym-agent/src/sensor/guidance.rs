//! `guidance` — what the task is asking for (section 05 task 5.3).
//!
//! Five columns: cross-track, bearing to the target relative to heading,
//! distance to it, the leg's bearing relative to the wind, and the rounding
//! side.
//!
//! # Nothing here recomputes anything
//!
//! `signed_cross_track` is computed in `sailgym-course`'s `guidance.rs` and
//! **nowhere else** (F15.5 §4, RV20). This sensor reports it. It cannot do
//! otherwise: [`WorldView`] carries no `Route`, so there is no geometry here to
//! recompute it from, and the section-04 handoff's warning — "an agent that
//! recomputes cross-track from `Guidance::target` has created RV20's second
//! definition, and the only thing that would catch it is a reviewer" — is
//! answered by the type rather than by a reviewer.
//!
//! # One column is privileged, and it says so
//!
//! `leg_bearing_vs_wind` is derived from the **true** wind, which no instrument
//! on the boat measures (RV30, RV31). A boat derives true wind from apparent
//! wind and boat speed, and a policy with no speed log has no honest route to
//! it. So the column is marked `privileged: true` in the layout, it travels
//! that way into the episode header, and an arm that must not see privileged
//! information drops it by filtering the layout rather than by remembering to.
//!
//! The other four columns are **not** privileged: cross-track, bearing,
//! distance and the required side are the task telling the boat what it is
//! being asked to do, which a sailor is told before the start.
//!
//! # No absolute position, and no absolute heading
//!
//! Every column is relative: the bearing is relative to the boat's own heading,
//! the distance is a magnitude, the cross-track is measured from the leg. A
//! policy that learned the course by absolute position would have learned the
//! course and not sailing (RV31), and
//! `tests::the_guidance_columns_are_invariant_under_a_rigid_motion` is what says
//! the columns cannot be carrying one.

use sailgym_course::route::position;
use sailgym_physics::frames::wrap_pi;
use sailgym_physics::rng::Pcg32;

use super::{FieldSpec, Sensor};
use crate::worldview::WorldView;

/// The task, as an observation.
#[derive(Clone, Copy, Debug, Default)]
pub struct GuidanceSensor;

impl GuidanceSensor {
    pub const ID: &'static str = "guidance";
    pub const VERSION: u32 = 1;
    pub const WIDTH: usize = 5;
}

impl Sensor for GuidanceSensor {
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
        let pi = std::f64::consts::PI;
        vec![
            // Positive to port of the leg's direction of travel (F15.5 §4).
            FieldSpec::sensed("cross_track", "m", None, None),
            // Positive to port of the bow (F2: `+y_H` is port).
            FieldSpec::sensed("bearing_to_target", "rad", Some(-pi), Some(pi)),
            FieldSpec::sensed("distance_to_target", "m", Some(0.0), None),
            // Derived from the TRUE wind: privileged, and it says so.
            FieldSpec::sensed("leg_bearing_vs_wind", "rad", Some(-pi), Some(pi)).mark_privileged(),
            // +1 = leave the mark to starboard, −1 = to port, 0 = either or a
            // gate (`Rounding::required_side`, the one place that sentence is
            // written down).
            FieldSpec::sensed("rounding_side", "1", Some(-1.0), Some(1.0)),
        ]
    }

    fn sense(&mut self, view: &WorldView, _rng: &mut Pcg32, out: &mut [f64]) {
        assert_eq!(
            out.len(),
            Self::WIDTH,
            "guidance writes exactly {} scalars",
            Self::WIDTH
        );
        // No route, no task: every column is zero. A layout is frozen at reset
        // (F14.3), so an episode that has no route should not configure this
        // sensor at all; zeros are what it reads if one does.
        let Some(g) = view.guidance else {
            out.fill(0.0);
            return;
        };

        let here = position(view.st);
        let to_target = g.target - here;

        out[0] = g.signed_cross_track;
        out[1] = if to_target.x == 0.0 && to_target.y == 0.0 {
            0.0
        } else {
            wrap_pi(to_target.y.atan2(to_target.x) - view.st.psi)
        };
        out[2] = to_target.length();
        out[3] = match g.line {
            // The angle from the direction the wind is blowing **from** to the
            // direction the leg runs: 0 is a dead beat, ±π is a dead run, and
            // the sign is positive when the leg lies to port of the wind's
            // eye. `wind.sample` returns the direction the air is blowing
            // toward (F6.1), so the eye is its negation.
            Some((a, b)) => {
                let leg = b - a;
                let w = view.true_wind_world();
                if (leg.x == 0.0 && leg.y == 0.0) || (w.x == 0.0 && w.y == 0.0) {
                    0.0
                } else {
                    wrap_pi(leg.y.atan2(leg.x) - (-w.y).atan2(-w.x))
                }
            }
            // F15.1's single-mark route has no leg, so it has no bearing.
            None => 0.0,
        };
        out[4] = g.rounding.required_side().unwrap_or(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sailgym_course::guidance::{guidance, CourseParams};
    use sailgym_course::route::{Mark, Route};
    use sailgym_course::Rounding;
    use sailgym_physics::parameters::BoatParameters;
    use sailgym_physics::state::{BoatState, Controls};
    use sailgym_physics::testkit::{mirror_state, uniform_wind};
    use sailgym_physics::vec::Vec2;

    const PI: f64 = std::f64::consts::PI;

    /// A two-mark route whose leg 0 runs from `a` to `b` — the shape
    /// `sailgym-course`'s own tests use, because a `Route` is a circuit and
    /// leg 0 comes from the **last** mark (F15.5 §1).
    fn leg_route(a: Vec2, b: Vec2, rounding: Rounding) -> Route {
        Route::new(
            vec![
                Mark {
                    position: b,
                    radius: 2.0,
                    rounding,
                },
                Mark {
                    position: a,
                    radius: 2.0,
                    rounding: Rounding::Either,
                },
            ],
            1,
        )
    }

    fn read(
        route: &Route,
        st: &BoatState,
        wind: &dyn sailgym_physics::environment::WindField,
    ) -> [f64; GuidanceSensor::WIDTH] {
        let p = BoatParameters::ilca7();
        let c = Controls::default();
        let g = guidance(route, 0, st, &CourseParams::default()).expect("leg 0 exists");
        let view = WorldView {
            st,
            controls: &c,
            p: &p,
            wind,
            guidance: Some(&g),
            others: &[],
            t: st.t,
        };
        let mut rng = Pcg32::seed_from_u64(4);
        let mut out = [0.0; GuidanceSensor::WIDTH];
        let mut s = GuidanceSensor;
        s.sense(&view, &mut rng, &mut out);
        out
    }

    #[test]
    fn width_matches_the_declared_layout() {
        assert_eq!(GuidanceSensor.width(), GuidanceSensor.fields().len());
        assert_eq!(GuidanceSensor.width(), GuidanceSensor.field_names().len());
    }

    /// The privilege flag is on exactly one column, and it is the true-wind
    /// one (RV30, RV31). Tested separately, as the PRD asks.
    #[test]
    fn exactly_one_guidance_column_is_privileged_and_it_is_the_true_wind_one() {
        let fields = GuidanceSensor.fields();
        let privileged: Vec<&str> = fields
            .iter()
            .filter(|f| f.privileged)
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(privileged, vec!["leg_bearing_vs_wind"]);
        assert!(
            GuidanceSensor.privileged(),
            "the sensor-level flag is any column privileged (F14.3)"
        );

        // And the flag is not decoration: that column is the only one whose
        // value moves when the wind moves and nothing else does.
        let route = leg_route(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Rounding::Port);
        let st = BoatState {
            x: 30.0,
            y: 5.0,
            ..BoatState::ZERO
        };
        let a = read(&route, &st, &uniform_wind(5.0, 0.0));
        let b = read(&route, &st, &uniform_wind(5.0, 90.0));
        for k in [0usize, 1, 2, 4] {
            assert_eq!(a[k], b[k], "column {k} moved with the wind");
        }
        assert_ne!(
            a[3], b[3],
            "the privileged column did not move with the wind"
        );
    }

    /// The reported cross-track is the course layer's own number, bit for bit
    /// (RV20).
    #[test]
    fn cross_track_is_reported_and_never_recomputed() {
        let route = leg_route(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Rounding::Port);
        for y in [-7.5, -0.25, 0.0, 3.0, 18.0] {
            let st = BoatState {
                x: 40.0,
                y,
                ..BoatState::ZERO
            };
            let g = guidance(&route, 0, &st, &CourseParams::default()).expect("leg 0");
            let out = read(&route, &st, &uniform_wind(5.0, 0.0));
            assert_eq!(
                out[0].to_bits(),
                g.signed_cross_track.to_bits(),
                "the sensor must report the course layer's number"
            );
        }
        // Positive to port of an east-running leg: north of it (F15.5 §4).
        let north = BoatState {
            x: 40.0,
            y: 9.0,
            ..BoatState::ZERO
        };
        assert!(read(&route, &north, &uniform_wind(5.0, 0.0))[0] > 0.0);
    }

    /// The bearing column is relative to the bow, positive to port.
    #[test]
    fn the_bearing_is_relative_to_the_bow() {
        let route = leg_route(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Rounding::Either);
        let wind = uniform_wind(5.0, 0.0);

        // On the leg, heading east: the target is dead ahead.
        let ahead = BoatState {
            x: 10.0,
            ..BoatState::ZERO
        };
        let out = read(&route, &ahead, &wind);
        assert!(out[1].abs() < 1e-12, "bearing {}", out[1]);
        assert!((out[2] - CourseParams::DEFAULT_LOOKAHEAD).abs() < 1e-12);

        // Same place, heading north: the target is now off the starboard bow.
        let turned = BoatState {
            psi: PI / 2.0,
            ..ahead
        };
        let out = read(&route, &turned, &wind);
        assert!((out[1] + PI / 2.0).abs() < 1e-12, "bearing {}", out[1]);
    }

    /// The leg-versus-wind column: 0 is a dead beat, ±π a dead run.
    #[test]
    fn the_leg_bearing_is_measured_from_the_winds_eye() {
        let st = BoatState {
            x: 10.0,
            ..BoatState::ZERO
        };
        // An east-running leg with the wind from the east: sailing straight at
        // the eye of the wind.
        let east = leg_route(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Rounding::Either);
        let from_east = uniform_wind(5.0, 90.0);
        assert!(read(&east, &st, &from_east)[3].abs() < 1e-12);

        // The same leg with the wind from the west: a dead run.
        let from_west = uniform_wind(5.0, 270.0);
        assert!((read(&east, &st, &from_west)[3].abs() - PI).abs() < 1e-12);

        // Wind from the north, leg running east: the leg lies 90° from the
        // eye of the wind, clockwise from it, so the column is negative.
        let from_north = uniform_wind(5.0, 0.0);
        assert!((read(&east, &st, &from_north)[3] + PI / 2.0).abs() < 1e-12);
    }

    #[test]
    fn the_rounding_side_is_the_course_layers_own_convention() {
        let st = BoatState {
            x: 10.0,
            ..BoatState::ZERO
        };
        let wind = uniform_wind(5.0, 0.0);
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(100.0, 0.0);
        assert_eq!(
            read(&leg_route(a, b, Rounding::Starboard), &st, &wind)[4],
            1.0
        );
        assert_eq!(read(&leg_route(a, b, Rounding::Port), &st, &wind)[4], -1.0);
        assert_eq!(read(&leg_route(a, b, Rounding::Either), &st, &wind)[4], 0.0);
    }

    /// RV31: translating and rotating the whole problem — boat, route and wind
    /// — leaves every column where it was. A column carrying absolute pose
    /// could not survive this.
    #[test]
    fn the_guidance_columns_are_invariant_under_a_rigid_motion() {
        let st = BoatState {
            x: 30.0,
            y: 6.0,
            psi: 0.3,
            ..BoatState::ZERO
        };
        let route = leg_route(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Rounding::Port);
        let before = read(&route, &st, &uniform_wind(5.0, 20.0));

        // Rotate everything by theta about the origin and shift it.
        let theta = 0.9_f64;
        let (s, c) = theta.sin_cos();
        let map = |v: Vec2| Vec2::new(v.x * c - v.y * s + 250.0, v.x * s + v.y * c - 400.0);
        let moved_route = leg_route(
            map(Vec2::new(0.0, 0.0)),
            map(Vec2::new(100.0, 0.0)),
            Rounding::Port,
        );
        let moved = BoatState {
            x: map(Vec2::new(st.x, st.y)).x,
            y: map(Vec2::new(st.x, st.y)).y,
            psi: st.psi + theta,
            ..st
        };
        // The field's FROM bearing is CW from north while psi is CCW from
        // world +x, so rotating the world by +theta takes the bearing to
        // b − theta (F6.1).
        let after = read(
            &moved_route,
            &moved,
            &uniform_wind(5.0, 20.0 - theta.to_degrees()),
        );
        for k in 0..GuidanceSensor::WIDTH {
            assert!(
                (after[k] - before[k]).abs() < 1e-9,
                "column {k}: {} vs {}",
                after[k],
                before[k]
            );
        }
    }

    /// The mirror negates the signed columns and leaves the magnitudes (R3).
    #[test]
    fn the_guidance_columns_mirror() {
        let st = BoatState {
            x: 30.0,
            y: 6.0,
            psi: 0.3,
            ..BoatState::ZERO
        };
        let route = leg_route(Vec2::new(0.0, 0.0), Vec2::new(100.0, 20.0), Rounding::Port);
        let before = read(&route, &st, &uniform_wind(5.0, 20.0));
        let after = read(
            &route.mirrored(),
            &mirror_state(&st),
            &uniform_wind(5.0, 180.0 - 20.0),
        );
        for (k, flips) in [true, true, false, true, true].into_iter().enumerate() {
            let expect = if flips { -before[k] } else { before[k] };
            assert!(
                (after[k] - expect).abs() < 1e-9,
                "column {k}: {} vs {expect}",
                after[k]
            );
        }
    }

    #[test]
    fn no_route_reads_zero() {
        let p = BoatParameters::ilca7();
        let c = Controls::default();
        let air = uniform_wind(5.0, 0.0);
        let st = BoatState {
            x: 10.0,
            y: 20.0,
            ..BoatState::ZERO
        };
        let view = WorldView {
            st: &st,
            controls: &c,
            p: &p,
            wind: &air,
            guidance: None,
            others: &[],
            t: 0.0,
        };
        let mut rng = Pcg32::seed_from_u64(4);
        let mut out = [1.0; GuidanceSensor::WIDTH];
        let mut s = GuidanceSensor;
        s.sense(&view, &mut rng, &mut out);
        assert_eq!(out, [0.0; GuidanceSensor::WIDTH]);
    }
}
