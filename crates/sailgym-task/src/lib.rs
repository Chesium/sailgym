//! Deterministic practice-task evaluation (v2 section 11, F18.4; v2 section 12,
//! D3 and D7).
//!
//! Three guided-practice skills — *get moving*, *complete a tack* and *recover
//! from excessive heel* — and three waypoint **courses**, as a pure observer
//! over the physics.
//!
//! ## `task → course`, added by v2 section 12 (D7)
//!
//! A course challenge needs passage and cuts, so this crate depends on
//! `sailgym-course`. The arrows become `task → {course, physics}`; there is no
//! cycle, because `course → physics` only and `sailgym-physics` depends on
//! nothing. The rule for what a cut **is** lives in `sailgym-course` and is
//! called from [`course`], never copied (RV66).
//!
//! ## What this crate is, and what it deliberately is not
//!
//! * **It is an observer.** [`TaskRun::observe`] takes a [`StepObservation`]
//!   by reference and returns an [`Outcome`]. It never touches a
//!   `Simulation`, never integrates anything and never writes state, so
//!   running a task cannot change a trajectory. `practice.rs`'s
//!   `task_evaluation_does_not_perturb_the_physics` measures that claim by
//!   running the same control script twice — once with an evaluator attached
//!   and once without — and comparing the 13 state scalars bit for bit.
//!
//! * **It is scored on physics steps, never on render frames** (v2 F18.4).
//!   Every event carries the step index it was decided on as well as the
//!   simulated time, and every duration is compared against simulated
//!   seconds. A browser that drops half its frames, or that calls
//!   `advance(10)` instead of ten `advance(1)`s, produces the identical
//!   outcome at the identical step — which is RV62, and
//!   `identical_control_sequences_agree_under_six_batch_sizes` is the
//!   measurement.
//!
//! * **It holds no physical coefficient.** Every number in a [`TaskSpec`] is a
//!   *task threshold*: a speed a challenge asks for, an angle that counts as
//!   settled, a duration that counts as held. They are versioned with the task
//!   ([`TaskSpec::version`]), visible through
//!   [`TaskSpec::thresholds`], recorded in the episode's
//!   [`TaskIdentity`], and chosen from scripted runs of section 08's baseline
//!   — never by changing a coefficient to make a challenge easier (brief §43,
//!   RV61). `docs/v2/practice-validation.md` records the runs.
//!
//! * **The dependency points one way.** `task → physics`, and
//!   `sailgym-physics` knows nothing about this crate (v2 F14.1). That is what
//!   keeps F8.1's "the physics core builds and tests on the host with plain
//!   `cargo test`" true.
//!
//! ## The shape of an attempt
//!
//! ```text
//! TaskRun::start(spec, &initial)        freeze the configuration, seed the machine
//!   └─ observe(&step_1) ─► Running      once per completed physics step
//!   └─ observe(&step_2) ─► Running
//!        …
//!   └─ observe(&step_n) ─► Succeeded    terminal; further observations are ignored
//! ```
//!
//! An [`Outcome`] is one of [`Outcome::Running`], [`Outcome::Succeeded`],
//! [`Outcome::Failed`] and [`Outcome::TimedOut`], exactly as the section PRD
//! fixes them. `TimedOut` carries no reason on purpose: what happened is in
//! the **events**, so a result can be explained by what was observed rather
//! than by a diagnosis nobody measured.

use std::collections::BTreeMap;

use sailgym_course::CourseId;
use sailgym_physics::frames::{world_to_body, wrap_pi};
use sailgym_physics::recording::{PracticeEnvelope, PracticeEvent, TaskIdentity};
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls};
use sailgym_physics::vec::Vec2;
use serde::{Deserialize, Serialize};

pub mod course;

pub use course::{CourseState, WaypointCourseConfig};

// ---------------------------------------------------------------------------
// Observation
// ---------------------------------------------------------------------------

/// Everything the evaluator is allowed to see about one completed physics
/// step.
///
/// Deliberately a **value**, not a borrow of the simulation: an evaluator that
/// held a `&Simulation` could reach `forces()`, which is cached once per
/// `advance(n)` call and would therefore make the outcome depend on how the
/// caller chunked its calls — the trap v2 F14.7 names for observations and
/// RV62 names for scores.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct StepObservation {
    /// The physics step index this observation describes.
    pub step: u64,
    /// The complete F3 state after that step.
    pub state: BoatState,
    /// The controls in force while it was taken.
    pub controls: Controls,
    /// m/s, true wind at the boat in the **world** frame — the direction the
    /// air is blowing *toward* (F6.1).
    pub wind_world: Vec2,
    /// `CapsizeState::capsized` after that step (F6.10). Reported by the
    /// physics, never decided here.
    pub capsized: bool,
}

impl StepObservation {
    /// The observation describing the simulation's currently published state.
    ///
    /// The one place this crate touches `Simulation`, and it only *reads*.
    /// `wind_at_boat()` is an `O(K)` allocation-free sample (F6.1), which is
    /// why the evaluator can afford to run on every step while a full
    /// `Diagnostics` record cannot.
    pub fn of(sim: &Simulation) -> Self {
        Self {
            step: sim.steps(),
            state: *sim.state(),
            controls: *sim.controls(),
            wind_world: sim.wind_at_boat(),
            capsized: sim.capsize().capsized,
        }
    }

    /// s, the simulated time of the step.
    pub fn t(&self) -> f64 {
        self.state.t
    }
}

/// The true wind angle off the bow: the **FROM** direction relative to the
/// bow, positive to starboard, wrapped to `(−π, π]`.
///
/// The rotation is `frames::world_to_body`, which is the only place a yaw
/// rotation is written (F2), and the `atan2(y, −x)` is the same expression
/// `diagnostics::apparent_wind_angle` uses — the same convention on the true
/// wind instead of the apparent. `0` is head to wind, `±π` is dead downwind,
/// a negative value puts the wind on the port bow (the boat is on **port**
/// tack, F2.1).
///
/// A zero wind vector gives `0`, which is the same guard
/// `diagnostics::apparent_wind_angle` uses; no direction exists and none is
/// invented.
pub fn true_wind_angle(state: &BoatState, wind_world: Vec2) -> f64 {
    let h = world_to_body(wind_world, state.psi);
    if h.x == 0.0 && h.y == 0.0 {
        0.0
    } else {
        wrap_pi(h.y.atan2(-h.x))
    }
}

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

/// Why an attempt failed.
///
/// Each variant is something that was **observed**, and each carries a stable
/// id so the recorded event, the JSON and the page all name it the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureReason {
    /// The boat made sternway past the task's threshold for long enough that
    /// it is not "getting moving" by any reading.
    BackwardDrift,
    /// The boat turned **away** from the wind past the task's downwind
    /// threshold instead of through it. A tack crosses the wind ahead; this
    /// one was going round astern.
    WrongWay,
    /// The boat crossed the task's boundary back and forth more times than the
    /// task allows. A threshold that is touched repeatedly has not been
    /// crossed.
    RepeatedJitter,
    /// The heel passed the task's threshold with no ease or release command
    /// ever observed.
    LateRelease,
    /// `CapsizeState::capsized` was reported (F6.10). Righting a capsized
    /// dinghy is deferred, so this ends the attempt (RV64).
    Capsized,
}

impl FailureReason {
    /// The stable id, used in events, JSON and the page.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BackwardDrift => "backward_drift",
            Self::WrongWay => "wrong_way",
            Self::RepeatedJitter => "repeated_jitter",
            Self::LateRelease => "late_release",
            Self::Capsized => "capsized",
        }
    }

    /// The id of the terminal event this reason emits.
    pub fn event_id(self) -> String {
        format!("failed_{}", self.as_str())
    }
}

/// The four outcomes the section PRD fixes.
///
/// `TimedOut` carries no reason: the events say what happened, and inventing a
/// cause for "the clock ran out" is exactly the unsupported diagnosis the PRD
/// forbids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Running,
    Succeeded,
    Failed { reason: FailureReason },
    TimedOut,
}

impl Outcome {
    /// Whether the attempt is over.
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    /// The stable id, for a log or a badge.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed { .. } => "failed",
            Self::TimedOut => "timed_out",
        }
    }

    /// The id of the terminal event this outcome emits, or `None` while
    /// running.
    fn event_id(self) -> Option<String> {
        match self {
            Self::Running => None,
            Self::Succeeded => Some("succeeded".to_string()),
            Self::TimedOut => Some("timed_out".to_string()),
            Self::Failed { reason } => Some(reason.event_id()),
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// A task configuration that cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    /// The offending field, by name.
    pub field: &'static str,
    /// What is wrong with it.
    pub problem: &'static str,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "task configuration: {} {}", self.field, self.problem)
    }
}

impl std::error::Error for ConfigError {}

fn finite_positive(field: &'static str, v: f64) -> Result<(), ConfigError> {
    if !v.is_finite() {
        return Err(ConfigError {
            field,
            problem: "is not finite",
        });
    }
    if v <= 0.0 {
        return Err(ConfigError {
            field,
            problem: "must be greater than zero",
        });
    }
    Ok(())
}

/// `lo < hi`, with an incomparable pair rejected rather than silently
/// accepted. Written through `partial_cmp` for the reason v2 F16.3 gives about
/// `delta_r_self_centre`: a two-way `<` on a partially ordered type is wrong
/// at the one value nobody tests.
fn ordered(field: &'static str, lo: f64, hi: f64) -> Result<(), ConfigError> {
    if lo.partial_cmp(&hi) != Some(std::cmp::Ordering::Less) {
        return Err(ConfigError {
            field,
            problem: "is out of order",
        });
    }
    Ok(())
}

/// Every shipped challenge, by stable id.
///
/// The three skills are section 11's. [`TaskId::Course`] is v2 section 12's and
/// carries a [`CourseId`], which is why that is a `Copy` enum: a `TaskSpec` has
/// to stay `Copy`.
///
/// `PartialOrd`/`Ord`/`Serialize`/`Deserialize` are written out rather than
/// derived, because deriving them would demand `CourseId: Ord` and a `serde`
/// shape for the new variant that nothing asked for. All four go through
/// [`TaskId::as_str`] and [`TaskId::parse`], so there is **one** spelling of a
/// task id: the one that appears in the recorded
/// [`TaskIdentity`](sailgym_physics::recording::TaskIdentity), in the JSON the
/// browser reads and in a log. The three skills' serialised form is byte for
/// byte what section 11 wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskId {
    GetMoving,
    CompleteTack,
    RecoverFromHeel,
    /// One of the shipped waypoint courses (v2 section 12).
    Course(CourseId),
}

impl PartialOrd for TaskId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaskId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Serialize for TaskId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("unknown task `{text}`")))
    }
}

/// The three **skills**, in the order the page offers them.
///
/// Unchanged by v2 section 12, deliberately: the courses have their own list,
/// so `practice_tasks_json` is unchanged until task 12.7 lists them and the
/// gate stays green between groups.
pub const TASK_IDS: [TaskId; 3] = [
    TaskId::GetMoving,
    TaskId::CompleteTack,
    TaskId::RecoverFromHeel,
];

/// The three **courses**, in the order the page offers them (v2 section 12).
pub const COURSE_TASK_IDS: [TaskId; 3] = [
    TaskId::Course(CourseId::Reach),
    TaskId::Course(CourseId::Triangle),
    TaskId::Course(CourseId::WindwardLeeward),
];

/// Every shipped challenge: the skills, then the courses.
pub fn all_task_ids() -> Vec<TaskId> {
    TASK_IDS
        .iter()
        .chain(COURSE_TASK_IDS.iter())
        .copied()
        .collect()
}

impl TaskId {
    /// The stable id.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GetMoving => "get_moving",
            Self::CompleteTack => "complete_tack",
            Self::RecoverFromHeel => "recover_from_heel",
            Self::Course(CourseId::Reach) => "course_reach",
            Self::Course(CourseId::Triangle) => "course_triangle",
            Self::Course(CourseId::WindwardLeeward) => "course_windward_leeward",
        }
    }

    /// The id, back from its string form.
    pub fn parse(s: &str) -> Option<Self> {
        all_task_ids().into_iter().find(|id| id.as_str() == s)
    }

    /// The course this challenge is sailed on, if it is a course.
    pub fn course(self) -> Option<CourseId> {
        match self {
            Self::Course(c) => Some(c),
            _ => None,
        }
    }

    /// The shipped scenario (brief §32) this challenge is set on.
    ///
    /// A challenge is a **scenario plus a task**: the same six documents the
    /// picker offers, with an instruction and an evaluator over them. No new
    /// scenario is shipped by section 11.
    /// A course challenge's scenario is the **course document's**, and the
    /// three literals below are a second copy of it.
    ///
    /// Written out because this method returns `&'static str` and a course
    /// document's `scenario` is a `String` parsed at run time; the alternative
    /// — widening the signature — would reach into
    /// `crates/sailgym-wasm/src/lib.rs`, which no task in this group owns
    /// (F13.2). The duplication is not left to a comment:
    /// `tests/course.rs::every_course_challenges_scenario_is_its_documents`
    /// loads all three documents and asserts the two agree, which is the same
    /// device `sailgym-env`'s `make_sensor` uses for the same reason (F19.6).
    pub fn scenario(self) -> &'static str {
        match self {
            Self::GetMoving => "free_sail",
            Self::CompleteTack => "tack",
            Self::RecoverFromHeel => "sheet_release_recovery",
            // Every shipped course is sailed in `free_sail`'s uniform
            // northerly: the browser's own conditions (task 12.1's
            // `courses/*.json`).
            Self::Course(_) => "free_sail",
        }
    }

    /// The event a result's **Inspect** action should jump to.
    ///
    /// The last event with this id, if the attempt produced one.
    pub fn highlight_event(self) -> &'static str {
        match self {
            Self::GetMoving => "speed_reached",
            Self::CompleteTack => "crossing",
            Self::RecoverFromHeel => "heel_max",
            // A course's **Inspect** jumps to the first miss if there was one,
            // and otherwise to the finish. `TaskRun::highlight` is where that
            // "first, not last" rule lives; this names the event it looks for.
            Self::Course(_) => "waypoint_missed",
        }
    }
}

/// *Get moving* — sustain forward speed for a fixed duration.
///
/// Thresholds measured on `free_sail` in `docs/v2/practice-validation.md` §2.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetMovingConfig {
    /// m/s, surge speed `u` the attempt must reach.
    pub target_speed_mps: f64,
    /// m/s, the speed the hold is broken below. Strictly under
    /// `target_speed_mps`: the gap is the hysteresis that stops a boat sitting
    /// exactly on the target from starting and breaking the hold every step.
    pub release_speed_mps: f64,
    /// s, how long `u` must stay at or above `release_speed_mps` once the
    /// target has been reached.
    pub hold_s: f64,
    /// m/s, sternway past which the attempt is going the wrong way. Compared
    /// against `−backward_speed_mps`.
    pub backward_speed_mps: f64,
    /// s, how long that sternway must last before the attempt fails.
    pub backward_hold_s: f64,
    /// s, the attempt's limit.
    pub time_limit_s: f64,
}

/// *Complete a tack* — cross head to wind and establish the opposite tack.
///
/// Thresholds measured on `tack` in `docs/v2/practice-validation.md` §3.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompleteTackConfig {
    /// rad, `|TWA|` at or below which the boat is approaching head to wind on
    /// its **starting** side.
    pub approach_twa_rad: f64,
    /// rad, `TWA` on the **opposite** side at which the crossing is declared.
    /// The gap between this and zero is the hysteresis: a boat that merely
    /// touches head to wind has not crossed.
    pub crossing_twa_rad: f64,
    /// rad, `TWA` on the opposite side at which the new tack counts as
    /// established.
    pub settled_twa_rad: f64,
    /// s, how long the boat must hold that angle **and** the recovered speed.
    pub settle_hold_s: f64,
    /// m/s, the surge speed that counts as forward-speed recovery.
    pub recover_speed_mps: f64,
    /// rad, `|TWA|` past which the boat has turned away from the wind rather
    /// than through it — a gybe, not a tack.
    pub wrong_way_twa_rad: f64,
    /// How many times the machine may fall back to an earlier phase before the
    /// attempt is jitter.
    pub max_reversals: u32,
    /// s, the attempt's limit.
    pub time_limit_s: f64,
}

/// *Recover from excessive heel* — ease or release early and regain control.
///
/// Thresholds measured on `sheet_release_recovery` in
/// `docs/v2/practice-validation.md` §4. **Nothing here rights a capsized
/// boat**: a declared capsize ends the attempt (RV64), and the challenge is
/// about the heel *before* that point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecoverHeelConfig {
    /// rad, `|φ|` the attempt must reach before a recovery means anything.
    pub qualify_heel_rad: f64,
    /// rad, `|φ|` at or below which the boat counts as recovered.
    pub recover_heel_rad: f64,
    /// s, how long it must stay there.
    pub recover_hold_s: f64,
    /// rad, `|φ|` past which an attempt that has still not eased or released
    /// has left it too late.
    pub late_release_heel_rad: f64,
    /// Normalised `sheet_rate_cmd` above which the sheet counts as being
    /// eased. `sheet_release` counts whatever this is.
    pub ease_command_min: f64,
    /// rad, how much further `|φ|` must climb past the last recorded peak
    /// before another `heel_max` event is emitted.
    pub heel_event_step_rad: f64,
    /// s, the attempt's limit.
    pub time_limit_s: f64,
}

/// One challenge: its id, its version and its frozen thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "id", rename_all = "snake_case")]
pub enum TaskSpec {
    GetMoving(GetMovingConfig),
    CompleteTack(CompleteTackConfig),
    RecoverFromHeel(RecoverHeelConfig),
    /// *Sail the course* — through every waypoint, in order (v2 section 12).
    WaypointCourse(WaypointCourseConfig),
}

/// The version of every shipped task configuration.
///
/// Bumped **by hand** whenever a threshold or an outcome rule changes meaning,
/// because two attempts scored under different rules are not the same
/// experiment — `ExperimentIdentity::compare` reads it out of the recorded
/// [`TaskIdentity`] and refuses the comparison (v2 F18.3).
///
/// **Unchanged by v2 section 12.** The courses are new tasks with new ids and
/// their own thresholds; no existing rule changed meaning, so an attempt at a
/// skill recorded before this section and one recorded after it are still the
/// same experiment.
pub const TASK_VERSION: u32 = 1;

impl TaskSpec {
    /// The shipped configuration for a challenge.
    ///
    /// **The only configuration the application offers.** The browser picks a
    /// challenge by id and Rust supplies the thresholds; there is no path by
    /// which a player can lower a bar and still call the result the same
    /// experiment (RV61).
    pub fn shipped(id: TaskId) -> Self {
        match id {
            // §2 of practice-validation.md: on `free_sail`, doing nothing
            // reaches u = 0.493 m/s in 25 s and a two-blocked sheet reaches
            // 1.076 m/s, so 1.2 m/s separates "trimmed for drive" from both
            // "untrimmed" and "over-sheeted". The hold is three seconds
            // because the eased boat's speed still creeps upward and a
            // momentary touch is not sustaining anything.
            TaskId::GetMoving => Self::GetMoving(GetMovingConfig {
                target_speed_mps: 1.2,
                release_speed_mps: 1.1,
                hold_s: 3.0,
                backward_speed_mps: 0.25,
                backward_hold_s: 2.0,
                time_limit_s: 45.0,
            }),
            // §3: the measured tack on `tack` reaches |TWA| ≤ 30° at 1.05 s,
            // +10° at 6.0 s and +35° with u ≥ 0.4 m/s at 14.1 s. 120° is well
            // past a beam reach and is where a bear-away has become a gybe.
            //
            // The four angles are stored in **radians** (F1) and written as
            // the degrees they were chosen in, converted here and nowhere
            // else. `to_radians` is the standard library's one conversion;
            // spelling the same numbers as decimal literals would put four
            // hand-rounded constants in the file for no gain.
            TaskId::CompleteTack => Self::CompleteTack(CompleteTackConfig {
                approach_twa_rad: 30.0f64.to_radians(),
                crossing_twa_rad: 10.0f64.to_radians(),
                settled_twa_rad: 35.0f64.to_radians(),
                settle_hold_s: 1.5,
                recover_speed_mps: 0.4,
                wrong_way_twa_rad: 120.0f64.to_radians(),
                max_reversals: 2,
                time_limit_s: 45.0,
            }),
            // §4: on `sheet_release_recovery` the heel passes 25° at 0.33 s
            // whatever the player does, the last release that still recovers
            // is at 7.0 s (peak 74.2°), the sheet-held run passes 75° at
            // 7.87 s and capsize is declared at 9.45 s.
            TaskId::RecoverFromHeel => Self::RecoverFromHeel(RecoverHeelConfig {
                qualify_heel_rad: 25.0f64.to_radians(),
                recover_heel_rad: 20.0f64.to_radians(),
                recover_hold_s: 2.0,
                late_release_heel_rad: 75.0f64.to_radians(),
                ease_command_min: 0.05,
                heel_event_step_rad: 5.0f64.to_radians(),
                time_limit_s: 30.0,
            }),
            // v2 section 12: the geometry is the course document's and the one
            // threshold is the limit, measured by the rule
            // `docs/v2/baseline-validation.md` records.
            TaskId::Course(course) => Self::WaypointCourse(WaypointCourseConfig {
                course,
                time_limit_s: course::shipped_time_limit_s(course),
            }),
        }
    }

    /// The challenge this specification configures.
    pub fn id(&self) -> TaskId {
        match self {
            Self::GetMoving(_) => TaskId::GetMoving,
            Self::CompleteTack(_) => TaskId::CompleteTack,
            Self::RecoverFromHeel(_) => TaskId::RecoverFromHeel,
            Self::WaypointCourse(c) => TaskId::Course(c.course),
        }
    }

    /// [`TASK_VERSION`].
    pub fn version(&self) -> u32 {
        TASK_VERSION
    }

    /// s, the attempt's limit.
    pub fn time_limit_s(&self) -> f64 {
        match self {
            Self::GetMoving(c) => c.time_limit_s,
            Self::CompleteTack(c) => c.time_limit_s,
            Self::RecoverFromHeel(c) => c.time_limit_s,
            Self::WaypointCourse(c) => c.time_limit_s,
        }
    }

    /// The id and unit of the challenge's headline metric.
    ///
    /// One metric per challenge, in SI (F1): degrees and other display forms
    /// are the UI's business and are converted there.
    pub fn metric(&self) -> (&'static str, &'static str) {
        match self {
            Self::GetMoving(_) => ("top_speed", "m/s"),
            Self::CompleteTack(_) => ("tack_time", "s"),
            Self::RecoverFromHeel(_) => ("peak_heel", "rad"),
            // Elapsed seconds, which is what a course is raced in (D5).
            Self::WaypointCourse(_) => ("course_time", "s"),
        }
    }

    /// Every threshold, by name, in the task's own units.
    ///
    /// A `BTreeMap`, so the record serialises in one order and equality of the
    /// text means equality of the content (F9.3). This is what travels in the
    /// episode's [`TaskIdentity`] and what the page displays: the thresholds
    /// are *visible*, which is half of what makes them task configuration
    /// rather than a hidden coefficient.
    pub fn thresholds(&self) -> BTreeMap<String, f64> {
        // The course arm is whole-map rather than key-by-key, because its
        // waypoint keys are generated from the document; it returns early so
        // the `put` closure's borrow of `m` ends before `extend` needs it.
        if let Self::WaypointCourse(c) = self {
            return course::thresholds(c);
        }
        let mut m = BTreeMap::new();
        let mut put = |k: &str, v: f64| {
            m.insert(k.to_string(), v);
        };
        match self {
            Self::GetMoving(c) => {
                put("target_speed_mps", c.target_speed_mps);
                put("release_speed_mps", c.release_speed_mps);
                put("hold_s", c.hold_s);
                put("backward_speed_mps", c.backward_speed_mps);
                put("backward_hold_s", c.backward_hold_s);
                put("time_limit_s", c.time_limit_s);
            }
            Self::CompleteTack(c) => {
                put("approach_twa_rad", c.approach_twa_rad);
                put("crossing_twa_rad", c.crossing_twa_rad);
                put("settled_twa_rad", c.settled_twa_rad);
                put("settle_hold_s", c.settle_hold_s);
                put("recover_speed_mps", c.recover_speed_mps);
                put("wrong_way_twa_rad", c.wrong_way_twa_rad);
                put("max_reversals", f64::from(c.max_reversals));
                put("time_limit_s", c.time_limit_s);
            }
            Self::RecoverFromHeel(c) => {
                put("qualify_heel_rad", c.qualify_heel_rad);
                put("recover_heel_rad", c.recover_heel_rad);
                put("recover_hold_s", c.recover_hold_s);
                put("late_release_heel_rad", c.late_release_heel_rad);
                put("ease_command_min", c.ease_command_min);
                put("heel_event_step_rad", c.heel_event_step_rad);
                put("time_limit_s", c.time_limit_s);
            }
            // Returned above: the limit **and the course geometry**, under
            // `course.*` keys, so the recorded identity is self-describing
            // (v2 section 12, D5 and [`course::thresholds`]).
            Self::WaypointCourse(_) => unreachable!("returned above"),
        }
        m
    }

    /// The record that travels in the episode header (v2 F18.3, section 10).
    pub fn identity(&self) -> TaskIdentity {
        TaskIdentity {
            id: self.id().as_str().to_string(),
            version: self.version(),
            thresholds: self.thresholds(),
        }
    }

    /// Reject a configuration that cannot be evaluated.
    ///
    /// Finite, positive, and **ordered**: a release speed above the target, a
    /// settled angle inside the approach band or a recovery angle above the
    /// late-release angle would each make a rule mean the opposite of what it
    /// says.
    pub fn validate(&self) -> Result<(), ConfigError> {
        match self {
            Self::GetMoving(c) => {
                finite_positive("target_speed_mps", c.target_speed_mps)?;
                finite_positive("release_speed_mps", c.release_speed_mps)?;
                finite_positive("hold_s", c.hold_s)?;
                finite_positive("backward_speed_mps", c.backward_speed_mps)?;
                finite_positive("backward_hold_s", c.backward_hold_s)?;
                finite_positive("time_limit_s", c.time_limit_s)?;
                ordered("release_speed_mps", c.release_speed_mps, c.target_speed_mps)?;
                ordered("hold_s", c.hold_s, c.time_limit_s)?;
            }
            Self::CompleteTack(c) => {
                finite_positive("approach_twa_rad", c.approach_twa_rad)?;
                finite_positive("crossing_twa_rad", c.crossing_twa_rad)?;
                finite_positive("settled_twa_rad", c.settled_twa_rad)?;
                finite_positive("settle_hold_s", c.settle_hold_s)?;
                finite_positive("recover_speed_mps", c.recover_speed_mps)?;
                finite_positive("wrong_way_twa_rad", c.wrong_way_twa_rad)?;
                finite_positive("time_limit_s", c.time_limit_s)?;
                ordered("crossing_twa_rad", c.crossing_twa_rad, c.settled_twa_rad)?;
                ordered("settled_twa_rad", c.settled_twa_rad, c.wrong_way_twa_rad)?;
                ordered("approach_twa_rad", c.approach_twa_rad, c.wrong_way_twa_rad)?;
                if c.wrong_way_twa_rad >= std::f64::consts::PI {
                    return Err(ConfigError {
                        field: "wrong_way_twa_rad",
                        problem: "must be inside (0, π)",
                    });
                }
                ordered("settle_hold_s", c.settle_hold_s, c.time_limit_s)?;
            }
            Self::RecoverFromHeel(c) => {
                finite_positive("qualify_heel_rad", c.qualify_heel_rad)?;
                finite_positive("recover_heel_rad", c.recover_heel_rad)?;
                finite_positive("recover_hold_s", c.recover_hold_s)?;
                finite_positive("late_release_heel_rad", c.late_release_heel_rad)?;
                finite_positive("ease_command_min", c.ease_command_min)?;
                finite_positive("heel_event_step_rad", c.heel_event_step_rad)?;
                finite_positive("time_limit_s", c.time_limit_s)?;
                ordered("recover_heel_rad", c.recover_heel_rad, c.qualify_heel_rad)?;
                ordered(
                    "qualify_heel_rad",
                    c.qualify_heel_rad,
                    c.late_release_heel_rad,
                )?;
                ordered("recover_hold_s", c.recover_hold_s, c.time_limit_s)?;
            }
            Self::WaypointCourse(c) => course::validate(c)?,
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Progress and the report
// ---------------------------------------------------------------------------

/// The minimum a player needs to see while sailing.
///
/// One phase name, one number against one target, and — where a task has one —
/// how far through a hold the attempt is. Nothing here is a second scoring
/// rule: every field is read straight out of the machine that decides the
/// outcome.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Progress {
    /// A stable phase id; the page maps it to words.
    pub phase: &'static str,
    /// The quantity the phase is watching, in SI.
    pub value: f64,
    /// What it has to reach.
    pub target: f64,
    /// s, how long the current hold has run.
    pub hold_s: f64,
    /// s, how long it has to run.
    pub hold_target_s: f64,
}

/// The headline number a result is reported with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetricRecord {
    /// Stable id; the page maps it to a label.
    pub id: String,
    /// The F1 unit, spelled as F1 spells it.
    pub unit: String,
    /// The value, in that unit. `NaN` is never produced: a metric the attempt
    /// did not establish reads `0.0` and its event list is empty.
    pub value: f64,
}

/// Everything the browser is told about an attempt, in one record.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TaskReport {
    pub task: TaskIdentity,
    pub outcome: Outcome,
    /// The scenario the challenge is set on.
    pub scenario: String,
    /// s, simulated time since the attempt began.
    pub elapsed_s: f64,
    /// Physics steps since the attempt began.
    pub elapsed_steps: u64,
    pub metric: MetricRecord,
    pub progress: Progress,
    pub events: Vec<PracticeEvent>,
    /// The event a result's **Inspect** action should jump to, if the attempt
    /// produced one.
    pub highlight: Option<PracticeEvent>,
}

// ---------------------------------------------------------------------------
// The runtime
// ---------------------------------------------------------------------------

/// A hold timer over simulated time.
///
/// Held in **seconds of the observations themselves**, never counted in steps
/// and never accumulated from a `dt` the evaluator was told: the elapsed time
/// is `now − since`, so a caller that batches its steps differently, or that
/// pauses between them, cannot change when a hold completes (RV62).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Hold {
    since: Option<f64>,
}

impl Hold {
    /// Fold one observation in. Returns the elapsed hold, `0.0` when the
    /// condition does not hold.
    fn update(&mut self, holds: bool, t: f64) -> f64 {
        if !holds {
            self.since = None;
            return 0.0;
        }
        let since = *self.since.get_or_insert(t);
        t - since
    }

    fn elapsed(&self, t: f64) -> f64 {
        self.since.map_or(0.0, |s| t - s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GetMovingState {
    /// m/s, the largest surge speed seen.
    top_speed: f64,
    /// The target has been reached at least once.
    reached: bool,
    hold: Hold,
    backward: Hold,
}

/// Where a tack attempt has got to. The order is the order they must happen
/// in; falling back to an earlier one is a reversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TackPhase {
    /// On the starting tack, outside the approach band.
    Sailing,
    /// Inside the approach band, still on the starting side.
    Approaching,
    /// Past the crossing threshold on the opposite side.
    Crossed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TackState {
    /// `+1` when the attempt started on starboard tack (wind from starboard,
    /// `TWA > 0`), `−1` on port. Frozen at [`TaskRun::start`].
    side: f64,
    phase: TackPhase,
    reversals: u32,
    settle: Hold,
    /// s, the time the crossing event was emitted at.
    crossed_t: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HeelState {
    /// rad, the largest `|φ|` seen.
    peak_heel: f64,
    /// rad, the `|φ|` of the last `heel_max` event emitted.
    last_event_heel: Option<f64>,
    qualified: bool,
    eased: bool,
    recover: Hold,
}

/// The state of whichever challenge is being evaluated.
///
/// **No longer `Copy`**, because [`CourseState`] owns a `Tracker` and a
/// `Tracker` owns a `Route`. The three skill states are still `Copy` and are
/// still read out by value; the course arm is matched by reference.
#[derive(Clone, Debug, PartialEq)]
enum Machine {
    GetMoving(GetMovingState),
    Tack(TackState),
    Heel(HeelState),
    Course(CourseState),
}

/// One attempt at one challenge.
///
/// Constructed with [`TaskRun::start`], fed one [`StepObservation`] per
/// completed physics step, and terminal for ever once it reports anything but
/// [`Outcome::Running`].
#[derive(Clone, Debug, PartialEq)]
pub struct TaskRun {
    spec: TaskSpec,
    outcome: Outcome,
    machine: Machine,
    events: Vec<PracticeEvent>,
    /// The step and time the attempt began at, frozen at [`TaskRun::start`].
    origin_step: u64,
    origin_t: f64,
    /// The last observation folded in, so a report after the attempt ends
    /// still describes where it ended.
    last_step: u64,
    last_t: f64,
    /// The progress at the last observation.
    progress: Progress,
}

impl TaskRun {
    /// Begin an attempt from an initial condition.
    ///
    /// `initial` is the state the attempt starts from — step 0 of the run, and
    /// the state the episode's header records. Nothing is scored on it: it
    /// only freezes the origin of the clock and, for the tack, which tack the
    /// boat is on.
    ///
    /// The configuration is validated here and **frozen**: a `TaskRun` holds
    /// its own copy, so an edit elsewhere cannot change the rules an attempt
    /// in progress is being judged by.
    pub fn start(spec: TaskSpec, initial: &StepObservation) -> Result<Self, ConfigError> {
        spec.validate()?;
        let machine = match spec {
            TaskSpec::GetMoving(_) => Machine::GetMoving(GetMovingState {
                top_speed: initial.state.u,
                reached: false,
                hold: Hold::default(),
                backward: Hold::default(),
            }),
            TaskSpec::CompleteTack(_) => {
                let twa = true_wind_angle(&initial.state, initial.wind_world);
                // A boat exactly head to wind has no tack; `+1` is chosen so
                // the side is always one of two values and the machine is
                // total. The approach/crossing rules then read the same way
                // whichever branch a degenerate start takes.
                let side = if twa < 0.0 { -1.0 } else { 1.0 };
                Machine::Tack(TackState {
                    side,
                    phase: TackPhase::Sailing,
                    reversals: 0,
                    settle: Hold::default(),
                    crossed_t: None,
                })
            }
            TaskSpec::RecoverFromHeel(_) => Machine::Heel(HeelState {
                peak_heel: initial.state.phi.abs(),
                last_event_heel: None,
                qualified: false,
                eased: false,
                recover: Hold::default(),
            }),
            // The route is built here, from the course document, so a course
            // that does not load is a **refused configuration** rather than an
            // attempt that fails on its first step.
            TaskSpec::WaypointCourse(c) => {
                Machine::Course(CourseState::start(c.course, &initial.state)?)
            }
        };
        let mut run = Self {
            spec,
            outcome: Outcome::Running,
            machine,
            events: Vec::new(),
            origin_step: initial.step,
            origin_t: initial.t(),
            last_step: initial.step,
            last_t: initial.t(),
            progress: Progress {
                phase: "starting",
                value: 0.0,
                target: 0.0,
                hold_s: 0.0,
                hold_target_s: 0.0,
            },
        };
        run.progress = run.compute_progress(initial.t());
        Ok(run)
    }

    /// The frozen configuration.
    pub fn spec(&self) -> &TaskSpec {
        &self.spec
    }

    /// The outcome as it stands.
    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// The ordered event list, keyed by physics step.
    pub fn events(&self) -> &[PracticeEvent] {
        &self.events
    }

    /// s, simulated time since the attempt began.
    pub fn elapsed_s(&self) -> f64 {
        self.last_t - self.origin_t
    }

    /// Physics steps since the attempt began.
    pub fn elapsed_steps(&self) -> u64 {
        self.last_step.saturating_sub(self.origin_step)
    }

    /// The envelope section 10 reserved, ready for `Recorder::set_practice`.
    ///
    /// Written **once**, at the start of a recording, with an empty event
    /// list; the events are appended afterwards through
    /// `Recorder::push_practice_event`, in step order, as they happen. Writing
    /// the whole envelope at the end instead would make a recording stopped
    /// mid-attempt carry a task identity and no events at all.
    pub fn envelope(&self) -> PracticeEnvelope {
        PracticeEnvelope {
            envelope_version: sailgym_physics::recording::PRACTICE_ENVELOPE_VERSION,
            task: self.spec.identity(),
            events: Vec::new(),
        }
    }

    /// Fold one completed physics step in and return the outcome.
    ///
    /// Idempotent once terminal: an attempt that has already succeeded cannot
    /// be failed by a later step, and a replay that re-feeds an episode cannot
    /// emit a second success.
    pub fn observe(&mut self, obs: &StepObservation) -> Outcome {
        if self.outcome.is_terminal() {
            return self.outcome;
        }
        self.last_step = obs.step;
        self.last_t = obs.t();

        // The capsize report is the physics crate's (F6.10) and ends every
        // challenge: righting a capsized dinghy is deferred, so an attempt
        // that goes over is over (RV64).
        if obs.capsized {
            return self.finish(
                Outcome::Failed {
                    reason: FailureReason::Capsized,
                },
                obs,
                obs.state.phi.abs(),
            );
        }

        let verdict = match &mut self.machine {
            Machine::GetMoving(_) => Self::step_get_moving(self, obs),
            Machine::Tack(_) => Self::step_tack(self, obs),
            Machine::Heel(_) => Self::step_heel(self, obs),
            Machine::Course(_) => Self::step_course(self, obs),
        };
        if let Some((outcome, value)) = verdict {
            return self.finish(outcome, obs, value);
        }

        // The limit is checked **after** the rules, so an attempt that
        // succeeds on the very step the clock runs out succeeds.
        if self.elapsed_s() >= self.spec.time_limit_s() {
            return self.finish(Outcome::TimedOut, obs, self.metric_value());
        }
        self.progress = self.compute_progress(obs.t());
        self.outcome
    }

    /// Record a terminal outcome and its event.
    fn finish(&mut self, outcome: Outcome, obs: &StepObservation, value: f64) -> Outcome {
        self.outcome = outcome;
        self.progress = self.compute_progress(obs.t());
        if let Some(id) = outcome.event_id() {
            self.push(id, obs, value);
        }
        outcome
    }

    fn push(&mut self, id: String, obs: &StepObservation, value: f64) {
        self.events.push(PracticeEvent {
            id,
            step: obs.step,
            t: obs.t(),
            value,
        });
    }

    // --- the three machines ------------------------------------------------

    /// Returns `Some((outcome, metric value))` when the step is terminal.
    fn step_get_moving(&mut self, obs: &StepObservation) -> Option<(Outcome, f64)> {
        let TaskSpec::GetMoving(c) = self.spec else {
            unreachable!("machine and spec are constructed together")
        };
        let Machine::GetMoving(ref state) = self.machine else {
            unreachable!("machine and spec are constructed together")
        };
        // A copy, because `GetMovingState` is `Copy`; `Machine` is not, so the read is
        // through a reference (see [`Machine`]).
        let mut s = *state;
        let u = obs.state.u;
        s.top_speed = s.top_speed.max(u);

        // Sternway first: a boat going backwards is not holding a speed, and
        // saying so is more useful than letting the clock run out.
        let drifting = u <= -c.backward_speed_mps;
        let backward_s = s.backward.update(drifting, obs.t());
        if backward_s >= c.backward_hold_s {
            self.machine = Machine::GetMoving(s);
            return Some((
                Outcome::Failed {
                    reason: FailureReason::BackwardDrift,
                },
                u,
            ));
        }

        // The hold starts when `u` first reaches the target and survives while
        // it stays at or above the release speed. The gap between the two is
        // the hysteresis; without it a boat sitting on the target restarts the
        // hold every other step and never completes it.
        let mut event: Option<(&str, f64)> = None;
        if !s.reached {
            if u >= c.target_speed_mps {
                s.reached = true;
                event = Some(("speed_reached", u));
                s.hold.update(true, obs.t());
            }
        } else if u < c.release_speed_mps {
            s.reached = false;
            s.hold.update(false, obs.t());
            event = Some(("hold_broken", u));
        } else {
            s.hold.update(true, obs.t());
        }
        let held = s.hold.elapsed(obs.t());
        self.machine = Machine::GetMoving(s);
        if let Some((id, v)) = event {
            self.push(id.to_string(), obs, v);
        }
        if s.reached && held >= c.hold_s {
            return Some((Outcome::Succeeded, s.top_speed));
        }
        None
    }

    fn step_tack(&mut self, obs: &StepObservation) -> Option<(Outcome, f64)> {
        let TaskSpec::CompleteTack(c) = self.spec else {
            unreachable!("machine and spec are constructed together")
        };
        let Machine::Tack(ref state) = self.machine else {
            unreachable!("machine and spec are constructed together")
        };
        // A copy, because `TackState` is `Copy`; `Machine` is not, so the read is
        // through a reference (see [`Machine`]).
        let mut s = *state;
        let twa = true_wind_angle(&obs.state, obs.wind_world);
        // `signed` is the angle measured **away from the starting tack**:
        // negative while the boat is still on the side it began on, positive
        // once it is on the other one. Every threshold below is then one
        // comparison rather than a pair keyed on a sign.
        let signed = -s.side * twa;
        let mut events: Vec<(&str, f64)> = Vec::new();

        if twa.abs() >= c.wrong_way_twa_rad {
            events.push(("bore_away", twa));
            self.machine = Machine::Tack(s);
            for (id, v) in events {
                self.push(id.to_string(), obs, v);
            }
            return Some((
                Outcome::Failed {
                    reason: FailureReason::WrongWay,
                },
                self.metric_value(),
            ));
        }

        // Forward through the phases, with a hysteresis band on each boundary
        // so that touching a threshold is not crossing it.
        let next = if signed >= c.crossing_twa_rad {
            TackPhase::Crossed
        } else if signed >= -c.approach_twa_rad {
            TackPhase::Approaching
        } else {
            TackPhase::Sailing
        };
        if next > s.phase {
            s.phase = next;
            match next {
                TackPhase::Approaching => events.push(("approach", twa)),
                TackPhase::Crossed => {
                    s.crossed_t = Some(obs.t());
                    events.push(("crossing", twa));
                }
                TackPhase::Sailing => {}
            }
        } else if next < s.phase {
            // A regression. The boat went back to a phase it had already left,
            // which is what a boat wallowing across head to wind does; the
            // task allows a couple and then calls it jitter rather than a
            // tack.
            s.phase = next;
            s.reversals += 1;
            s.settle.update(false, obs.t());
            events.push(("reversal", twa));
            if s.reversals > c.max_reversals {
                self.machine = Machine::Tack(s);
                for (id, v) in events {
                    self.push(id.to_string(), obs, v);
                }
                return Some((
                    Outcome::Failed {
                        reason: FailureReason::RepeatedJitter,
                    },
                    self.metric_value(),
                ));
            }
        }

        // Settled: far enough onto the new tack, going forward again, and
        // both for long enough.
        let settling = s.phase == TackPhase::Crossed
            && signed >= c.settled_twa_rad
            && obs.state.u >= c.recover_speed_mps;
        let settled_s = s.settle.update(settling, obs.t());
        self.machine = Machine::Tack(s);
        for (id, v) in events {
            self.push(id.to_string(), obs, v);
        }
        if settling && settled_s >= c.settle_hold_s {
            self.push("settled".to_string(), obs, twa);
            return Some((Outcome::Succeeded, self.elapsed_s()));
        }
        None
    }

    fn step_heel(&mut self, obs: &StepObservation) -> Option<(Outcome, f64)> {
        let TaskSpec::RecoverFromHeel(c) = self.spec else {
            unreachable!("machine and spec are constructed together")
        };
        let Machine::Heel(ref state) = self.machine else {
            unreachable!("machine and spec are constructed together")
        };
        // A copy, because `HeelState` is `Copy`; `Machine` is not, so the read is
        // through a reference (see [`Machine`]).
        let mut s = *state;
        let heel = obs.state.phi.abs();
        s.peak_heel = s.peak_heel.max(heel);
        let mut events: Vec<(&str, f64)> = Vec::new();

        // The sheet was eased or released. `sheet_release` is the button and
        // the space bar; a positive `sheet_rate_cmd` is a drag. Either counts,
        // because both pay rope out (F4.3) — this records *that* it happened
        // and when, and claims nothing about why.
        if !s.eased
            && (obs.controls.sheet_release || obs.controls.sheet_rate_cmd > c.ease_command_min)
        {
            s.eased = true;
            events.push(("release", heel));
        }

        if !s.qualified && heel >= c.qualify_heel_rad {
            s.qualified = true;
            events.push(("heel_qualified", heel));
        }

        // A `heel_max` event per `heel_event_step_rad` of new peak, so the
        // last one is the peak and the list stays bounded and ordered by step.
        if s.qualified {
            let due = match s.last_event_heel {
                None => true,
                Some(last) => heel >= last + c.heel_event_step_rad,
            };
            if due {
                s.last_event_heel = Some(heel);
                events.push(("heel_max", heel));
            }
        }

        if !s.eased && heel >= c.late_release_heel_rad {
            self.machine = Machine::Heel(s);
            for (id, v) in events {
                self.push(id.to_string(), obs, v);
            }
            return Some((
                Outcome::Failed {
                    reason: FailureReason::LateRelease,
                },
                s.peak_heel,
            ));
        }

        let recovering = s.qualified && heel <= c.recover_heel_rad;
        let recovered_s = s.recover.update(recovering, obs.t());
        self.machine = Machine::Heel(s);
        for (id, v) in events {
            self.push(id.to_string(), obs, v);
        }
        if recovering && recovered_s >= c.recover_hold_s {
            self.push("heel_recovered".to_string(), obs, heel);
            return Some((Outcome::Succeeded, s.peak_heel));
        }
        None
    }

    /// The course machine: the tracker's passages, and `cut_between`'s cuts.
    ///
    /// Nothing here decides a passage or a cut. The tracker decides the first
    /// and `sailgym-course`'s own `cut_between` decides the second, which is
    /// what makes this evaluator and `sailgym-env`'s episode runner incapable
    /// of disagreeing about what a miss is (D2, RV66).
    fn step_course(&mut self, obs: &StepObservation) -> Option<(Outcome, f64)> {
        let origin_t = self.origin_t;
        let Machine::Course(ref mut state) = self.machine else {
            unreachable!("machine and spec are constructed together")
        };
        let (events, finished) = state.observe(obs, origin_t);
        for (id, waypoint) in events {
            self.push(id.to_string(), obs, f64::from(waypoint));
        }
        if finished {
            // The elapsed time **is** the metric, so it is computed here rather
            // than read back from a machine that has nothing else to say.
            return Some((course::FINISHED, obs.t() - origin_t));
        }
        None
    }

    // --- reporting ---------------------------------------------------------

    /// The headline metric's value as it stands.
    pub fn metric_value(&self) -> f64 {
        match &self.machine {
            Machine::GetMoving(s) => s.top_speed,
            // The tack's metric is how long the manoeuvre took, which only
            // exists once it has been completed.
            Machine::Tack(_) => {
                if self.outcome == Outcome::Succeeded {
                    self.elapsed_s()
                } else {
                    0.0
                }
            }
            Machine::Heel(s) => s.peak_heel,
            // Like the tack's: a course time only exists once the course has
            // been finished, and reporting the clock of an attempt that did not
            // would read as a result.
            Machine::Course(_) => {
                if self.outcome == Outcome::Succeeded {
                    self.elapsed_s()
                } else {
                    0.0
                }
            }
        }
    }

    fn compute_progress(&self, t: f64) -> Progress {
        if let Machine::Course(state) = &self.machine {
            return state.progress();
        }
        match (self.spec, self.machine.clone()) {
            (TaskSpec::GetMoving(c), Machine::GetMoving(s)) => Progress {
                phase: if s.reached { "holding" } else { "building" },
                value: s.top_speed,
                target: c.target_speed_mps,
                hold_s: s.hold.elapsed(t),
                hold_target_s: c.hold_s,
            },
            (TaskSpec::CompleteTack(c), Machine::Tack(s)) => Progress {
                phase: match s.phase {
                    TackPhase::Sailing => "sailing",
                    TackPhase::Approaching => "approaching",
                    TackPhase::Crossed => "crossed",
                },
                value: f64::from(s.reversals),
                target: f64::from(c.max_reversals),
                hold_s: s.settle.elapsed(t),
                hold_target_s: c.settle_hold_s,
            },
            (TaskSpec::RecoverFromHeel(c), Machine::Heel(s)) => Progress {
                phase: if s.eased { "recovering" } else { "heeling" },
                value: s.peak_heel,
                target: c.recover_heel_rad,
                hold_s: s.recover.elapsed(t),
                hold_target_s: c.recover_hold_s,
            },
            // Unreachable: the machine is built from the spec in `start`.
            _ => Progress {
                phase: "starting",
                value: 0.0,
                target: 0.0,
                hold_s: 0.0,
                hold_target_s: 0.0,
            },
        }
    }

    /// The event a result's **Inspect** action should jump to.
    ///
    /// The **last** event with the challenge's highlight id, which is what a
    /// skill wants: the peak heel, the moment the target speed was reached, the
    /// crossing.
    ///
    /// A course is the one exception, and the PRD states it: *Inspect jumps to
    /// the first miss, or the finish.* A player who cut waypoint 2 wants to see
    /// waypoint 2, not the last of four cuts; a player who sailed it clean
    /// wants the finish. So the course arm looks **forward** for a
    /// `waypoint_missed` and falls back to the terminal event.
    pub fn highlight(&self) -> Option<&PracticeEvent> {
        let id = self.spec.id().highlight_event();
        if self.spec.id().course().is_some() {
            return self
                .events
                .iter()
                .find(|e| e.id == id)
                .or_else(|| self.events.last());
        }
        self.events.iter().rev().find(|e| e.id == id)
    }

    /// s since the attempt began, one per waypoint passed, in order.
    ///
    /// Empty for a skill. The page shows them against the baseline's; it does
    /// not compute them (D5).
    pub fn splits(&self) -> &[f64] {
        match &self.machine {
            Machine::Course(s) => s.splits(),
            _ => &[],
        }
    }

    /// The course attempt's state, for a page that draws the overlay.
    pub fn course_state(&self) -> Option<&CourseState> {
        match &self.machine {
            Machine::Course(s) => Some(s),
            _ => None,
        }
    }

    /// Everything the browser is told, in one record.
    pub fn report(&self) -> TaskReport {
        let (metric_id, unit) = self.spec.metric();
        TaskReport {
            task: self.spec.identity(),
            outcome: self.outcome,
            scenario: self.spec.id().scenario().to_string(),
            elapsed_s: self.elapsed_s(),
            elapsed_steps: self.elapsed_steps(),
            metric: MetricRecord {
                id: metric_id.to_string(),
                unit: unit.to_string(),
                value: self.metric_value(),
            },
            progress: self.progress,
            events: self.events.clone(),
            highlight: self.highlight().cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(step: u64, t: f64, state: BoatState) -> StepObservation {
        StepObservation {
            step,
            state: BoatState { t, ..state },
            controls: Controls::default(),
            wind_world: Vec2::new(0.0, -5.0),
            capsized: false,
        }
    }

    #[test]
    fn every_shipped_configuration_validates() {
        for id in TASK_IDS {
            let spec = TaskSpec::shipped(id);
            spec.validate()
                .unwrap_or_else(|e| panic!("{}: {e}", id.as_str()));
            assert_eq!(spec.id(), id);
            assert_eq!(spec.version(), TASK_VERSION);
            assert!(!spec.thresholds().is_empty());
            assert_eq!(TaskId::parse(id.as_str()), Some(id));
        }
    }

    #[test]
    fn the_identity_carries_the_thresholds_it_was_built_from() {
        let spec = TaskSpec::shipped(TaskId::GetMoving);
        let identity = spec.identity();
        assert_eq!(identity.id, "get_moving");
        assert_eq!(identity.version, TASK_VERSION);
        assert_eq!(identity.thresholds, spec.thresholds());
        // A `BTreeMap`, so the serialised order is fixed (F9.3).
        let json = serde_json::to_string(&identity).expect("serialise");
        let first = json.find("backward_hold_s").expect("first key");
        let last = json.find("time_limit_s").expect("last key");
        assert!(first < last, "{json}");
    }

    #[test]
    fn a_wind_from_the_port_bow_is_a_negative_true_wind_angle() {
        // F2: psi = 0 points the bow along world +x. A wind blowing toward
        // −y (a northerly, blowing south) is then on the port beam, so the
        // FROM angle is −90°: the boat is on port tack (F2.1).
        let st = BoatState {
            psi: 0.0,
            ..BoatState::ZERO
        };
        let twa = true_wind_angle(&st, Vec2::new(0.0, -5.0));
        assert!((twa + std::f64::consts::FRAC_PI_2).abs() < 1e-12, "{twa}");
        // Mirror: a wind blowing toward +y is from starboard.
        let twa = true_wind_angle(&st, Vec2::new(0.0, 5.0));
        assert!((twa - std::f64::consts::FRAC_PI_2).abs() < 1e-12, "{twa}");
        // Air moving toward −x is air arriving over the bow: head to wind.
        assert!(true_wind_angle(&st, Vec2::new(-5.0, 0.0)).abs() < 1e-12);
        // Air moving toward +x is a following wind: dead astern.
        assert!(
            (true_wind_angle(&st, Vec2::new(5.0, 0.0)).abs() - std::f64::consts::PI).abs() < 1e-12
        );
        // No wind, no direction, and no invented one.
        assert_eq!(true_wind_angle(&st, Vec2::ZERO), 0.0);
    }

    #[test]
    fn a_terminal_run_ignores_every_later_step() {
        let spec = TaskSpec::shipped(TaskId::GetMoving);
        let mut run = TaskRun::start(spec, &obs(0, 0.0, BoatState::ZERO)).expect("start");
        let fast = BoatState {
            u: 2.0,
            ..BoatState::ZERO
        };
        for i in 1..=1400u64 {
            run.observe(&obs(i, i as f64 * 0.005, fast));
        }
        assert_eq!(run.outcome(), Outcome::Succeeded);
        let at_success = run.events().len();
        let elapsed = run.elapsed_s();
        // Twenty more steps, including a capsize, change nothing.
        for i in 1401..=1420u64 {
            let mut o = obs(i, i as f64 * 0.005, fast);
            o.capsized = true;
            assert_eq!(run.observe(&o), Outcome::Succeeded);
        }
        assert_eq!(run.events().len(), at_success);
        assert_eq!(run.elapsed_s(), elapsed);
    }

    #[test]
    fn a_rejected_configuration_names_the_field() {
        let bad = TaskSpec::GetMoving(GetMovingConfig {
            target_speed_mps: 1.0,
            // Above the target, so the hold could never break.
            release_speed_mps: 1.5,
            hold_s: 3.0,
            backward_speed_mps: 0.25,
            backward_hold_s: 2.0,
            time_limit_s: 45.0,
        });
        assert_eq!(bad.validate().unwrap_err().field, "release_speed_mps");
        assert!(TaskRun::start(bad, &obs(0, 0.0, BoatState::ZERO)).is_err());

        let nan = TaskSpec::RecoverFromHeel(RecoverHeelConfig {
            qualify_heel_rad: f64::NAN,
            ..match TaskSpec::shipped(TaskId::RecoverFromHeel) {
                TaskSpec::RecoverFromHeel(c) => c,
                _ => unreachable!(),
            }
        });
        assert_eq!(nan.validate().unwrap_err().field, "qualify_heel_rad");
    }

    #[test]
    fn the_envelope_is_the_reserved_one_and_starts_empty() {
        let spec = TaskSpec::shipped(TaskId::CompleteTack);
        let run = TaskRun::start(spec, &obs(0, 0.0, BoatState::ZERO)).expect("start");
        let envelope = run.envelope();
        assert_eq!(
            envelope.envelope_version,
            sailgym_physics::recording::PRACTICE_ENVELOPE_VERSION
        );
        assert_eq!(envelope.task, spec.identity());
        assert!(envelope.events.is_empty());
    }
}
