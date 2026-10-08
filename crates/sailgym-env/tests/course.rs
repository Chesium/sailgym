//! Open routes, one definition of a cut, and recorded practice (v2
//! section 12, task 12.2).
//!
//! # What this file is for
//!
//! Section 12's D1 gave a [`Route`] an optional `start`, and section 06's
//! episode runner kept a **copy** of the route — `Route::new(marks, laps)`
//! with every `Rounding` replaced by `Either` — to decide what a cut was.
//! `Route::new` sets `start: None`, so the copy's leg 0 ran from somewhere the
//! real route's did not, and the two disagreed about the one thing they are
//! not allowed to disagree about (v2 F19.4, RV66).
//!
//! The repair is that there is no copy: `passage::cut_between` builds its
//! probe from the leg the real route computed, so the start and the laps
//! travel with it by construction. `a_probe_that_drops_the_start_disagrees_
//! about_leg_0` is the named regression, and it measures the disagreement
//! rather than describing it.
//!
//! Nothing here authors physics. Every route below is built on the boat's
//! **own** hands-off track, measured first by [`track`], so a passage test is
//! about the geometry and not about a guess at where the boat goes.

use sailgym_agent::spec::Cadence;
use sailgym_course::passage::left_normal;
use sailgym_course::route::position;
use sailgym_course::{passage, Mark, Rounding, Route, Vec2};
use sailgym_env::episode::{manual_source, EpisodeConfig};
use sailgym_env::{Episode, Outcome, TerminationReason};
use sailgym_physics::recording::PracticeEvent;
use sailgym_physics::scenario::load_shipped;
use sailgym_task::{TaskId, TaskSpec};

/// Half the width of every gate in this file, metres.
///
/// Course geometry, not a physical coefficient (v2 F14.9): it is a little
/// under half a hull length, chosen small so that a gate crossed eight metres
/// off its centre is unambiguously outside the posts.
const HALF_WIDTH: f64 = 2.0;

/// A `free_sail` episode with nothing attached, driven by `manual`.
fn config() -> EpisodeConfig {
    let mut cfg = EpisodeConfig::new(load_shipped("free_sail").expect("a shipped scenario"));
    cfg.max_steps = Some(12_000);
    cfg
}

/// A hands-off episode, sheet hauled, with the action pushed once.
fn hands_off(cfg: EpisodeConfig, seed: u64) -> Episode {
    let mut ep = Episode::new(cfg, manual_source(Cadence::EVERY_STEP), seed).expect("valid");
    // Rudder neutral, sheet hauled, release off (`rate`'s third scalar is the
    // release flag's sign). The boat sails itself; nothing steers it.
    ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
    ep
}

/// Where the hands-off `free_sail` boat actually goes, one position per step.
///
/// Measured before any route is built, which is what makes every waypoint
/// below a point the boat really sails through rather than one it is hoped to.
fn track(steps: u32) -> Vec<Vec2> {
    let mut ep = hands_off(config(), 5);
    let mut out = Vec::with_capacity(steps as usize);
    for _ in 0..steps {
        ep.advance(1).expect("a valid action");
        out.push(position(ep.state()));
    }
    out
}

/// Run to a terminal outcome, or give up after `limit` steps.
fn run_to_end(ep: &mut Episode, limit: u32) {
    let mut done = 0u32;
    while done < limit && !ep.outcome().is_terminal() {
        done += ep.advance(10).expect("a valid action");
    }
}

// ---------------------------------------------------------------------------
// 1. An open route is sailed, and finished
// ---------------------------------------------------------------------------

#[test]
fn a_clean_pass_through_every_gate_of_an_open_route_finishes() {
    let t = track(4200);
    let start = Vec2::ZERO;
    let points = [t[999], t[1999], t[2999], t[3999]];
    let route = Route::waypoints(start, &points, HALF_WIDTH).expect("a valid open course");
    assert_eq!(route.legs(), 4);
    assert_eq!(route.start, Some(start));

    let mut cfg = config();
    cfg.route = Some(route.clone());
    let mut ep = hands_off(cfg, 5);
    run_to_end(&mut ep, 12_000);

    match ep.outcome() {
        Outcome::Finished { time } => {
            // The last gate is 20 s into the run, so a finish at t ≈ 0 would
            // mean the tracker had skipped the course rather than sailed it.
            assert!(time > 15.0, "an open course finished at t = {time}");
        }
        other => panic!("a clean pass through four gates did not finish: {other:?}"),
    }
    let progress = ep.progress().expect("a route");
    assert!(progress.finished(&route));
    // Leg 0 ran from the start, so the boat passed four marks and not three:
    // `legs()` is the mark count on a one-lap route and the tracker walked
    // every one of them.
    assert_eq!(progress.leg_index, route.legs());
}

// ---------------------------------------------------------------------------
// 2. A cut on leg 0 is a cut
// ---------------------------------------------------------------------------

#[test]
fn a_cut_on_leg_0_of_an_open_route_terminates_as_mark_missed() {
    let t = track(12_000);
    let start = Vec2::ZERO;
    // One waypoint, shifted six metres sideways from a point the boat sails
    // through. The gate is square to the **shifted** leg and centred on the
    // shifted waypoint, so the boat crosses its line far outside the posts:
    // three of F15.3's four clauses hold and the lateral one does not, which
    // is exactly a cut.
    let on_track = t[3999];
    let aside = on_track + left_normal((on_track - start).normalize()) * 6.0;
    let route = Route::waypoints(start, &[aside], HALF_WIDTH).expect("a valid open course");
    // The fixture proves itself before the episode runs: the geometry has to
    // contain a cut, or asserting that the episode finds one proves nothing.
    let cut_at = (1..t.len())
        .find(|&i| passage::cut_between(&route, 0, t[i - 1], t[i]))
        .expect("the shifted gate must be cut on this track");
    assert!(
        !(1..t.len()).any(|i| passage::passed_between(&route, 0, t[i - 1], t[i])),
        "the shifted gate is passed as well as cut, so the fixture is ambiguous"
    );

    let mut cfg = config();
    cfg.route = Some(route);
    let mut ep = hands_off(cfg, 5);
    run_to_end(&mut ep, 12_000);
    assert_eq!(
        ep.outcome(),
        Outcome::Terminated(TerminationReason::MarkMissed),
        "crossing a gate's line outside its posts is a cut (D2)"
    );
    // On the step the geometry says, not merely eventually.
    assert_eq!(ep.steps(), cut_at as u64 + 1);

    // …and the same waypoint back on the track is not a cut, so the detector
    // is reading the lateral clause and not merely the plane.
    let route = Route::waypoints(start, &[on_track], HALF_WIDTH).expect("valid");
    let mut cfg = config();
    cfg.route = Some(route);
    let mut ep = hands_off(cfg, 5);
    run_to_end(&mut ep, 12_000);
    assert!(
        matches!(ep.outcome(), Outcome::Finished { .. }),
        "a gate crossed between its posts is a passage: {:?}",
        ep.outcome()
    );
}

// ---------------------------------------------------------------------------
// 3. The named regression (RV66)
// ---------------------------------------------------------------------------

/// Section 06's probe, verbatim: the same marks and laps, every rounding
/// replaced by `Either` — and the start **dropped**, because `Route::new`
/// has no way to carry one.
fn probe_that_drops_the_start(route: &Route) -> Route {
    Route::new(
        route
            .marks
            .iter()
            .map(|m| Mark {
                rounding: Rounding::Either,
                ..*m
            })
            .collect(),
        route.laps,
    )
}

#[test]
fn a_probe_that_drops_the_start_disagrees_about_leg_0() {
    let t = track(2600);
    let start = Vec2::ZERO;
    let on_track = t[1999];
    let route = Route::waypoints(start, &[on_track], HALF_WIDTH).expect("valid");
    let dropped = probe_that_drops_the_start(&route);

    // **The false positive.** With one mark and no start the probe has no
    // incoming leg at all, so `Route::leg(0).from` is `None` and `Either`
    // degrades to arrival at the mark's own disc — a radius check, which is
    // what F15.3 forbids and RV65 names. It fires while the boat is still
    // short of the gate line, on a step at which the real route has passed
    // nothing and cut nothing.
    let mut false_positive = None;
    for i in 1..t.len() {
        let (prev, cur) = (t[i - 1], t[i]);
        let probe_says = passage::passed_between(&dropped, 0, prev, cur);
        let really_passed = passage::passed_between(&route, 0, prev, cur);
        let really_cut = passage::cut_between(&route, 0, prev, cur);
        if probe_says && !really_passed && !really_cut {
            false_positive = Some(i);
            break;
        }
    }
    let at = false_positive.expect(
        "the dropped-start probe must disagree with the real route somewhere, or this \
         regression proves nothing",
    );
    // It fires strictly before the gate is crossed, which is what made it a
    // false `MarkMissed` on a clean approach.
    let crossed_at = (1..t.len())
        .find(|&i| passage::passed_between(&route, 0, t[i - 1], t[i]))
        .expect("the boat sails through the gate");
    assert!(
        at < crossed_at,
        "the probe fired at step {at} and the gate was crossed at {crossed_at}"
    );

    // **And the episode it would have killed finishes.** This is the half
    // that fails if the copy is ever reintroduced.
    let mut cfg = config();
    cfg.route = Some(route.clone());
    let mut ep = hands_off(cfg, 5);
    run_to_end(&mut ep, 12_000);
    assert!(
        matches!(ep.outcome(), Outcome::Finished { .. }),
        "a boat sailing straight at its only waypoint was terminated: {:?}",
        ep.outcome()
    );

    // **The false negative, on the shipped `reach` course.** Its leg 0 runs
    // east from the start; the dropped-start probe's leg 0 runs **west**, from
    // the last waypoint to the first, so it reports the opposite crossing —
    // and a genuine eastward cut of waypoint 1, outside the posts, goes
    // unreported. One rule, in one place, is what fixes both halves.
    let reach = sailgym_course::CourseId::Reach
        .load()
        .expect("the shipped reach course")
        .route()
        .expect("a valid route");
    let dropped = probe_that_drops_the_start(&reach);
    let w = reach.marks[0].position;
    let outside = w.y + reach.marks[0].radius + 10.0;
    let (prev, cur) = (Vec2::new(w.x - 1.0, outside), Vec2::new(w.x + 1.0, outside));
    assert!(
        passage::cut_between(&reach, 0, prev, cur),
        "crossing waypoint 1's line 12 m to port of it is a cut"
    );
    assert!(
        !passage::passed_between(&dropped, 0, prev, cur),
        "the dropped-start probe reported the eastward crossing after all; this half of \
         the regression needs a new fixture"
    );
}

// ---------------------------------------------------------------------------
// 4. Recorded practice
// ---------------------------------------------------------------------------

/// Field for field, including the step a threshold was crossed on (F18.4).
fn same_event(a: &PracticeEvent, b: &PracticeEvent) -> bool {
    a.id == b.id
        && a.step == b.step
        && a.t.to_bits() == b.t.to_bits()
        && a.value.to_bits() == b.value.to_bits()
}

#[test]
fn the_recorded_practice_events_are_the_episodes_own() {
    // `docs/v2/practice-validation.md` §3.4's own successful script: full helm
    // to port and a full haul together for 8 s, then hands off. It produces
    // four events, so the comparison below is about a list and not about one
    // entry.
    let mut cfg = EpisodeConfig::new(load_shipped("tack").expect("a shipped scenario"));
    cfg.task = Some(TaskSpec::shipped(TaskId::CompleteTack));
    cfg.log_hz = Some(20.0);
    cfg.max_steps = Some(9200);
    let mut ep =
        Episode::new(cfg, manual_source(Cadence::EVERY_STEP), 20_250_903).expect("a valid episode");
    ep.push_action(&[-1.0, -1.0, -1.0]).expect("in bounds");
    while ep.steps() < 1600 && !ep.outcome().is_terminal() {
        ep.advance(10).expect("a valid action");
    }
    ep.push_action(&[0.0, 0.0, -1.0]).expect("in bounds");
    run_to_end(&mut ep, 9200);

    let events: Vec<PracticeEvent> = ep.task_events().to_vec();
    assert!(
        events.len() >= 3,
        "the attempt produced {} events, so agreeing about them proves little",
        events.len()
    );
    let envelope = ep
        .take_envelope()
        .expect("log_hz was set, so there is a recording");
    let practice = envelope
        .recording
        .header
        .practice
        .as_ref()
        .expect("a task was attached, so the recording carries its envelope");
    assert_eq!(
        practice.task,
        TaskSpec::shipped(TaskId::CompleteTack).identity(),
        "the envelope must name the task the episode was flown against"
    );
    assert_eq!(practice.events.len(), events.len());
    for (recorded, own) in practice.events.iter().zip(events.iter()) {
        assert!(
            same_event(recorded, own),
            "recorded {recorded:?} against the episode's own {own:?}"
        );
    }
    // Ordered by step, as the envelope's contract requires.
    assert!(practice.events.windows(2).all(|w| w[0].step <= w[1].step));
}

#[test]
fn a_free_sail_records_no_practice_envelope() {
    let mut cfg = config();
    cfg.log_hz = Some(20.0);
    cfg.max_steps = Some(600);
    let mut ep = hands_off(cfg, 5);
    run_to_end(&mut ep, 600);
    let envelope = ep.take_envelope().expect("a recording");
    assert!(
        envelope.recording.header.practice.is_none(),
        "an episode with no task must carry no practice envelope"
    );
    assert!(ep.task_events().is_empty());
    assert!(
        !envelope.recording.frames.is_empty(),
        "the recording is empty, so its header proves nothing"
    );
}
