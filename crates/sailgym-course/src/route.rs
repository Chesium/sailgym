//! `Route`, `Mark`, `Rounding`, and the leg geometry every other module in
//! this crate reads (v2 F15, section 04 task 4.1).
//!
//! ## One route type, two UX modes (F15.1)
//!
//! "Drag a point and sail at it" is **not** a second code path. It is a
//! one-mark [`Route`] with [`Rounding::Either`] and a large [`Mark::radius`].
//! There is one passage implementation, one guidance implementation and one
//! set of tests; the only thing the lookahead mode changes is that the route
//! has no incoming leg, which [`Leg::from`] states as `None` rather than
//! inventing one.
//!
//! ## The route is a circuit
//!
//! Leg `i` ends at mark `i mod n` and starts at mark `(i − 1) mod n`, so the
//! leg into mark 0 comes from the **last** mark. With `laps > 1` that is
//! literally where the boat has just been. With `laps == 1` it is a stated
//! convention rather than an observation, and a course that wants a distinct
//! start makes the start a mark — which is also how a start line is spelled,
//! as a [`Rounding::Gate`].
//!
//! ## `laps` repeats the mark list; it does not duplicate it (RV24)
//!
//! A 3-lap 4-mark course has **4** marks and **12** legs.
//! `Route::marks.len()` does not depend on `laps`, and
//! `progress::tests::laps_repeat_the_marks_rather_than_duplicating_them`
//! asserts it.
//!
//! ## What `radius` means
//!
//! A mark is a physical object with a size, and the size does two jobs:
//!
//! * on a sided rounding it is the **clearance**: the boat must cross the
//!   mark's plane at a lateral offset of at least `radius` on the required
//!   side, so grazing the buoy tangentially on the correct side counts and
//!   sailing over the top of it does not (see `passage`);
//! * on the single-mark route of F15.1, where no incoming leg exists and no
//!   side can be required, it is the **arrival disc** — the "large radius"
//!   that clause asks for.
//!
//! It is a course geometry input, not a physical coefficient: v1 brief §43
//! governs `parameters.rs`, and v2 F14.9 says so again for everything above
//! the crate boundary. Nothing in this file reaches the physics core.

use sailgym_physics::state::BoatState;
use sailgym_physics::vec::Vec2;
use serde::{Deserialize, Serialize};

/// `{x, y}`, the shape [`Vec2`]'s own `Serialize` already emits.
///
/// `sailgym_physics::vec::Vec2` derives `Serialize` and **not**
/// `Deserialize`, and this section may not edit the physics crate (section
/// acceptance 3). This shim is the same arrangement `parameters.rs` uses for
/// `Vec3`, and it emits the identical JSON, so a document written by either
/// side reads on the other.
mod vec2_serde {
    use super::Vec2;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Components {
        x: f64,
        y: f64,
    }

    pub fn serialize<S: Serializer>(v: &Vec2, s: S) -> Result<S::Ok, S::Error> {
        Components { x: v.x, y: v.y }.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec2, D::Error> {
        let c = Components::deserialize(d)?;
        Ok(Vec2::new(c.x, c.y))
    }
}

/// Which side of the boat a mark must be left on, or — for a gate — that it
/// is a directed segment crossing rather than a rounding at all.
///
/// The sides are named from the **boat's** point of view, as they are on the
/// water: [`Rounding::Port`] means *leave the mark to port*, so the boat
/// passes on the mark's starboard hand.
///
/// A gate is **not two marks** (F15.3). It is one crossing of the segment
/// between its two posts, it is passed or it is not, and there is no per-post
/// state anywhere in this crate.
// Externally tagged, serde's default: `"port"` for the unit variants and
// `{"gate": [{"x":…,"y":…}, …]}` for the gate. An internal tag cannot carry a
// tuple variant, and a tuple variant is what the PRD's type signature is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    /// Leave the mark to port.
    Port,
    /// Leave the mark to starboard.
    Starboard,
    /// Either side; the crossing still has to be in the leg's direction.
    Either,
    /// A directed crossing of the segment between two posts.
    Gate(
        #[serde(with = "vec2_serde")] Vec2,
        #[serde(with = "vec2_serde")] Vec2,
    ),
}

impl Rounding {
    /// The mirror image about the world `x` axis, matching
    /// `sailgym_physics::testkit::mirror_state`.
    ///
    /// **The side flips.** A mirror reverses handedness, so the mirrored boat
    /// leaves the mirrored mark on the other hand; a `mirrored` that left
    /// `Port` alone would be asserting that the mirror test passes by not
    /// mirroring anything (RV21).
    pub fn mirrored(self) -> Self {
        match self {
            Self::Port => Self::Starboard,
            Self::Starboard => Self::Port,
            Self::Either => Self::Either,
            Self::Gate(a, b) => Self::Gate(Vec2::new(a.x, -a.y), Vec2::new(b.x, -b.y)),
        }
    }

    /// The required signed lateral offset of the boat from the mark at the
    /// crossing, in units of the leg's left normal: `Some(+1)` for a mark
    /// left to starboard, `Some(−1)` to port, `None` when no side is
    /// required.
    ///
    /// A positive offset means the boat crossed to the **left** of the
    /// mark, looking along the leg — so the mark was on the boat's starboard
    /// hand, which is to say it was left to starboard. The one place that
    /// sentence is written down.
    pub fn required_side(self) -> Option<f64> {
        match self {
            Self::Starboard => Some(1.0),
            Self::Port => Some(-1.0),
            Self::Either | Self::Gate(_, _) => None,
        }
    }
}

/// One mark of a route: where it is, how big it is, and how it is passed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    /// Position in the world frame `W` (F2), metres.
    #[serde(with = "vec2_serde")]
    pub position: Vec2,
    /// The mark's own size, metres. See the module documentation.
    pub radius: f64,
    /// How this mark is passed.
    pub rounding: Rounding,
}

impl Mark {
    /// A mark with no required side — the shape F15.1's lookahead point takes.
    pub fn either(position: Vec2, radius: f64) -> Self {
        Self {
            position,
            radius,
            rounding: Rounding::Either,
        }
    }

    /// The mirror image about the world `x` axis. The side flips; see
    /// [`Rounding::mirrored`].
    pub fn mirrored(&self) -> Self {
        Self {
            position: Vec2::new(self.position.x, -self.position.y),
            radius: self.radius,
            rounding: self.rounding.mirrored(),
        }
    }
}

/// An ordered course. `Vec`-ordered, never a map (F9.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Route {
    /// The marks, in the order they are sailed.
    pub marks: Vec<Mark>,
    /// How many times the mark list is sailed. `laps` repeats the list; it
    /// does not duplicate it (RV24).
    pub laps: u32,
}

/// Why a [`Route`] is not sailable.
///
/// Every variant names the offending mark, the way
/// `GzCurve::fit` names the offending field (v2 F18.1a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteError {
    /// A route with no marks has no legs.
    NoMarks,
    /// Zero laps is zero legs. A course that is not sailed is a mistake, not
    /// a degenerate case worth supporting.
    NoLaps,
    /// A position or a radius that is not finite.
    NotFinite { mark: usize, field: &'static str },
    /// A negative radius.
    NegativeRadius { mark: usize },
    /// Two consecutive marks in the same place, so the leg between them has
    /// no direction and no perpendicular plane.
    ZeroLengthLeg { mark: usize },
    /// A gate whose two posts are in the same place, so the segment has no
    /// length and no side.
    GatePostsCoincide { mark: usize },
    /// A side — or a gate — required on a route that has no incoming leg,
    /// which is to say a single-mark route. A side is meaningless without a
    /// direction to measure it against, and inventing one is exactly the
    /// silent reinterpretation F15.3 is written to prevent.
    SidedRoundingWithoutALeg { mark: usize },
    /// More than one lap of a single-mark route: every leg after the first
    /// would run from the mark to itself.
    LapsNeedTwoMarks,
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMarks => write!(f, "a route needs at least one mark"),
            Self::NoLaps => write!(f, "a route needs at least one lap"),
            Self::NotFinite { mark, field } => {
                write!(f, "mark {mark}: {field} is not finite")
            }
            Self::NegativeRadius { mark } => write!(f, "mark {mark}: radius is negative"),
            Self::ZeroLengthLeg { mark } => write!(
                f,
                "mark {mark}: the leg into it has zero length, so it has no direction"
            ),
            Self::GatePostsCoincide { mark } => {
                write!(f, "mark {mark}: the gate's two posts are in the same place")
            }
            Self::SidedRoundingWithoutALeg { mark } => write!(
                f,
                "mark {mark}: a single-mark route has no incoming leg, so no side can be \
                 required (F15.1); use Rounding::Either"
            ),
            Self::LapsNeedTwoMarks => {
                write!(
                    f,
                    "laps > 1 needs at least two marks, or every leg after \
                          the first runs from a mark to itself"
                )
            }
        }
    }
}

impl std::error::Error for RouteError {}

/// One leg of a sailed route: which mark it ends at, and the directed line
/// that leads to it.
///
/// `from` is `None` **only** for the single-mark route of F15.1, which has no
/// incoming leg at all. Every other leg has one, because
/// [`Route::validate`] refuses a route whose consecutive marks coincide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    /// The global leg index, `0 .. Route::legs()`.
    pub index: u32,
    /// Index into [`Route::marks`] of the mark this leg ends at.
    pub mark_index: usize,
    /// Where the leg starts, or `None` on a single-mark route.
    pub from: Option<Vec2>,
    /// Where the leg ends: the mark's position.
    pub to: Vec2,
}

impl Leg {
    /// The leg as a directed line, or `None` when there is no incoming leg.
    ///
    /// This is [`crate::guidance::Guidance::line`]'s value, and it is the one
    /// definition of the line: cross-track error is measured against it,
    /// once, in `guidance.rs` (RV20).
    pub fn line(&self) -> Option<(Vec2, Vec2)> {
        self.from.map(|from| (from, self.to))
    }

    /// The unit vector along the leg, or `None` when there is no incoming leg
    /// or the leg has no length.
    ///
    /// The length test is `> 0.0` rather than a comparison with an epsilon:
    /// `Vec2::normalize` returns `Vec2::ZERO` below `EPS_FLOW`, and a silent
    /// zero direction would make every downstream sign meaningless. A route
    /// that reaches here with a zero-length leg has skipped
    /// [`Route::validate`].
    pub fn direction(&self) -> Option<Vec2> {
        let from = self.from?;
        let d = self.to - from;
        let len = d.length();
        if len > 0.0 {
            Some(d / len)
        } else {
            None
        }
    }

    /// The leg's length in metres, or `None` when there is no incoming leg.
    pub fn length(&self) -> Option<f64> {
        self.from.map(|from| (self.to - from).length())
    }
}

impl Route {
    /// A route from marks and a lap count.
    pub fn new(marks: Vec<Mark>, laps: u32) -> Self {
        Self { marks, laps }
    }

    /// F15.1's lookahead point: one mark, no side, a large radius.
    ///
    /// There is no second code path behind this constructor — it builds the
    /// same [`Route`] every other caller builds.
    pub fn lookahead_point(target: Vec2, radius: f64) -> Self {
        Self::new(vec![Mark::either(target, radius)], 1)
    }

    /// Total number of legs: `marks.len() × laps` (RV24).
    pub fn legs(&self) -> u32 {
        self.marks.len() as u32 * self.laps
    }

    /// The mark leg `index` ends at, or `None` past the end of the course.
    pub fn mark_at(&self, index: u32) -> Option<&Mark> {
        self.leg(index).map(|leg| &self.marks[leg.mark_index])
    }

    /// Leg `index`, or `None` past the end of the course.
    ///
    /// The route is a circuit: leg 0 comes from the **last** mark. See the
    /// module documentation for why, and for the one case — a single-mark
    /// route — where `from` is `None` instead.
    pub fn leg(&self, index: u32) -> Option<Leg> {
        let n = self.marks.len();
        if n == 0 || index >= self.legs() {
            return None;
        }
        let mark_index = (index as usize) % n;
        let from = if n == 1 {
            None
        } else {
            Some(self.marks[(mark_index + n - 1) % n].position)
        };
        Some(Leg {
            index,
            mark_index,
            from,
            to: self.marks[mark_index].position,
        })
    }

    /// Whether `index` is past the last leg.
    pub fn is_finished(&self, index: u32) -> bool {
        index >= self.legs()
    }

    /// The mirror image about the world `x` axis, matching
    /// `sailgym_physics::testkit::mirror_state`: `y → −y`, and every side
    /// flips.
    ///
    /// Task 4.3's mirror assertion feeds this and `mirror_state` to
    /// `guidance` and requires the cross-track error to be the **bit-exact**
    /// negation. F11's R3 names sign-convention drift as the
    /// highest-probability defect class in this repository and guidance is
    /// nothing but signs, so the assertion is exact and not a tolerance
    /// (RV21).
    pub fn mirrored(&self) -> Self {
        Self {
            marks: self.marks.iter().map(Mark::mirrored).collect(),
            laps: self.laps,
        }
    }

    /// Whether this route can be sailed, naming the first offending mark.
    pub fn validate(&self) -> Result<(), RouteError> {
        let n = self.marks.len();
        if n == 0 {
            return Err(RouteError::NoMarks);
        }
        if self.laps == 0 {
            return Err(RouteError::NoLaps);
        }
        if n == 1 && self.laps > 1 {
            return Err(RouteError::LapsNeedTwoMarks);
        }

        for (i, mark) in self.marks.iter().enumerate() {
            if !mark.position.x.is_finite() || !mark.position.y.is_finite() {
                return Err(RouteError::NotFinite {
                    mark: i,
                    field: "position",
                });
            }
            if !mark.radius.is_finite() {
                return Err(RouteError::NotFinite {
                    mark: i,
                    field: "radius",
                });
            }
            if mark.radius < 0.0 {
                return Err(RouteError::NegativeRadius { mark: i });
            }
            if let Rounding::Gate(a, b) = mark.rounding {
                if !a.x.is_finite() || !a.y.is_finite() || !b.x.is_finite() || !b.y.is_finite() {
                    return Err(RouteError::NotFinite {
                        mark: i,
                        field: "gate post",
                    });
                }
                if (b - a).length_squared() <= 0.0 {
                    return Err(RouteError::GatePostsCoincide { mark: i });
                }
            }
            // A single-mark route has no incoming leg, so it has no direction
            // to measure a side against and no plane to cross. Only
            // `Rounding::Either` is meaningful there (F15.1).
            if n == 1 && mark.rounding != Rounding::Either {
                return Err(RouteError::SidedRoundingWithoutALeg { mark: i });
            }
            // Every leg must have a direction. Leg `i` runs from mark
            // `i − 1 mod n`, so the check is cyclic and covers leg 0.
            if n > 1 {
                let prev = self.marks[(i + n - 1) % n].position;
                if (mark.position - prev).length_squared() <= 0.0 {
                    return Err(RouteError::ZeroLengthLeg { mark: i });
                }
            }
        }
        Ok(())
    }
}

/// The boat's position in the world frame `W`.
///
/// Course geometry is horizontal: `phi` heels the rig, not the track, and
/// nothing in this crate reads a velocity. One line, in one place, so no
/// module invents its own projection.
pub fn position(st: &BoatState) -> Vec2 {
    Vec2::new(st.x, st.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A four-mark rectangle, all marks left to port, 25 m by 40 m.
    pub(crate) fn rectangle() -> Route {
        Route::new(
            vec![
                Mark {
                    position: Vec2::new(0.0, 0.0),
                    radius: 1.0,
                    rounding: Rounding::Port,
                },
                Mark {
                    position: Vec2::new(0.0, 40.0),
                    radius: 1.0,
                    rounding: Rounding::Port,
                },
                Mark {
                    position: Vec2::new(25.0, 40.0),
                    radius: 1.0,
                    rounding: Rounding::Port,
                },
                Mark {
                    position: Vec2::new(25.0, 0.0),
                    radius: 1.0,
                    rounding: Rounding::Port,
                },
            ],
            1,
        )
    }

    #[test]
    fn a_route_json_round_trip_is_the_identity() {
        // The shape `state.rs`'s round trip uses: value → text → value is
        // equal, and text → value → text is byte identical.
        let mut route = rectangle();
        route.laps = 3;
        route.marks[2].rounding = Rounding::Starboard;
        route.marks[3].rounding = Rounding::Gate(Vec2::new(25.0, -3.0), Vec2::new(25.0, 3.0));
        route.marks[1].rounding = Rounding::Either;

        let once = serde_json::to_string(&route).expect("a route must serialise");
        let back: Route = serde_json::from_str(&once).expect("…and deserialise");
        assert_eq!(back, route);
        let twice = serde_json::to_string(&back).expect("…and serialise again");
        assert_eq!(once, twice);

        // And the `Vec2` shim emits what `Vec2`'s own `Serialize` emits, so a
        // document written by the physics crate reads here.
        assert!(once.contains(r#""position":{"x":25.0,"y":40.0}"#), "{once}");
    }

    #[test]
    fn laps_repeat_the_mark_list_rather_than_duplicating_it() {
        // RV24, stated at the representation. `progress.rs` asserts it again
        // over a driven course.
        let mut route = rectangle();
        assert_eq!(route.marks.len(), 4);
        assert_eq!(route.legs(), 4);
        route.laps = 3;
        assert_eq!(route.marks.len(), 4, "laps must not duplicate marks");
        assert_eq!(route.legs(), 12);

        // Leg 4 is the second lap's first leg: same mark, same geometry.
        assert_eq!(route.leg(0).unwrap().mark_index, 0);
        assert_eq!(route.leg(4).unwrap().mark_index, 0);
        assert_eq!(route.leg(0).unwrap().to, route.leg(4).unwrap().to);
        assert_eq!(route.leg(0).unwrap().from, route.leg(4).unwrap().from);
        assert_eq!(route.leg(11).unwrap().mark_index, 3);
        assert!(route.leg(12).is_none());
        assert!(route.is_finished(12));
    }

    #[test]
    fn the_route_is_a_circuit_so_leg_zero_comes_from_the_last_mark() {
        let route = rectangle();
        let leg0 = route.leg(0).unwrap();
        assert_eq!(leg0.from, Some(Vec2::new(25.0, 0.0)));
        assert_eq!(leg0.to, Vec2::new(0.0, 0.0));
        assert_eq!(leg0.direction(), Some(Vec2::new(-1.0, 0.0)));
        assert_eq!(leg0.length(), Some(25.0));
        assert_eq!(
            leg0.line(),
            Some((Vec2::new(25.0, 0.0), Vec2::new(0.0, 0.0)))
        );

        let leg1 = route.leg(1).unwrap();
        assert_eq!(leg1.direction(), Some(Vec2::new(0.0, 1.0)));
    }

    #[test]
    fn a_lookahead_point_is_a_one_mark_route_with_no_incoming_leg() {
        // F15.1. The mode exists as data, not as a code path.
        let route = Route::lookahead_point(Vec2::new(12.0, -7.0), 15.0);
        route
            .validate()
            .expect("a lookahead point is a valid route");
        assert_eq!(route.marks.len(), 1);
        assert_eq!(route.legs(), 1);
        let leg = route.leg(0).unwrap();
        assert_eq!(leg.from, None);
        assert_eq!(leg.line(), None);
        assert_eq!(leg.direction(), None);
        assert_eq!(leg.to, Vec2::new(12.0, -7.0));
    }

    #[test]
    fn validate_names_the_offending_mark() {
        assert_eq!(Route::new(vec![], 1).validate(), Err(RouteError::NoMarks));

        let mut r = rectangle();
        r.laps = 0;
        assert_eq!(r.validate(), Err(RouteError::NoLaps));

        let mut r = rectangle();
        r.marks[2].radius = -0.5;
        assert_eq!(r.validate(), Err(RouteError::NegativeRadius { mark: 2 }));

        let mut r = rectangle();
        r.marks[1].position = Vec2::new(0.0, f64::NAN);
        assert_eq!(
            r.validate(),
            Err(RouteError::NotFinite {
                mark: 1,
                field: "position"
            })
        );

        let mut r = rectangle();
        r.marks[2].position = r.marks[1].position;
        assert_eq!(r.validate(), Err(RouteError::ZeroLengthLeg { mark: 2 }));

        let mut r = rectangle();
        r.marks[3].rounding = Rounding::Gate(Vec2::new(5.0, 5.0), Vec2::new(5.0, 5.0));
        assert_eq!(r.validate(), Err(RouteError::GatePostsCoincide { mark: 3 }));

        // A side on a route that has no direction to measure it against.
        let r = Route::new(
            vec![Mark {
                position: Vec2::new(1.0, 2.0),
                radius: 10.0,
                rounding: Rounding::Port,
            }],
            1,
        );
        assert_eq!(
            r.validate(),
            Err(RouteError::SidedRoundingWithoutALeg { mark: 0 })
        );

        let mut r = Route::lookahead_point(Vec2::new(1.0, 2.0), 10.0);
        r.laps = 2;
        assert_eq!(r.validate(), Err(RouteError::LapsNeedTwoMarks));

        rectangle().validate().expect("the rectangle is sailable");
    }

    #[test]
    fn mirroring_flips_the_side_and_negates_y() {
        let route = rectangle().mirrored();
        assert_eq!(route.marks[1].position, Vec2::new(0.0, -40.0));
        assert_eq!(route.marks[1].rounding, Rounding::Starboard);
        // Twice is the identity, like `testkit::mirror_involution`.
        assert_eq!(route.mirrored(), rectangle());

        let gate = Rounding::Gate(Vec2::new(1.0, 2.0), Vec2::new(3.0, -4.0));
        assert_eq!(
            gate.mirrored(),
            Rounding::Gate(Vec2::new(1.0, -2.0), Vec2::new(3.0, 4.0))
        );
        assert_eq!(gate.mirrored().mirrored(), gate);
    }

    #[test]
    fn the_required_side_is_written_down_once() {
        // Leaving a mark to starboard puts the boat to the left of the leg
        // direction, which is a positive offset along the left normal.
        assert_eq!(Rounding::Starboard.required_side(), Some(1.0));
        assert_eq!(Rounding::Port.required_side(), Some(-1.0));
        assert_eq!(Rounding::Either.required_side(), None);
        assert_eq!(
            Rounding::Gate(Vec2::ZERO, Vec2::new(1.0, 0.0)).required_side(),
            None
        );
    }

    #[test]
    fn position_reads_the_world_frame_pose() {
        let st = BoatState {
            x: 3.0,
            y: -4.0,
            psi: 1.0,
            phi: 0.5,
            ..BoatState::ZERO
        };
        assert_eq!(position(&st), Vec2::new(3.0, -4.0));
    }

    /// v2 F14.1: the arrow runs `course → physics` and never the other way.
    ///
    /// Section acceptance criterion 2, asserted here rather than left to
    /// review (RV22). The same assertion `sailgym-task` makes.
    #[test]
    fn physics_does_not_depend_on_the_course_crate() {
        let out = std::process::Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["tree", "-p", "sailgym-physics", "--edges", "all"])
            .output();
        let Ok(out) = out else {
            eprintln!("skip: cargo is not runnable here");
            return;
        };
        if !out.status.success() {
            eprintln!("skip: cargo tree failed");
            return;
        }
        let tree = String::from_utf8_lossy(&out.stdout);
        for forbidden in ["sailgym-course", "sailgym-task", "wasm-bindgen"] {
            assert!(
                !tree.contains(forbidden),
                "sailgym-physics must not depend on {forbidden} (v2 F14.1, F8.1):\n{tree}"
            );
        }
    }
}
