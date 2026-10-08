//! The rule sailor, closed loop (v2 section 12, task 12.3).
//!
//! The decision-level tests — the mirror, the privilege perturbation, the
//! missing column and one per layer — are in
//! `crates/sailgym-agent/tests/rule_sailor.rs`, which is where the controller
//! lives. What needs an `Episode` is here: **does it finish?** (RV68), and
//! does F9.7 still hold with it attached?

use std::collections::BTreeMap;

use sailgym_agent::pilot::{Mode, RuleSailor};
use sailgym_agent::spec::Agent;

use sailgym_course::CourseId;

use sailgym_env::episode::{EpisodeConfig, Source};
use sailgym_env::{Episode, Outcome};

use sailgym_physics::scenario::load_shipped;

/// The final state and the whole decision log of one run.
type Run = (Vec<f64>, Vec<(u64, Vec<f64>)>);

/// Steps of 0.005 s: 600 s, which is five times the slowest shipped course's
/// measured time and therefore a budget rather than a bound on the result.
const BUDGET: u64 = 120_000;

/// An episode on `course` under the conditions the browser uses: the course's
/// own shipped scenario, the tier-0 suite, and no bounds, no task and no
/// recording.
fn course_episode(course: CourseId, seed: u64, log_decisions: bool) -> Episode {
    let doc = course.load().expect("a shipped course");
    let route = doc.route().expect("a valid route");
    let sc = load_shipped(&doc.scenario).expect("a shipped scenario");
    let mut cfg = EpisodeConfig::new(sc);
    cfg.route = Some(route);
    cfg.max_steps = Some(BUDGET);
    cfg.log_decisions = log_decisions;
    Episode::new(cfg, Source::Policy(Box::new(RuleSailor::new())), seed).expect("a valid episode")
}

// ---------------------------------------------------------------------------
// 1. The closed loop (RV68)
// ---------------------------------------------------------------------------

/// The rule sailor finishes all three shipped courses under their browser
/// conditions, with no capsize and no miss.
///
/// RV68 is the risk that this boat cannot be tacked or run reliably at all —
/// `docs/v2/practice-validation.md` §3.1 swept 1 344 helm-only scripts and none
/// of them completed a tack. Two of the three courses need a beat, so this test
/// is the one that says the baseline is a baseline.
#[test]
fn the_rule_sailor_finishes_every_shipped_course() {
    // The measured times, for the record. They are **reported** here, not
    // asserted to the second: a bound tight enough to be a target would make
    // any later gain change a test rather than a number.
    let mut measured: BTreeMap<&str, f64> = BTreeMap::new();
    for course in CourseId::ALL {
        let mut ep = course_episode(course, 12, false);
        let mut peak_heel = 0.0f64;
        while !ep.outcome().is_terminal() {
            ep.advance(50).expect("a valid action");
            peak_heel = peak_heel.max(ep.state().phi.abs());
        }
        let name = course.as_str();
        match ep.outcome() {
            Outcome::Finished { time } => {
                measured.insert(name, time);
                eprintln!(
                    "{name}: finished in {time:.2} s, peak heel {:.1}°",
                    peak_heel.to_degrees()
                );
            }
            other => panic!(
                "{name}: the rule sailor did not finish ({other:?}) after {:.1} s at \
                 ({:.1}, {:.1}); RV68 has fired",
                ep.state().t,
                ep.state().x,
                ep.state().y
            ),
        }
        assert!(
            !ep.simulation().capsize().capsized,
            "{name}: the baseline capsized"
        );
        // A cut would have ended the episode as `Terminated(MarkMissed)`
        // before it could finish, so finishing **is** "no miss" — asserted
        // here so the claim is in the file rather than in an argument.
        assert_ne!(
            ep.outcome(),
            Outcome::Terminated(sailgym_env::TerminationReason::MarkMissed)
        );
        assert!(
            ep.progress()
                .expect("a route")
                .finished(ep.config().route.as_ref().expect("a route")),
            "{name}: the tracker says the route is not finished"
        );
    }
    assert_eq!(measured.len(), 3);
    // Every course took a plausible amount of time: long enough to have been
    // sailed, short enough to be inside the budget.
    for (name, time) in &measured {
        assert!(
            (10.0..500.0).contains(time),
            "{name} finished in {time} s, which is not a sailed course"
        );
    }
}

/// Every course is sailed the way the PRD says it is: `reach` as one long
/// reach, the other two with the beat and the gybe they are set to teach.
///
/// Counted in **decisions spent** in each mode rather than in mode entries. A
/// course's last decision or two are spent with the mark a metre away and the
/// bearing to it swinging through the wind, so `reach` does enter `tacking` —
/// for one decision, 0.05 s before it finishes. Counting entries would make
/// that transient indistinguishable from a tack, and suppressing it would mean
/// giving the navigator a distance to the target to reason about, which is a
/// column the PRD's layer table does not give it.
#[test]
fn each_course_is_sailed_the_way_it_is_meant_to_be() {
    let spent = |course: CourseId| -> (BTreeMap<&'static str, usize>, usize) {
        let mut ep = course_episode(course, 12, false);
        let mut seen: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut total = 0usize;
        while !ep.outcome().is_terminal() {
            // One decision period, so each sample is one decision.
            ep.advance(ep.agent_spec().cadence.period_steps)
                .expect("a valid action");
            let code = ep
                .agent_debug()
                .notes
                .iter()
                .find(|(k, _)| k == "mode")
                .map(|(_, v)| *v)
                .expect("the rule sailor reports its mode");
            let mode = Mode::from_code(code).expect("a known mode");
            *seen.entry(mode.as_str()).or_default() += 1;
            total += 1;
        }
        (seen, total)
    };
    let share = |seen: &BTreeMap<&'static str, usize>, total: usize, key: &str| -> f64 {
        seen.get(key).copied().unwrap_or(0) as f64 / total as f64
    };

    // `reach`: every leg is on port tack, so the boat sails it without a
    // manoeuvre worth the name — under 2 % of its decisions, and no irons.
    let (reach, n) = spent(CourseId::Reach);
    let manoeuvring = [
        "tacking",
        "settling",
        "gybing",
        "gybe_settling",
        "recovering",
    ]
    .iter()
    .map(|k| share(&reach, n, k))
    .sum::<f64>();
    assert!(
        manoeuvring < 0.02,
        "`reach` spent {:.1} % of its decisions manoeuvring: {reach:?}",
        manoeuvring * 100.0
    );
    assert_eq!(
        reach.get("recovering"),
        None,
        "`reach` must not go into irons: {reach:?}"
    );

    // `triangle`: a beat, then a gybe at waypoint 2.
    let (triangle, n) = spent(CourseId::Triangle);
    assert!(
        share(&triangle, n, "beating") > 0.2,
        "`triangle` starts with a beat: {triangle:?}"
    );
    assert!(
        share(&triangle, n, "tacking") > 0.0,
        "`triangle`'s beat needs a tack: {triangle:?}"
    );
    assert!(
        share(&triangle, n, "gybing") > 0.0,
        "`triangle` gybes at waypoint 2: {triangle:?}"
    );

    // `windward_leeward`: a beat and a dead run back.
    let (wl, n) = spent(CourseId::WindwardLeeward);
    assert!(
        share(&wl, n, "beating") > 0.2,
        "`windward_leeward` starts with a beat: {wl:?}"
    );
    assert!(
        share(&wl, n, "tacking") > 0.0,
        "`windward_leeward`'s beat needs a tack: {wl:?}"
    );
    assert!(
        share(&wl, n, "fetching") > 0.2,
        "…and a run back, steered at the mark: {wl:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. F9.7 with the rule sailor attached
// ---------------------------------------------------------------------------

/// `advance(n) == n × advance(1)` with the rule sailor attached, over six
/// chunkings, compared on the **decision log** and on the final state bit for
/// bit.
///
/// The same assertion section 05 and section 06 each make with a stub; what it
/// adds is a controller with real internal state, whose mode machine and
/// decision counters are a second thing that could depend on how the caller
/// chunked its calls (F14.6, RV62).
#[test]
fn f9_7_holds_with_the_rule_sailor_attached() {
    const CHUNKS: [u32; 6] = [1, 3, 10, 17, 100, 1000];
    const STEPS: u64 = 8_000;

    for course in CourseId::ALL {
        let mut reference: Option<Run> = None;
        for chunk in CHUNKS {
            let mut ep = course_episode(course, 12, true);
            let mut done = 0u64;
            while done < STEPS && !ep.outcome().is_terminal() {
                let want = u32::try_from((STEPS - done).min(u64::from(chunk))).expect("fits");
                let taken = ep.advance(want).expect("a valid action");
                if taken == 0 {
                    break;
                }
                done += u64::from(taken);
            }
            let state = ep.state().to_array().to_vec();
            let log: Vec<(u64, Vec<f64>)> = ep
                .decisions()
                .into_iter()
                .map(|d| (d.step, d.action))
                .collect();
            assert!(
                log.len() > 100,
                "{}: only {} decisions at chunk {chunk}",
                course.as_str(),
                log.len()
            );
            match &reference {
                None => reference = Some((state, log)),
                Some((want_state, want_log)) => {
                    assert_eq!(
                        want_log.len(),
                        log.len(),
                        "{}: chunk {chunk} took a different number of decisions",
                        course.as_str()
                    );
                    for (i, (a, b)) in want_log.iter().zip(log.iter()).enumerate() {
                        assert_eq!(a.0, b.0, "{}: chunk {chunk}, decision {i}", course.as_str());
                        for (k, (x, y)) in a.1.iter().zip(b.1.iter()).enumerate() {
                            assert_eq!(
                                x.to_bits(),
                                y.to_bits(),
                                "{}: chunk {chunk}, decision {i}, scalar {k}",
                                course.as_str()
                            );
                        }
                    }
                    for (i, (x, y)) in want_state.iter().zip(state.iter()).enumerate() {
                        assert_eq!(
                            x.to_bits(),
                            y.to_bits(),
                            "{}: chunk {chunk}, state scalar {i}: {x} vs {y}",
                            course.as_str()
                        );
                    }
                }
            }
        }
    }
}

/// Two episodes from one seed produce the same trajectory, and the controller
/// draws nothing from its RNG — so the baseline is a function of the conditions
/// alone and `run_baseline` can be called twice with the same answer (D5).
#[test]
fn the_baseline_is_a_function_of_the_conditions_alone() {
    for course in CourseId::ALL {
        let run = |seed: u64| -> Vec<f64> {
            let mut ep = course_episode(course, seed, false);
            let mut done = 0u64;
            while done < 6_000 && !ep.outcome().is_terminal() {
                done += u64::from(ep.advance(100).expect("a valid action"));
            }
            ep.state().to_array().to_vec()
        };
        let a = run(12);
        let b = run(12);
        assert_eq!(a, b, "{}: one seed, two trajectories", course.as_str());
        // …and a different seed changes nothing, because the shipped courses
        // all run in a **uniform** field and the controller is deterministic in
        // its observation. That is a property worth asserting rather than
        // assuming: if it ever stops holding, something has acquired a second
        // source of randomness (F9.2, F14.8).
        let c = run(987_654_321);
        for (i, (x, y)) in a.iter().zip(c.iter()).enumerate() {
            assert_eq!(
                x.to_bits(),
                y.to_bits(),
                "{}: scalar {i} moved with the seed in a uniform field",
                course.as_str()
            );
        }
    }
}

/// The agent's own declared contract, through the episode it ran in.
#[test]
fn the_episode_records_the_baselines_spec() {
    let ep = course_episode(CourseId::Reach, 12, true);
    let spec = ep.agent_spec();
    assert_eq!(spec.id, "rule_sailor");
    assert_eq!(spec.version, 1);
    assert_eq!(spec.action_space, sailgym_agent::spec::ActionSpace::Rates);
    assert_eq!(spec.cadence.period_steps, 10);
    assert_eq!(ep.action_dim(), 3);
    // `AgentDebug` is written for observers and read by no controller: the
    // episode exposes it and nothing in the crate consumes it.
    let notes: BTreeMap<String, f64> = ep.agent_debug().notes.into_iter().collect();
    for key in ["mode", "side", "target_delta_r", "target_l_sheet"] {
        assert!(notes.contains_key(key), "no debug note `{key}`");
    }
    assert!(
        !notes.contains_key("missing_fields"),
        "the tier-0 layout carries every column the baseline reads"
    );
    assert!(Mode::from_code(notes["mode"]).is_some());
    // And the controller itself reports no missing column for that layout.
    let mut fresh = RuleSailor::new();
    let mut rng = sailgym_physics::rng::Pcg32::seed_from_u64(1);
    fresh.reset(&ep.layout().names(), &mut rng);
    assert!(fresh.missing_fields().is_empty());
}
