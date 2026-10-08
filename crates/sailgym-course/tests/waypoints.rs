//! Waypoint courses, end to end over the course layer (v2 section 12, task
//! 12.1).
//!
//! No controller and no simulation: synthetic tracks, sampled every 0.25 m —
//! finer than a boat at 3 m/s moves in one 200 Hz physics step — driven
//! through the same [`Tracker`] the episode runner uses, with cuts detected by
//! the same [`passage::cut_between`] it will use (RV66).
//!
//! | test | what it measures |
//! |---|---|
//! | [`every_shipped_course_is_sailable_in_its_own_scenario`] | each course validates, names a shipped scenario, and starts where that scenario puts the boat |
//! | [`every_shipped_waypoint_is_a_gate_square_to_its_leg_and_centred`] | RV65: no waypoint is a radius check or an unbounded `Either` |
//! | [`a_clean_track_passes_every_waypoint_in_order_and_cuts_none`] | the courses are sailable by the rule as written, overshoot included |
//! | [`a_track_outside_a_gate_cuts_it_and_can_come_back_through`] | a miss is a cut, the index does not move, and passage is still available afterwards |
//! | [`the_mirror_of_a_course_is_sailed_identically_by_the_mirrored_track`] | the open route's start mirrors with it, and so do the decisions |
//! | [`every_committed_route_round_trips_to_the_same_json`] | D1 changed no existing document |

use std::path::Path;

use sailgym_course::catalogue::CourseId;
use sailgym_course::passage::{self, left_normal};
use sailgym_course::progress::Tracker;
use sailgym_course::route::{Rounding, Route};
use sailgym_physics::scenario::{load_shipped, shipped_names};
use sailgym_physics::state::BoatState;
use sailgym_physics::vec::Vec2;

/// Sample spacing along a synthetic track, metres.
const STEP_M: f64 = 0.25;

/// How far a synthetic track carries on past each waypoint before turning
/// for the next one. Passage is half-open, so a track that touches an acute
/// waypoint's centre and turns away on the same sample has not passed it
/// (`waypoints.rs`).
const OVERSHOOT_M: f64 = 3.0;

fn state(p: Vec2, i: usize) -> BoatState {
    BoatState {
        x: p.x,
        y: p.y,
        t: i as f64 * 0.005,
        ..BoatState::ZERO
    }
}

/// Points along a polyline, at most `STEP_M` apart, every vertex included.
fn sample(polyline: &[Vec2]) -> Vec<Vec2> {
    let mut out = vec![polyline[0]];
    for w in polyline.windows(2) {
        let (a, b) = (w[0], w[1]);
        let n = ((b - a).length() / STEP_M).ceil().max(1.0) as usize;
        for k in 1..=n {
            out.push(if k == n {
                b
            } else {
                a + (b - a) * (k as f64 / n as f64)
            });
        }
    }
    out
}

/// What a drive over a track produced.
#[derive(Debug, PartialEq)]
struct Drive {
    /// `(leg, sample)` for every passage.
    passages: Vec<(u32, usize)>,
    /// `(leg, sample)` for every cut.
    cuts: Vec<(u32, usize)>,
    finished: bool,
}

/// Drive a tracker over `points`, detecting cuts the way the episode runner
/// does: on a step that did not pass the current leg's mark.
fn drive(route: &Route, points: &[Vec2]) -> Drive {
    let mut tracker = Tracker::start(route.clone(), &state(points[0], 0)).expect("a valid route");
    let mut d = Drive {
        passages: Vec::new(),
        cuts: Vec::new(),
        finished: false,
    };
    for (i, w) in points.windows(2).enumerate() {
        let leg = tracker.leg_index();
        if tracker.observe(&state(w[1], i + 1)).is_some() {
            d.passages.push((leg, i + 1));
        } else if passage::cut_between(route, leg, w[0], w[1]) {
            d.cuts.push((leg, i + 1));
        }
    }
    d.finished = tracker.finished();
    d
}

/// The unit direction of the leg arriving at waypoint `i`.
fn incoming(route: &Route, i: usize) -> Vec2 {
    route
        .leg(i as u32)
        .and_then(|l| l.direction())
        .expect("every leg of a waypoint course has a direction")
}

/// Start, then through each waypoint's centre with an overshoot.
fn clean_track(route: &Route) -> Vec<Vec2> {
    let mut poly = vec![route.start.expect("a waypoint course has a start")];
    for (i, mark) in route.marks.iter().enumerate() {
        poly.push(mark.position);
        poly.push(mark.position + incoming(route, i) * OVERSHOOT_M);
    }
    sample(&poly)
}

#[test]
fn every_shipped_course_is_sailable_in_its_own_scenario() {
    for id in CourseId::ALL {
        let course = id.load().unwrap_or_else(|e| panic!("{}: {e}", id.as_str()));
        assert!(
            shipped_names().contains(&course.scenario.as_str()),
            "{}: scenario `{}` is not shipped",
            id.as_str(),
            course.scenario
        );
        let scenario = load_shipped(&course.scenario).expect("a shipped scenario loads");
        assert_eq!(
            (course.start.x, course.start.y),
            (scenario.initial_state.x, scenario.initial_state.y),
            "{}: the course must start where its scenario puts the boat",
            id.as_str()
        );
        let route = course.route().expect("a shipped course builds");
        route.validate().expect("…and validates");
        assert_eq!(route.start, Some(course.start));
        assert_eq!(route.marks.len(), course.waypoints.len());
    }
}

#[test]
fn every_shipped_waypoint_is_a_gate_square_to_its_leg_and_centred() {
    // RV65. Exact on axis-aligned legs (`waypoints.rs`); to 1e-12 of the
    // gate's width on the diagonal ones, which is rounding and nothing else.
    for id in CourseId::ALL {
        let course = id.load().expect("loads");
        let route = course.route().expect("builds");
        let w = course.half_width;
        for (i, mark) in route.marks.iter().enumerate() {
            let Rounding::Gate(a, b) = mark.rounding else {
                panic!("{} waypoint {}: not a gate", id.as_str(), i + 1);
            };
            let d = incoming(&route, i);
            let tol = 1e-12 * w;
            assert!(
                (b - a).dot(d).abs() <= tol,
                "{} waypoint {}: not square to its leg",
                id.as_str(),
                i + 1
            );
            let mid = (a + b) * 0.5;
            assert!(
                (mid - mark.position).length() <= tol,
                "{} waypoint {}: not centred",
                id.as_str(),
                i + 1
            );
            assert!(((b - a).length() - 2.0 * w).abs() <= tol);
            assert_eq!(mark.radius, w);
        }
    }
}

#[test]
fn a_clean_track_passes_every_waypoint_in_order_and_cuts_none() {
    for id in CourseId::ALL {
        let route = id.load().expect("loads").route().expect("builds");
        let d = drive(&route, &clean_track(&route));
        let legs: Vec<u32> = d.passages.iter().map(|&(leg, _)| leg).collect();
        assert_eq!(
            legs,
            (0..route.legs()).collect::<Vec<_>>(),
            "{}: every waypoint, in order",
            id.as_str()
        );
        assert!(d.cuts.is_empty(), "{}: {:?}", id.as_str(), d.cuts);
        assert!(d.finished, "{}", id.as_str());
    }
}

#[test]
fn a_track_outside_a_gate_cuts_it_and_can_come_back_through() {
    let route = CourseId::Triangle
        .load()
        .expect("loads")
        .route()
        .expect("builds");
    let start = route.start.expect("a start");
    let p1 = route.marks[0].position;
    let d1 = incoming(&route, 0);
    let n1 = left_normal(d1);
    let w = route.marks[0].radius;

    // Past waypoint 1 at 1.2 × the half-width to one side: outside the gate.
    let wide = p1 + n1 * (1.2 * w);
    let poly = vec![
        start + n1 * (1.2 * w),
        wide,
        wide + d1 * OVERSHOOT_M,
        // Back round behind the line, and through the centre.
        p1 - d1 * 5.0,
        p1,
        p1 + d1 * OVERSHOOT_M,
    ];
    let d = drive(&route, &sample(&poly));

    assert_eq!(
        d.cuts.len(),
        1,
        "one crossing outside the gate: {:?}",
        d.cuts
    );
    assert_eq!(d.cuts[0].0, 0, "it is waypoint 1 that was cut");
    assert_eq!(d.passages.len(), 1, "…and then passed: {:?}", d.passages);
    assert_eq!(d.passages[0].0, 0);
    assert!(
        d.passages[0].1 > d.cuts[0].1,
        "the passage comes after the cut, on the way back through"
    );
    assert!(!d.finished);
}

#[test]
fn the_mirror_of_a_course_is_sailed_identically_by_the_mirrored_track() {
    let mirror = |p: Vec2| Vec2::new(p.x, -p.y);
    for id in CourseId::ALL {
        let course = id.load().expect("loads");
        let route = course.route().expect("builds");
        let track = clean_track(&route);
        let mirrored_track: Vec<Vec2> = track.iter().copied().map(mirror).collect();

        let original = drive(&route, &track);

        // The route mirrored as a value, start included: every product in
        // the passage rule is the same product with both signs flipped, so
        // the decisions are the same to the sample.
        let by_value = route.mirrored();
        assert_eq!(by_value.start, route.start.map(mirror));
        assert_eq!(
            drive(&by_value, &mirrored_track),
            original,
            "{}",
            id.as_str()
        );

        // The course rebuilt from mirrored inputs has each gate's posts in
        // the other order — a gate has no orientation of its own — so the
        // crossing is measured from the other post. A sample lying exactly on
        // a gate line (each waypoint vertex of this track does) can round to
        // the other side of zero and pass one sample later or earlier. The
        // outcome is what must not change.
        let rebuilt = Route::waypoints(
            mirror(course.start),
            &course
                .waypoints
                .iter()
                .copied()
                .map(mirror)
                .collect::<Vec<_>>(),
            course.half_width,
        )
        .expect("builds");
        let r = drive(&rebuilt, &mirrored_track);
        let legs = |d: &Drive| d.passages.iter().map(|&(leg, _)| leg).collect::<Vec<_>>();
        assert_eq!(legs(&r), legs(&original), "{}", id.as_str());
        assert!(r.cuts.is_empty() && r.finished, "{}: {r:?}", id.as_str());
        for (a, b) in r.passages.iter().zip(&original.passages) {
            assert!(a.1.abs_diff(b.1) <= 1, "{}: {a:?} vs {b:?}", id.as_str());
        }
    }
}

#[test]
fn every_committed_route_round_trips_to_the_same_json() {
    // D1 must change no existing document: section 04's committed routes,
    // read and written again, are the same JSON values, with no `start` key.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replay/routes.json");
    let text = std::fs::read_to_string(&path).expect("routes.json is readable");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("routes.json parses");
    let scenarios = doc["scenarios"].as_object().expect("a scenarios map");
    assert_eq!(scenarios.len(), 6);
    for (name, fixture) in scenarios {
        let original = &fixture["route"];
        let route: Route = serde_json::from_value(original.clone()).expect("a route");
        assert_eq!(route.start, None, "{name}");
        let again = serde_json::to_value(&route).expect("serialise");
        assert_eq!(&again, original, "{name}");
        assert!(again.get("start").is_none(), "{name}");
    }
}
