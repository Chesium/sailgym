//! The waypoint-course challenge, measured (v2 section 12, task 12.6).
//!
//! The same three kinds of test `practice.rs` uses, for the same reasons.
//!
//! 1. **Table-driven traces** drive the evaluator along a synthetic track — a
//!    list of positions, with no physics in it at all — so a failing assertion
//!    names the rule and not the boat. One per outcome the PRD lists: success,
//!    miss-then-recover, capsize and timeout.
//! 2. **A contract test** for RV62: identical traces score identically under
//!    six batch sizes, and the events land on the same **steps**.
//! 3. **An agreement test** for RV66: every cut the task reports is a cut
//!    `passage::cut_between` reports, on the same step. The task calls that
//!    function, so what this asserts is the wiring — which is the half that can
//!    silently come loose.
//!
//! Run them with the numbers on screen:
//!
//! ```text
//! cargo test -p sailgym-task --test course -- --nocapture
//! ```

use sailgym_course::{passage, CourseId, Vec2};
use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls};
use sailgym_task::{
    FailureReason, Outcome, StepObservation, TaskId, TaskRun, TaskSpec, COURSE_TASK_IDS, TASK_IDS,
    TASK_VERSION,
};

/// The physics timestep the whole repository runs at (F7, `sim.dt`), read from
/// the catalogue rather than written here.
fn dt() -> f64 {
    sailgym_physics::parameters::BoatParameters::ilca7().sim.dt
}

/// A northerly: the air moves toward −y (F6.1).
const NORTHERLY: Vec2 = Vec2 { x: 0.0, y: -5.0 };

/// One synthetic step: where the boat is, and whether it has gone over.
#[derive(Clone, Copy)]
struct Step {
    at: Vec2,
    capsized: bool,
}

impl Step {
    fn at(x: f64, y: f64) -> Self {
        Self {
            at: Vec2::new(x, y),
            capsized: false,
        }
    }

    fn capsized(mut self) -> Self {
        self.capsized = true;
        self
    }
}

/// Feed a track to a fresh attempt at `course`, one step per point.
///
/// The initial state is the course's own start, so the first "previous
/// position" is where the course begins — exactly as
/// `Tracker::start` requires, and the reason a mark the boat is already past at
/// `t = 0` is not passed by starting.
fn run(course: CourseId, track: &[Step], batch: u64) -> TaskRun {
    let spec = TaskSpec::shipped(TaskId::Course(course));
    let doc = course.load().expect("a shipped course");
    let initial = obs(0, doc.start, false);
    let mut attempt = TaskRun::start(spec, &initial).expect("a valid configuration");
    // `batch` changes nothing about what is fed in; it exists so the contract
    // test can prove that (RV62). The evaluator is fed one step at a time
    // whatever it is — a batch is a property of the *caller*, and the whole
    // point is that it cannot be observed.
    let mut i = 0usize;
    while i < track.len() {
        let upto = (i + batch as usize).min(track.len());
        for (k, step) in track.iter().enumerate().take(upto).skip(i) {
            attempt.observe(&obs(k as u64 + 1, step.at, step.capsized));
        }
        i = upto;
    }
    attempt
}

/// One observation at step `step`.
fn obs(step: u64, at: Vec2, capsized: bool) -> StepObservation {
    StepObservation {
        step,
        state: BoatState {
            x: at.x,
            y: at.y,
            t: step as f64 * dt(),
            ..BoatState::ZERO
        },
        controls: Controls::default(),
        wind_world: NORTHERLY,
        capsized,
    }
}

/// A straight track from `a` to `b`, `n` steps, excluding `a`.
fn leg(a: Vec2, b: Vec2, n: usize) -> Vec<Step> {
    (1..=n)
        .map(|i| {
            let f = i as f64 / n as f64;
            Step::at(a.x + f * (b.x - a.x), a.y + f * (b.y - a.y))
        })
        .collect()
}

/// A clean track through every waypoint of `course`, with a little overshoot
/// past the last one.
///
/// Passage is half-open — the boat must get **strictly** past the gate line
/// (F15.3) — so a track that stops exactly on a waypoint has not passed it.
/// `waypoints.rs` says so in as many words.
fn clean_track(course: CourseId) -> Vec<Step> {
    let doc = course.load().expect("a shipped course");
    let mut out = Vec::new();
    let mut from = doc.start;
    for p in &doc.waypoints {
        out.extend(leg(from, *p, 40));
        // Two metres beyond, along the leg, so the crossing is strict.
        let d = (*p - from).normalize();
        out.extend(leg(*p, *p + d * 2.0, 4));
        from = *p;
    }
    out
}

// ---------------------------------------------------------------------------
// 1. Table-driven traces
// ---------------------------------------------------------------------------

#[test]
fn a_clean_track_through_every_waypoint_succeeds() {
    for id in COURSE_TASK_IDS {
        let course = id.course().expect("a course id");
        let doc = course.load().expect("a shipped course");
        let attempt = run(course, &clean_track(course), 1);
        assert_eq!(
            attempt.outcome(),
            Outcome::Succeeded,
            "{}: {:?}",
            id.as_str(),
            attempt.outcome()
        );
        // One `waypoint_passed` per waypoint, numbered from 1, in order.
        let passed: Vec<f64> = attempt
            .events()
            .iter()
            .filter(|e| e.id == "waypoint_passed")
            .map(|e| e.value)
            .collect();
        let want: Vec<f64> = (1..=doc.waypoints.len()).map(|n| n as f64).collect();
        assert_eq!(passed, want, "{}", id.as_str());
        assert!(
            attempt.events().iter().all(|e| e.id != "waypoint_missed"),
            "{}: a clean track cut a waypoint",
            id.as_str()
        );
        // A split per waypoint, increasing, and the metric is the elapsed time.
        let splits = attempt.splits();
        assert_eq!(splits.len(), doc.waypoints.len());
        assert!(splits.windows(2).all(|w| w[0] < w[1]), "{splits:?}");
        assert!((attempt.metric_value() - attempt.elapsed_s()).abs() < 1e-12);
        assert_eq!(attempt.report().metric.id, "course_time");
        assert_eq!(attempt.report().metric.unit, "s");
        // The last event is the success, and **Inspect** lands on it because
        // there was no miss.
        assert_eq!(
            attempt.highlight().map(|e| e.id.clone()),
            Some("succeeded".to_string()),
            "{}",
            id.as_str()
        );
    }
}

#[test]
fn a_cut_is_an_event_and_the_attempt_continues() {
    // `reach`: start (0, 0), waypoints (30, 0), (60, −10), (90, 0), gates 5 m
    // either side and square to the leg. Cross waypoint 1's line **12 m to
    // port** of it — outside the posts — then come back behind the line and
    // through the gate properly, and go on to finish the course.
    let course = CourseId::Reach;
    let doc = course.load().expect("a shipped course");
    let w1 = doc.waypoints[0];
    let mut track = Vec::new();
    // Out to the side and across the line: three clauses of F15.3 hold and the
    // lateral one does not, which is exactly a cut (D2).
    track.extend(leg(doc.start, Vec2::new(w1.x - 4.0, w1.y + 12.0), 20));
    track.extend(leg(
        Vec2::new(w1.x - 4.0, w1.y + 12.0),
        Vec2::new(w1.x + 4.0, w1.y + 12.0),
        8,
    ));
    // Back behind the line, then through the gate.
    track.extend(leg(
        Vec2::new(w1.x + 4.0, w1.y + 12.0),
        Vec2::new(w1.x - 6.0, w1.y),
        10,
    ));
    track.extend(leg(
        Vec2::new(w1.x - 6.0, w1.y),
        Vec2::new(w1.x + 2.0, 0.0),
        8,
    ));
    // …and on round the rest of the course.
    let mut from = Vec2::new(w1.x + 2.0, 0.0);
    for p in &doc.waypoints[1..] {
        track.extend(leg(from, *p, 40));
        let d = (*p - from).normalize();
        track.extend(leg(*p, *p + d * 2.0, 4));
        from = *p;
    }

    let attempt = run(course, &track, 1);
    assert_eq!(
        attempt.outcome(),
        Outcome::Succeeded,
        "a miss is an event and not the end (D3): {:?}",
        attempt.outcome()
    );
    let missed: Vec<(String, f64)> = attempt
        .events()
        .iter()
        .filter(|e| e.id == "waypoint_missed")
        .map(|e| (e.id.clone(), e.value))
        .collect();
    assert_eq!(
        missed,
        vec![("waypoint_missed".to_string(), 1.0)],
        "exactly one cut, of waypoint 1"
    );
    // The ids, in order: the cut, then every passage.
    let ids: Vec<&str> = attempt.events().iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "waypoint_missed",
            "waypoint_passed",
            "waypoint_passed",
            "waypoint_passed",
            "succeeded"
        ]
    );
    // **Inspect** lands on the first miss, not on the finish and not on the
    // last event.
    let highlight = attempt.highlight().expect("an event to inspect");
    assert_eq!(highlight.id, "waypoint_missed");
    assert_eq!(highlight.value, 1.0);
    // And the events are ordered by step, as the envelope requires.
    assert!(attempt.events().windows(2).all(|w| w[0].step <= w[1].step));
}

#[test]
fn a_capsize_ends_a_course_attempt() {
    let course = CourseId::Triangle;
    let mut track = clean_track(course);
    // Half way round, the boat goes over. F6.10's flag is the physics crate's
    // and is read, never decided here (RV64).
    let half = track.len() / 2;
    track.truncate(half);
    track.push(track[half - 1].capsized());
    track.push(track[half - 1].capsized());
    let attempt = run(course, &track, 1);
    assert_eq!(
        attempt.outcome(),
        Outcome::Failed {
            reason: FailureReason::Capsized
        }
    );
    assert_eq!(
        attempt.metric_value(),
        0.0,
        "an unfinished course has no time"
    );
    assert_eq!(
        attempt.events().last().map(|e| e.id.clone()),
        Some("failed_capsized".to_string())
    );
}

#[test]
fn a_course_that_goes_nowhere_times_out() {
    let course = CourseId::Reach;
    let spec = TaskSpec::shipped(TaskId::Course(course));
    let limit = spec.time_limit_s();
    // Sitting still, one step past the limit.
    let steps = (limit / dt()).ceil() as usize + 1;
    let track: Vec<Step> = (0..steps).map(|_| Step::at(0.0, 0.0)).collect();
    let attempt = run(course, &track, 1);
    assert_eq!(attempt.outcome(), Outcome::TimedOut);
    assert!(attempt.elapsed_s() >= limit);
    assert!(attempt.splits().is_empty());
    assert_eq!(
        attempt.events().len(),
        1,
        "nothing happened but the clock: {:?}",
        attempt.events()
    );
    assert_eq!(
        attempt.events().last().map(|e| e.id.clone()),
        Some("timed_out".to_string())
    );
}

// ---------------------------------------------------------------------------
// 2. The contract (RV62)
// ---------------------------------------------------------------------------

#[test]
fn identical_tracks_agree_under_six_batch_sizes() {
    const BATCHES: [u64; 6] = [1, 2, 7, 10, 100, 100_000];
    for id in COURSE_TASK_IDS {
        let course = id.course().expect("a course id");
        let track = clean_track(course);
        let mut reference: Option<TaskRun> = None;
        for batch in BATCHES {
            let attempt = run(course, &track, batch);
            match &reference {
                None => reference = Some(attempt),
                Some(want) => {
                    assert_eq!(
                        want.outcome(),
                        attempt.outcome(),
                        "{} at batch {batch}",
                        id.as_str()
                    );
                    assert_eq!(
                        want.events().len(),
                        attempt.events().len(),
                        "{} at batch {batch}",
                        id.as_str()
                    );
                    for (a, b) in want.events().iter().zip(attempt.events().iter()) {
                        assert_eq!(a.id, b.id, "{} at batch {batch}", id.as_str());
                        assert_eq!(
                            a.step,
                            b.step,
                            "{} at batch {batch}: an event moved step",
                            id.as_str()
                        );
                        assert_eq!(a.t.to_bits(), b.t.to_bits());
                        assert_eq!(a.value.to_bits(), b.value.to_bits());
                    }
                    assert_eq!(want.splits(), attempt.splits());
                    assert_eq!(
                        want.metric_value().to_bits(),
                        attempt.metric_value().to_bits()
                    );
                }
            }
        }
    }
}

/// Attaching a course evaluator cannot change a trajectory.
///
/// The same assertion `practice.rs` makes for the three skills, on the real
/// `Simulation`, with the course evaluator attached: thirteen state scalars,
/// bit for bit.
#[test]
fn a_course_evaluator_does_not_perturb_the_physics() {
    let script = |with_task: bool| -> [f64; sailgym_physics::state::STATE_LEN] {
        let sc = load_shipped("free_sail").expect("a shipped scenario");
        let params = sc.to_parameters().expect("a valid catalogue");
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("the scenario loads");
        let mut attempt = if with_task {
            Some(
                TaskRun::start(
                    TaskSpec::shipped(TaskId::Course(CourseId::Reach)),
                    &StepObservation::of(&sim),
                )
                .expect("a valid configuration"),
            )
        } else {
            None
        };
        for step in 0..6_000u32 {
            // A script a person could reproduce: haul for ten seconds, then
            // steer to port for five.
            let (rudder, sheet) = if step < 2_000 {
                (0.0, -1.0)
            } else if step < 3_000 {
                (-1.0, -1.0)
            } else {
                (0.0, 0.0)
            };
            sim.set_controls(Controls {
                rudder_rate_cmd: rudder,
                sheet_rate_cmd: sheet,
                sheet_release: false,
            });
            sim.advance(1);
            if let Some(a) = attempt.as_mut() {
                a.observe(&StepObservation::of(&sim));
            }
        }
        sim.state().to_array()
    };
    let without = script(false);
    let with = script(true);
    for (i, (a, b)) in without.iter().zip(with.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "state scalar {i} moved when a course evaluator was attached: {a} vs {b}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Agreement with the one definition of a cut (RV66)
// ---------------------------------------------------------------------------

#[test]
fn every_cut_the_task_reports_is_a_cut_cut_between_reports() {
    let course = CourseId::Reach;
    let doc = course.load().expect("a shipped course");
    let route = doc.route().expect("a valid route");
    let w1 = doc.waypoints[0];
    // A track that cuts waypoint 1 twice: across the line wide, back, and
    // across wide again, before finally going through the gate.
    let wide_a = Vec2::new(w1.x - 4.0, w1.y + 12.0);
    let wide_b = Vec2::new(w1.x + 4.0, w1.y + 12.0);
    let mut track = Vec::new();
    track.extend(leg(doc.start, wide_a, 20));
    track.extend(leg(wide_a, wide_b, 8));
    track.extend(leg(wide_b, wide_a, 8));
    track.extend(leg(wide_a, wide_b, 8));
    track.extend(leg(wide_b, Vec2::new(w1.x - 6.0, w1.y), 10));
    track.extend(leg(
        Vec2::new(w1.x - 6.0, w1.y),
        Vec2::new(w1.x + 2.0, 0.0),
        8,
    ));

    let attempt = run(course, &track, 1);
    let task_cuts: Vec<u64> = attempt
        .events()
        .iter()
        .filter(|e| e.id == "waypoint_missed")
        .map(|e| e.step)
        .collect();

    // The same question asked of `sailgym-course` directly, walking the same
    // track with the same leg index. **One rule, in one place** (D2, RV66).
    let mut leg_index = 0u32;
    let mut prev = doc.start;
    let mut direct_cuts: Vec<u64> = Vec::new();
    for (i, step) in track.iter().enumerate() {
        let cur = step.at;
        let a = BoatState {
            x: prev.x,
            y: prev.y,
            ..BoatState::ZERO
        };
        let b = BoatState {
            x: cur.x,
            y: cur.y,
            ..BoatState::ZERO
        };
        if passage::passed(&route, leg_index, &a, &b) {
            leg_index += 1;
        } else if passage::cut_between(&route, leg_index, prev, cur) {
            direct_cuts.push(i as u64 + 1);
        }
        prev = cur;
    }

    assert!(
        task_cuts.len() >= 2,
        "the fixture must cut twice, or the agreement proves little: {task_cuts:?}"
    );
    assert_eq!(
        task_cuts, direct_cuts,
        "the task and `cut_between` disagree about which steps were cuts"
    );
    // …and the attempt is still going, because a miss is not the end (D3).
    assert_eq!(attempt.outcome(), Outcome::Running);
    // The report is the record the page is handed, and it carries the same
    // events and the same splits.
    let report = attempt.report();
    assert_eq!(report.events, attempt.events());
    assert_eq!(report.outcome, Outcome::Running);
    // The boat came back and went through the gate, so the cut is no longer
    // outstanding and waypoint 1 is passed.
    assert_eq!(report.progress.phase, "sailing");
    assert_eq!(report.progress.value, 1.0);
    assert_eq!(report.progress.target, 3.0);
}

// ---------------------------------------------------------------------------
// 4. The configuration contract
// ---------------------------------------------------------------------------

#[test]
fn every_shipped_course_challenge_validates_and_describes_itself() {
    for id in COURSE_TASK_IDS {
        let spec = TaskSpec::shipped(id);
        spec.validate()
            .unwrap_or_else(|e| panic!("{}: {e}", id.as_str()));
        assert_eq!(spec.id(), id);
        assert_eq!(spec.version(), TASK_VERSION);
        assert_eq!(TaskId::parse(id.as_str()), Some(id));
        // The ids are the ones the PRD names.
        assert!(id.as_str().starts_with("course_"));
        // The identity is self-describing: the geometry is in it.
        let t = spec.thresholds();
        let doc = id.course().expect("a course").load().expect("a document");
        assert_eq!(t.get("course.half_width"), Some(&doc.half_width));
        assert_eq!(
            t.get("course.waypoint_count"),
            Some(&(doc.waypoints.len() as f64))
        );
        assert_eq!(t.get("course.start_x"), Some(&doc.start.x));
        assert_eq!(t.get("course.start_y"), Some(&doc.start.y));
        for (i, p) in doc.waypoints.iter().enumerate() {
            assert_eq!(t.get(&format!("course.waypoint_{}_x", i + 1)), Some(&p.x));
            assert_eq!(t.get(&format!("course.waypoint_{}_y", i + 1)), Some(&p.y));
        }
        assert_eq!(t.get("time_limit_s"), Some(&spec.time_limit_s()));
        assert_eq!(spec.identity().id, id.as_str());
        assert_eq!(spec.identity().thresholds, t);
    }

    // Two courses are never the same experiment, because the geometry is in
    // the identity (F18.3).
    let a = TaskSpec::shipped(COURSE_TASK_IDS[0]).identity();
    let b = TaskSpec::shipped(COURSE_TASK_IDS[1]).identity();
    assert_ne!(a.thresholds, b.thresholds);
    assert_ne!(a.id, b.id);

    // The three skills are untouched: same list, same ids, same thresholds.
    assert_eq!(TASK_IDS.len(), 3);
    assert_eq!(
        TASK_IDS.map(|id| id.as_str()),
        ["get_moving", "complete_tack", "recover_from_heel"]
    );
    assert_eq!(TASK_VERSION, 1);
}

/// A course challenge's scenario and its document's are the same string.
///
/// `TaskId::scenario` returns `&'static str` and a course document's
/// `scenario` is parsed at run time, so the three literals in that method are a
/// second copy. This is where the copy is checked rather than trusted (F19.6's
/// device, same reason).
#[test]
fn every_course_challenges_scenario_is_its_documents() {
    for id in COURSE_TASK_IDS {
        let doc = id.course().expect("a course").load().expect("a document");
        assert_eq!(
            id.scenario(),
            doc.scenario,
            "{} names a different scenario from its document",
            id.as_str()
        );
        // …and it is one of the shipped six.
        load_shipped(id.scenario()).unwrap_or_else(|e| panic!("{}: {e}", id.as_str()));
    }
}

#[test]
fn a_rejected_course_configuration_names_the_field() {
    let bad = TaskSpec::WaypointCourse(sailgym_task::WaypointCourseConfig {
        course: CourseId::Reach,
        time_limit_s: 0.0,
    });
    assert_eq!(bad.validate().unwrap_err().field, "time_limit_s");
    let nan = TaskSpec::WaypointCourse(sailgym_task::WaypointCourseConfig {
        course: CourseId::Reach,
        time_limit_s: f64::NAN,
    });
    assert_eq!(nan.validate().unwrap_err().field, "time_limit_s");
    assert!(TaskRun::start(bad, &obs(0, Vec2::ZERO, false)).is_err());
}
