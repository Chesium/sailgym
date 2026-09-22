//! One independent episode: `Simulation + Route + Agent → reset / step`
//! (v2 section 06 task 6.2).
//!
//! # Independent means independent
//!
//! An [`Episode`] owns its `Simulation`, and therefore its wind field, its
//! seed, its clock, its F6.10 capsize accumulator and its reset state. It
//! shares nothing with any other episode, which is what makes stepping N of
//! them on N threads bit-identical to stepping them one after another
//! (F16.5). [`crate::vec_env::VecEnv`] is an ordered `Vec` of these and adds
//! no coupling.
//!
//! # One seed, named streams
//!
//! [`Episode::reset`] takes a single `u64`. The wind field derives from it
//! through `STREAM_WIND` inside `ProceduralWind::new`, the agent through
//! `STREAM_AGENT` and its sensors through their per-sensor substreams
//! (F14.8), and the **next** episode's seed after an autoreset is drawn from
//! `STREAM_SCENARIO` — the stream `rng.rs` has reserved for scenario
//! randomisation since v1. There is no second RNG and no wall clock
//! (F9.1, F9.2, F17.3).
//!
//! # The loop, and why it steps one at a time
//!
//! `Outcome` is evaluated after **every** physics step, so the loop is
//! `advance(1)` per step and not `advance(n)` per decision period. That is
//! deliberate and it is not free: `Simulation::advance` refreshes its cached
//! force breakdown once per call, so a per-step loop pays one extra force
//! evaluation per step against a batched one. It buys three things that
//! cannot be had otherwise — a capsize resolved on the step it happens, a
//! mark passage tested at the physics rate rather than at the sample rate
//! (section 04's own warning), and a step budget that truncates on the step
//! the budget names. Task 6.6 measures what it costs, on the real runner,
//! rather than leaving it to an argument.
//!
//! ```text
//!   decide if `episode_step % period_steps == 0`   ← F14.6, the episode's own counter
//!   sim.advance(1)
//!   tracker / missed-mark probe / task observer / recorder
//!   evaluate Outcome                               ← the fixed order in `outcome.rs`
//!   accumulate reward
//! ```
//!
//! # How a missed mark is detected without a second passage rule
//!
//! F15.3's rule has four clauses and lives in `sailgym-course`. "Cut the
//! mark" is exactly *three* of them: the boat crossed the mark's plane, in
//! the leg's direction, and failed the side-and-clearance clause. Rather
//! than write that out again here — RV20's mistake in a new place — the
//! episode keeps a **probe route**: the same marks, the same laps, every
//! `Rounding` replaced by [`Rounding::Either`](sailgym_course::Rounding).
//! `Either` is the course crate's own spelling of "directed plane crossing,
//! no side", so `passage::passed_between` on the probe answers the first two
//! clauses using the definition that already exists. A step on which the
//! probe passes and the real route does not is a mark cut.

use sailgym_agent::actuation::{apply, rate::Rate, Actuation};
use sailgym_agent::observation::ObsLayout;
use sailgym_agent::sensor::guidance::GuidanceSensor;
use sailgym_agent::sensor::imu::Imu;
use sailgym_agent::sensor::rig::{ActuatorState, RigState};
use sailgym_agent::sensor::wind::ApparentWind;
use sailgym_agent::sensor::{sensor_stream, Sensor};
use sailgym_agent::spec::{agent_rng, Agent, AgentSpec, SpecError};
use sailgym_agent::worldview::WorldView;
use sailgym_agent::Manual;

use sailgym_course::guidance::{guidance, CourseParams};
use sailgym_course::route::position;
use sailgym_course::{passage, Guidance, Mark, Progress, Rounding, Route, RouteError, Tracker};

use sailgym_physics::diagnostics::diagnostics;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::recording::{
    ActionIdentity, Episode as RecordedEpisode, EpisodeHeader, Recorded, Recorder,
};
use sailgym_physics::rng::{Pcg32, STREAM_SCENARIO};
use sailgym_physics::scenario::{Scenario, ScenarioError};
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::BoatState;
use sailgym_physics::vec::Vec2;

use sailgym_task::{Outcome as TaskOutcome, StepObservation, TaskRun, TaskSpec};

use crate::outcome::{AutoresetMode, ZeroReward};
use crate::outcome::{Bounds, BoundsError, Outcome, Reward, RewardContext, TerminationReason};
use crate::recording::{
    Decision, ResearchEnvelope, ResearchIdentity, RewardIdentity, RESEARCH_ENVELOPE_VERSION,
    RESEARCH_IDENTITY_VERSION,
};

/// The tier-0 sensor suite, in the order section 05 registered it.
pub const TIER0_SENSORS: [&str; 5] = [
    Imu::ID,
    ApparentWind::ID,
    RigState::ID,
    ActuatorState::ID,
    GuidanceSensor::ID,
];

/// Build one sensor by id, as a `Send` trait object.
///
/// **This is a second construction site for the tier-0 suite, and it is one
/// on purpose.** `SensorRegistry::resolve` returns `Box<dyn Sensor>`, which
/// is not `Send`, and `Box<dyn Sensor>` cannot be coerced to
/// `Box<dyn Sensor + Send>` after the fact — so an episode built through the
/// registry could not cross a rayon boundary, and F16.5's whole parallel path
/// would be unavailable. This section may not edit `sailgym-agent`
/// (F13.2), so the repair — `pub trait Sensor: Send` — is **reported in
/// `docs/v2/progress/06-handoff.md`, not made**.
///
/// It costs two more local walks, for the same reason:
/// `ObsLayout::of` and `observation::observe` both take
/// `&[Box<dyn Sensor>]` exactly, so [`obs_layout_of`] and [`observe_into`]
/// repeat their bodies over the `Send` suite. Neither is physics and
/// neither is a second rule — both are the *ordered concatenation* F14.3
/// defines — and the duplication is not left to a comment:
/// `tests::the_env_suite_is_the_registrys_suite` builds the same ids both
/// ways and asserts the two [`ObsLayout`]s and the two **observation
/// vectors** are equal, bit for bit, so a sensor added to the registry and
/// not here, or an `observe` that changed, fails the gate rather than
/// silently diverging.
fn make_sensor(id: &str) -> Option<Box<dyn Sensor + Send>> {
    match id {
        Imu::ID => Some(Box::new(Imu)),
        ApparentWind::ID => Some(Box::new(ApparentWind::new())),
        RigState::ID => Some(Box::new(RigState)),
        ActuatorState::ID => Some(Box::new(ActuatorState)),
        GuidanceSensor::ID => Some(Box::new(GuidanceSensor)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Everything an episode is built from.
///
/// Cloneable, because [`crate::vec_env::VecEnv`] builds N independent
/// episodes from one configuration. The clone is written out rather than
/// derived only because [`Reward`] is a trait object; every other field
/// derives.
pub struct EpisodeConfig {
    /// The initial condition and the environment (brief §32).
    pub scenario: Scenario,
    /// The configured sensor suite, in observation order (F14.3).
    pub sensors: Vec<String>,
    /// The course, or `None` for a free sail.
    pub route: Option<Route>,
    /// The course layer's own tunables. Not physical coefficients (F14.9).
    pub course: CourseParams,
    /// The sailing area.
    pub bounds: Bounds,
    /// The step budget, in **physics** steps. The only thing that ever
    /// produces [`Outcome::Truncated`]. A decision budget of `b` at cadence
    /// `p` is `Some(b * p)`.
    pub max_steps: Option<u64>,
    /// Which autoreset convention [`Episode::step`] applies.
    pub autoreset: AutoresetMode,
    /// The reward. [`ZeroReward`] unless an experiment configured one.
    pub reward: Box<dyn Reward>,
    /// A section 11 practice task, attached as an **observer**. It scores
    /// the attempt; it does not decide the episode boundary — see
    /// [`Episode::task_outcome`].
    pub task: Option<TaskSpec>,
    /// Hz, the sampled-recording rate, or `None` to record no frames. The
    /// **decision log is separate and always complete** (task 6.4, RV39).
    pub log_hz: Option<f64>,
    /// Whether to keep the decision log. `true` unless a throughput
    /// measurement asks otherwise.
    pub log_decisions: bool,
    /// Metadata for a recording's header; never read by physics (F9.1).
    pub created_utc: String,
}

impl Clone for EpisodeConfig {
    fn clone(&self) -> Self {
        Self {
            scenario: self.scenario.clone(),
            sensors: self.sensors.clone(),
            route: self.route.clone(),
            course: self.course,
            bounds: self.bounds,
            max_steps: self.max_steps,
            autoreset: self.autoreset,
            reward: self.reward.boxed_clone(),
            task: self.task,
            log_hz: self.log_hz,
            log_decisions: self.log_decisions,
            created_utc: self.created_utc.clone(),
        }
    }
}

impl std::fmt::Debug for EpisodeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpisodeConfig")
            .field("scenario", &self.scenario.name)
            .field("sensors", &self.sensors)
            .field("route", &self.route.as_ref().map(|r| r.marks.len()))
            .field("bounds", &self.bounds)
            .field("max_steps", &self.max_steps)
            .field("autoreset", &self.autoreset)
            .field("reward", &self.reward)
            .field("log_hz", &self.log_hz)
            .finish_non_exhaustive()
    }
}

impl EpisodeConfig {
    /// A free sail on `scenario` with the tier-0 suite, no route, no bounds,
    /// no budget, no reward and no recording.
    ///
    /// Every default is the one that invents nothing: `Bounds::Unbounded`,
    /// `max_steps: None`, `ZeroReward`. A configuration that wants a number
    /// supplies it.
    pub fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            sensors: TIER0_SENSORS.iter().map(|s| (*s).to_string()).collect(),
            route: None,
            course: CourseParams::default(),
            bounds: Bounds::Unbounded,
            max_steps: None,
            autoreset: AutoresetMode::default(),
            reward: Box::new(ZeroReward),
            task: None,
            log_hz: None,
            log_decisions: true,
            created_utc: String::new(),
        }
    }

    /// Reject a configuration no runner could honour, before anything is
    /// built. The whole configuration is checked, so a caller gets the first
    /// real problem and not the first symptom.
    pub fn validate(&self) -> Result<(), EnvError> {
        self.scenario.validate()?;
        self.bounds.validate()?;
        if let Some(route) = &self.route {
            route.validate()?;
        }
        if self.sensors.is_empty() {
            return Err(EnvError::NoSensors);
        }
        let mut seen: Vec<&str> = Vec::new();
        for id in &self.sensors {
            if seen.contains(&id.as_str()) {
                return Err(EnvError::DuplicateSensor(id.clone()));
            }
            seen.push(id);
            if make_sensor(id).is_none() {
                return Err(EnvError::UnknownSensor(id.clone()));
            }
        }
        if let Some(spec) = &self.task {
            spec.validate().map_err(|e| EnvError::Task(e.to_string()))?;
        }
        if self.max_steps == Some(0) {
            return Err(EnvError::ZeroBudget);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why an episode was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum EnvError {
    Scenario(ScenarioError),
    Route(RouteError),
    Bounds(BoundsError),
    Spec(SpecError),
    Action(String),
    Task(String),
    UnknownSensor(String),
    DuplicateSensor(String),
    NoSensors,
    ZeroBudget,
    /// [`Episode::step`] was called part-way through a decision period.
    NotAtDecisionBoundary {
        step: u64,
        period_steps: u32,
    },
    /// [`Episode::step`] was called on a terminal episode under
    /// [`AutoresetMode::Disabled`].
    Terminal(Outcome),
    /// An action was pushed into an episode driven by a policy.
    NotAnExternalSource,
}

impl std::fmt::Display for EnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scenario(e) => write!(f, "{e}"),
            Self::Route(e) => write!(f, "{e}"),
            Self::Bounds(e) => write!(f, "{e}"),
            Self::Spec(e) => write!(f, "{e}"),
            Self::Action(e) => write!(f, "{e}"),
            Self::Task(e) => write!(f, "practice task: {e}"),
            Self::UnknownSensor(id) => write!(f, "no sensor is registered as `{id}`"),
            Self::DuplicateSensor(id) => write!(
                f,
                "the sensor `{id}` is configured twice: two copies emit two identical column \
                 names and share one noise substream (F14.8)"
            ),
            Self::NoSensors => write!(f, "an episode with no sensors has no observation"),
            Self::ZeroBudget => write!(
                f,
                "max_steps = 0 truncates before the first step; use None for no budget"
            ),
            Self::NotAtDecisionBoundary { step, period_steps } => write!(
                f,
                "step() was called at episode step {step}, which is not a multiple of the \
                 cadence period {period_steps}: a decision period must start at a decision \
                 (F14.6)"
            ),
            Self::Terminal(o) => write!(
                f,
                "the episode ended ({}) and AutoresetMode::Disabled does not reset it",
                o.as_str()
            ),
            Self::NotAnExternalSource => write!(
                f,
                "this episode is driven by a policy; an action can only be pushed into one \
                 driven by `manual`"
            ),
        }
    }
}

impl std::error::Error for EnvError {}

impl From<ScenarioError> for EnvError {
    fn from(e: ScenarioError) -> Self {
        Self::Scenario(e)
    }
}
impl From<RouteError> for EnvError {
    fn from(e: RouteError) -> Self {
        Self::Route(e)
    }
}
impl From<BoundsError> for EnvError {
    fn from(e: BoundsError) -> Self {
        Self::Bounds(e)
    }
}
impl From<SpecError> for EnvError {
    fn from(e: SpecError) -> Self {
        Self::Spec(e)
    }
}

// ---------------------------------------------------------------------------
// The action source
// ---------------------------------------------------------------------------

/// Where an episode's actions come from.
///
/// Both arms are [`Agent`]s and both go through
/// [`sailgym_agent::actuation::apply`]: what differs is where the numbers
/// came from, which is legitimate, and not what happens to them afterwards,
/// which is not (RV27). The enum exists only because an external source has
/// to be *written into*, and `dyn Agent` has no way to say "and also, you may
/// push an action at me".
///
/// `Box<dyn Agent + Send>`: F16.5's parallel path needs the episode to cross
/// a thread boundary. `Agent` itself is not declared `Send` in
/// `sailgym-agent` and this section may not change it, so the bound is added
/// at the use site.
pub enum Source {
    /// A controller: a rule sailor, a policy, a test stub.
    Policy(Box<dyn Agent + Send>),
    /// An external action source — a human at a keyboard, or a batch API
    /// handing in a row of a `numpy` array.
    Manual(Manual),
}

impl Source {
    fn agent_mut(&mut self) -> &mut dyn Agent {
        match self {
            Self::Policy(a) => a.as_mut(),
            Self::Manual(m) => m,
        }
    }

    fn agent(&self) -> &dyn Agent {
        match self {
            Self::Policy(a) => a.as_ref(),
            Self::Manual(m) => m,
        }
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// What one decision period produced.
///
/// **There is no `done` field** (RV34): [`StepResult::terminated`] and
/// [`StepResult::truncated`] are separate, and a caller that wants their
/// union computes it and knows it has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepResult {
    /// The reward summed over the physics steps this call executed.
    pub reward: f64,
    /// The outcome those steps produced.
    pub outcome: Outcome,
    /// The episode ended for a **task** reason.
    pub terminated: bool,
    /// The episode ended because the **step budget** ran out.
    pub truncated: bool,
    /// Physics steps executed. Less than the cadence period when the episode
    /// ended part-way, and zero on an autoreset call.
    pub steps: u32,
    /// This call **was** the reset ([`AutoresetMode::NextStep`]) or
    /// **performed** one ([`AutoresetMode::SameStep`]).
    pub autoreset: bool,
    /// [`Episode::final_observation`] holds the observation the episode
    /// ended on, because [`Episode::observation`] is a reset observation.
    /// Only ever true under [`AutoresetMode::SameStep`].
    pub final_obs_valid: bool,
}

/// The episode an autoreset just ended.
///
/// Kept because under [`AutoresetMode::SameStep`] the reset happens inside
/// the same call that reports the termination, so a caller that only looked
/// at [`Episode::decisions`] afterwards would find the next episode's log.
#[derive(Clone, Debug, PartialEq)]
pub struct FinishedEpisode {
    /// The `u64` it was reset from.
    pub seed: u64,
    /// Its index within this slot's chain of episodes: `0` for the one
    /// [`Episode::reset`] started.
    pub index: u64,
    pub outcome: Outcome,
    pub executed_steps: u64,
    pub decisions: Vec<Decision>,
    /// The attached practice task's verdict, if one was attached.
    pub task_outcome: Option<TaskOutcome>,
}

// ---------------------------------------------------------------------------
// The episode
// ---------------------------------------------------------------------------

/// One independent episode.
pub struct Episode {
    config: EpisodeConfig,
    sim: Simulation,
    sensors: Vec<Box<dyn Sensor + Send>>,
    streams: Vec<Pcg32>,
    layout: ObsLayout,
    adapter: Rate,
    source: Source,
    spec: AgentSpec,
    /// The agent's own stream (F14.8).
    rng: Pcg32,
    /// `STREAM_SCENARIO`, which supplies the **next** episode's seed after an
    /// autoreset and nothing else.
    scenario_rng: Pcg32,
    tracker: Option<Tracker>,
    /// The same marks with every rounding replaced by `Either`: the course
    /// crate's own spelling of "crossed the plane, in the leg's direction".
    probe: Option<Route>,
    task: Option<TaskRun>,
    recorder: Option<Recorder>,
    /// The **episode** step counter. Not a per-`advance` counter, and not a
    /// clock (F14.6).
    step: u64,
    /// The seed this episode was begun from.
    seed: u64,
    /// The seed the whole chain was reset from.
    root_seed: u64,
    /// This episode's index in the chain.
    index: u64,
    outcome: Outcome,
    missed: bool,
    prev_pos: Vec2,
    obs: Vec<f64>,
    obs_step: Option<u64>,
    final_obs: Vec<f64>,
    /// The decision log, flat: `log_steps[i]` is the episode step and
    /// `log_actions[i*dim .. (i+1)*dim]` the normalised action. Flat so a
    /// decision costs no allocation (task 6.4).
    log_steps: Vec<u64>,
    log_actions: Vec<f64>,
    reward_acc: f64,
    pending_reset: bool,
    finished: Option<FinishedEpisode>,
}

impl Episode {
    /// Build an episode and reset it from `seed`.
    pub fn new(config: EpisodeConfig, source: Source, seed: u64) -> Result<Self, EnvError> {
        config.validate()?;
        let spec = source.agent().spec();
        spec.validate()?;

        let mut sensors: Vec<Box<dyn Sensor + Send>> = Vec::with_capacity(config.sensors.len());
        for id in &config.sensors {
            sensors.push(make_sensor(id).ok_or_else(|| EnvError::UnknownSensor(id.clone()))?);
        }
        let layout = obs_layout_of(&sensors);

        let params: BoatParameters = config.scenario.to_parameters()?;
        let sim = Simulation::new(params, seed);
        let probe = config.route.as_ref().map(probe_route);

        let mut ep = Self {
            config,
            sim,
            sensors,
            streams: Vec::new(),
            layout,
            adapter: Rate,
            source,
            spec,
            rng: Pcg32::seed_from_u64(seed),
            scenario_rng: Pcg32::seed_from_u64(seed).stream(STREAM_SCENARIO),
            tracker: None,
            probe,
            task: None,
            recorder: None,
            step: 0,
            seed,
            root_seed: seed,
            index: 0,
            outcome: Outcome::Running,
            missed: false,
            prev_pos: Vec2::ZERO,
            obs: Vec::new(),
            obs_step: None,
            final_obs: Vec::new(),
            log_steps: Vec::new(),
            log_actions: Vec::new(),
            reward_acc: 0.0,
            pending_reset: false,
            finished: None,
        };
        ep.reset(seed)?;
        Ok(ep)
    }

    /// Restart the whole chain from `seed`.
    ///
    /// Everything procedural derives from this one `u64`: the wind field
    /// through `STREAM_WIND`, the agent through `STREAM_AGENT`, and the next
    /// episode's seed through `STREAM_SCENARIO` (F17.3).
    pub fn reset(&mut self, seed: u64) -> Result<(), EnvError> {
        self.root_seed = seed;
        self.scenario_rng = Pcg32::seed_from_u64(seed).stream(STREAM_SCENARIO);
        self.index = 0;
        self.finished = None;
        self.final_obs.clear();
        self.begin(seed)
    }

    /// Begin one episode of the chain from `seed`.
    fn begin(&mut self, seed: u64) -> Result<(), EnvError> {
        self.seed = seed;
        let mut sc = self.config.scenario.clone();
        sc.seed = seed;
        self.sim.load_scenario(&sc)?;

        let initial = *self.sim.state();
        self.tracker = match &self.config.route {
            Some(r) => Some(Tracker::start(r.clone(), &initial)?),
            None => None,
        };
        self.task = match &self.config.task {
            Some(spec) => Some(
                TaskRun::start(*spec, &StepObservation::of(&self.sim))
                    .map_err(|e| EnvError::Task(e.to_string()))?,
            ),
            None => None,
        };
        let action_identity = self.action_identity();
        let observation_identity = self.layout.to_identity();
        let params = *self.sim.params();
        let controls = *self.sim.controls();
        let created_utc = self.config.created_utc.clone();
        self.recorder = self.config.log_hz.map(|hz| {
            let mut header = EpisodeHeader::manual(
                sc.clone(),
                params,
                hz,
                created_utc.clone(),
                initial,
                controls,
            );
            // `EpisodeHeader::manual` marks the research slots
            // `NotApplicable`, which is right for a hand-flown browser run
            // and wrong here: this episode **has** an adapter and an
            // observation layout, and saying so is what lets two runs be
            // compared (F18.3). The fields are section 10's; they are
            // filled, never widened.
            header.action = Recorded::Value(action_identity.clone());
            header.observation = Recorded::Value(observation_identity.clone());
            Recorder::start(hz, header)
        });

        self.rng = agent_rng(&Pcg32::seed_from_u64(seed));
        // `observation::sensor_streams` takes `&[Box<dyn Sensor>]`; this is
        // the same one-line map over the `Send` suite, and it calls the
        // same `sensor_stream` (F14.8), so the substreams are identical.
        self.streams = self
            .sensors
            .iter()
            .map(|s| sensor_stream(&self.rng, s.id()))
            .collect();
        self.adapter.reset();
        self.config.reward.reset();
        let names = self.layout.names();
        self.source.agent_mut().reset(&names, &mut self.rng);

        self.step = 0;
        self.outcome = Outcome::Running;
        self.missed = false;
        self.prev_pos = position(&initial);
        self.obs.clear();
        self.obs_step = None;
        // `final_obs` is **not** cleared here. It belongs to `step`, which
        // fills it from the observation the ending episode finished on and
        // then calls this function: under `AutoresetMode::SameStep` the
        // reset happens inside the very call that reports the final
        // observation, so clearing it here would hand the caller an empty
        // buffer on exactly the step it asked for one. `reset` clears it,
        // because a fresh chain has no final observation yet.
        self.log_steps.clear();
        self.log_actions.clear();
        self.reward_acc = 0.0;
        self.pending_reset = false;
        Ok(())
    }

    // -- accessors ---------------------------------------------------------

    pub fn config(&self) -> &EpisodeConfig {
        &self.config
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// Physics steps executed in the current episode.
    pub fn steps(&self) -> u64 {
        self.step
    }

    /// The seed the current episode was begun from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The seed the chain was reset from.
    pub fn root_seed(&self) -> u64 {
        self.root_seed
    }

    /// The current episode's index in the chain; `0` until the first
    /// autoreset.
    pub fn episode_index(&self) -> u64 {
        self.index
    }

    pub fn state(&self) -> &BoatState {
        self.sim.state()
    }

    pub fn params(&self) -> &BoatParameters {
        self.sim.params()
    }

    pub fn simulation(&self) -> &Simulation {
        &self.sim
    }

    pub fn layout(&self) -> &ObsLayout {
        &self.layout
    }

    pub fn agent_spec(&self) -> &AgentSpec {
        &self.spec
    }

    /// How many scalars an action carries.
    pub fn action_dim(&self) -> usize {
        self.adapter.dim()
    }

    /// The episode an autoreset last ended, if one has.
    pub fn finished(&self) -> Option<&FinishedEpisode> {
        self.finished.as_ref()
    }

    /// The attached practice task's verdict, if one is attached.
    ///
    /// The task is an **observer** (section 11's discipline, and F6.10's):
    /// it scores the attempt and it does **not** decide the episode
    /// boundary. Conflating the two would give `sailgym-task` and
    /// `sailgym-env` two different opinions about when an episode ended,
    /// which is the disagreement F17.4 exists to prevent.
    pub fn task_outcome(&self) -> Option<TaskOutcome> {
        self.task.as_ref().map(TaskRun::outcome)
    }

    /// The practice events the attached task recorded, in order.
    pub fn task_events(&self) -> &[sailgym_physics::recording::PracticeEvent] {
        match &self.task {
            Some(t) => t.events(),
            None => &[],
        }
    }

    /// Where the boat has got to on the route.
    pub fn progress(&self) -> Option<Progress> {
        self.tracker.as_ref().map(|t| t.progress(self.sim.state()))
    }

    /// What the course layer says the boat is being asked to sail.
    pub fn guidance(&self) -> Option<Guidance> {
        let t = self.tracker.as_ref()?;
        guidance(
            t.route(),
            t.leg_index(),
            self.sim.state(),
            &self.config.course,
        )
    }

    /// The decision log, as records.
    ///
    /// Built on demand from the flat store, so a decision costs no
    /// allocation while the episode runs.
    pub fn decisions(&self) -> Vec<Decision> {
        let dim = self.adapter.dim();
        self.log_steps
            .iter()
            .enumerate()
            .map(|(i, step)| Decision {
                step: *step,
                action: self.log_actions[i * dim..(i + 1) * dim].to_vec(),
            })
            .collect()
    }

    /// How many decisions the current episode has taken.
    pub fn decision_count(&self) -> usize {
        self.log_steps.len()
    }

    /// The observation at the current episode step.
    ///
    /// Computed once per episode step and cached on the **step**, never on
    /// the call, so asking twice costs one evaluation and — when a noise
    /// model lands — draws one set of samples (F14.8).
    pub fn observation(&mut self) -> &[f64] {
        self.refresh_observation();
        &self.obs
    }

    /// The observation the episode ended on, when
    /// [`StepResult::final_obs_valid`] says so.
    pub fn final_observation(&self) -> &[f64] {
        &self.final_obs
    }

    /// The adapter half of the identity (section 10, F16.4).
    pub fn action_identity(&self) -> ActionIdentity {
        sailgym_agent::actuation::action_identity(&self.adapter, self.spec.cadence)
    }

    /// The research half of this episode's identity.
    pub fn research_identity(&self) -> ResearchIdentity {
        ResearchIdentity {
            identity_version: RESEARCH_IDENTITY_VERSION,
            agent: Recorded::Value(self.spec.clone()),
            action: Recorded::Value(self.action_identity()),
            observation: Recorded::Value(self.layout.to_identity()),
            obs_layout: Recorded::Value(self.layout.clone()),
            autoreset: Recorded::Value(self.config.autoreset),
            route: match &self.config.route {
                Some(r) => Recorded::Value(r.clone()),
                None => Recorded::NotApplicable,
            },
            bounds: Recorded::Value(self.config.bounds),
            max_steps: match self.config.max_steps {
                Some(m) => Recorded::Value(m),
                None => Recorded::NotApplicable,
            },
            reward: Recorded::Value(RewardIdentity::of(self.config.reward.as_ref())),
        }
    }

    /// The research envelope for this episode, taking the recording with it.
    ///
    /// It **takes** the recorder, because `Recorder::finish` consumes it and
    /// this crate may not add a borrowing accessor to section 10's type
    /// (F13.2). Recording therefore stops here, which is the right shape:
    /// an envelope is what an episode produced, and an episode that has
    /// produced it is over. A caller that records **and** autoresets must
    /// take the envelope before the next episode begins, so recording is
    /// meant to be used with [`AutoresetMode::Disabled`]; `begin` drops a
    /// recorder that was not taken.
    pub fn take_envelope(&mut self) -> Option<ResearchEnvelope> {
        let identity = self.research_identity();
        let decisions = if self.config.log_decisions {
            Recorded::Value(self.decisions())
        } else {
            Recorded::NotApplicable
        };
        let recorded: RecordedEpisode = self.recorder.take().map(Recorder::finish)?;
        Some(ResearchEnvelope {
            envelope_version: RESEARCH_ENVELOPE_VERSION,
            research: identity,
            outcome: Recorded::Value(self.outcome),
            decisions,
            executed_steps: Recorded::Value(self.step),
            seed: Recorded::Value(self.seed),
            recording: recorded,
        })
    }

    /// Push the action an external source wants applied from the next
    /// decision onward.
    ///
    /// Refused unless the source is [`Source::Manual`]: an action pushed at
    /// a policy would be an action the policy did not take, and the log
    /// would say otherwise.
    pub fn push_action(&mut self, values: &[f64]) -> Result<(), EnvError> {
        match &mut self.source {
            Source::Manual(m) => Ok(m.set_action(values)?),
            Source::Policy(_) => Err(EnvError::NotAnExternalSource),
        }
    }

    // -- the loop ----------------------------------------------------------

    /// Recompute the observation if it is not already the one for this
    /// episode step.
    ///
    /// Not named `observe_now`: `no_wall_clock_in_the_env_crate` runs the
    /// **same four needles** the physics and agent crates run, and one of
    /// them is `now(`. Keeping the grep identical across the three crates
    /// is worth more than the shorter name (F14.8).
    fn refresh_observation(&mut self) {
        if self.obs_step == Some(self.step) {
            return;
        }
        let st = *self.sim.state();
        let controls = *self.sim.controls();
        let params = *self.sim.params();
        let g = self.guidance();
        let view = WorldView {
            st: &st,
            controls: &controls,
            p: &params,
            wind: self.sim.wind(),
            guidance: g.as_ref(),
            // Present and empty. There is no boat-to-boat interaction of any
            // kind in this crate (`docs/v2/brief.md` §3).
            others: &[],
            t: st.t,
        };
        observe_into(&mut self.sensors, &mut self.streams, &view, &mut self.obs);
        self.obs_step = Some(self.step);
    }

    /// One decision at the current episode step.
    fn decide(&mut self) -> Result<(), EnvError> {
        self.refresh_observation();
        let action = self.source.agent_mut().decide(&self.obs, &mut self.rng);
        let st = *self.sim.state();
        let params = *self.sim.params();
        let controls = apply(&mut self.adapter, &action, &st, &params)
            .map_err(|e| EnvError::Action(e.to_string()))?;
        if self.config.log_decisions {
            self.log_steps.push(self.step);
            self.log_actions.extend_from_slice(action.values());
        }
        self.sim.set_controls(controls);
        Ok(())
    }

    /// Advance up to `n` physics steps, deciding at the episode's own
    /// cadence and evaluating [`Outcome`] after every one.
    ///
    /// Returns the steps actually taken, which is fewer than `n` when the
    /// episode ended part-way and zero when it had already ended. Where the
    /// decisions land is a property of the episode and **never** of `n`
    /// (F14.6), which is what makes `advance(n) == n × advance(1)` hold with
    /// an agent attached (F9.7).
    pub fn advance(&mut self, n: u32) -> Result<u32, EnvError> {
        if self.outcome.is_terminal() {
            return Ok(0);
        }
        let mut taken = 0u32;
        while taken < n {
            if self.spec.cadence.decides_at(self.step) {
                self.decide()?;
            }
            let prev = *self.sim.state();
            self.sim.advance(1);
            self.step += 1;
            taken += 1;
            let st = *self.sim.state();

            // The course layer, at the physics step rate.
            if let Some(tracker) = &mut self.tracker {
                let leg = tracker.leg_index();
                let passed = tracker.observe(&st).is_some();
                if !passed {
                    if let Some(probe) = &self.probe {
                        // The probe answers the plane-and-direction clauses
                        // with the course crate's own rule; the real route
                        // answered all four and said no. The difference is a
                        // cut mark (F15.3).
                        if passage::passed_between(probe, leg, self.prev_pos, position(&st)) {
                            self.missed = true;
                        }
                    }
                }
            }
            self.prev_pos = position(&st);

            if let Some(task) = &mut self.task {
                task.observe(&StepObservation::of(&self.sim));
            }
            if let Some(rec) = &mut self.recorder {
                if rec.due(st.t) {
                    let d = diagnostics(&self.sim);
                    rec.observe(&self.sim, &d);
                }
            }

            let outcome = self.evaluate_outcome(&st);
            let g = self.guidance();
            let progress = self.progress();
            let ctx = RewardContext {
                st: &st,
                prev: &prev,
                controls: self.sim.controls(),
                guidance: g.as_ref(),
                progress: progress.as_ref(),
                outcome,
                dt: self.sim.params().sim.dt,
                step: self.step,
            };
            self.reward_acc += self.config.reward.value(&ctx);

            if outcome.is_terminal() {
                self.outcome = outcome;
                break;
            }
        }
        Ok(taken)
    }

    /// The fixed order of `outcome.rs`, applied to the state just published.
    fn evaluate_outcome(&self, st: &BoatState) -> Outcome {
        // 1. F6.10's accumulator, **read** and never recomputed (RV36).
        if self.sim.capsize().capsized {
            return Outcome::Terminated(TerminationReason::Capsized);
        }
        // 2. The sailing area.
        if !self.config.bounds.contains(position(st)) {
            return Outcome::Terminated(TerminationReason::OutOfBounds);
        }
        // 3. A mark cut.
        if self.missed {
            return Outcome::Terminated(TerminationReason::MarkMissed);
        }
        // 4. The route completed.
        if let Some(tracker) = &self.tracker {
            if tracker.finished() {
                return Outcome::Finished { time: st.t };
            }
        }
        // 5. The step budget, and nothing else, truncates.
        if let Some(max) = self.config.max_steps {
            if self.step >= max {
                return Outcome::Truncated;
            }
        }
        Outcome::Running
    }

    /// One decision period, with the configured autoreset convention
    /// applied.
    ///
    /// This is the batch API's unit of work and the unit the returns test
    /// reasons in. It must be called **at** a decision boundary: a period
    /// that started half-way through one would put the decisions somewhere
    /// the cadence did not.
    pub fn step(&mut self) -> Result<StepResult, EnvError> {
        if self.pending_reset {
            // `AutoresetMode::NextStep`: this call ignores its action,
            // resets, and reports the reset observation with reward 0 and
            // both flags clear. Gymnasium 1.3.0,
            // `gymnasium/vector/sync_vector_env.py:252-266`.
            let seed = self.next_seed();
            self.index += 1;
            self.begin(seed)?;
            self.refresh_observation();
            return Ok(StepResult {
                reward: 0.0,
                outcome: Outcome::Running,
                terminated: false,
                truncated: false,
                steps: 0,
                autoreset: true,
                final_obs_valid: false,
            });
        }
        if self.outcome.is_terminal() {
            return Err(EnvError::Terminal(self.outcome));
        }
        let period = self.spec.cadence.period_steps;
        if !self.spec.cadence.decides_at(self.step) {
            return Err(EnvError::NotAtDecisionBoundary {
                step: self.step,
                period_steps: period,
            });
        }

        self.reward_acc = 0.0;
        let steps = self.advance(period)?;
        // Read **before** any autoreset below. `begin` clears
        // `reward_acc`, so under `AutoresetMode::SameStep` — where the
        // reset happens inside this very call — reading it afterwards
        // returns 0 and every terminal reward silently vanishes from the
        // returns. `tests/returns.rs` is what found that, which is the
        // whole reason it exists (RV33).
        let reward = self.reward_acc;
        let outcome = self.outcome;
        self.refresh_observation();

        let mut final_obs_valid = false;
        let mut autoreset = false;
        if outcome.is_terminal() {
            match self.config.autoreset {
                AutoresetMode::NextStep => self.pending_reset = true,
                AutoresetMode::SameStep => {
                    self.final_obs.clear();
                    self.final_obs.extend_from_slice(&self.obs);
                    final_obs_valid = true;
                    autoreset = true;
                    let seed = self.next_seed();
                    self.index += 1;
                    self.begin(seed)?;
                    self.refresh_observation();
                }
                AutoresetMode::Disabled => {}
            }
        }
        Ok(StepResult {
            reward,
            outcome,
            terminated: outcome.terminated(),
            truncated: outcome.truncated(),
            steps,
            autoreset,
            final_obs_valid,
        })
    }

    /// The next episode's seed, drawn from `STREAM_SCENARIO`.
    ///
    /// The stream `rng.rs` reserved for scenario randomisation, used for
    /// exactly that: there is no second RNG, no counter added to the seed
    /// (which would collide between adjacent `VecEnv` slots) and no clock.
    /// The draw also records the ending episode, because under
    /// [`AutoresetMode::SameStep`] this is the last moment it exists.
    fn next_seed(&mut self) -> u64 {
        self.finished = Some(FinishedEpisode {
            seed: self.seed,
            index: self.index,
            outcome: self.outcome,
            executed_steps: self.step,
            decisions: self.decisions(),
            task_outcome: self.task_outcome(),
        });
        let hi = u64::from(self.scenario_rng.next_u32());
        let lo = u64::from(self.scenario_rng.next_u32());
        (hi << 32) | lo
    }
}

/// The layout of a `Send` suite. [`ObsLayout::of`] takes
/// `&[Box<dyn Sensor>]`, which this suite is not, so the same walk is done
/// here — and `tests::the_env_suite_is_the_registrys_suite` asserts the
/// result is identical to the registry's.
fn obs_layout_of(sensors: &[Box<dyn Sensor + Send>]) -> ObsLayout {
    use sailgym_agent::observation::{ObsColumn, LAYOUT_VERSION};
    let mut columns = Vec::new();
    for s in sensors {
        let fields = s.fields();
        assert_eq!(
            fields.len(),
            s.width(),
            "sensor `{}` declares width {} and {} fields",
            s.id(),
            s.width(),
            fields.len()
        );
        for (index_in_sensor, field) in fields.into_iter().enumerate() {
            columns.push(ObsColumn {
                sensor: s.id().to_string(),
                sensor_version: s.version(),
                index_in_sensor,
                field,
            });
        }
    }
    ObsLayout {
        layout_version: LAYOUT_VERSION,
        columns,
    }
}

/// Fill `out` with the ordered concatenation of the suite's outputs.
///
/// `observation::observe`'s body, over a `Send` suite. See [`make_sensor`].
fn observe_into(
    sensors: &mut [Box<dyn Sensor + Send>],
    streams: &mut [Pcg32],
    view: &WorldView,
    out: &mut Vec<f64>,
) {
    assert_eq!(
        sensors.len(),
        streams.len(),
        "one RNG substream per sensor (F14.8)"
    );
    let total: usize = sensors.iter().map(|s| s.width()).sum();
    out.clear();
    out.resize(total, 0.0);
    let mut at = 0usize;
    for (s, rng) in sensors.iter_mut().zip(streams.iter_mut()) {
        let w = s.width();
        s.sense(view, rng, &mut out[at..at + w]);
        at += w;
    }
    debug_assert_eq!(at, total);
}

/// The same course with every rounding replaced by
/// [`Rounding::Either`](sailgym_course::Rounding).
///
/// Built once, at construction, so the missed-mark test costs no
/// allocation. One case is approximate and is recorded rather than hidden:
/// a [`Rounding::Gate`](sailgym_course::Rounding) is a crossing of the
/// segment between its posts, and its `Either` probe is the plane through
/// the **mark's own position** perpendicular to the leg. For a gate square
/// to the leg and centred on its mark those coincide, and passing outside
/// the posts is a cut either way; for an oblique gate the two can differ by
/// a step. A route with no leg direction — F15.1's lookahead point — has no
/// plane at all, so its probe is its own disc and a miss can never fire,
/// which is correct: a mark with no side cannot be cut.
fn probe_route(route: &Route) -> Route {
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

/// The one shipped external-source episode: `manual`, at the given cadence.
pub fn manual_source(cadence: sailgym_agent::spec::Cadence) -> Source {
    Source::Manual(Manual::new(cadence, Rate::DIM))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sailgym_agent::observation::{observe, sensor_streams};
    use sailgym_agent::sensor::SensorRegistry;
    use sailgym_agent::spec::{Action, ActionSpace, ActionVec, Cadence};
    use sailgym_physics::scenario::load_shipped;
    use sailgym_physics::state::{STATE_FIELDS, STATE_LEN};
    use std::path::{Path, PathBuf};

    // -----------------------------------------------------------------
    // A stub policy that actually reads its observation
    // -----------------------------------------------------------------

    /// It steers on the sway acceleration and trims on the apparent wind
    /// speed, so it is sensitive to exactly the quantity F14.7's trap is
    /// about: a stale force cache would change `imu.accel_sway`, which
    /// would change the action, which would change the trajectory. A policy
    /// that ignored its observation would make the F9.7 test vacuous. It
    /// also draws from its `Pcg32` every decision, so the number of draws is
    /// part of what the chunking test compares.
    struct Stub {
        cadence: Cadence,
        accel_sway: usize,
        aws: usize,
    }

    impl Stub {
        fn boxed(period: u32) -> Source {
            Source::Policy(Box::new(Self {
                cadence: Cadence::new(period),
                accel_sway: 0,
                aws: 0,
            }))
        }
    }

    impl Agent for Stub {
        fn spec(&self) -> AgentSpec {
            AgentSpec::new("stub", 1, ActionSpace::Rates, self.cadence)
        }

        fn reset(&mut self, fields: &[String], _rng: &mut Pcg32) {
            let find = |name: &str| {
                fields
                    .iter()
                    .position(|f| f == name)
                    .unwrap_or_else(|| panic!("no column `{name}` in {fields:?}"))
            };
            self.accel_sway = find("imu.accel_sway");
            self.aws = find("apparent_wind.aws");
        }

        fn decide(&mut self, obs: &[f64], rng: &mut Pcg32) -> Action {
            let jitter = 0.01 * (rng.next_f64() - 0.5);
            let rudder = (-0.5 * obs[self.accel_sway] + jitter).clamp(-1.0, 1.0);
            let sheet = (0.2 * obs[self.aws] - 0.6).clamp(-1.0, 1.0);
            Action::Rates(ActionVec::new(&[rudder, sheet, -1.0]).expect("clamped"))
        }
    }

    fn config(scenario: &str) -> EpisodeConfig {
        EpisodeConfig::new(load_shipped(scenario).expect("a shipped scenario"))
    }

    fn state_of(ep: &Episode) -> [f64; STATE_LEN] {
        ep.state().to_array()
    }

    fn assert_same_state(a: &[f64; STATE_LEN], b: &[f64; STATE_LEN], what: &str) {
        for i in 0..STATE_LEN {
            assert_eq!(
                a[i].to_bits(),
                b[i].to_bits(),
                "{what}: field {} — {} vs {}",
                STATE_FIELDS[i],
                a[i],
                b[i]
            );
        }
    }

    /// Run `total` steps in chunks of `chunk`, returning the final state and
    /// the decision log.
    fn run(
        cfg: &EpisodeConfig,
        source: Source,
        seed: u64,
        total: u32,
        chunk: u32,
    ) -> ([f64; STATE_LEN], Vec<Decision>) {
        let mut ep = Episode::new(cfg.clone(), source, seed).expect("a valid episode");
        let mut done = 0u32;
        while done < total {
            let n = chunk.min(total - done);
            let took = ep.advance(n).expect("a valid action");
            done += n;
            if took < n {
                break;
            }
        }
        (state_of(&ep), ep.decisions())
    }

    // -----------------------------------------------------------------
    // 1. F9.7 with an agent attached, in `sailgym-env`
    // -----------------------------------------------------------------

    /// Section acceptance 4 and task 6.2's first criterion: `advance(n)`
    /// equals `n × advance(1)`, bit for bit, over the **same six chunkings
    /// section 05 used**.
    ///
    /// Demonstrated able to fail: `docs/v2/progress/06-handoff.md` §5
    /// records the run in which the cadence was keyed off a per-call
    /// counter and this test went red.
    #[test]
    fn advance_n_equals_n_advance_1_with_an_agent_attached() {
        const CHUNKS: [u32; 6] = [1, 3, 7, 10, 13, 200];
        let total = 3000u32;
        let cfg = config("close_hauled");

        let (reference, log) = run(&cfg, Stub::boxed(10), 7, total, 1);
        assert!(
            reference[0].abs() + reference[1].abs() > 1.0,
            "the boat never moved, so the comparison proves nothing"
        );
        assert_eq!(log.len(), (total / 10) as usize);

        for chunk in CHUNKS {
            let (state, chunked) = run(&cfg, Stub::boxed(10), 7, total, chunk);
            assert_same_state(&reference, &state, &format!("stub, chunk {chunk}"));
            assert_eq!(
                chunked, log,
                "chunk {chunk}: the decisions themselves moved"
            );
        }

        // `manual`, at the degenerate cadence a browser uses. The latch is
        // pushed after the episode is built, because a reset returns it to
        // hands-off.
        let free = config("free_sail");
        let manual_run = |chunk: u32| -> [f64; STATE_LEN] {
            let mut ep = Episode::new(free.clone(), manual_source(Cadence::EVERY_STEP), 11)
                .expect("a valid episode");
            ep.push_action(&[0.3, -1.0, -1.0]).expect("in bounds");
            let mut done = 0u32;
            while done < total {
                let n = chunk.min(total - done);
                ep.advance(n).expect("a valid action");
                done += n;
            }
            state_of(&ep)
        };
        let single = manual_run(1);
        assert!(
            single[0].abs() + single[1].abs() > 1.0,
            "manual never moved"
        );
        for chunk in CHUNKS {
            assert_same_state(
                &manual_run(chunk),
                &single,
                &format!("manual, chunk {chunk}"),
            );
        }
    }

    /// F14.6.1: decisions land on the **episode's** step multiples, whatever
    /// the caller's chunk size (RV26).
    #[test]
    fn cadence_lands_on_episode_steps_whatever_the_chunking() {
        let total = 1000u32;
        let cfg = config("tack");
        for period in [7u32, 10] {
            let (_, log) = run(&cfg, Stub::boxed(period), 3, total, 1);
            let steps: Vec<u64> = log.iter().map(|d| d.step).collect();
            let want: Vec<u64> = (0..u64::from(total)).step_by(period as usize).collect();
            assert_eq!(steps, want, "period {period}");
            for chunk in [1u32, 3, 7, 10, 13, 200] {
                let (_, chunked) = run(&cfg, Stub::boxed(period), 3, total, chunk);
                assert_eq!(
                    chunked.iter().map(|d| d.step).collect::<Vec<_>>(),
                    want,
                    "period {period}, chunk {chunk}: the decisions moved with the chunk size"
                );
            }
        }
    }

    /// Task 6.2's second criterion: a fixed seed reproduces a trajectory
    /// bit-for-bit **across two processes**.
    ///
    /// The child is this same test binary, re-executed with the marker
    /// environment variable set, so the two runs share no memory, no
    /// allocator state and no address-space layout.
    #[test]
    fn a_fixed_seed_reproduces_a_trajectory_across_two_processes() {
        const MARKER: &str = "SAILGYM_ENV_CROSS_PROCESS";
        const LINE: &str = "trajectory-bits:";

        fn bits() -> String {
            let cfg = config("gybe");
            let (state, log) = run(&cfg, Stub::boxed(10), 20_260_922, 2000, 37);
            let mut out = String::new();
            for v in state {
                out.push_str(&format!("{:016x},", v.to_bits()));
            }
            out.push('|');
            for d in &log {
                out.push_str(&format!("{}:", d.step));
                for v in &d.action {
                    out.push_str(&format!("{:016x},", v.to_bits()));
                }
            }
            out
        }

        if std::env::var(MARKER).is_ok() {
            println!("{LINE}{}", bits());
            return;
        }

        let mine = bits();
        let Ok(exe) = std::env::current_exe() else {
            eprintln!("skip: no current_exe on this host");
            return;
        };
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                "--nocapture",
                "episode::tests::a_fixed_seed_reproduces_a_trajectory_across_two_processes",
            ])
            .env(MARKER, "1")
            .output();
        let Ok(out) = out else {
            eprintln!("skip: the test binary is not re-runnable here");
            return;
        };
        assert!(
            out.status.success(),
            "the child run failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let theirs = stdout
            .lines()
            .find_map(|l| l.strip_prefix(LINE))
            .unwrap_or_else(|| panic!("the child printed no trajectory:\n{stdout}"));
        assert_eq!(mine, theirs, "two processes disagreed about one seed");
        assert!(mine.contains('|') && mine.len() > 64, "the digest is empty");
    }

    // -----------------------------------------------------------------
    // 2. Outcome transitions, one per reason
    // -----------------------------------------------------------------

    /// Run to a terminal outcome, or give up after `limit` steps.
    fn run_to_end(cfg: EpisodeConfig, source: Source, seed: u64, limit: u32) -> Episode {
        let mut ep = Episode::new(cfg, source, seed).expect("a valid episode");
        let mut done = 0u32;
        while done < limit && !ep.outcome().is_terminal() {
            let n = 100.min(limit - done);
            done += ep.advance(n).expect("a valid action");
            if ep.outcome().is_terminal() {
                break;
            }
        }
        ep
    }

    /// Where a hands-off `free_sail` boat actually goes, so the route tests
    /// place their marks on the track rather than on a guess.
    fn free_sail_track(steps: u32) -> Vec<Vec2> {
        let mut ep = Episode::new(config("free_sail"), manual_source(Cadence::EVERY_STEP), 5)
            .expect("a valid episode");
        ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
        let mut out = Vec::new();
        for _ in 0..steps {
            ep.advance(1).expect("a valid action");
            out.push(position(ep.state()));
        }
        out
    }

    #[test]
    fn outcome_transitions_are_asserted_one_per_reason() {
        // -- Truncated: the step budget, and nothing else ---------------
        let mut cfg = config("free_sail");
        cfg.max_steps = Some(250);
        let ep = run_to_end(cfg, Stub::boxed(10), 1, 4000);
        assert_eq!(ep.outcome(), Outcome::Truncated);
        assert_eq!(ep.steps(), 250, "truncation lands on the step it names");
        assert!(ep.outcome().truncated() && !ep.outcome().terminated());

        // -- Capsized: F6.10's accumulator, read not recomputed ---------
        let mut cfg = config("beam_reach_capsize");
        cfg.max_steps = Some(40_000);
        let mut ep = Episode::new(cfg, manual_source(Cadence::EVERY_STEP), 2).expect("valid");
        ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
        let mut done = 0u32;
        while done < 40_000 && !ep.outcome().is_terminal() {
            done += ep.advance(50).expect("a valid action");
        }
        assert_eq!(
            ep.outcome(),
            Outcome::Terminated(TerminationReason::Capsized),
            "beam_reach_capsize did not capsize in 200 s"
        );
        // The accumulator, not the instantaneous angle: `capsized` is set
        // only after `|φ| > φ_capsize` has held for `t_capsize`.
        let p = ep.params();
        assert!(ep.state().phi.abs() > p.stability.phi_capsize);
        assert!(ep.simulation().capsize().since > 0.0);

        // -- OutOfBounds -------------------------------------------------
        // A box the boat is known to leave, because the track says so
        // before the episode is built.
        let track = free_sail_track(2000);
        const EDGE: f64 = 5.0;
        assert!(
            track.iter().any(|p| p.x.abs() > EDGE || p.y.abs() > EDGE),
            "the hands-off boat never left a {EDGE} m box, so this fixture proves nothing"
        );
        let mut cfg = config("free_sail");
        cfg.bounds = Bounds::Rect {
            min: [-EDGE, -EDGE],
            max: [EDGE, EDGE],
        };
        cfg.max_steps = Some(4000);
        let mut ep = Episode::new(cfg, manual_source(Cadence::EVERY_STEP), 5).expect("valid");
        ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
        let mut done = 0u32;
        while done < 4000 && !ep.outcome().is_terminal() {
            done += ep.advance(10).expect("a valid action");
        }
        assert_eq!(
            ep.outcome(),
            Outcome::Terminated(TerminationReason::OutOfBounds),
            "the boat stayed inside a box it was meant to leave"
        );

        // -- Finished: F15.1's lookahead point on the boat's own track ---
        let target = track[1200];
        let mut cfg = config("free_sail");
        cfg.route = Some(Route::lookahead_point(target, 2.0));
        cfg.max_steps = Some(4000);
        let mut ep = Episode::new(cfg, manual_source(Cadence::EVERY_STEP), 5).expect("valid");
        ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
        let mut done = 0u32;
        while done < 4000 && !ep.outcome().is_terminal() {
            done += ep.advance(10).expect("a valid action");
        }
        match ep.outcome() {
            Outcome::Finished { time } => {
                // Not merely positive: a disc that already contains the
                // start would finish at t ≈ 0 and prove nothing.
                assert!(time > 1.0, "a finish at t = {time}");
                assert!(ep
                    .progress()
                    .expect("a route")
                    .finished(ep.config().route.as_ref().expect("a route")));
            }
            other => panic!("the boat did not reach a mark on its own track: {other:?}"),
        }

        // -- MarkMissed: the boat crosses the plane on the wrong side ----
        // Two marks, so leg 0 runs from mark 1 to mark 0 and has a
        // direction. Mark 0 sits **on** the track at step 1200 and must be
        // left on the side the boat will not leave it on, so the crossing
        // satisfies the plane-and-direction clauses and fails the side one.
        let a = track[1200];
        let d = (track[1200] - track[1100]).normalize();
        let behind = a - d * 40.0;
        // The boat must start **behind** the mark's plane, or there is no
        // directed crossing to fail the side clause of (F15.3).
        assert!(
            (track[0] - a).dot(d) < 0.0,
            "the track starts past the mark's plane, so nothing can be missed"
        );
        for rounding in [Rounding::Port, Rounding::Starboard] {
            let route = Route::new(
                vec![
                    Mark {
                        position: a,
                        radius: 5.0,
                        rounding,
                    },
                    Mark {
                        position: behind,
                        radius: 5.0,
                        rounding: Rounding::Either,
                    },
                ],
                1,
            );
            let mut cfg = config("free_sail");
            cfg.route = Some(route);
            cfg.max_steps = Some(4000);
            let mut ep = Episode::new(cfg, manual_source(Cadence::EVERY_STEP), 5).expect("valid");
            ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
            let mut done = 0u32;
            while done < 4000 && !ep.outcome().is_terminal() {
                done += ep.advance(10).expect("a valid action");
            }
            // One of the two sides is the wrong one, and the boat passing
            // within `radius` of the mark cannot satisfy the clearance
            // clause on **either**: it is a cut both ways.
            assert_eq!(
                ep.outcome(),
                Outcome::Terminated(TerminationReason::MarkMissed),
                "sailing over the top of a {rounding:?} mark is not a passage (F15.3)"
            );
        }

        // …and the same geometry with `Either` is **not** a miss, so the
        // detector is reading the side clause and not merely the plane.
        let route = Route::new(
            vec![
                Mark {
                    position: a,
                    radius: 5.0,
                    rounding: Rounding::Either,
                },
                Mark {
                    position: behind,
                    radius: 5.0,
                    rounding: Rounding::Either,
                },
            ],
            1,
        );
        let mut cfg = config("free_sail");
        cfg.route = Some(route);
        cfg.max_steps = Some(4000);
        let mut ep = Episode::new(cfg, manual_source(Cadence::EVERY_STEP), 5).expect("valid");
        ep.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
        let mut done = 0u32;
        while done < 4000 && !ep.outcome().is_terminal() {
            done += ep.advance(10).expect("a valid action");
        }
        assert_ne!(
            ep.outcome(),
            Outcome::Terminated(TerminationReason::MarkMissed),
            "an `Either` mark has no side to miss"
        );
    }

    // -----------------------------------------------------------------
    // 3. The suite, the greps and the refusals
    // -----------------------------------------------------------------

    /// The `Send` suite this crate builds is the registry's suite, column
    /// for column. See [`make_sensor`] for why there are two.
    #[test]
    fn the_env_suite_is_the_registrys_suite() {
        let registry = SensorRegistry::tier0();
        assert_eq!(
            registry.ids(),
            TIER0_SENSORS.to_vec(),
            "the registry's catalogue and this crate's list have diverged"
        );
        let theirs = ObsLayout::of(&registry.resolve(&TIER0_SENSORS).expect("registered"));
        let mine: Vec<Box<dyn Sensor + Send>> = TIER0_SENSORS
            .iter()
            .map(|id| make_sensor(id).expect("built here too"))
            .collect();
        let mut mine_boxed = mine;
        let mine = obs_layout_of(&mine_boxed);
        assert_eq!(mine, theirs, "the two construction sites disagree");
        assert_eq!(
            sailgym_agent::obs_digest(&mine),
            sailgym_agent::obs_digest(&theirs)
        );
        assert_eq!(mine.len(), 5 + 2 + 4 + 2 + 5);
        assert!(make_sensor("lidar2d").is_none());

        // …and the two concatenations agree on a real state, bit for bit,
        // so `observe_into` cannot drift from `observation::observe`.
        let mut ep = Episode::new(config("close_hauled"), Stub::boxed(10), 17).expect("valid");
        ep.advance(137).expect("a valid action");
        let st = *ep.state();
        let controls = *ep.simulation().controls();
        let params = *ep.params();
        let view = WorldView {
            st: &st,
            controls: &controls,
            p: &params,
            wind: ep.simulation().wind(),
            guidance: None,
            others: &[],
            t: st.t,
        };
        let root = Pcg32::seed_from_u64(17);
        let rng = agent_rng(&root);

        let mut theirs_sensors = registry.resolve(&TIER0_SENSORS).expect("registered");
        let mut theirs_streams = sensor_streams(&rng, &theirs_sensors);
        let mut theirs_obs = Vec::new();
        observe(
            &mut theirs_sensors,
            &mut theirs_streams,
            &view,
            &mut theirs_obs,
        );

        let mut my_streams: Vec<Pcg32> = mine_boxed
            .iter()
            .map(|s| sensor_stream(&rng, s.id()))
            .collect();
        let mut my_obs = Vec::new();
        observe_into(&mut mine_boxed, &mut my_streams, &view, &mut my_obs);

        assert_eq!(my_obs.len(), theirs_obs.len());
        assert!(
            my_obs.iter().any(|v| *v != 0.0),
            "the observation is all zeros, so agreeing proves nothing"
        );
        for (i, (a, b)) in my_obs.iter().zip(theirs_obs.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "column {i} ({}): {a} vs {b}",
                mine.names()[i]
            );
        }
    }

    /// v1 brief §43 and v2 F14.9, as a test rather than as a promise: no F7
    /// coefficient has been copied into this crate. The same audit
    /// `sailgym-agent` runs, pointed at `sailgym-env`.
    #[test]
    fn no_f7_literal_appears_in_the_env_crate() {
        let params =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../sailgym-physics/src/parameters.rs");
        let catalogue = std::fs::read_to_string(&params).expect("parameters.rs must be readable");
        let f7: Vec<f64> = catalogue
            .lines()
            .flat_map(float_literals)
            .filter(|v| !universal(*v))
            .collect();
        assert!(f7.len() > 40, "the F7 scan found only {} values", f7.len());

        let mut offenders = Vec::new();
        let mut files = 0usize;
        for path in sources() {
            let text = std::fs::read_to_string(&path).expect("source");
            for (n, line) in code_lines(&text) {
                let code = line.split("//").next().unwrap_or("");
                for v in float_literals(code) {
                    if !universal(v) && f7.contains(&v) {
                        offenders.push(format!("{}:{n}: {v} in `{}`", path.display(), code.trim()));
                    }
                }
            }
            files += 1;
        }
        assert!(files >= 4, "the env source scan found only {files} files");
        assert!(
            offenders.is_empty(),
            "an F7 coefficient has been copied into the env crate (brief §43, F14.9):\n{}",
            offenders.join("\n")
        );
        eprintln!(
            "{files} env source files scanned against {} F7 values",
            f7.len()
        );
    }

    /// F9.1 and F14.8: nothing in this crate reads a wall clock. The same
    /// needles and the same `#[cfg(test)]` exclusion the physics and agent
    /// crates use, extended to this one rather than rewritten.
    #[test]
    fn no_wall_clock_in_the_env_crate() {
        const NEEDLES: [&str; 4] = ["Instant", "SystemTime", "now(", "rand::thread_rng"];
        let mut offenders = Vec::new();
        for path in sources() {
            let text = std::fs::read_to_string(&path).expect("source");
            for (n, line) in code_lines(&text) {
                let code = line.split("//").next().unwrap_or("");
                for needle in NEEDLES {
                    if code.contains(needle) {
                        offenders.push(format!("{}:{n}: {}", path.display(), code.trim()));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "a wall clock has reached the env crate (F9.1):\n{}",
            offenders.join("\n")
        );
        // …and the scanner would notice one.
        let probe = "let t = std::time::Instant::now();";
        assert!(NEEDLES.iter().any(|n| probe.contains(n)));
        // …and it ignores one inside a `#[cfg(test)]` item, which is what
        // lets this very test spell the needles out.
        let fixture = "#[cfg(test)]\nmod t {\n    use std::time::Instant;\n}\nfn f() {}\n";
        assert!(code_lines(fixture)
            .iter()
            .all(|(_, l)| !l.contains("Instant")));
    }

    #[test]
    fn step_refuses_what_it_cannot_honour() {
        let mut cfg = config("free_sail");
        cfg.autoreset = AutoresetMode::Disabled;
        cfg.max_steps = Some(20);
        let mut ep = Episode::new(cfg, Stub::boxed(10), 3).expect("valid");

        // Part-way through a decision period.
        ep.advance(3).expect("a valid action");
        assert_eq!(
            ep.step(),
            Err(EnvError::NotAtDecisionBoundary {
                step: 3,
                period_steps: 10
            })
        );
        ep.advance(7).expect("a valid action");
        assert!(ep.step().is_ok(), "step 10 is a decision boundary");

        // Terminal, with autoreset disabled: refused, not silently
        // continued.
        while !ep.outcome().is_terminal() {
            ep.advance(1).expect("a valid action");
        }
        assert_eq!(ep.outcome(), Outcome::Truncated);
        assert_eq!(ep.advance(100), Ok(0), "a terminal episode takes no steps");
        assert_eq!(ep.step(), Err(EnvError::Terminal(Outcome::Truncated)));

        // An action pushed at a policy is refused: the log would say the
        // policy took it.
        let mut ep = Episode::new(config("free_sail"), Stub::boxed(10), 3).expect("valid");
        assert_eq!(
            ep.push_action(&[0.0, 0.0, 0.0]),
            Err(EnvError::NotAnExternalSource)
        );
    }

    #[test]
    fn a_configuration_is_validated_before_anything_is_built() {
        let mut cfg = config("free_sail");
        cfg.sensors = vec!["imu".to_string(), "imu".to_string()];
        assert_eq!(
            cfg.validate(),
            Err(EnvError::DuplicateSensor("imu".to_string()))
        );
        cfg.sensors = vec!["lidar2d".to_string()];
        assert_eq!(
            cfg.validate(),
            Err(EnvError::UnknownSensor("lidar2d".to_string()))
        );
        cfg.sensors.clear();
        assert_eq!(cfg.validate(), Err(EnvError::NoSensors));

        let mut cfg = config("free_sail");
        cfg.max_steps = Some(0);
        assert_eq!(cfg.validate(), Err(EnvError::ZeroBudget));

        let mut cfg = config("free_sail");
        cfg.bounds = Bounds::Rect {
            min: [1.0, 1.0],
            max: [0.0, 0.0],
        };
        assert!(matches!(cfg.validate(), Err(EnvError::Bounds(_))));

        let mut cfg = config("free_sail");
        cfg.route = Some(Route::new(Vec::new(), 1));
        assert!(matches!(cfg.validate(), Err(EnvError::Route(_))));
    }

    /// The seed is the whole of an episode's identity, and the chain of
    /// seeds an autoreset produces is reproducible too — drawn from
    /// `STREAM_SCENARIO` and never from a counter, which would collide
    /// between adjacent `VecEnv` slots.
    #[test]
    fn one_seed_reproduces_the_episode_and_the_chain_after_it() {
        let mut cfg = config("free_sail");
        cfg.max_steps = Some(120);
        cfg.autoreset = AutoresetMode::NextStep;

        let chain = |seed: u64| -> Vec<u64> {
            let mut ep = Episode::new(cfg.clone(), Stub::boxed(10), seed).expect("valid");
            let mut seeds = vec![ep.seed()];
            for _ in 0..60 {
                let r = ep.step().expect("a valid step");
                if r.autoreset || r.terminated || r.truncated {
                    // The reset lands on the following call under NextStep.
                }
                if ep.seed() != *seeds.last().expect("a seed") {
                    seeds.push(ep.seed());
                }
            }
            seeds
        };
        let a = chain(99);
        let b = chain(99);
        assert_eq!(a, b, "the same root seed gave two different chains");
        assert!(a.len() >= 3, "the chain never autoreset: {a:?}");
        assert_eq!(a[0], 99, "the first episode uses the seed it was given");
        assert_ne!(a[1], a[0]);
        // A neighbouring root seed does not produce an overlapping chain,
        // which a `seed + index` rule would.
        let c = chain(100);
        assert!(
            !a[1..].iter().any(|s| c.contains(s)),
            "two adjacent root seeds produced overlapping episode seeds: {a:?} vs {c:?}"
        );
    }

    // -----------------------------------------------------------------
    // Scanners
    // -----------------------------------------------------------------

    fn sources() -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            // A fixed order: a directory listing is no more ordered than a
            // hash map (F9.3).
            let mut entries: Vec<PathBuf> =
                entries.filter_map(Result::ok).map(|e| e.path()).collect();
            entries.sort();
            for path in entries {
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
        out
    }

    /// Lines outside a `#[cfg(test)]` item, one-based.
    fn code_lines(source: &str) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let mut skipping = false;
        for (i, line) in source.lines().enumerate() {
            if skipping {
                if line == "}" {
                    skipping = false;
                }
                continue;
            }
            if line.trim() == "#[cfg(test)]" {
                skipping = true;
                continue;
            }
            out.push((i + 1, line.to_string()));
        }
        out
    }

    /// `0.0`, `1.0`, `2.0`, `3.0` and `0.5` are structural, not
    /// coefficients — the universal set `tests/provenance.rs` exempts.
    fn universal(v: f64) -> bool {
        [0.0, 1.0, 2.0, 3.0, 0.5].contains(&v)
    }

    /// Every decimal float literal on a line. The same scanner
    /// `sailgym-agent`'s audit uses.
    fn float_literals(line: &str) -> Vec<f64> {
        let mut out = Vec::new();
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !bytes[i].is_ascii_digit() {
                i += 1;
                continue;
            }
            if i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'.')
            {
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
                {
                    i += 1;
                }
                continue;
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                i += 1;
            }
            if i < bytes.len()
                && bytes[i] == b'.'
                && i + 1 < bytes.len()
                && bytes[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                let text: String = line[start..i].chars().filter(|c| *c != '_').collect();
                if let Ok(v) = text.parse::<f64>() {
                    out.push(v);
                }
            }
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
        }
        out
    }
}
