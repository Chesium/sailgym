//! Passage and guidance, driven from the recorded golden trajectories
//! (section 04 task 4.4).
//!
//! # Why this is the cheap test
//!
//! No controller, no simulation and **no new source of truth**. The six
//! golden trajectories in `crates/sailgym-physics/tests/golden/*.json` are
//! already committed and already regression-tested by gate step 5, so a
//! course laid over one of them is a fixture that costs nothing to keep and
//! that nobody can quietly regenerate: `gen_golden` refuses to run on a dirty
//! physics tree, and `--test regression` compares every sample.
//!
//! # What is asserted
//!
//! | test | what it measures |
//! |---|---|
//! | [`the_six_goldens_produce_their_committed_passage_sequences`] | each hand-laid route's exact `(leg, sample)` sequence |
//! | [`reordering_one_mark_changes_the_sequence`] | the committed sequence is a property of the route, not of the track |
//! | [`flipping_one_required_side_loses_that_passage`] | the sided clause is load-bearing on real tracks, not only on fixtures |
//! | [`passage_is_monotone_over_all_six_goldens`] | task 4.2's monotonicity clause, over every recorded track |
//! | [`guidance_agrees_with_an_independent_cross_track_on_every_sample`] | the one definition of cross-track error, checked against a different formula at 906 points |
//! | [`the_committed_routes_are_valid_and_the_clearances_are_as_recorded`] | the fixtures are what the data file says they are |
//!
//! # The sample rate
//!
//! The goldens are sampled at 5 Hz — every 0.2 s, every 40th physics step —
//! and the tracker is driven at exactly that rate. It is coarse: at 3 m/s the
//! boat moves 0.6 m per step, and a mark whose plane is crossed twice inside
//! one step would be missed (`progress::Tracker::observe` says so). The
//! committed routes are laid out so that does not happen, and
//! [`the_committed_routes_are_valid_and_the_clearances_are_as_recorded`]
//! keeps the clearances large enough — several metres — that it cannot start
//! happening quietly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sailgym_course::guidance::{guidance_at, CourseParams};
use sailgym_course::passage;
use sailgym_course::progress::Tracker;
use sailgym_course::route::{Rounding, Route};
use sailgym_physics::state::BoatState;
use sailgym_physics::vec::Vec2;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// The fixtures
// ---------------------------------------------------------------------------

/// The committed expectations, `tests/replay/routes.json`.
#[derive(Debug, Deserialize)]
struct Fixtures {
    /// Why the file exists and how to read it. Deserialised so a reader who
    /// deletes it gets a test failure rather than a silent loss.
    #[allow(dead_code)]
    note: Vec<String>,
    scenarios: BTreeMap<String, Fixture>,
}

#[derive(Clone, Debug, Deserialize)]
struct Fixture {
    route: Route,
    final_leg: u32,
    events: Vec<Event>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
struct Event {
    leg: u32,
    mark: usize,
    sample: usize,
    /// Signed lateral clearance at the crossing, metres. Absent for a gate.
    #[serde(default)]
    clearance_m: Option<f64>,
    /// Fractional position between the posts. Absent for a mark.
    #[serde(default)]
    along_gate: Option<f64>,
}

/// One golden trajectory, read for the two fields this crate cares about.
///
/// The `x` and `y` columns are found **by name** from the file's own `fields`
/// list, which is `state::STATE_FIELDS` in F8.3 order. Indexing by a hard
/// 0 and 1 would make this test depend on the snapshot layout, which is
/// exactly the coupling F8.3 exists to make explicit.
#[derive(Debug, Deserialize)]
struct Golden {
    scenario: String,
    dt: f64,
    sample_hz: f64,
    fields: Vec<String>,
    samples: Vec<Vec<f64>>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> Fixtures {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replay/routes.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn golden(scenario: &str) -> Golden {
    let path = repo_root()
        .join("crates/sailgym-physics/tests/golden")
        .join(format!("{scenario}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let g: Golden =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(g.scenario, scenario);
    g
}

impl Golden {
    /// The states, carrying the recorded `t` as well as the position.
    fn states(&self) -> Vec<BoatState> {
        let it = self.column("t");
        let ix = self.column("x");
        let iy = self.column("y");
        self.samples
            .iter()
            .map(|s| BoatState {
                x: s[ix],
                y: s[iy],
                t: s[it],
                ..BoatState::ZERO
            })
            .collect()
    }

    fn column(&self, name: &str) -> usize {
        self.fields
            .iter()
            .position(|f| f == name)
            .unwrap_or_else(|| panic!("{}: no `{name}` column", self.scenario))
    }
}

/// Drive a tracker over a whole track and collect `(leg, sample)`.
fn run(route: &Route, states: &[BoatState]) -> (Vec<(u32, usize)>, u32) {
    let mut tracker = Tracker::start(route.clone(), &states[0]).expect("a valid committed route");
    let mut out = Vec::new();
    for (i, st) in states.iter().enumerate().skip(1) {
        if tracker.observe(st).is_some() {
            out.push((tracker.leg_index() - 1, i));
        }
    }
    (out, tracker.leg_index())
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[test]
fn the_six_goldens_produce_their_committed_passage_sequences() {
    let f = fixtures();
    assert_eq!(f.scenarios.len(), 6, "all six goldens carry a route");
    for (scenario, fixture) in &f.scenarios {
        let g = golden(scenario);
        let states = g.states();
        let (seen, final_leg) = run(&fixture.route, &states);
        let want: Vec<(u32, usize)> = fixture.events.iter().map(|e| (e.leg, e.sample)).collect();
        assert_eq!(
            seen, want,
            "{scenario}: passage sequence (leg, sample) does not match the committed one"
        );
        assert_eq!(final_leg, fixture.final_leg, "{scenario}: final leg");
        // …and each passage ended at the mark the data file names, which is
        // not the same statement as the leg index once `laps > 1`.
        let mut tracker =
            Tracker::start(fixture.route.clone(), &states[0]).expect("a valid committed route");
        for st in states.iter().skip(1) {
            tracker.observe(st);
        }
        let marks: Vec<usize> = tracker.passages().iter().map(|p| p.mark_index).collect();
        let want_marks: Vec<usize> = fixture.events.iter().map(|e| e.mark).collect();
        assert_eq!(marks, want_marks, "{scenario}: marks");
        // The recorded time of each passage is the later sample's own `t`,
        // and the goldens sample every `1 / sample_hz` seconds.
        for (p, e) in tracker.passages().iter().zip(&fixture.events) {
            let expected_t = e.sample as f64 / g.sample_hz;
            assert!(
                (p.t - expected_t).abs() < g.dt,
                "{scenario}: passage at sample {} recorded t = {} not {expected_t}",
                e.sample,
                p.t
            );
        }
        assert!(
            !fixture.events.is_empty(),
            "{scenario}: a route with no passages proves nothing"
        );
    }
}

#[test]
fn reordering_one_mark_changes_the_sequence() {
    // Task 4.4's acceptance: "a deliberate one-mark reorder makes it fail".
    // Swapping the first two marks of each committed route changes what the
    // rule decides, on every one of the six tracks — which is what says the
    // committed sequences describe the route rather than the track.
    let f = fixtures();
    for (scenario, fixture) in &f.scenarios {
        let states = golden(scenario).states();
        let (before, _) = run(&fixture.route, &states);

        let mut reordered = fixture.route.clone();
        reordered.marks.swap(0, 1);
        let (after, _) = match Tracker::start(reordered.clone(), &states[0]) {
            Ok(_) => run(&reordered, &states),
            // A reorder that makes the route invalid is also a change, and a
            // louder one.
            Err(_) => (vec![(u32::MAX, usize::MAX)], 0),
        };
        assert_ne!(
            before, after,
            "{scenario}: swapping marks 0 and 1 left the passage sequence unchanged"
        );
    }
}

#[test]
fn flipping_one_required_side_loses_that_passage() {
    // The sided clause, on recorded tracks rather than on a fixture. For
    // every mark that carries a side, requiring the other one loses that
    // passage — the boat went where it went.
    let f = fixtures();
    let mut flipped = 0usize;
    for (scenario, fixture) in &f.scenarios {
        let states = golden(scenario).states();
        for (i, mark) in fixture.route.marks.iter().enumerate() {
            let other = match mark.rounding {
                Rounding::Port => Rounding::Starboard,
                Rounding::Starboard => Rounding::Port,
                _ => continue,
            };
            let mut route = fixture.route.clone();
            route.marks[i].rounding = other;
            let (seen, _) = run(&route, &states);
            let want: Vec<(u32, usize)> =
                fixture.events.iter().map(|e| (e.leg, e.sample)).collect();
            assert_ne!(
                seen, want,
                "{scenario}: mark {i} passes on either side, so its rounding says nothing"
            );
            flipped += 1;
        }
    }
    assert!(flipped >= 10, "only {flipped} sided marks were exercised");
}

#[test]
fn passage_is_monotone_over_all_six_goldens() {
    // Task 4.2's acceptance clause, measured over every recorded track: the
    // leg index rises by one or not at all, never falls, and a boat that
    // crosses back out has not un-passed its mark.
    let f = fixtures();
    let mut steps = 0usize;
    for (scenario, fixture) in &f.scenarios {
        let states = golden(scenario).states();
        let mut leg = 0u32;
        let mut tracker =
            Tracker::start(fixture.route.clone(), &states[0]).expect("a valid committed route");
        for (i, st) in states.iter().enumerate().skip(1) {
            tracker.observe(st);
            let next = tracker.leg_index();
            assert!(next >= leg, "{scenario}: leg index fell at sample {i}");
            assert!(
                next <= leg + 1,
                "{scenario}: leg index jumped by {} at sample {i}",
                next - leg
            );
            leg = next;
            steps += 1;
        }
    }
    assert_eq!(steps, 6 * 150, "every sample of every golden was driven");
}

#[test]
fn guidance_agrees_with_an_independent_cross_track_on_every_sample() {
    // RV20's other half: cross-track error has one definition, and this is a
    // different formula for it — the perpendicular component of the boat's
    // offset from the line's start, taken by subtracting the along-track
    // projection rather than by a cross product. The two must agree.
    let f = fixtures();
    let p = CourseParams::default();
    let mut checked = 0usize;
    let mut worst = 0.0f64;
    for (scenario, fixture) in &f.scenarios {
        let states = golden(scenario).states();
        let track: Vec<Vec2> = states.iter().map(|s| Vec2::new(s.x, s.y)).collect();
        let mut tracker =
            Tracker::start(fixture.route.clone(), &states[0]).expect("a valid committed route");
        for (i, st) in states.iter().enumerate() {
            if i > 0 {
                tracker.observe(st);
            }
            let Some(g) = guidance_at(&fixture.route, tracker.leg_index(), track[i], &p) else {
                continue; // the course is finished
            };
            let (a, b) = g.line.expect("every committed route has legs");
            let d = b - a;
            let len = d.length();
            let unit = d / len;
            let rel = track[i] - a;
            let along = rel.dot(unit);
            let perp = rel - unit * along;
            // Signed by the side: `perp` is the vector, `left` says which way
            // is positive.
            let left = passage::left_normal(unit);
            let independent = perp.dot(left);
            let delta = (independent - g.signed_cross_track).abs();
            worst = worst.max(delta);
            assert!(
                delta < 1e-9,
                "{scenario} sample {i}: {independent} vs {}",
                g.signed_cross_track
            );

            // The target is on the leg's own segment, and never beyond the
            // mark.
            let t = (g.target - a).dot(unit);
            assert!(
                (-1e-9..=len + 1e-9).contains(&t),
                "{scenario} sample {i}: target is {t} along a {len} m leg"
            );
            assert!(
                (g.target - (a + unit * t)).length() < 1e-9,
                "{scenario} sample {i}: target is off the line"
            );
            checked += 1;
        }
    }
    assert!(checked >= 600, "only {checked} samples carried guidance");
    eprintln!("[replay] worst cross-track disagreement over {checked} samples: {worst:.3e} m");
}

#[test]
fn the_committed_routes_are_valid_and_the_clearances_are_as_recorded() {
    // The fixtures are what the data file says they are: every route is
    // sailable, every mark that carries a side records the clearance that
    // side was measured at, and every clearance is comfortably outside the
    // mark's own radius — which is what keeps the 5 Hz sample rate adequate.
    let f = fixtures();
    for (scenario, fixture) in &f.scenarios {
        fixture
            .route
            .validate()
            .unwrap_or_else(|e| panic!("{scenario}: {e}"));
        assert_eq!(fixture.route.laps, 1, "{scenario}");
        for e in &fixture.events {
            let mark = &fixture.route.marks[e.mark];
            match mark.rounding {
                Rounding::Gate(_, _) => {
                    let along = e.along_gate.expect("a gate event records `along_gate`");
                    assert!(
                        (0.0..=1.0).contains(&along),
                        "{scenario}: gate crossing at {along}"
                    );
                    assert!(e.clearance_m.is_none());
                }
                Rounding::Port | Rounding::Starboard => {
                    let c = e.clearance_m.expect("a sided event records `clearance_m`");
                    let want_positive = mark.rounding == Rounding::Starboard;
                    assert_eq!(
                        c > 0.0,
                        want_positive,
                        "{scenario}: mark {} recorded clearance {c} for {:?}",
                        e.mark,
                        mark.rounding
                    );
                    assert!(
                        c.abs() > mark.radius * 1.5,
                        "{scenario}: mark {} clears by only {c} m against a {} m radius",
                        e.mark,
                        mark.radius
                    );
                }
                Rounding::Either => {}
            }
        }
    }
}
