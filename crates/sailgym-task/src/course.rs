//! The waypoint-course challenge (v2 section 12, D3 and D7).
//!
//! "Sail through each waypoint in order." The rule is **not** written here:
//! passage and cuts live in `sailgym-course` and this module calls them, which
//! is the whole of D7 and the whole point of RV66. A boat that cuts waypoint 2
//! is cut by `passage::cut_between`, the same function the episode runner asks,
//! and the two cannot disagree about what a miss *is* — only about what it
//! *costs*.
//!
//! # A miss is an event and not the end
//!
//! D3: a cut emits `waypoint_missed` and the attempt **continues**. The player
//! has to come back behind the gate line and cross through it, which F15.3's
//! directed crossing already requires — there is no "re-arm" flag anywhere,
//! because the tracker never advanced in the first place.
//!
//! That is a deliberate disagreement with `sailgym-env`, whose F19.2
//! `Terminated(MarkMissed)` still ends a *research* episode. The practice
//! evaluator and the episode runner are allowed to disagree about what a miss
//! costs. The PRD says so, and D2's one definition of a cut is why they cannot
//! disagree about anything else.
//!
//! # No physical coefficient, and no geometry of its own
//!
//! Every number here is a task threshold or a piece of **course geometry**, and
//! the geometry is the course document's (`courses/*.json`, task 12.1). This
//! module holds no coefficient, no equation and no copy of a waypoint.

use std::collections::BTreeMap;

use sailgym_course::guidance::CourseParams;
use sailgym_course::route::position;
use sailgym_course::{passage, CourseId, Route, Tracker, Vec2};
use sailgym_physics::state::BoatState;
use serde::{Deserialize, Serialize};

use crate::{ConfigError, Outcome, Progress, StepObservation};

/// *Sail the course* — through every waypoint, in order.
///
/// `Copy`, like every other shipped configuration, which is why [`CourseId`] is
/// a `Copy` enum (task 12.1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaypointCourseConfig {
    /// Which shipped course. Its geometry, its gate width and the scenario it
    /// is sailed in are the course document's.
    pub course: CourseId,
    /// s, the attempt's limit.
    ///
    /// **Measured, by the rule recorded in `docs/v2/baseline-validation.md`:**
    /// three times the baseline's time under the browser's own conditions.
    pub time_limit_s: f64,
}

/// Where a course attempt has got to.
///
/// Holds a [`Tracker`], which is the course crate's own walker: it owns the
/// route, the leg index and the passages, and it is the **only** thing that
/// decides a passage here.
#[derive(Clone, Debug)]
pub struct CourseState {
    tracker: Tracker,
    /// The position the previous step ended at, for the cut test. The tracker
    /// keeps its own copy for its own rule; this one is this module's, because
    /// `cut_between` takes the step and not a state.
    prev: Vec2,
    /// A cut on the **current** leg has been reported and the boat has not
    /// since passed that waypoint. Presentation only — it changes no rule.
    cut_pending: bool,
    /// How many cuts have been reported, in total.
    cuts: u32,
    /// s since the attempt began, one per waypoint passed, in order.
    splits: Vec<f64>,
}

/// Two course runs are equal when they have got to the same place by the same
/// passages.
///
/// Written out because [`Tracker`] is not `PartialEq` — it owns a `Route`, and
/// this crate may not add a derive to `sailgym-course` (F13.2). Comparing the
/// route would compare the *configuration*, which [`WaypointCourseConfig`]
/// already carries and `TaskRun` already compares.
impl PartialEq for CourseState {
    fn eq(&self, other: &Self) -> bool {
        self.tracker.leg_index() == other.tracker.leg_index()
            && self.tracker.passages() == other.tracker.passages()
            && self.prev == other.prev
            && self.cut_pending == other.cut_pending
            && self.cuts == other.cuts
            && self.splits == other.splits
    }
}

impl CourseState {
    /// Begin a course attempt from an initial state.
    pub fn start(course: CourseId, initial: &BoatState) -> Result<Self, ConfigError> {
        let route = route_of(course)?;
        let tracker = Tracker::start(route, initial).map_err(|_| ConfigError {
            field: "course",
            problem: "names a course whose route does not validate",
        })?;
        Ok(Self {
            tracker,
            prev: position(initial),
            cut_pending: false,
            cuts: 0,
            splits: Vec::new(),
        })
    }

    /// The route being sailed.
    pub fn route(&self) -> &Route {
        self.tracker.route()
    }

    /// Waypoints passed so far.
    pub fn passed(&self) -> u32 {
        self.tracker.leg_index()
    }

    /// How many waypoints the course has.
    pub fn waypoints(&self) -> u32 {
        self.tracker.route().legs()
    }

    /// s since the attempt began, one per waypoint passed.
    pub fn splits(&self) -> &[f64] {
        &self.splits
    }

    /// Cuts reported so far.
    pub fn cuts(&self) -> u32 {
        self.cuts
    }

    /// A cut on the current leg is outstanding.
    pub fn cut_pending(&self) -> bool {
        self.cut_pending
    }

    /// Fold one completed physics step in.
    ///
    /// Returns the events it produced, in order, each as `(id, 1-based
    /// waypoint number)`, and whether the course is now finished.
    pub fn observe(
        &mut self,
        obs: &StepObservation,
        origin_t: f64,
    ) -> (Vec<(&'static str, u32)>, bool) {
        let cur = position(&obs.state);
        let leg = self.tracker.leg_index();
        let mut events: Vec<(&'static str, u32)> = Vec::new();
        if self.tracker.observe(&obs.state).is_some() {
            // One passage per step at most, which is the tracker's own rule.
            self.splits.push(obs.t() - origin_t);
            self.cut_pending = false;
            events.push(("waypoint_passed", leg + 1));
        } else if passage::cut_between(self.tracker.route(), leg, self.prev, cur) {
            // **The one definition of a cut** (v2 F19.4, D2, RV66). The
            // tracker did not advance, so the boat still has waypoint `leg` to
            // pass; coming back behind the gate line and crossing through it is
            // what F15.3's directed crossing already asks for, and no flag here
            // re-arms anything.
            self.cuts = self.cuts.saturating_add(1);
            self.cut_pending = true;
            events.push(("waypoint_missed", leg + 1));
        }
        self.prev = cur;
        (events, self.tracker.finished())
    }

    /// What the page shows while sailing.
    pub fn progress(&self) -> Progress {
        Progress {
            phase: if self.cut_pending {
                "recover"
            } else {
                "sailing"
            },
            value: f64::from(self.passed()),
            target: f64::from(self.waypoints()),
            hold_s: 0.0,
            hold_target_s: 0.0,
        }
    }
}

/// The route a shipped course is sailed round.
///
/// One call into `sailgym-course`, which owns the document, the geometry and
/// the gate construction. This crate holds no copy of any of them.
pub fn route_of(course: CourseId) -> Result<Route, ConfigError> {
    course
        .load()
        .map_err(|_| ConfigError {
            field: "course",
            problem: "names a course whose document does not load",
        })?
        .route()
        .map_err(|_| ConfigError {
            field: "course",
            problem: "names a course whose route does not validate",
        })
}

/// The course geometry, as named scalars under `course.*`.
///
/// This is what makes a course attempt's [`TaskIdentity`](sailgym_physics::recording::TaskIdentity)
/// **self-describing**: two attempts on different geometry are refused by
/// `ExperimentIdentity::compare`, and a replay can redraw the course the
/// episode was actually flown on rather than today's catalogue (F18.3). No
/// schema change was needed, as in section 11: the thresholds map was already
/// `BTreeMap<String, f64>`.
///
/// Every value is a scalar because the map is one. The waypoints are therefore
/// `course.waypoint_<n>_x` and `_y`, one-based to match what the page numbers
/// them, and the gate posts are **not** listed: they are derived from the
/// waypoints and `course.half_width` by `Route::waypoints`, and a second copy
/// of a derived value is a second thing to keep in step.
pub fn thresholds(config: &WaypointCourseConfig) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    m.insert("time_limit_s".to_string(), config.time_limit_s);
    let Ok(doc) = config.course.load() else {
        return m;
    };
    m.insert(
        "course.schema_version".to_string(),
        f64::from(doc.schema_version),
    );
    m.insert("course.half_width".to_string(), doc.half_width);
    m.insert(
        "course.waypoint_count".to_string(),
        doc.waypoints.len() as f64,
    );
    m.insert("course.start_x".to_string(), doc.start.x);
    m.insert("course.start_y".to_string(), doc.start.y);
    for (i, p) in doc.waypoints.iter().enumerate() {
        m.insert(format!("course.waypoint_{}_x", i + 1), p.x);
        m.insert(format!("course.waypoint_{}_y", i + 1), p.y);
    }
    // The lookahead the guidance a player is shown was computed with. It is a
    // **tracked debt** rather than a complete record: `CourseParams` is not in
    // `ResearchIdentity`, so changing it silently changes what an observation
    // means. Recording it here at least makes a practice attempt say which one
    // it was scored beside (v2 section 12, "Deliberate debts").
    m.insert(
        "course.lookahead_m".to_string(),
        CourseParams::default().lookahead,
    );
    m
}

/// The time limit the shipped challenge uses, seconds.
///
/// **Measured.** `docs/v2/baseline-validation.md` records the rule and the
/// runs: three times the rule sailor's time under the browser's own
/// conditions, rounded up to the next five seconds so that the number on the
/// page is a number and not a measurement artefact.
///
/// | course | baseline | limit |
/// |---|---|---|
/// | `reach` | 42.50 s | 130 s |
/// | `triangle` | 105.50 s | 320 s |
/// | `windward_leeward` | 91.80 s | 280 s |
pub fn shipped_time_limit_s(course: CourseId) -> f64 {
    match course {
        CourseId::Reach => 130.0,
        CourseId::Triangle => 320.0,
        CourseId::WindwardLeeward => 280.0,
    }
}

/// Reject a configuration that cannot be evaluated.
pub fn validate(config: &WaypointCourseConfig) -> Result<(), ConfigError> {
    if !config.time_limit_s.is_finite() {
        return Err(ConfigError {
            field: "time_limit_s",
            problem: "is not finite",
        });
    }
    if config.time_limit_s <= 0.0 {
        return Err(ConfigError {
            field: "time_limit_s",
            problem: "must be greater than zero",
        });
    }
    // The course has to exist, load and build a route, or the attempt would
    // fail on its first step instead of being refused.
    route_of(config.course)?;
    Ok(())
}

/// The outcome a course attempt reaches when the last waypoint is passed.
pub const FINISHED: Outcome = Outcome::Succeeded;
