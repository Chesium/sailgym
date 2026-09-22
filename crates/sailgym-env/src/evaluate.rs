//! The evaluation report (v2 section 06 task 6.7).
//!
//! # Why a reward is not a report
//!
//! A scalar return tells you a policy got more of something. It does not
//! tell you *what*: a boat that finishes slowly and a boat that cuts every
//! mark can score the same, and the second one is not sailing. So the
//! report is a set of named counts and times, and the one that matters most
//! — a cut mark — is detectable at all only because section 04's passage
//! rule is ordered, sided and directed rather than a radius check (F15.3).
//!
//! # Held out by the caller, named by the type
//!
//! [`EvalSuite`] carries the wind seeds and the courses an evaluation runs
//! on. Whether they were held out of training is the caller's business and
//! cannot be checked here; what this module does is make the two axes
//! **explicit and separate**, so a report that swept only seeds cannot be
//! read as one that swept courses too.
//!
//! # Realised, not commanded
//!
//! Control effort is the distance the actuators actually moved —
//! `Σ|Δδr|` in radians and `Σ|ΔL|` in metres — and not the integral of the
//! commands. A rate command that was clamped by F4.3, or a rudder that
//! self-centred while the command was zero, produces exactly the
//! difference between the two, and the realised figure is the one a sailor
//! would recognise. The two are reported **separately** because they have
//! different units: adding radians to metres to get one "effort" number is
//! a category error that no amount of weighting fixes.
//!
//! # This module drives the episodes it measures
//!
//! `advance(1)` in a loop, accumulating the actuator travel from the
//! published state between steps. That is deliberately not a field of
//! [`Episode`]: an evaluation metric belongs to the evaluator, and an
//! episode that accumulated one would pay for it in
//! [`crate::vec_env::VecEnv`]'s hot loop whether or not anybody wanted it.

use serde::Serialize;

use sailgym_course::Route;
use sailgym_task::{FailureReason, Outcome as TaskOutcome};

use crate::episode::{EnvError, Episode, EpisodeConfig, Source};
use crate::outcome::{Outcome, TerminationReason};

/// The seeds and courses an evaluation sweeps.
///
/// The report is the cross product: every seed against every course, in
/// index order (F9.3 — a `Vec`, never a set).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvalSuite {
    /// Wind seeds. Each one is a whole episode's `u64` (F17.3).
    pub seeds: Vec<u64>,
    /// Courses. `None` is a free sail, and an empty list means "one run
    /// per seed, on whatever the configuration already carries".
    pub routes: Vec<Option<Route>>,
}

impl EvalSuite {
    /// Seeds only, on the configuration's own course.
    pub fn seeds(seeds: &[u64]) -> Self {
        Self {
            seeds: seeds.to_vec(),
            routes: Vec::new(),
        }
    }

    /// How many episodes this suite runs.
    pub fn episodes(&self) -> usize {
        self.seeds.len() * self.routes.len().max(1)
    }
}

/// What one episode produced.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EpisodeSummary {
    /// The `u64` the episode was reset from.
    pub seed: u64,
    /// Which course of the suite it sailed, or `None` for the
    /// configuration's own.
    pub route_index: Option<usize>,
    pub outcome: Outcome,
    /// Physics steps executed.
    pub executed_steps: u64,
    /// s, simulated time at the last step.
    pub elapsed_s: f64,
    /// s, the time the route was completed at, if it was.
    pub finish_time_s: Option<f64>,
    /// Legs completed — marks passed, counting laps (RV24).
    pub legs_completed: u32,
    /// The attached practice task's verdict, if one was attached.
    pub task_outcome: Option<TaskOutcome>,
    /// rad, `Σ|Δδr|`: the distance the rudder actually moved.
    pub rudder_travel_rad: f64,
    /// m, `Σ|ΔL|`: the mainsheet actually paid out and hauled.
    pub sheet_travel_m: f64,
    /// Decisions the agent took.
    pub decisions: usize,
}

impl EpisodeSummary {
    /// Whether this episode's attached task recorded a failed manoeuvre.
    ///
    /// Section 11's [`Outcome::Failed`](sailgym_task::Outcome::Failed) and
    /// nothing else: this module defines no second notion of a failed
    /// manoeuvre, because `sailgym-task` already owns one and two would
    /// disagree.
    pub fn failed_manoeuvre(&self) -> Option<FailureReason> {
        match self.task_outcome {
            Some(TaskOutcome::Failed { reason }) => Some(reason),
            _ => None,
        }
    }
}

/// The report.
///
/// Every field is a count, a rate or a mean over the summaries it was built
/// from, and [`EvaluationReport::of`] is pure arithmetic over them — which
/// is what makes `tests::the_report_arithmetic_is_hand_checked` a check of
/// the report rather than of the simulator.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EvaluationReport {
    pub episodes: usize,
    /// Episodes that completed their route.
    pub finished: usize,
    /// `finished / episodes`, or `0.0` for an empty suite.
    pub finish_rate: f64,
    /// s, the mean completion time over the **finished** episodes only.
    /// `None` when none finished — not `0.0`, which would read as "instant".
    pub mean_finish_time_s: Option<f64>,
    pub capsizes: usize,
    pub out_of_bounds: usize,
    pub marks_missed: usize,
    /// Episodes the step budget ended. Kept apart from the terminations
    /// above, always (RV34).
    pub truncated: usize,
    /// Episodes whose attached task recorded a failure.
    pub failed_manoeuvres: usize,
    /// rad, mean realised rudder travel per episode.
    pub mean_rudder_travel_rad: f64,
    /// m, mean realised sheet travel per episode.
    pub mean_sheet_travel_m: f64,
    /// Every episode, in the order it was run.
    pub summaries: Vec<EpisodeSummary>,
}

impl EvaluationReport {
    /// The arithmetic, over summaries somebody else produced.
    pub fn of(summaries: Vec<EpisodeSummary>) -> Self {
        let episodes = summaries.len();
        let finished = summaries
            .iter()
            .filter(|s| s.outcome.finish_time().is_some())
            .count();
        let finish_times: Vec<f64> = summaries.iter().filter_map(|s| s.finish_time_s).collect();
        let reason = |want: TerminationReason| {
            summaries
                .iter()
                .filter(|s| s.outcome.reason() == Some(want))
                .count()
        };
        let capsizes = reason(TerminationReason::Capsized);
        let out_of_bounds = reason(TerminationReason::OutOfBounds);
        let marks_missed = reason(TerminationReason::MarkMissed);
        let truncated = summaries.iter().filter(|s| s.outcome.truncated()).count();
        let failed_manoeuvres = summaries
            .iter()
            .filter(|s| s.failed_manoeuvre().is_some())
            .count();
        let rudder: f64 = summaries.iter().map(|s| s.rudder_travel_rad).sum();
        let sheet: f64 = summaries.iter().map(|s| s.sheet_travel_m).sum();
        let per_episode = |total: f64| {
            if episodes == 0 {
                0.0
            } else {
                total / episodes as f64
            }
        };
        Self {
            episodes,
            finished,
            finish_rate: if episodes == 0 {
                0.0
            } else {
                finished as f64 / episodes as f64
            },
            mean_finish_time_s: if finish_times.is_empty() {
                None
            } else {
                Some(finish_times.iter().sum::<f64>() / finish_times.len() as f64)
            },
            capsizes,
            out_of_bounds,
            marks_missed,
            truncated,
            failed_manoeuvres,
            mean_rudder_travel_rad: per_episode(rudder),
            mean_sheet_travel_m: per_episode(sheet),
            summaries,
        }
    }

    /// One line, for a log.
    ///
    /// The finish rate is printed as a **fraction**, not a percentage: a
    /// percentage needs a literal `100.0`, and `no_f7_literal_appears_in_the_env_crate`
    /// refuses one — `a_y` is 100 kg in the F7 catalogue, and an audit that
    /// exempted the number so a log could read prettily would stop being an
    /// audit.
    pub fn describe(&self) -> String {
        format!(
            "{} episodes: {:.3} finished{}, {} capsized, {} out of bounds, {} marks missed, \
             {} truncated, {} failed manoeuvres; rudder {:.2} rad, sheet {:.2} m per episode",
            self.episodes,
            self.finish_rate,
            match self.mean_finish_time_s {
                Some(t) => format!(" in {t:.1} s"),
                None => String::new(),
            },
            self.capsizes,
            self.out_of_bounds,
            self.marks_missed,
            self.truncated,
            self.failed_manoeuvres,
            self.mean_rudder_travel_rad,
            self.mean_sheet_travel_m,
        )
    }
}

/// Run one episode to its end and summarise it.
///
/// `limit` is a hard cap on the steps this harness will take, separate from
/// the configuration's own `max_steps`: an evaluation that hangs is worse
/// than one that reports a truncation.
pub fn run_one(
    config: EpisodeConfig,
    source: Source,
    seed: u64,
    route_index: Option<usize>,
    limit: u64,
) -> Result<EpisodeSummary, EnvError> {
    let mut ep = Episode::new(config, source, seed)?;
    let mut rudder_travel = 0.0;
    let mut sheet_travel = 0.0;
    let mut previous = *ep.state();
    while !ep.outcome().is_terminal() && ep.steps() < limit {
        if ep.advance(1)? == 0 {
            break;
        }
        let now = *ep.state();
        rudder_travel += (now.delta_r - previous.delta_r).abs();
        sheet_travel += (now.l_sheet - previous.l_sheet).abs();
        previous = now;
    }
    Ok(EpisodeSummary {
        seed,
        route_index,
        outcome: ep.outcome(),
        executed_steps: ep.steps(),
        elapsed_s: ep.state().t,
        finish_time_s: ep.outcome().finish_time(),
        legs_completed: ep.progress().map_or(0, |p| p.leg_index),
        task_outcome: ep.task_outcome(),
        rudder_travel_rad: rudder_travel,
        sheet_travel_m: sheet_travel,
        decisions: ep.decision_count(),
    })
}

/// Run a whole suite: every seed against every course, in index order.
///
/// `source` is a factory because an [`Source`] is consumed by the episode
/// it drives — a controller carries state, and reusing one across episodes
/// would carry it too.
pub fn evaluate(
    config: &EpisodeConfig,
    suite: &EvalSuite,
    source: &mut dyn FnMut() -> Source,
    limit: u64,
) -> Result<EvaluationReport, EnvError> {
    let mut summaries = Vec::with_capacity(suite.episodes());
    if suite.routes.is_empty() {
        for seed in &suite.seeds {
            summaries.push(run_one(config.clone(), source(), *seed, None, limit)?);
        }
    } else {
        for (route_index, route) in suite.routes.iter().enumerate() {
            for seed in &suite.seeds {
                let mut cfg = config.clone();
                cfg.route = route.clone();
                summaries.push(run_one(cfg, source(), *seed, Some(route_index), limit)?);
            }
        }
    }
    Ok(EvaluationReport::of(summaries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::episode::manual_source;
    use crate::outcome::{AutoresetMode, Bounds};
    use sailgym_agent::spec::{Action, ActionSpace, ActionVec, Agent, AgentSpec, Cadence};
    use sailgym_course::route::position;
    use sailgym_physics::rng::Pcg32;
    use sailgym_physics::scenario::load_shipped;

    /// A policy that emits one fixed action, for ever.
    ///
    /// A **policy** and not a pre-loaded `manual` latch: `Manual::reset`
    /// deliberately returns the latch to hands-off (section 05 §3.1), and
    /// an episode resets its source at `reset`, so an action pushed before
    /// the episode is built is discarded. A scripted episode is a policy
    /// that emits a script.
    #[derive(Clone, Copy, Debug)]
    struct Hold([f64; 3]);

    impl Hold {
        fn source(action: [f64; 3]) -> Source {
            Source::Policy(Box::new(Self(action)))
        }
    }

    impl Agent for Hold {
        fn spec(&self) -> AgentSpec {
            AgentSpec::new("hold", 1, ActionSpace::Rates, Cadence::EVERY_STEP)
        }
        fn reset(&mut self, _fields: &[String], _rng: &mut Pcg32) {}
        fn decide(&mut self, _obs: &[f64], _rng: &mut Pcg32) -> Action {
            Action::Rates(ActionVec::new(&self.0).expect("in bounds"))
        }
    }

    fn config(scenario: &str) -> EpisodeConfig {
        let mut cfg = EpisodeConfig::new(load_shipped(scenario).expect("a shipped scenario"));
        cfg.autoreset = AutoresetMode::Disabled;
        cfg
    }

    fn summary(outcome: Outcome, finish: Option<f64>, rudder: f64, sheet: f64) -> EpisodeSummary {
        EpisodeSummary {
            seed: 1,
            route_index: None,
            outcome,
            executed_steps: 100,
            elapsed_s: 0.5,
            finish_time_s: finish,
            legs_completed: 0,
            task_outcome: None,
            rudder_travel_rad: rudder,
            sheet_travel_m: sheet,
            decisions: 10,
        }
    }

    /// Every metric, over hand-written summaries, against hand-computed
    /// values. The arithmetic is checked here so that the scripted-episode
    /// test below can check the *measurement*.
    #[test]
    fn the_report_arithmetic_is_hand_checked() {
        let r = EvaluationReport::of(vec![
            summary(Outcome::Finished { time: 20.0 }, Some(20.0), 1.0, 2.0),
            summary(Outcome::Finished { time: 30.0 }, Some(30.0), 3.0, 0.0),
            summary(
                Outcome::Terminated(TerminationReason::Capsized),
                None,
                0.0,
                6.0,
            ),
            summary(
                Outcome::Terminated(TerminationReason::MarkMissed),
                None,
                4.0,
                0.0,
            ),
            summary(Outcome::Truncated, None, 2.0, 2.0),
        ]);
        assert_eq!(r.episodes, 5);
        assert_eq!(r.finished, 2);
        assert_eq!(r.finish_rate, 0.4);
        assert_eq!(r.mean_finish_time_s, Some(25.0));
        assert_eq!(r.capsizes, 1);
        assert_eq!(r.marks_missed, 1);
        assert_eq!(r.out_of_bounds, 0);
        assert_eq!(r.truncated, 1);
        assert_eq!(r.failed_manoeuvres, 0);
        assert_eq!(r.mean_rudder_travel_rad, 2.0); // (1 + 3 + 0 + 4 + 2) / 5
        assert_eq!(r.mean_sheet_travel_m, 2.0); // (2 + 0 + 6 + 0 + 2) / 5

        // No finishes is `None`, never `0.0`: a mean of nothing is not
        // "instant".
        let none = EvaluationReport::of(vec![summary(Outcome::Truncated, None, 0.0, 0.0)]);
        assert_eq!(none.mean_finish_time_s, None);
        assert_eq!(none.finish_rate, 0.0);

        // An empty suite reports zeros and divides by nothing.
        let empty = EvaluationReport::of(Vec::new());
        assert_eq!(empty.episodes, 0);
        assert_eq!(empty.finish_rate, 0.0);
        assert_eq!(empty.mean_rudder_travel_rad, 0.0);
        assert!(empty.describe().contains("0 episodes"));

        // A failed manoeuvre is section 11's verdict and nothing else.
        let mut failed = summary(Outcome::Truncated, None, 0.0, 0.0);
        failed.task_outcome = Some(TaskOutcome::Failed {
            reason: FailureReason::WrongWay,
        });
        let report = EvaluationReport::of(vec![failed.clone()]);
        assert_eq!(report.failed_manoeuvres, 1);
        assert_eq!(failed.failed_manoeuvre(), Some(FailureReason::WrongWay));
        let mut ok = summary(Outcome::Truncated, None, 0.0, 0.0);
        ok.task_outcome = Some(TaskOutcome::Succeeded);
        assert_eq!(ok.failed_manoeuvre(), None);
    }

    /// The measurement, over a **scripted** episode whose answer is known
    /// from F7 rather than from a previous run of this code.
    ///
    /// Hold `rudder_rate_cmd = +1` and `sheet_rate_cmd = 0` for `N` steps,
    /// starting inside the rudder's range. F4.3 integrates `δ̇r` at exactly
    /// `delta_r_rate_max` while the command is held and the stop is not
    /// reached, so the realised rudder travel is `N · dt · rate` and the
    /// realised sheet travel is **exactly zero** — a number with no
    /// rounding in it at all, which is the one worth asserting exactly.
    #[test]
    fn a_scripted_episode_measures_what_f7_says_it_should() {
        const N: u64 = 40;
        let mut cfg = config("free_sail");
        cfg.max_steps = Some(N);

        let ep = Episode::new(cfg.clone(), manual_source(Cadence::EVERY_STEP), 3)
            .expect("a valid episode");
        let p = *ep.params();
        let start = *ep.state();
        assert!(
            (start.delta_r.abs() + (N as f64) * p.sim.dt * p.rudder.delta_r_rate_max)
                < p.rudder.delta_r_max,
            "the script must not reach the rudder stop, or the arithmetic changes"
        );
        drop(ep);

        let summary =
            run_one(cfg, Hold::source([1.0, 0.0, -1.0]), 3, None, 10_000).expect("a valid episode");

        assert_eq!(summary.outcome, Outcome::Truncated);
        assert_eq!(summary.executed_steps, N);
        assert_eq!(
            summary.decisions, N as usize,
            "cadence 1 decides every step"
        );
        assert!((summary.elapsed_s - (N as f64) * p.sim.dt).abs() < 1e-12);

        let want = (N as f64) * p.sim.dt * p.rudder.delta_r_rate_max;
        assert!(
            (summary.rudder_travel_rad - want).abs() < 1e-9,
            "realised rudder travel {} rad, F7 says {want}",
            summary.rudder_travel_rad
        );
        assert_eq!(
            summary.sheet_travel_m, 0.0,
            "a zero sheet command moved the sheet"
        );
        assert_eq!(summary.finish_time_s, None);
        assert_eq!(summary.legs_completed, 0);
        assert_eq!(summary.task_outcome, None);

        // …and the measurement is not identically zero for the other
        // actuator either: hauling moves the sheet and not the rudder.
        let summary = run_one(
            config_with_budget(N),
            Hold::source([0.0, -1.0, -1.0]),
            3,
            None,
            10_000,
        )
        .expect("a valid episode");
        // The rudder self-centres from zero, so it does not move; the
        // sheet hauls at `sheet_haul_rate` until it reaches its stop.
        assert_eq!(summary.rudder_travel_rad, 0.0);
        assert!(
            summary.sheet_travel_m > 0.0,
            "hauling did not move the sheet"
        );
        assert!(
            summary.sheet_travel_m <= (N as f64) * p.sim.dt * p.sheet.sheet_haul_rate + 1e-12,
            "the sheet moved faster than F7's haul rate"
        );
    }

    fn config_with_budget(n: u64) -> EpisodeConfig {
        let mut cfg = config("free_sail");
        cfg.max_steps = Some(n);
        cfg
    }

    /// A suite over held-out seeds **and** held-out courses, with both
    /// axes visible in the summaries.
    #[test]
    fn a_suite_sweeps_seeds_and_courses_separately() {
        // Where a hands-off boat goes, so the courses are on its track and
        // the report has something other than zeros in it.
        let track = {
            let mut ep = Episode::new(
                config_with_budget(1500),
                manual_source(Cadence::EVERY_STEP),
                5,
            )
            .expect("valid");
            ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
            let mut out = Vec::new();
            while ep.advance(1).expect("a valid action") == 1 {
                out.push(position(ep.state()));
            }
            out
        };

        let mut cfg = config("free_sail");
        cfg.max_steps = Some(1500);
        cfg.bounds = Bounds::Rect {
            min: [-500.0, -500.0],
            max: [500.0, 500.0],
        };
        let suite = EvalSuite {
            seeds: vec![3, 5, 8],
            routes: vec![
                // Reachable: a mark the boat sails over, far enough from
                // the start that arriving takes real time. A hands-off
                // `free_sail` boat covers about 5 m in 7 s, so a 1 m disc
                // at `track[1400]` is a finish and not a formality — the
                // assertion on the mean finish time below is what keeps it
                // that way.
                Some(sailgym_course::Route::lookahead_point(track[1400], 1.0)),
                // Not reachable in the budget: a mark far off the track.
                Some(sailgym_course::Route::lookahead_point(
                    sailgym_course::Vec2::new(400.0, 400.0),
                    3.0,
                )),
            ],
        };
        let report = evaluate(&cfg, &suite, &mut || Hold::source([0.0, -1.0, -1.0]), 2000)
            .expect("a valid suite");

        assert_eq!(report.episodes, 6, "3 seeds × 2 courses");
        assert_eq!(suite.episodes(), 6);
        assert_eq!(
            report
                .summaries
                .iter()
                .filter(|s| s.route_index == Some(0))
                .count(),
            3
        );
        assert!(report.finished > 0, "no seed reached the reachable mark");
        assert!(
            report.finished < report.episodes,
            "the unreachable mark was reached"
        );
        assert!(report.finish_rate > 0.0 && report.finish_rate < 1.0);
        assert!(
            report.mean_finish_time_s.expect("a finish") > 1.0,
            "the mark was reached at t = {:?}; a disc that contains the start is not a \
             course",
            report.mean_finish_time_s
        );
        assert!(report.truncated > 0);
        assert!(report.mean_sheet_travel_m > 0.0, "the sheet never moved");
        eprintln!("evaluate: {}", report.describe());

        // Seeds only, on the configuration's own course.
        let seeds_only = EvalSuite::seeds(&[1, 2]);
        assert_eq!(seeds_only.episodes(), 2);
        let report = evaluate(
            &config_with_budget(200),
            &seeds_only,
            &mut || manual_source(Cadence::EVERY_STEP),
            1000,
        )
        .expect("a valid suite");
        assert_eq!(report.episodes, 2);
        assert!(report.summaries.iter().all(|s| s.route_index.is_none()));
        assert_eq!(report.truncated, 2);
    }
}
