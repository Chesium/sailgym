//! Ordered, sided, directed mark passage (v2 F15.3, section 04 task 4.2).
//!
//! # The rule
//!
//! > A mark is passed at the first step at which **all four** hold: the
//! > previous mark is already passed; the boat has crossed the plane through
//! > the mark perpendicular to the incoming leg; it crossed on the side
//! > `rounding` requires; and it crossed in the direction the leg runs.
//!
//! **A radius check satisfies none of these on its own, and is forbidden**
//! (F15.3). It permits cutting the corner, and a policy will learn to — which
//! then shows up as an unexplained improvement in completion time that
//! survives review because nobody re-reads the passage test. So the passage
//! test is read: [`tests::a_corner_cut_that_a_radius_check_would_accept_is_rejected`]
//! is the named regression, and it is the whole value of this file.
//!
//! # How each clause is spelled
//!
//! | clause | here |
//! |---|---|
//! | ordered | the `leg_index` argument, and [`advance`], which only ever returns `leg_index` or `leg_index + 1` |
//! | crossed the plane | `s(p) = (p − mark) · d̂` changes sign across the step |
//! | in the leg's direction | the sign change must be `s ≤ 0` then `s > 0`, never the reverse |
//! | on the required side | the crossing point's offset along the leg's left normal clears `radius` on the required side |
//!
//! The half-open convention `s ≤ 0` then `s > 0` is deliberate and is the
//! same shape as `frames::wrap_pi`'s `(−π, π]`: a boat sitting **exactly** on
//! the plane has not yet passed, and passes on the step where `s` becomes
//! strictly positive. Without the half-open convention a boat parked on the
//! plane would pass on every step, or on none, depending on which way the
//! comparison happened to be written.
//!
//! # A gate is not two marks
//!
//! [`Rounding::Gate`] is **one** directed crossing of the segment between two
//! posts. It is passed or it is not; there is no per-post state anywhere.
//! Crossing the gate's *line* outside the posts is not a passage, and
//! [`tests::a_gate_crossed_outside_its_posts_is_not_passed`] says so.
//!
//! # Purity
//!
//! Every decision here is a pure function of
//! `(previous state, current state, route, leg_index)`. There is no
//! hysteresis, no timer, no "nearly", and no interior mutability. A boat that
//! crosses back out has **not** un-passed the mark: passage is monotone in
//! `leg_index` because [`advance`] is the only way it moves, and
//! [`tests::passage_is_monotone_over_a_reversing_trajectory`] asserts it.

use std::cmp::Ordering;

use sailgym_physics::state::BoatState;
use sailgym_physics::vec::Vec2;

use crate::route::{position, Leg, Mark, Rounding, Route};

/// Whether the boat passed leg `leg_index`'s mark during the step from `prev`
/// to `cur`.
///
/// `false` when `leg_index` is past the end of the route.
pub fn passed(route: &Route, leg_index: u32, prev: &BoatState, cur: &BoatState) -> bool {
    passed_between(route, leg_index, position(prev), position(cur))
}

/// [`passed`] on bare positions — the geometry, with the state unwrapped.
///
/// The step is the straight segment from `prev` to `cur`. Nothing here reads
/// a velocity, a heading or a heel angle: a mark is passed by where the boat
/// went, not by where it was pointing.
pub fn passed_between(route: &Route, leg_index: u32, prev: Vec2, cur: Vec2) -> bool {
    let Some(leg) = route.leg(leg_index) else {
        return false;
    };
    let mark = &route.marks[leg.mark_index];
    match (mark.rounding, leg.direction()) {
        (Rounding::Gate(a, b), Some(d)) => gate_crossed(d, a, b, prev, cur),
        // A gate needs a leg to give it a direction. `Route::validate`
        // refuses one on a route that has none; an unvalidated route gets a
        // `false` rather than an invented direction.
        (Rounding::Gate(_, _), None) => false,
        (_, Some(d)) => plane_crossed(&leg, mark, d, prev, cur),
        // The single-mark route of F15.1 — the dragged lookahead point. It
        // has no incoming leg, so it has no plane and no side, and arrival is
        // the only thing that can be meant. `Route::validate` refuses a sided
        // rounding here, which is what stops this branch from ever becoming a
        // way to spell a radius check for a mark that does have a side.
        (_, None) => arrived(mark, prev, cur),
    }
}

/// The leg index after this step: `leg_index + 1` if the mark was passed,
/// `leg_index` otherwise.
///
/// The **only** way a leg index moves in this crate, which is what makes
/// passage monotone rather than merely usually monotone.
pub fn advance(route: &Route, leg_index: u32, prev: &BoatState, cur: &BoatState) -> u32 {
    if passed(route, leg_index, prev, cur) {
        leg_index + 1
    } else {
        leg_index
    }
}

/// The plane through the mark, perpendicular to the incoming leg, crossed in
/// the leg's direction and on the required side.
fn plane_crossed(leg: &Leg, mark: &Mark, d: Vec2, prev: Vec2, cur: Vec2) -> bool {
    let m = leg.to;
    let s_prev = (prev - m).dot(d);
    let s_cur = (cur - m).dot(d);
    // Directed, and half-open: at or behind the plane, then strictly ahead of
    // it. The reverse crossing is not a passage, which is the clause a boat
    // sailing back down the leg would otherwise satisfy.
    if !(s_prev <= 0.0 && s_cur > 0.0) {
        return false;
    }
    let Some(side) = mark.rounding.required_side() else {
        return true;
    };
    // Where on the step the plane was crossed. `s_prev - s_cur` is strictly
    // negative here — `s_prev ≤ 0 < s_cur` — so there is no division by zero
    // and no epsilon.
    let q = crossing_point(prev, cur, s_prev, s_cur);
    // Positive offset = the boat crossed to the left of the mark, looking
    // along the leg = the mark was on the boat's starboard hand = it was left
    // to starboard. `Rounding::required_side` is where that is encoded.
    let offset = (q - m).dot(left_normal(d));
    side * offset >= mark.radius
}

/// A directed crossing of the segment between two gate posts.
fn gate_crossed(d: Vec2, a: Vec2, b: Vec2, prev: Vec2, cur: Vec2) -> bool {
    let g = b - a;
    let n = left_normal(g);
    // Orient the gate's normal the way the leg runs through it. This is a
    // **three-way** comparison and not a two-way `if`: a leg exactly parallel
    // to the gate line gives no orientation at all, and then the boat's own
    // displacement is the only thing that says which way "through" is. A
    // validated route can still contain that geometry, so it is handled
    // rather than asserted away, and the `None` arm catches a NaN with it.
    let n = match d.dot(n).partial_cmp(&0.0) {
        Some(Ordering::Greater) => n,
        Some(Ordering::Less) => -n,
        _ if (cur - prev).dot(n) < 0.0 => -n,
        _ => n,
    };

    let s_prev = (prev - a).dot(n);
    let s_cur = (cur - a).dot(n);
    if !(s_prev <= 0.0 && s_cur > 0.0) {
        return false;
    }
    // Between the posts? The gate's *line* extends for ever; the gate does
    // not. `g.length_squared()` is strictly positive for a validated route
    // and the guard keeps an unvalidated one from dividing by zero.
    let gg = g.length_squared();
    if gg > 0.0 {
        let q = crossing_point(prev, cur, s_prev, s_cur);
        let along = (q - a).dot(g) / gg;
        // Inclusive: a crossing exactly through a post is through the gate.
        (0.0..=1.0).contains(&along)
    } else {
        false
    }
}

/// Arrival at F15.1's lookahead point: the step's segment reaches the mark's
/// disc.
///
/// The **segment**, not the end point, so a fast boat cannot hop over a small
/// disc between two samples. Inclusive at the boundary: a track exactly
/// tangent to the disc has arrived.
fn arrived(mark: &Mark, prev: Vec2, cur: Vec2) -> bool {
    distance_to_segment(mark.position, prev, cur) <= mark.radius
}

/// The point on the step at which `s` changes sign, by linear interpolation.
///
/// The caller guarantees `s_prev ≤ 0 < s_cur`, so the denominator is strictly
/// negative and `t ∈ [0, 1)`.
fn crossing_point(prev: Vec2, cur: Vec2, s_prev: f64, s_cur: f64) -> Vec2 {
    let t = s_prev / (s_prev - s_cur);
    prev + (cur - prev) * t
}

/// The direction rotated 90° counter-clockwise — the leg's **left** normal.
///
/// Written once. Every sign in this crate that says "which side" is measured
/// along it, and `guidance.rs` measures cross-track error along the same one.
pub fn left_normal(d: Vec2) -> Vec2 {
    Vec2::new(-d.y, d.x)
}

/// Shortest distance from a point to the segment `a → b`, or to `a` when the
/// segment has no length.
fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq > 0.0 {
        let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
        (p - (a + ab * t)).length()
    } else {
        (p - a).length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::Mark;

    /// A single leg running east along `y = 0`, ending at the origin, with
    /// the mark to be left to port.
    ///
    /// Leg 1 continues north from the origin, so the route is a two-mark
    /// circuit and both legs have a direction.
    fn two_mark_route(rounding: Rounding) -> Route {
        Route::new(
            vec![
                Mark {
                    position: Vec2::new(0.0, 0.0),
                    radius: 2.0,
                    rounding,
                },
                Mark {
                    position: Vec2::new(-30.0, 0.0),
                    radius: 2.0,
                    rounding: Rounding::Either,
                },
            ],
            1,
        )
    }

    fn at(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    /// Leg 0 of [`two_mark_route`] runs from `(−30, 0)` to `(0, 0)`, i.e.
    /// east: `d̂ = (+1, 0)`, left normal `(0, +1)`.
    #[test]
    fn the_fixture_leg_runs_east() {
        let route = two_mark_route(Rounding::Port);
        route.validate().expect("valid");
        let leg = route.leg(0).unwrap();
        assert_eq!(leg.from, Some(at(-30.0, 0.0)));
        assert_eq!(leg.direction(), Some(at(1.0, 0.0)));
        assert_eq!(left_normal(leg.direction().unwrap()), at(0.0, 1.0));
    }

    // -----------------------------------------------------------------
    // One case per clause, failing in isolation
    // -----------------------------------------------------------------

    #[test]
    fn all_four_clauses_hold_and_the_mark_is_passed() {
        // Leaving the mark to port means passing on its starboard hand, which
        // on an eastbound leg is to the south: offset −5 m, clearing the 2 m
        // radius on the required side.
        let route = two_mark_route(Rounding::Port);
        assert!(passed_between(&route, 0, at(-1.0, -5.0), at(1.0, -5.0)));
    }

    #[test]
    fn the_correct_side_in_the_wrong_direction_is_not_passed() {
        // Same side, same clearance, sailing west instead of east.
        let route = two_mark_route(Rounding::Port);
        assert!(!passed_between(&route, 0, at(1.0, -5.0), at(-1.0, -5.0)));
    }

    #[test]
    fn the_correct_direction_on_the_wrong_side_is_not_passed() {
        // Eastbound, but leaving the mark to starboard when port was asked
        // for.
        let route = two_mark_route(Rounding::Port);
        assert!(!passed_between(&route, 0, at(-1.0, 5.0), at(1.0, 5.0)));
        // …and the mirror image of the requirement accepts exactly that track.
        let route = two_mark_route(Rounding::Starboard);
        assert!(passed_between(&route, 0, at(-1.0, 5.0), at(1.0, 5.0)));
    }

    #[test]
    fn a_correct_crossing_of_a_later_mark_with_the_previous_one_unpassed_does_not_count() {
        // The ordering clause. The boat sails leg 1's crossing perfectly
        // while the tracker is still on leg 0; `advance` only ever asks about
        // the leg it is on, so leg 1 is not passed and the index does not
        // move.
        let route = two_mark_route(Rounding::Either);
        let (prev, cur) = (at(-20.0, 0.0), at(-40.0, 0.0)); // westbound through mark 1
        assert!(
            passed_between(&route, 1, prev, cur),
            "the geometry of leg 1 is satisfied by this step"
        );
        assert!(
            !passed_between(&route, 0, prev, cur),
            "…but leg 0's mark is not where this step went"
        );
        let prev_st = state(prev);
        let cur_st = state(cur);
        assert_eq!(
            advance(&route, 0, &prev_st, &cur_st),
            0,
            "no leg may be skipped"
        );
    }

    #[test]
    fn a_gate_crossed_outside_its_posts_is_not_passed() {
        // A 6 m gate straddling the origin, on the eastbound leg. The plane
        // is crossed 10 m to the north, well outside the posts.
        let mut route = two_mark_route(Rounding::Either);
        route.marks[0].rounding = Rounding::Gate(at(0.0, -3.0), at(0.0, 3.0));
        route.validate().expect("valid");
        assert!(!passed_between(&route, 0, at(-1.0, 10.0), at(1.0, 10.0)));
        // Between the posts, it is.
        assert!(passed_between(&route, 0, at(-1.0, 1.0), at(1.0, 1.0)));
    }

    // -----------------------------------------------------------------
    // The named regression
    // -----------------------------------------------------------------

    /// A naive radius check: "the boat came within `radius` of the mark".
    ///
    /// Defined **here, in the test**, and nowhere in the shipped code, so the
    /// contrast below is between the rule and the thing F15.3 forbids.
    fn a_radius_check_would_accept(route: &Route, leg_index: u32, prev: Vec2, cur: Vec2) -> bool {
        let mark = route.mark_at(leg_index).expect("a leg");
        distance_to_segment(mark.position, prev, cur) <= mark.radius
    }

    #[test]
    fn a_corner_cut_that_a_radius_check_would_accept_is_rejected() {
        // RV19, and the reason this file exists.
        //
        // The mark is to be left to **port**, so the boat must pass south of
        // it on an eastbound leg. This track passes 1.5 m **north** of the
        // mark — inside the 2 m radius, so a radius check accepts it — and it
        // is the corner cut: the boat has taken the inside of the turn.
        let route = two_mark_route(Rounding::Port);
        let (prev, cur) = (at(-1.0, 1.5), at(1.0, 1.5));

        assert!(
            a_radius_check_would_accept(&route, 0, prev, cur),
            "the fixture must be one a radius check accepts, or it proves nothing"
        );
        assert!(
            !passed_between(&route, 0, prev, cur),
            "a corner cut inside the mark's radius on the wrong side is not a passage (F15.3)"
        );

        // And the same track on the required side, at the same distance, is.
        assert!(passed_between(&route, 0, at(-1.0, -2.5), at(1.0, -2.5)));
    }

    #[test]
    fn clearing_the_mark_by_less_than_its_radius_on_the_right_side_is_not_enough() {
        // The second half of the same idea: the boat is on the correct side
        // but is sailing over the top of the buoy.
        let route = two_mark_route(Rounding::Port);
        assert!(!passed_between(&route, 0, at(-1.0, -1.9), at(1.0, -1.9)));
        assert!(passed_between(&route, 0, at(-1.0, -2.1), at(1.0, -2.1)));
    }

    // -----------------------------------------------------------------
    // The boundaries, asserted rather than left to chance (F16.3)
    // -----------------------------------------------------------------

    #[test]
    fn exactly_on_the_plane_is_not_yet_passed_and_the_next_step_passes() {
        let route = two_mark_route(Rounding::Either);
        // Ending exactly on the plane: `s_cur = 0`, not yet passed.
        assert!(!passed_between(&route, 0, at(-2.0, -5.0), at(0.0, -5.0)));
        // Starting exactly on it and moving off: passed, and counted once.
        assert!(passed_between(&route, 0, at(0.0, -5.0), at(2.0, -5.0)));
        // Sitting exactly on it for ever: never passed, on any step.
        for _ in 0..8 {
            assert!(!passed_between(&route, 0, at(0.0, -5.0), at(0.0, -5.0)));
        }
    }

    #[test]
    fn exactly_at_the_required_clearance_counts_and_a_hair_inside_does_not() {
        // `radius` is 2.0 and the mark is left to port, so the boat must be
        // at offset ≤ −2.0. Exactly −2.0 is a tangent pass on the correct
        // side and counts.
        let route = two_mark_route(Rounding::Port);
        assert!(passed_between(&route, 0, at(-1.0, -2.0), at(1.0, -2.0)));
        let inside = -2.0 + f64::EPSILON * 2.0;
        assert!(!passed_between(
            &route,
            0,
            at(-1.0, inside),
            at(1.0, inside)
        ));
    }

    #[test]
    fn a_crossing_exactly_through_a_gate_post_is_through_the_gate() {
        let mut route = two_mark_route(Rounding::Either);
        route.marks[0].rounding = Rounding::Gate(at(0.0, -3.0), at(0.0, 3.0));
        // Exactly through the northern post.
        assert!(passed_between(&route, 0, at(-1.0, 3.0), at(1.0, 3.0)));
        // Exactly through the southern post.
        assert!(passed_between(&route, 0, at(-1.0, -3.0), at(1.0, -3.0)));
        // A hair beyond it is outside the gate.
        let beyond = 3.0 + 3.0 * f64::EPSILON;
        assert!(!passed_between(
            &route,
            0,
            at(-1.0, beyond),
            at(1.0, beyond)
        ));
    }

    #[test]
    fn a_gate_sailed_backwards_is_not_passed() {
        let mut route = two_mark_route(Rounding::Either);
        route.marks[0].rounding = Rounding::Gate(at(0.0, -3.0), at(0.0, 3.0));
        assert!(!passed_between(&route, 0, at(1.0, 1.0), at(-1.0, 1.0)));
    }

    #[test]
    fn a_gate_exactly_parallel_to_its_leg_falls_back_to_the_boats_own_direction() {
        // Degenerate but constructible: the gate lies **along** the leg, so
        // the leg direction says nothing about which way "through" is. The
        // boat crossing it northbound passes; southbound it does not, and the
        // two are each other's mirror.
        let mut route = two_mark_route(Rounding::Either);
        route.marks[0].rounding = Rounding::Gate(at(-3.0, 0.0), at(3.0, 0.0));
        assert!(passed_between(&route, 0, at(0.0, -1.0), at(0.0, 1.0)));
        assert!(passed_between(&route, 0, at(0.0, 1.0), at(0.0, -1.0)));
        // Never crossing it is never passing it.
        assert!(!passed_between(&route, 0, at(0.0, 1.0), at(0.0, 2.0)));
    }

    #[test]
    fn a_lookahead_point_is_reached_by_its_disc_and_by_nothing_else() {
        // F15.1. The one branch where proximity is the rule, because the
        // route has no leg and therefore no side and no direction — and
        // `Route::validate` refuses a sided rounding here, which is what
        // keeps this from being a radius check in disguise.
        let route = Route::lookahead_point(at(10.0, 0.0), 3.0);
        assert!(!passed_between(&route, 0, at(0.0, 0.0), at(5.0, 0.0)));
        assert!(passed_between(&route, 0, at(5.0, 0.0), at(8.0, 0.0)));
        // The segment counts, not the end points: a step that jumps clean
        // over the disc still arrives.
        assert!(passed_between(&route, 0, at(0.0, 0.0), at(20.0, 0.0)));
        // Exactly tangent to the disc arrives; a hair outside does not.
        assert!(passed_between(&route, 0, at(0.0, 3.0), at(20.0, 3.0)));
        let outside = 3.0 + 8.0 * f64::EPSILON;
        assert!(!passed_between(
            &route,
            0,
            at(0.0, outside),
            at(20.0, outside)
        ));
    }

    #[test]
    fn a_leg_past_the_end_of_the_route_is_never_passed() {
        let route = two_mark_route(Rounding::Either);
        assert_eq!(route.legs(), 2);
        assert!(!passed_between(&route, 2, at(-1.0, 0.0), at(1.0, 0.0)));
        assert!(!passed_between(&route, 99, at(-1.0, 0.0), at(1.0, 0.0)));
    }

    // -----------------------------------------------------------------
    // Monotonicity
    // -----------------------------------------------------------------

    fn state(p: Vec2) -> BoatState {
        BoatState {
            x: p.x,
            y: p.y,
            ..BoatState::ZERO
        }
    }

    #[test]
    fn passage_is_monotone_over_a_reversing_trajectory() {
        // The boat passes mark 0 on the first step, then crosses its plane
        // back and forth four more times before sailing on to mark 1. The leg
        // index rises by one on the first step and on the last, and never
        // falls: a boat that crosses back out has not un-passed its mark, and
        // re-crossing a plane it has already passed does nothing at all.
        let route = two_mark_route(Rounding::Either);
        let track = [
            at(-10.0, -4.0),
            at(2.0, -4.0),  // passes leg 0
            at(-6.0, -4.0), // back out
            at(4.0, -4.0),  // and in again
            at(-6.0, -4.0),
            at(4.0, -4.0),
            at(-20.0, 0.0),
            at(-40.0, 0.0), // passes leg 1
        ];
        let mut leg = 0u32;
        let mut seen = vec![leg];
        for w in track.windows(2) {
            let next = advance(&route, leg, &state(w[0]), &state(w[1]));
            assert!(next >= leg, "the leg index may never fall");
            assert!(next <= leg + 1, "at most one mark per step");
            leg = next;
            seen.push(leg);
        }
        assert_eq!(seen, vec![0, 1, 1, 1, 1, 1, 1, 2]);
        assert!(route.is_finished(leg));
    }

    #[test]
    fn advance_moves_by_at_most_one_and_never_backwards() {
        let route = two_mark_route(Rounding::Port);
        let before = state(at(-1.0, -5.0));
        let after = state(at(1.0, -5.0));
        assert_eq!(advance(&route, 0, &before, &after), 1);
        // Asking again from the new leg does not re-pass the old one.
        assert_eq!(advance(&route, 1, &before, &after), 1);
        // Past the end, nothing moves.
        assert_eq!(advance(&route, 2, &before, &after), 2);
    }
}
