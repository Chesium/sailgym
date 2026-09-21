//! The practice evaluator, measured (v2 section 11, task 11.1).
//!
//! Three kinds of test live here, and they answer different questions.
//!
//! 1. **Table-driven traces** ([`traces`]) drive the evaluator with a
//!    synthetic observation sequence — a state history written by hand, with
//!    no physics in it at all. That is the point: each trace isolates **one**
//!    rule, so a failing assertion names the rule rather than the boat. Every
//!    success and every failure reason the section PRD lists has one.
//!
//! 2. **Scripted baseline runs** ([`baseline`]) drive the real
//!    `Simulation`, from the shipped scenario the challenge is set on, at
//!    section 08's corrected baseline, with a control script a person could
//!    reproduce. They are what the thresholds were chosen from, they are the
//!    evidence `docs/v2/practice-validation.md` records, and they run in the
//!    gate so a physics change that moves them shows up as a red test rather
//!    than as a challenge that quietly became impossible.
//!
//! 3. **Contract tests** ([`contract`]) for the two properties the section's
//!    acceptance names directly: that batching cannot change a score (RV62),
//!    and that `sailgym-physics` does not depend on this crate (v2 F14.1).
//!
//! Run the baselines with their numbers on screen:
//!
//! ```text
//! cargo test -p sailgym-task --test practice baseline -- --nocapture
//! ```

use sailgym_physics::scenario::load_shipped;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls};
use sailgym_physics::vec::Vec2;
use sailgym_task::{
    true_wind_angle, FailureReason, Outcome, StepObservation, TaskId, TaskRun, TaskSpec,
};

/// The physics timestep the whole repository runs at (F7, `sim.dt`). Read from
/// the catalogue rather than written here, so a change to it moves these
/// traces with it.
fn dt() -> f64 {
    sailgym_physics::parameters::BoatParameters::ilca7().sim.dt
}

/// A northerly: the air moves toward −y (F6.1 — the vector is the direction
/// the wind blows *toward*).
const NORTHERLY: Vec2 = Vec2 { x: 0.0, y: -5.0 };

/// The heading that puts the true wind at `twa` off the bow, for [`NORTHERLY`].
///
/// With `w = (0, −W)`, `frames::world_to_body` gives `(−W sin ψ, −W cos ψ)`
/// and so `TWA = atan2(−cos ψ, sin ψ) = ψ − π/2`. Inverting it is how a trace
/// asks for an angle instead of a heading.
fn heading_for(twa: f64) -> f64 {
    twa + std::f64::consts::FRAC_PI_2
}

/// One synthetic step: the state, the controls in force, and the capsize flag.
struct Step {
    state: BoatState,
    controls: Controls,
    capsized: bool,
}

impl Step {
    fn new() -> Self {
        Self {
            state: BoatState::ZERO,
            controls: Controls::default(),
            capsized: false,
        }
    }
    fn u(mut self, u: f64) -> Self {
        self.state.u = u;
        self
    }
    fn twa(mut self, twa_deg: f64) -> Self {
        self.state.psi = heading_for(twa_deg.to_radians());
        self
    }
    fn heel(mut self, deg: f64) -> Self {
        self.state.phi = deg.to_radians();
        self
    }
    fn release(mut self) -> Self {
        self.controls.sheet_release = true;
        self
    }
    fn ease(mut self, cmd: f64) -> Self {
        self.controls.sheet_rate_cmd = cmd;
        self
    }
    fn capsized(mut self) -> Self {
        self.capsized = true;
        self
    }
}

/// Drive a task with a synthetic trace of `seconds` and return the run.
///
/// `f(t)` describes the boat at simulated time `t`. Nothing here integrates
/// anything: the trace **is** the history.
fn drive(spec: TaskSpec, seconds: f64, f: impl Fn(f64) -> Step) -> TaskRun {
    let dt = dt();
    let n = (seconds / dt).round() as u64;
    let first = f(0.0);
    let initial = StepObservation {
        step: 0,
        state: BoatState {
            t: 0.0,
            ..first.state
        },
        controls: first.controls,
        wind_world: NORTHERLY,
        capsized: first.capsized,
    };
    let mut run = TaskRun::start(spec, &initial).expect("the shipped configuration validates");
    for i in 1..=n {
        let t = i as f64 * dt;
        let s = f(t);
        run.observe(&StepObservation {
            step: i,
            state: BoatState { t, ..s.state },
            controls: s.controls,
            wind_world: NORTHERLY,
            capsized: s.capsized,
        });
    }
    run
}

/// The event ids a run produced, in order.
fn ids(run: &TaskRun) -> Vec<&str> {
    run.events().iter().map(|e| e.id.as_str()).collect()
}

/// Assert that `needles` appear in `ids`, in this order, without requiring
/// that nothing else appears between them.
fn assert_ordered(label: &str, run: &TaskRun, needles: &[&str]) {
    let got = ids(run);
    let mut at = 0usize;
    for needle in needles {
        match got[at..].iter().position(|g| g == needle) {
            Some(i) => at += i + 1,
            None => panic!("{label}: expected `{needle}` after index {at}, got {got:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Table-driven traces
// ---------------------------------------------------------------------------

mod traces {
    use super::*;

    /// One row of the table: a name, a task, a trace and what must come out.
    struct Case {
        name: &'static str,
        id: TaskId,
        seconds: f64,
        trace: fn(f64) -> Step,
        outcome: Outcome,
        /// Event ids that must appear, in this order.
        expect: &'static [&'static str],
        /// Event ids that must **not** appear at all.
        forbid: &'static [&'static str],
    }

    // --- get moving --------------------------------------------------------

    /// Accelerates past 1.2 m/s and stays there.
    fn moving_success(t: f64) -> Step {
        Step::new().u((t * 0.8).min(1.5))
    }

    /// Reaches the target, drops back under the release speed before the hold
    /// is up, and does it again and again. The target is *touched*, never
    /// *held*.
    fn moving_insufficient_hold(t: f64) -> Step {
        let phase = t % 4.0;
        Step::new().u(if phase < 1.5 { 1.3 } else { 0.6 })
    }

    /// Sternway, and enough of it for long enough.
    fn moving_backward(t: f64) -> Step {
        Step::new().u(if t < 1.0 { 0.1 } else { -0.5 })
    }

    /// Never gets near the target, and nothing else happens either.
    fn moving_timeout(_t: f64) -> Step {
        Step::new().u(0.3)
    }

    /// Fast, and then over. The capsize report ends every challenge.
    fn moving_capsize(t: f64) -> Step {
        let s = Step::new().u(1.5).heel(if t < 2.0 { 20.0 } else { 95.0 });
        if t >= 2.5 {
            s.capsized()
        } else {
            s
        }
    }

    // --- complete a tack ---------------------------------------------------

    /// Port tack at −45°, through head to wind, settled at +40° with way on.
    fn tack_success(t: f64) -> Step {
        // −45° → +40° over eight seconds, then held.
        let twa = (-45.0 + t * 10.625).min(40.0);
        Step::new().twa(twa).u(0.6)
    }

    /// Bears away instead of luffing: −45° → −130°, past the downwind
    /// threshold. That is a gybe in the making, not a tack.
    fn tack_wrong_way(t: f64) -> Step {
        Step::new().twa((-45.0 - t * 12.0).max(-130.0)).u(0.6)
    }

    /// Wallows across the approach boundary again and again and never crosses.
    fn tack_jitter(t: f64) -> Step {
        let phase = t % 4.0;
        Step::new()
            .twa(if phase < 2.0 { -20.0 } else { -40.0 })
            .u(0.6)
    }

    /// Sails on, steadily, on the tack it started on.
    fn tack_timeout(_t: f64) -> Step {
        Step::new().twa(-45.0).u(1.0)
    }

    /// Crosses, but never gets going again on the new tack: the angle is
    /// there, the forward speed is not.
    fn tack_no_speed_recovery(t: f64) -> Step {
        let twa = (-45.0 + t * 10.625).min(40.0);
        Step::new().twa(twa).u(if t < 4.0 { 0.6 } else { -0.3 })
    }

    // --- recover from excessive heel ---------------------------------------

    /// Heels to 60°, the sheet is released, the boat comes back up and stays.
    fn heel_success(t: f64) -> Step {
        let heel = if t < 3.0 {
            t * 20.0
        } else {
            (60.0 - (t - 3.0) * 30.0).max(5.0)
        };
        let s = Step::new().heel(heel);
        if t >= 3.0 {
            s.release()
        } else {
            s
        }
    }

    /// The sheet is never touched and the heel runs past the late-release
    /// threshold.
    fn heel_late_release(t: f64) -> Step {
        Step::new().heel(t * 12.0)
    }

    /// Released — but too late to matter, and the boat goes over anyway.
    fn heel_capsize(t: f64) -> Step {
        let s = Step::new().heel((t * 12.0).min(120.0));
        let s = if t >= 5.0 { s.release() } else { s };
        if t >= 8.0 {
            s.capsized()
        } else {
            s
        }
    }

    /// Comes back under the threshold, but never stays there.
    fn heel_insufficient_hold(t: f64) -> Step {
        let phase = t % 4.0;
        Step::new()
            .heel(if t < 2.0 {
                t * 25.0
            } else if phase < 1.0 {
                15.0
            } else {
                45.0
            })
            .ease(0.5)
    }

    const TABLE: &[Case] = &[
        Case {
            name: "get moving: reaches the target and sustains it",
            id: TaskId::GetMoving,
            seconds: 20.0,
            trace: moving_success,
            outcome: Outcome::Succeeded,
            expect: &["speed_reached", "succeeded"],
            forbid: &["hold_broken", "timed_out"],
        },
        Case {
            name: "get moving: insufficient hold duration",
            id: TaskId::GetMoving,
            seconds: 45.0,
            trace: moving_insufficient_hold,
            outcome: Outcome::TimedOut,
            expect: &["speed_reached", "hold_broken", "speed_reached", "timed_out"],
            forbid: &["succeeded"],
        },
        Case {
            name: "get moving: backward drift",
            id: TaskId::GetMoving,
            seconds: 20.0,
            trace: moving_backward,
            outcome: Outcome::Failed {
                reason: FailureReason::BackwardDrift,
            },
            expect: &["failed_backward_drift"],
            forbid: &["speed_reached", "succeeded", "timed_out"],
        },
        Case {
            name: "get moving: time expiry with nothing else observed",
            id: TaskId::GetMoving,
            seconds: 50.0,
            trace: moving_timeout,
            outcome: Outcome::TimedOut,
            expect: &["timed_out"],
            forbid: &["speed_reached", "hold_broken", "succeeded"],
        },
        Case {
            name: "get moving: capsize ends the attempt",
            id: TaskId::GetMoving,
            seconds: 20.0,
            trace: moving_capsize,
            outcome: Outcome::Failed {
                reason: FailureReason::Capsized,
            },
            expect: &["speed_reached", "failed_capsized"],
            forbid: &["succeeded", "timed_out"],
        },
        Case {
            name: "tack: approach, crossing, settled, in that order",
            id: TaskId::CompleteTack,
            seconds: 20.0,
            trace: tack_success,
            outcome: Outcome::Succeeded,
            expect: &["approach", "crossing", "settled", "succeeded"],
            forbid: &["reversal", "bore_away", "timed_out"],
        },
        Case {
            name: "tack: wrong-way gybe",
            id: TaskId::CompleteTack,
            seconds: 20.0,
            trace: tack_wrong_way,
            outcome: Outcome::Failed {
                reason: FailureReason::WrongWay,
            },
            expect: &["bore_away", "failed_wrong_way"],
            forbid: &["crossing", "settled", "succeeded"],
        },
        Case {
            name: "tack: repeated boundary jitter",
            id: TaskId::CompleteTack,
            seconds: 45.0,
            trace: tack_jitter,
            outcome: Outcome::Failed {
                reason: FailureReason::RepeatedJitter,
            },
            expect: &[
                "approach",
                "reversal",
                "approach",
                "reversal",
                "approach",
                "reversal",
                "failed_repeated_jitter",
            ],
            forbid: &["crossing", "succeeded"],
        },
        Case {
            name: "tack: time expiry on the tack it started on",
            id: TaskId::CompleteTack,
            seconds: 50.0,
            trace: tack_timeout,
            outcome: Outcome::TimedOut,
            expect: &["timed_out"],
            forbid: &["approach", "crossing", "succeeded"],
        },
        Case {
            name: "tack: crossed, but no forward-speed recovery",
            id: TaskId::CompleteTack,
            seconds: 50.0,
            trace: tack_no_speed_recovery,
            outcome: Outcome::TimedOut,
            expect: &["approach", "crossing", "timed_out"],
            forbid: &["settled", "succeeded"],
        },
        Case {
            name: "heel: released, and the boat comes back up",
            id: TaskId::RecoverFromHeel,
            seconds: 15.0,
            trace: heel_success,
            outcome: Outcome::Succeeded,
            expect: &[
                "heel_qualified",
                "heel_max",
                "release",
                "heel_recovered",
                "succeeded",
            ],
            forbid: &["failed_late_release", "failed_capsized", "timed_out"],
        },
        Case {
            name: "heel: late release",
            id: TaskId::RecoverFromHeel,
            seconds: 20.0,
            trace: heel_late_release,
            outcome: Outcome::Failed {
                reason: FailureReason::LateRelease,
            },
            expect: &["heel_qualified", "heel_max", "failed_late_release"],
            forbid: &["release", "succeeded", "failed_capsized"],
        },
        Case {
            name: "heel: released and capsized anyway",
            id: TaskId::RecoverFromHeel,
            seconds: 20.0,
            trace: heel_capsize,
            outcome: Outcome::Failed {
                reason: FailureReason::Capsized,
            },
            expect: &["heel_qualified", "release", "failed_capsized"],
            forbid: &["failed_late_release", "succeeded"],
        },
        Case {
            name: "heel: insufficient hold under the recovery threshold",
            id: TaskId::RecoverFromHeel,
            seconds: 30.0,
            trace: heel_insufficient_hold,
            outcome: Outcome::TimedOut,
            // The ease is on from the first step, so it is observed before
            // the heel ever qualifies — which is the right order to record.
            expect: &["release", "heel_qualified", "timed_out"],
            forbid: &["succeeded", "failed_capsized"],
        },
    ];

    #[test]
    fn every_success_and_every_isolated_failure() {
        for case in TABLE {
            let spec = TaskSpec::shipped(case.id);
            let run = drive(spec, case.seconds, case.trace);
            assert_eq!(
                run.outcome(),
                case.outcome,
                "{}: events {:?}",
                case.name,
                ids(&run)
            );
            assert_ordered(case.name, &run, case.expect);
            for forbidden in case.forbid {
                assert!(
                    !ids(&run).contains(forbidden),
                    "{}: `{forbidden}` must not appear; got {:?}",
                    case.name,
                    ids(&run)
                );
            }
            // Events are ordered by step, which is what
            // `docs/v2/recording-format.md` §6 promises a reader.
            let steps: Vec<u64> = run.events().iter().map(|e| e.step).collect();
            let mut sorted = steps.clone();
            sorted.sort_unstable();
            assert_eq!(steps, sorted, "{}: events out of step order", case.name);
            // And every event's time is that step's time, not an interpolated
            // instant (v2 F18.4, section 10 handoff §9 item 3).
            for e in run.events() {
                let expected = e.step as f64 * dt();
                assert!(
                    (e.t - expected).abs() < 1e-9,
                    "{}: event {} at step {} says t = {}, step time is {expected}",
                    case.name,
                    e.id,
                    e.step,
                    e.t
                );
                assert!(e.value.is_finite(), "{}: {} value", case.name, e.id);
            }
            println!(
                "trace {:<58} {:>10} after {:6.2} s in {} events",
                case.name,
                run.outcome().as_str(),
                run.elapsed_s(),
                run.events().len()
            );
        }
    }

    /// The **last** `heel_max` is the peak, so **Inspect** lands on the worst
    /// moment rather than the first bad one.
    #[test]
    fn the_highlight_event_is_the_one_inspect_should_jump_to() {
        let heel = drive(
            TaskSpec::shipped(TaskId::RecoverFromHeel),
            15.0,
            heel_success,
        );
        let peak = heel.highlight().expect("a heel_max event");
        assert_eq!(peak.id, "heel_max");
        let reported = heel.report().metric.value;
        // The last milestone is within one milestone step of the true peak.
        assert!(
            (reported - peak.value).abs() <= 0.0873 + 1e-9,
            "peak {reported} vs last event {}",
            peak.value
        );
        for e in heel.events().iter().filter(|e| e.id == "heel_max") {
            assert!(e.value <= peak.value + 1e-12);
        }

        let tack = drive(TaskSpec::shipped(TaskId::CompleteTack), 20.0, tack_success);
        assert_eq!(tack.highlight().expect("a crossing").id, "crossing");

        let moving = drive(TaskSpec::shipped(TaskId::GetMoving), 20.0, moving_success);
        assert_eq!(
            moving.highlight().expect("a speed_reached").id,
            "speed_reached"
        );
    }

    /// The tack detector reads the **wind angle**, not the heading's sign.
    ///
    /// The same manoeuvre, rotated so that the boat never changes the sign of
    /// `psi`, must produce the same events at the same steps. A detector keyed
    /// on a heading sign flip would see nothing at all here.
    #[test]
    fn the_tack_detector_is_not_a_heading_sign_flip() {
        let spec = TaskSpec::shipped(TaskId::CompleteTack);
        let a = drive(spec, 20.0, tack_success);
        // The same TWA history, with every heading pushed a quarter turn so
        // `psi` stays positive throughout. `wind_world` is fixed, so this is
        // done by rotating the *wind* instead — the same relative geometry.
        let dt = dt();
        let n = (20.0 / dt).round() as u64;
        let wind = Vec2::new(5.0, 0.0); // an easterly: air moving toward +x
        let state_at = |t: f64| {
            let twa = (-45.0 + t * 10.625f64).min(40.0).to_radians();
            // TWA = atan2(h.y, −h.x) with h = R_z(−ψ)·w; for w = (W, 0) this
            // works out as ψ = TWA + π, which never leaves the positive half
            // once wrapped the other way. `wrap_pi` is the core's.
            BoatState {
                psi: sailgym_physics::frames::wrap_pi(twa + std::f64::consts::PI),
                u: 0.6,
                t,
                ..BoatState::ZERO
            }
        };
        let initial = StepObservation {
            step: 0,
            state: state_at(0.0),
            controls: Controls::default(),
            wind_world: wind,
            capsized: false,
        };
        // The rotated construction really does reproduce the angle.
        assert!(
            (true_wind_angle(&initial.state, wind) - (-45f64).to_radians()).abs() < 1e-9,
            "{}",
            true_wind_angle(&initial.state, wind).to_degrees()
        );
        let mut b = TaskRun::start(spec, &initial).expect("start");
        for i in 1..=n {
            let t = i as f64 * dt;
            b.observe(&StepObservation {
                step: i,
                state: state_at(t),
                controls: Controls::default(),
                wind_world: wind,
                capsized: false,
            });
        }
        assert_eq!(b.outcome(), Outcome::Succeeded);
        let steps_a: Vec<(String, u64)> =
            a.events().iter().map(|e| (e.id.clone(), e.step)).collect();
        let steps_b: Vec<(String, u64)> =
            b.events().iter().map(|e| (e.id.clone(), e.step)).collect();
        assert_eq!(steps_a, steps_b);
    }
}

// ---------------------------------------------------------------------------
// 2. Scripted baseline runs
// ---------------------------------------------------------------------------

mod baseline {
    use super::*;

    /// Run a shipped scenario under a control script with the challenge
    /// attached, and return the finished run.
    ///
    /// The control script is a pure function of simulated time, so the run is
    /// reproducible from the scenario's own seed and nothing else (F9).
    fn sail(id: TaskId, seconds: f64, script: impl Fn(f64) -> Controls) -> TaskRun {
        let spec = TaskSpec::shipped(id);
        let sc = load_shipped(id.scenario()).expect("a shipped scenario");
        let mut sim = Simulation::new(sc.to_parameters().expect("catalogue"), sc.seed);
        sim.load_scenario(&sc).expect("scenario");
        let mut run =
            TaskRun::start(spec, &StepObservation::of(&sim)).expect("the shipped configuration");
        let dt = sim.params().sim.dt;
        let n = (seconds / dt).round() as u64;
        for _ in 0..n {
            sim.set_controls(script(sim.state().t));
            sim.advance(1);
            if run.observe(&StepObservation::of(&sim)).is_terminal() {
                break;
            }
        }
        run
    }

    fn haul(until: f64) -> impl Fn(f64) -> Controls {
        move |t| Controls {
            rudder_rate_cmd: 0.0,
            sheet_rate_cmd: if t < until { -1.0 } else { 0.0 },
            sheet_release: false,
        }
    }

    fn report(label: &str, run: &TaskRun) {
        let r = run.report();
        println!(
            "baseline {:<44} {:>10} {:>6} at {:6.2} s / step {:<7} {} = {:.4} {}",
            label,
            r.outcome.as_str(),
            match r.outcome {
                Outcome::Failed { reason } => reason.as_str(),
                _ => "",
            },
            r.elapsed_s,
            r.elapsed_steps,
            r.metric.id,
            r.metric.value,
            r.metric.unit,
        );
        for e in run.events() {
            println!(
                "           event {:<24} step {:>6}  t {:7.3}  value {:+.4}",
                e.id, e.step, e.t, e.value
            );
        }
    }

    /// `free_sail`, trimmed for drive: a one-second haul takes the sheet from
    /// 4.5 m to 3.0 m and the boat past 1.2 m/s.
    #[test]
    fn get_moving_succeeds_when_the_sheet_is_trimmed() {
        let run = sail(TaskId::GetMoving, 45.0, haul(1.0));
        report("get_moving / free_sail / haul 1.0 s", &run);
        assert_eq!(run.outcome(), Outcome::Succeeded);
        assert!(run.elapsed_s() < 12.0, "{}", run.elapsed_s());
        assert!(run.report().metric.value > 1.2);
        assert_ordered("get_moving success", &run, &["speed_reached", "succeeded"]);
    }

    /// The same scenario, untouched. The boat drifts off and never gets near
    /// the target: this is the failure the challenge exists to make visible.
    #[test]
    fn get_moving_times_out_when_nothing_is_trimmed() {
        let run = sail(TaskId::GetMoving, 45.0, |_| Controls::default());
        report("get_moving / free_sail / no input", &run);
        assert_eq!(run.outcome(), Outcome::TimedOut);
        assert!(run.report().metric.value < 1.2, "{:?}", run.report().metric);
        assert!(ids(&run).iter().all(|e| *e != "speed_reached"));
    }

    /// Over-sheeted: hauled all the way to the stop, the boat lies down and
    /// bears away and never makes the target either.
    #[test]
    fn get_moving_times_out_when_the_sheet_is_two_blocked() {
        let run = sail(TaskId::GetMoving, 45.0, haul(4.0));
        report("get_moving / free_sail / haul to the stop", &run);
        assert_eq!(run.outcome(), Outcome::TimedOut);
        assert!(run.report().metric.value < 1.2, "{:?}", run.report().metric);
    }

    /// `tack`, helm hard to port with the sheet hauled in through the turn —
    /// the technique that keeps the boat driving across head to wind.
    #[test]
    fn complete_tack_succeeds_with_helm_and_sheet_together() {
        let run = sail(TaskId::CompleteTack, 45.0, |t| Controls {
            rudder_rate_cmd: if t < 8.0 { -1.0 } else { 0.0 },
            sheet_rate_cmd: if t < 8.0 { -1.0 } else { 0.0 },
            sheet_release: false,
        });
        report("complete_tack / tack / helm + haul 8 s", &run);
        assert_eq!(run.outcome(), Outcome::Succeeded);
        assert_ordered(
            "complete_tack success",
            &run,
            &["approach", "crossing", "settled", "succeeded"],
        );
        assert!(run.elapsed_s() < 25.0, "{}", run.elapsed_s());
    }

    /// The same helm, the sheet left where it was. The boat stops head to
    /// wind, makes sternway and never establishes the new tack.
    #[test]
    fn complete_tack_times_out_when_the_sheet_is_left_alone() {
        let run = sail(TaskId::CompleteTack, 45.0, |t| Controls {
            rudder_rate_cmd: if t < 8.0 { -1.0 } else { 0.0 },
            sheet_rate_cmd: 0.0,
            sheet_release: false,
        });
        report("complete_tack / tack / helm only", &run);
        assert_eq!(run.outcome(), Outcome::TimedOut);
        assert!(ids(&run).contains(&"crossing"));
        assert!(!ids(&run).contains(&"settled"));
    }

    /// Too little helm: the boat luffs, falls back onto the tack it started
    /// on and then bears away past the downwind threshold. A tack crosses the
    /// wind ahead; this one was going round astern.
    #[test]
    fn complete_tack_fails_wrong_way_when_the_turn_is_abandoned() {
        let run = sail(TaskId::CompleteTack, 45.0, |t| Controls {
            rudder_rate_cmd: if t < 3.0 { -1.0 } else { 0.0 },
            sheet_rate_cmd: if t < 3.0 { -1.0 } else { 0.0 },
            sheet_release: false,
        });
        report("complete_tack / tack / helm + haul 3 s", &run);
        assert_eq!(
            run.outcome(),
            Outcome::Failed {
                reason: FailureReason::WrongWay
            }
        );
        assert_ordered("complete_tack wrong way", &run, &["approach", "bore_away"]);
        assert!(!ids(&run).contains(&"crossing"));
    }

    /// `sheet_release_recovery`, released at four seconds: the boat peaks at
    /// about 66° and comes back up.
    #[test]
    fn recover_from_heel_succeeds_when_the_sheet_is_released_in_time() {
        let run = sail(TaskId::RecoverFromHeel, 30.0, |t| Controls {
            rudder_rate_cmd: 0.0,
            sheet_rate_cmd: 0.0,
            sheet_release: (4.0..7.0).contains(&t),
        });
        report("recover_from_heel / release at 4 s", &run);
        assert_eq!(run.outcome(), Outcome::Succeeded);
        let peak = run.report().metric.value.to_degrees();
        assert!((60.0..72.0).contains(&peak), "peak heel {peak}");
        assert_ordered(
            "recover_from_heel success",
            &run,
            &["heel_qualified", "release", "heel_recovered", "succeeded"],
        );
    }

    /// Released earlier, the peak is lower — which is the whole lesson, and
    /// the reason the metric is the peak rather than a pass mark.
    #[test]
    fn releasing_earlier_lowers_the_peak_heel() {
        let mut peaks = Vec::new();
        for at in [0.0f64, 2.0, 4.0, 6.0] {
            let run = sail(TaskId::RecoverFromHeel, 30.0, move |t| Controls {
                rudder_rate_cmd: 0.0,
                sheet_rate_cmd: 0.0,
                sheet_release: (at..at + 3.0).contains(&t),
            });
            assert_eq!(run.outcome(), Outcome::Succeeded, "release at {at}");
            peaks.push((at, run.report().metric.value.to_degrees()));
        }
        println!("baseline recover_from_heel peaks by release time: {peaks:?}");
        for w in peaks.windows(2) {
            assert!(
                w[0].1 < w[1].1,
                "releasing at {} peaked at {:.2}°, at {} it peaked at {:.2}°",
                w[0].0,
                w[0].1,
                w[1].0,
                w[1].1
            );
        }
    }

    /// The sheet is never touched: the heel passes the late-release threshold
    /// before the capsize is declared, and the attempt says so.
    #[test]
    fn recover_from_heel_fails_late_release_when_the_sheet_is_held() {
        let run = sail(TaskId::RecoverFromHeel, 30.0, |_| Controls::default());
        report("recover_from_heel / sheet held", &run);
        assert_eq!(
            run.outcome(),
            Outcome::Failed {
                reason: FailureReason::LateRelease
            }
        );
        assert!(run.elapsed_s() < 9.0, "{}", run.elapsed_s());
        assert!(!ids(&run).contains(&"release"));
    }

    /// Released, but at seven and a half seconds — past the point of no
    /// return. The ease is observed, so this is not a late release; the boat
    /// goes over, and `capsized` is what ends it (RV64).
    #[test]
    fn recover_from_heel_fails_on_capsize_when_the_release_is_too_late() {
        let run = sail(TaskId::RecoverFromHeel, 30.0, |t| Controls {
            rudder_rate_cmd: 0.0,
            sheet_rate_cmd: 0.0,
            sheet_release: t >= 7.5,
        });
        report("recover_from_heel / release at 7.5 s", &run);
        assert_eq!(
            run.outcome(),
            Outcome::Failed {
                reason: FailureReason::Capsized
            }
        );
        assert!(ids(&run).contains(&"release"));
    }
}

// ---------------------------------------------------------------------------
// 3. Contract tests
// ---------------------------------------------------------------------------

mod contract {
    use super::*;

    /// RV62: the score must not depend on how the caller chunks its steps.
    ///
    /// The same control script is run six ways — one step at a time, and in
    /// batches of 2, 3, 5, 10 and 37 — and the outcome, the elapsed step
    /// count and **every event's step** must be identical. 37 is deliberately
    /// coprime with everything else so that no batch boundary lines up with a
    /// threshold crossing in more than one arm.
    #[test]
    fn identical_control_sequences_agree_under_six_batch_sizes() {
        for id in [
            TaskId::GetMoving,
            TaskId::CompleteTack,
            TaskId::RecoverFromHeel,
        ] {
            let mut reference: Option<Vec<(String, u64, f64)>> = None;
            let mut reference_outcome = None;
            for batch in [1u32, 2, 3, 5, 10, 37] {
                let spec = TaskSpec::shipped(id);
                let sc = load_shipped(id.scenario()).expect("scenario");
                let mut sim = Simulation::new(sc.to_parameters().expect("catalogue"), sc.seed);
                sim.load_scenario(&sc).expect("scenario");
                let mut run =
                    TaskRun::start(spec, &StepObservation::of(&sim)).expect("configuration");
                let dt = sim.params().sim.dt;
                let total = (30.0 / dt).round() as u64;
                let mut done = 0u64;
                'outer: while done < total {
                    // One `advance(1)` per evaluated step, `batch` of them per
                    // trip round the outer loop: exactly the shape the WASM
                    // wrapper's `advance(n)` takes while a task is attached.
                    for _ in 0..batch {
                        if done >= total {
                            break;
                        }
                        sim.set_controls(Controls {
                            rudder_rate_cmd: if sim.state().t < 6.0 { -1.0 } else { 0.0 },
                            sheet_rate_cmd: if sim.state().t < 6.0 { -1.0 } else { 0.0 },
                            sheet_release: false,
                        });
                        sim.advance(1);
                        done += 1;
                        if run.observe(&StepObservation::of(&sim)).is_terminal() {
                            break 'outer;
                        }
                    }
                }
                let events: Vec<(String, u64, f64)> = run
                    .events()
                    .iter()
                    .map(|e| (e.id.clone(), e.step, e.value))
                    .collect();
                match &reference {
                    None => {
                        reference = Some(events);
                        reference_outcome = Some((run.outcome(), run.elapsed_steps()));
                    }
                    Some(r) => {
                        assert_eq!(&events, r, "{} at batch {batch}", id.as_str());
                        assert_eq!(
                            Some((run.outcome(), run.elapsed_steps())),
                            reference_outcome,
                            "{} at batch {batch}",
                            id.as_str()
                        );
                    }
                }
            }
            println!(
                "batching {:<20} identical across 1, 2, 3, 5, 10 and 37 steps per call",
                id.as_str()
            );
        }
    }

    /// The evaluator is an observer: attaching one changes no state scalar.
    ///
    /// Compared **bit for bit**, not to a tolerance — there is no arithmetic
    /// in the evaluator that could touch the trajectory, so any difference at
    /// all would be a defect rather than a rounding.
    #[test]
    fn task_evaluation_does_not_perturb_the_physics() {
        for id in [
            TaskId::GetMoving,
            TaskId::CompleteTack,
            TaskId::RecoverFromHeel,
        ] {
            let script = |t: f64| Controls {
                rudder_rate_cmd: if t < 3.0 { -1.0 } else { 0.0 },
                sheet_rate_cmd: if t < 2.0 { -1.0 } else { 0.0 },
                sheet_release: (8.0..9.0).contains(&t),
            };
            let build = || {
                let sc = load_shipped(id.scenario()).expect("scenario");
                let mut sim = Simulation::new(sc.to_parameters().expect("catalogue"), sc.seed);
                sim.load_scenario(&sc).expect("scenario");
                sim
            };
            let mut plain = build();
            let mut scored = build();
            let spec = TaskSpec::shipped(id);
            let mut run =
                TaskRun::start(spec, &StepObservation::of(&scored)).expect("configuration");
            let dt = plain.params().sim.dt;
            let n = (20.0 / dt).round() as u64;
            for _ in 0..n {
                let c = script(plain.state().t);
                plain.set_controls(c);
                plain.advance(1);
                let c = script(scored.state().t);
                scored.set_controls(c);
                scored.advance(1);
                run.observe(&StepObservation::of(&scored));
            }
            assert_eq!(
                plain.state().to_array(),
                scored.state().to_array(),
                "{}: a task changed the trajectory",
                id.as_str()
            );
            println!(
                "observer {:<20} 13/13 state scalars bit-identical over {n} steps ({})",
                id.as_str(),
                run.outcome().as_str()
            );
        }
    }

    /// v2 F14.1: `task → physics`, and never the reverse.
    ///
    /// Asserted with `cargo tree`, because the property that matters is what
    /// the *build graph* says, not what an import list looks like. A
    /// dependency the other way would end F8.1's "the physics core builds and
    /// tests on the host with plain `cargo test`".
    #[test]
    fn physics_does_not_depend_on_the_task_crate() {
        let out = std::process::Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["tree", "-p", "sailgym-physics", "--edges", "all"])
            .output();
        let Ok(out) = out else {
            eprintln!("skip: cargo is not runnable here");
            return;
        };
        assert!(
            out.status.success(),
            "cargo tree failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let tree = String::from_utf8_lossy(&out.stdout);
        assert!(
            tree.contains("sailgym-physics"),
            "cargo tree printed nothing useful:\n{tree}"
        );
        for forbidden in ["sailgym-task", "sailgym-wasm", "wasm-bindgen"] {
            assert!(
                !tree.contains(forbidden),
                "`{forbidden}` is in sailgym-physics's dependency tree:\n{tree}"
            );
        }
        println!("dependency tree of sailgym-physics carries no task or UI crate");
    }

    /// The report the browser is handed round-trips through JSON with every
    /// field the page reads present.
    #[test]
    fn the_report_serialises_with_everything_the_page_needs() {
        let spec = TaskSpec::shipped(TaskId::RecoverFromHeel);
        let run = drive(spec, 15.0, |t| {
            let heel = if t < 3.0 {
                t * 20.0
            } else {
                (60.0 - (t - 3.0) * 30.0).max(5.0)
            };
            let s = Step::new().heel(heel);
            if t >= 3.0 {
                s.release()
            } else {
                s
            }
        });
        let json = serde_json::to_string(&run.report()).expect("serialise");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
        for key in [
            "task",
            "outcome",
            "scenario",
            "elapsed_s",
            "elapsed_steps",
            "metric",
            "progress",
            "events",
            "highlight",
        ] {
            assert!(parsed.get(key).is_some(), "missing `{key}` in {json}");
        }
        assert_eq!(parsed["outcome"]["kind"], "succeeded");
        assert_eq!(parsed["scenario"], "sheet_release_recovery");
        assert_eq!(parsed["metric"]["unit"], "rad");
        assert_eq!(parsed["task"]["version"], sailgym_task::TASK_VERSION);
        assert_eq!(parsed["highlight"]["id"], "heel_max");
    }
}
