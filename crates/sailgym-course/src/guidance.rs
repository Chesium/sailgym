//! `Guidance`: the current leg as a line, a point to aim at, and the **one**
//! definition of cross-track error (v2 F15.2, section 04 task 4.3).
//!
//! # Why both a line and a point
//!
//! Jaulin & Le Bars' line-following controller needs the **line**; a pursuit
//! controller needs the **point**. F15.2 carries both so that neither is
//! derived from the other inside an agent, which is how two subtly different
//! definitions of cross-track error come to exist (RV20).
//!
//! Cross-track error is defined against the **line**, and it is computed
//! **here**, once. A `signed_cross_track` computed anywhere outside this file
//! is the risk RV20 names firing.
//!
//! # The sign, stated against F2
//!
//! ```text
//! signed_cross_track = d̂ × (p − a)        d̂ = the leg's unit direction
//! ```
//!
//! with `Vec2::cross(u, v) = u.x·v.y − u.y·v.x`, so the value is **positive
//! when the boat lies to port of the leg's direction of travel**. That is the
//! same side `+y_H` points to (F2: `+y_H` is to port) for a boat whose
//! heading is the leg's direction — which is the whole reason to pick this
//! sign rather than its negation. A positive error therefore means *the leg
//! is off my starboard bow; bear away to starboard to get back on it*.
//!
//! F11's R3 names sign-convention drift as the highest-probability defect
//! class in this repository, and guidance is nothing but signs: which side of
//! the line, which way to turn, which tack. So the convention is asserted in
//! all four quadrants of leg bearing, and
//! [`tests::mirroring_the_route_and_the_state_negates_the_cross_track_bit_for_bit`]
//! reuses `testkit::mirror_state` to require the **bit-exact** negation
//! (RV21).
//!
//! # Lookahead
//!
//! The lookahead distance is a **course parameter, not a physical
//! coefficient** (F14.9): v1 brief §43 governs `parameters.rs`, and the crate
//! boundary is the distinction. It may therefore be tuned freely — but it has
//! not been tuned, because tuning it needs a controller to tune it against.
//! [`CourseParams::DEFAULT_LOOKAHEAD`] is a placeholder and says so, and the
//! PRD tracks it as a debt repaid by section 05.

use sailgym_physics::state::BoatState;
use sailgym_physics::vec::Vec2;
use serde::Serialize;

use crate::route::{position, Rounding, Route};

/// Tunables of the course layer.
///
/// Nothing here is a physical coefficient and nothing here reaches the
/// physics core (F14.9).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CourseParams {
    /// How far along the leg, beyond the boat's own projection onto it, the
    /// [`Guidance::target`] is placed. Metres.
    pub lookahead: f64,
}

impl CourseParams {
    /// **A placeholder, not a tuned value.**
    ///
    /// 20 m: a round number, deliberately not derived from any boat dimension
    /// so that no reader can mistake it for one, and large enough that a
    /// pursuit target on a short leg is ahead of the boat rather than under
    /// it. Tuning it requires a controller to tune it against, which is
    /// section 05; the PRD records that as a deliberate debt.
    pub const DEFAULT_LOOKAHEAD: f64 = 20.0;
}

impl Default for CourseParams {
    fn default() -> Self {
        Self {
            lookahead: Self::DEFAULT_LOOKAHEAD,
        }
    }
}

/// What the boat is being asked to sail, this decision (F15.2).
///
/// Computed, never loaded: there is no `Deserialize` here on purpose. A
/// recorded guidance is a derived quantity, and re-deriving it from the route
/// and the state is what keeps it honest.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Guidance {
    /// The current leg as a directed line, or `None` on the single-mark route
    /// of F15.1, which has no incoming leg.
    pub line: Option<(Vec2, Vec2)>,
    /// A point to aim at: the lookahead along the line, or the raw point when
    /// there is no line.
    pub target: Vec2,
    /// Metres, positive to port of the leg's direction of travel. **Zero, and
    /// meaningless, when `line` is `None`** — a route with no line has no
    /// cross-track error, and `line: None` is what says so.
    pub signed_cross_track: f64,
    /// The leg this guidance is for.
    pub leg_index: u32,
    /// How the leg's mark is passed. Carried so a consumer never has to reach
    /// back into the route to find out which way it is rounding.
    pub rounding: Rounding,
}

/// The guidance for `leg_index`, or `None` past the end of the route.
pub fn guidance(
    route: &Route,
    leg_index: u32,
    st: &BoatState,
    p: &CourseParams,
) -> Option<Guidance> {
    guidance_at(route, leg_index, position(st), p)
}

/// [`guidance`] on a bare position.
pub fn guidance_at(
    route: &Route,
    leg_index: u32,
    boat: Vec2,
    p: &CourseParams,
) -> Option<Guidance> {
    let leg = route.leg(leg_index)?;
    let rounding = route.marks[leg.mark_index].rounding;

    let Some(d) = leg.direction() else {
        // F15.1's lookahead point: no line, so no cross-track error, and the
        // target is the point itself. One route type, two UX modes — and this
        // is the whole of the difference between them.
        return Some(Guidance {
            line: None,
            target: leg.to,
            signed_cross_track: 0.0,
            leg_index,
            rounding,
        });
    };

    let a = leg.from.expect("a leg with a direction has a start");
    let from_a = boat - a;

    // The one definition. `cross` is `u.x·v.y − u.y·v.x`, so this is positive
    // to port of the leg direction; see the module documentation.
    let signed_cross_track = d.cross(from_a);

    // Where the boat is along the leg, and the aiming point a lookahead
    // further on — clamped to the leg's own ends, so the target is never
    // behind the start and never beyond the mark. Aiming past the mark would
    // make the boat sail through it, which is the one thing a rounding is
    // about.
    let length = leg.length().expect("a leg with a direction has a length");
    let along = from_a.dot(d).clamp(0.0, length);
    let reach = (along + p.lookahead).clamp(0.0, length);

    Some(Guidance {
        line: leg.line(),
        target: a + d * reach,
        signed_cross_track,
        leg_index,
        rounding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::Mark;
    use sailgym_physics::testkit::mirror_state;

    fn at(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    /// `mirrored` is the bit-exact negation of `original` — with the one
    /// carve-out the mirror test's documentation states: `−(+0.0)` is `−0.0`,
    /// whose bits differ, while a mirrored zero comes out `+0.0`. IEEE 754
    /// says the two are equal, so at zero that is what is asserted.
    fn negates(mirrored: f64, original: f64) -> bool {
        if original == 0.0 {
            mirrored == 0.0
        } else {
            mirrored.to_bits() == (-original).to_bits()
        }
    }

    /// A two-mark route whose leg 0 runs from `a` to `b`.
    fn leg_route(a: Vec2, b: Vec2) -> Route {
        Route::new(
            vec![
                Mark {
                    position: b,
                    radius: 1.0,
                    rounding: Rounding::Port,
                },
                Mark {
                    position: a,
                    radius: 1.0,
                    rounding: Rounding::Starboard,
                },
            ],
            1,
        )
    }

    #[test]
    fn the_sign_follows_f2_on_both_sides_of_the_line() {
        // Leg running due east. To port of "east" is north, so a boat to the
        // north of the line has a positive cross-track error — the same side
        // `+y_H` points to for a boat on the leg's heading (F2).
        let route = leg_route(at(-10.0, 0.0), at(10.0, 0.0));
        let p = CourseParams::default();
        let north = guidance_at(&route, 0, at(0.0, 3.0), &p).unwrap();
        let south = guidance_at(&route, 0, at(0.0, -3.0), &p).unwrap();
        assert_eq!(north.signed_cross_track, 3.0);
        assert_eq!(south.signed_cross_track, -3.0);
        // On the line it is zero, on both approaches.
        assert_eq!(
            guidance_at(&route, 0, at(4.0, 0.0), &p)
                .unwrap()
                .signed_cross_track,
            0.0
        );
    }

    #[test]
    fn the_sign_holds_in_all_four_quadrants_of_leg_bearing() {
        let p = CourseParams::default();
        // Four legs, one per quadrant of bearing, each 20 m long.
        let quadrants = [
            (at(1.0, 1.0), "north-east"),
            (at(-1.0, 1.0), "north-west"),
            (at(-1.0, -1.0), "south-west"),
            (at(1.0, -1.0), "south-east"),
        ];
        for (dir, name) in quadrants {
            let d = dir / dir.length();
            let a = at(-5.0, 7.0);
            let b = a + d * 20.0;
            let route = leg_route(a, b);
            let midpoint = a + d * 10.0;
            // Displace the boat 4 m along the leg's **left** normal, which is
            // to port of the direction of travel.
            let left = Vec2::new(-d.y, d.x);
            let to_port = guidance_at(&route, 0, midpoint + left * 4.0, &p).unwrap();
            let to_stbd = guidance_at(&route, 0, midpoint - left * 4.0, &p).unwrap();
            assert!(
                to_port.signed_cross_track > 0.0,
                "{name}: to port of the leg must be positive, got {}",
                to_port.signed_cross_track
            );
            assert!(
                to_stbd.signed_cross_track < 0.0,
                "{name}: to starboard of the leg must be negative, got {}",
                to_stbd.signed_cross_track
            );
            // …and the magnitude is the perpendicular distance, 4 m.
            assert!((to_port.signed_cross_track - 4.0).abs() < 1e-12, "{name}");
            assert!((to_stbd.signed_cross_track + 4.0).abs() < 1e-12, "{name}");
        }
    }

    #[test]
    fn a_one_mark_either_route_has_no_line_and_aims_at_the_mark() {
        // F15.1, and task 4.3's acceptance, literally.
        let route = Route::lookahead_point(at(12.0, -7.0), 15.0);
        let g = guidance_at(&route, 0, at(0.0, 0.0), &CourseParams::default()).unwrap();
        assert_eq!(g.line, None);
        assert_eq!(g.target, at(12.0, -7.0));
        assert_eq!(g.signed_cross_track, 0.0);
        assert_eq!(g.rounding, Rounding::Either);
        assert_eq!(g.leg_index, 0);
    }

    #[test]
    fn the_target_is_a_lookahead_along_the_line_and_never_past_the_mark() {
        let route = leg_route(at(0.0, 0.0), at(100.0, 0.0));
        let p = CourseParams { lookahead: 20.0 };
        // Boat 30 m along the leg and 5 m off it: the target is 50 m along.
        let g = guidance_at(&route, 0, at(30.0, 5.0), &p).unwrap();
        assert_eq!(g.line, Some((at(0.0, 0.0), at(100.0, 0.0))));
        assert_eq!(g.target, at(50.0, 0.0));
        // Near the mark, the target is the mark — never beyond it.
        let g = guidance_at(&route, 0, at(95.0, 1.0), &p).unwrap();
        assert_eq!(g.target, at(100.0, 0.0));
        // Behind the start, the target is a lookahead from the start.
        let g = guidance_at(&route, 0, at(-40.0, 0.0), &p).unwrap();
        assert_eq!(g.target, at(20.0, 0.0));
    }

    #[test]
    fn guidance_past_the_end_of_the_route_is_none() {
        let route = leg_route(at(0.0, 0.0), at(10.0, 0.0));
        assert_eq!(route.legs(), 2);
        assert!(guidance_at(&route, 2, at(0.0, 0.0), &CourseParams::default()).is_none());
    }

    #[test]
    fn the_rounding_travels_with_the_guidance() {
        let route = leg_route(at(0.0, 0.0), at(10.0, 0.0));
        let p = CourseParams::default();
        assert_eq!(
            guidance_at(&route, 0, at(1.0, 1.0), &p).unwrap().rounding,
            Rounding::Port
        );
        assert_eq!(
            guidance_at(&route, 1, at(1.0, 1.0), &p).unwrap().rounding,
            Rounding::Starboard
        );
    }

    /// The important one (RV21).
    ///
    /// Mirroring the route and the state about the world `x` axis — exactly
    /// the mirror `testkit::mirror_state` applies, which is the backbone of
    /// the brief §35 port/starboard symmetry invariant — must negate the
    /// cross-track error **bit for bit**, not to a tolerance.
    ///
    /// The one carve-out is the signed zero: for a boat exactly on the line
    /// the error is `+0.0` on both sides of the mirror while `−(+0.0)` is
    /// `−0.0`, whose bits differ. IEEE 754 says `+0.0 == −0.0`, so those rows
    /// assert equality with zero instead — and the sweep below contains such
    /// a row on purpose rather than avoiding one.
    #[test]
    fn mirroring_the_route_and_the_state_negates_the_cross_track_bit_for_bit() {
        let p = CourseParams::default();
        // A four-mark course with every kind of rounding on it, sailed from a
        // spread of positions on both sides of every leg.
        let route = Route::new(
            vec![
                Mark {
                    position: at(0.0, 0.0),
                    radius: 1.5,
                    rounding: Rounding::Port,
                },
                Mark {
                    position: at(37.0, 61.0),
                    radius: 2.5,
                    rounding: Rounding::Starboard,
                },
                Mark {
                    position: at(-19.0, 44.0),
                    radius: 1.0,
                    rounding: Rounding::Either,
                },
                Mark {
                    position: at(-23.0, -8.0),
                    radius: 3.0,
                    rounding: Rounding::Gate(at(-26.0, -11.0), at(-20.0, -5.0)),
                },
            ],
            2,
        );
        route.validate().expect("the fixture route is sailable");
        let mirrored = route.mirrored();
        mirrored.validate().expect("and so is its mirror");

        let mut zeros = 0usize;
        let mut nonzeros = 0usize;
        for leg_index in 0..route.legs() {
            for (i, boat) in [
                at(0.0, 0.0),
                at(5.0, 31.0),
                at(-12.0, 3.0),
                at(40.0, -17.0),
                at(-30.0, 70.0),
                at(18.5, 30.5), // exactly on leg 0→1, giving a signed zero
            ]
            .into_iter()
            .enumerate()
            {
                let g = guidance_at(&route, leg_index, boat, &p).unwrap();
                let m = guidance_at(&mirrored, leg_index, at(boat.x, -boat.y), &p).unwrap();

                let what = format!("leg {leg_index}, point {i}");
                if g.signed_cross_track == 0.0 {
                    zeros += 1;
                } else {
                    nonzeros += 1;
                }
                assert!(
                    negates(m.signed_cross_track, g.signed_cross_track),
                    "{what}: {} is not the bit-exact negation of {}",
                    m.signed_cross_track,
                    g.signed_cross_track
                );

                // The target and the line mirror too, and exactly.
                assert_eq!(
                    m.target.x.to_bits(),
                    g.target.x.to_bits(),
                    "{what}: target x"
                );
                assert!(negates(m.target.y, g.target.y), "{what}: target y");
                let (ga, gb) = g.line.expect("every leg of this route has a line");
                let (ma, mb) = m.line.expect("…and so does its mirror");
                assert_eq!((ma.x, ma.y), (ga.x, -ga.y), "{what}: line start");
                assert_eq!((mb.x, mb.y), (gb.x, -gb.y), "{what}: line end");
                // And the side flips, which is what makes the mirror a mirror.
                assert_eq!(m.rounding, g.rounding.mirrored(), "{what}: rounding");
            }
        }
        // The sweep must contain both kinds of row, or it is asserting less
        // than it looks like it is.
        assert!(nonzeros >= 40, "only {nonzeros} non-zero rows");
        assert!(zeros >= 1, "the signed-zero row never fired");
    }

    #[test]
    fn the_mirror_holds_through_mirror_state_itself() {
        // The same assertion driven through `testkit::mirror_state` rather
        // than a hand-written `-y`, so the two mirrors cannot drift apart.
        let p = CourseParams::default();
        let route = leg_route(at(-10.0, -3.0), at(25.0, 17.0));
        let mirrored = route.mirrored();
        for (x, y) in [(3.0, 9.0), (-4.0, -11.0), (20.0, 0.5)] {
            let st = BoatState {
                x,
                y,
                psi: 0.4,
                phi: -0.2,
                u: 1.3,
                v: 0.2,
                r: 0.05,
                p: -0.1,
                beta: 0.3,
                beta_dot: 0.01,
                delta_r: -0.2,
                l_sheet: 2.0,
                t: 4.0,
            };
            let g = guidance(&route, 0, &st, &p).unwrap();
            let m = guidance(&mirrored, 0, &mirror_state(&st), &p).unwrap();
            assert_ne!(g.signed_cross_track, 0.0);
            assert_eq!(
                m.signed_cross_track.to_bits(),
                (-g.signed_cross_track).to_bits()
            );
        }
    }
}
