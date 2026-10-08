//! Numbered waypoint courses: an open route with one gate per waypoint (v2
//! section 12, D2).
//!
//! # Why a waypoint is a gate
//!
//! "Sail through waypoint 3" is not a radius check (F15.3, RV19), and it is
//! not [`Rounding::Either`] either. On a leg, `Either` requires only a
//! directed crossing of the mark's perpendicular **line**, with no lateral
//! bound, so an `Either` waypoint 200 m off the track is passed by a boat that
//! never went near it. A gate is a directed crossing of a **segment**, which
//! is exactly "through the waypoint, within `half_width` of it" — and F15.3
//! already defines it, so this file adds geometry and no rule.
//!
//! # Square and centred is load-bearing
//!
//! Each gate is `2·half_width` wide, square to the leg arriving at its
//! waypoint, and centred on it. That makes the gate's line the same line as
//! the plane v2 F19.4's probe tests — the plane through the mark's position,
//! perpendicular to the leg — so [`crate::passage::cut_between`] reports
//! "crossed the line outside the posts" as a cut and nothing else as one
//! (RV65, RV66).
//!
//! # Turning at a waypoint needs a little overshoot
//!
//! Passage is half-open: the boat must get **strictly** past the gate line
//! (F15.3, `passage.rs`). A track that touches the centre of an acute
//! waypoint and turns away on the same step has not passed it. A boat steering
//! at the waypoint keeps going until it is through, because guidance only
//! moves on after the passage; a synthetic test track has to overshoot, and
//! `tests/waypoints.rs`'s do.
//!
//! # What this is not
//!
//! No physics, and no number here is a physical coefficient (v1 brief §43, v2
//! F14.9). `half_width` is course geometry — task configuration a course
//! file carries — and the shipped values are in `courses/`.

use crate::passage::left_normal;
use crate::route::{Mark, Rounding, Route, RouteError};
use sailgym_physics::vec::Vec2;

impl Route {
    /// An open course from `start` through `points` in order: one gate per
    /// point, `2·half_width` wide, square to the leg arriving at it, centred
    /// on it. One lap.
    ///
    /// Waypoint `i`'s posts are `pᵢ ∓ half_width · n̂ᵢ`, with `n̂ᵢ` the left
    /// normal of the leg arriving at `pᵢ` — from `start` for the first. Each
    /// [`Mark::position`] is its waypoint and each [`Mark::radius`] is
    /// `half_width`: passage ignores a gate's radius, and a display draws it.
    ///
    /// Every refusal is [`Route::validate`]'s, naming the offending waypoint
    /// by its index into `points`: no points, a non-finite start or point, a
    /// waypoint on top of the previous one or of the start, a negative or
    /// non-finite `half_width`, and a zero `half_width`, whose posts coincide.
    pub fn waypoints(start: Vec2, points: &[Vec2], half_width: f64) -> Result<Route, RouteError> {
        // The skeleton first, so every geometric refusal is `validate`'s and
        // every leg below is known to have a direction.
        let mut route = Route::new(
            points
                .iter()
                .map(|&p| Mark::either(p, half_width))
                .collect(),
            1,
        )
        .with_start(start);
        route.validate()?;

        for i in 0..route.marks.len() {
            let leg = route
                .leg(i as u32)
                .expect("a validated one-lap route has one leg per mark");
            let d = leg
                .direction()
                .expect("every leg of a validated route with a start has a direction");
            let n = left_normal(d);
            let p = route.marks[i].position;
            route.marks[i].rounding = Rounding::Gate(p - n * half_width, p + n * half_width);
        }

        // Again, for the one refusal only the posts can produce: a zero
        // `half_width`.
        route.validate()?;
        Ok(route)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn on_axis_aligned_legs_the_posts_are_exact() {
        // East, then north, then west: three legs whose normals are exact.
        let route = Route::waypoints(
            at(0.0, 0.0),
            &[at(30.0, 0.0), at(30.0, 20.0), at(10.0, 20.0)],
            5.0,
        )
        .expect("a valid course");
        assert_eq!(route.laps, 1);
        assert_eq!(route.start, Some(at(0.0, 0.0)));
        assert_eq!(
            route.marks[0].rounding,
            Rounding::Gate(at(30.0, -5.0), at(30.0, 5.0))
        );
        assert_eq!(
            route.marks[1].rounding,
            Rounding::Gate(at(35.0, 20.0), at(25.0, 20.0))
        );
        assert_eq!(
            route.marks[2].rounding,
            Rounding::Gate(at(10.0, 25.0), at(10.0, 15.0))
        );
        for (mark, p) in route
            .marks
            .iter()
            .zip([at(30.0, 0.0), at(30.0, 20.0), at(10.0, 20.0)])
        {
            assert_eq!(mark.position, p);
            assert_eq!(mark.radius, 5.0);
        }
    }

    #[test]
    fn every_refusal_names_the_waypoint() {
        let p = [at(10.0, 0.0), at(10.0, 10.0)];
        assert_eq!(
            Route::waypoints(at(0.0, 0.0), &[], 5.0),
            Err(RouteError::NoMarks)
        );
        assert_eq!(
            Route::waypoints(at(f64::NAN, 0.0), &p, 5.0),
            Err(RouteError::StartNotFinite)
        );
        assert_eq!(
            Route::waypoints(at(10.0, 0.0), &p, 5.0),
            Err(RouteError::ZeroLengthLeg { mark: 0 })
        );
        assert_eq!(
            Route::waypoints(at(0.0, 0.0), &[p[0], p[0]], 5.0),
            Err(RouteError::ZeroLengthLeg { mark: 1 })
        );
        assert_eq!(
            Route::waypoints(at(0.0, 0.0), &p, -1.0),
            Err(RouteError::NegativeRadius { mark: 0 })
        );
        assert_eq!(
            Route::waypoints(at(0.0, 0.0), &p, f64::INFINITY),
            Err(RouteError::NotFinite {
                mark: 0,
                field: "radius"
            })
        );
        assert_eq!(
            Route::waypoints(at(0.0, 0.0), &p, 0.0),
            Err(RouteError::GatePostsCoincide { mark: 0 })
        );
    }

    #[test]
    fn a_course_may_return_through_an_earlier_waypoint() {
        // A, B, A — up to the windward mark, down, and up again.
        let a = at(0.0, 40.0);
        let route = Route::waypoints(at(0.0, 0.0), &[a, at(0.0, 5.0), a], 5.0)
            .expect("an open course may revisit a point");
        assert_eq!(route.legs(), 3);
    }
}
