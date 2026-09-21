//! Episode recording (brief §33, v1 task 9.3, v2 section 10 task 10.1).
//!
//! The recording schema is not a debug convenience. brief §45 makes it the
//! forward interface to the episode inspector and to later RL work, so it is
//! versioned from day one ([`EPISODE_SCHEMA_VERSION`]), flat, and
//! typed-array-friendly: every sample is a fixed run of `f64`s in a fixed
//! order, which is what makes [`Episode::to_binary`] a header plus one
//! contiguous `Float64Array`.
//!
//! ## Schema 2 — what section 10 added, and why
//!
//! v2 F18.3 requires that **every visible quantity in a replay belongs to that
//! episode and that time**. Schema 1 recorded the F3 state, the four component
//! forces, four moments, the sheet tension and the wind vector — enough to draw
//! the boat, and not enough to drive the HUD, the force overlay, the charts or
//! the capsize readout without falling back on the *live* simulation. Schema 2
//! therefore adds two things and nothing else:
//!
//! 1. [`FrameDiagnostics`] — the diagnostic subset those four consumers need,
//!    captured at the same state and time as the sample it sits beside.
//!    The subset is deliberately a subset: [`FrameDiagnostics`]'s own doc
//!    comment lists, by name, every [`crate::diagnostics::Diagnostics`] field
//!    that is **not** retained, and a replay must show those as *unavailable*
//!    rather than computing them from the model currently loaded (RV59).
//! 2. A canonical [`ExperimentIdentity`] in the header — model and source
//!    identity (F18.1d), the resolved catalogue, integrator and `dt`, the full
//!    initial state and controls, the scenario, the wind configuration and the
//!    seed, plus the task, action and observation contracts where they apply.
//!    Equality of these records is what licenses the phrase "the same
//!    conditions"; [`Recorded::Unknown`] is not equal to anything, and is not
//!    the same as [`Recorded::NotApplicable`].
//!
//! ## Migration
//!
//! [`SUPPORTED_SCHEMA_VERSIONS`] is `[1, 2]`. A schema-1 document — JSON or
//! binary — decodes into the same [`Episode`] type with `frame.diag = None`
//! and every added header field [`Recorded::Unknown`]; **nothing is filled in
//! with zero and nothing is recomputed** (RV60). An episode is re-encoded in
//! the schema its own header declares, so a legacy file round-trips through
//! both codecs unchanged. A version this build does not implement is refused
//! with a message naming what it does implement.
//!
//! ## The recorder is an observer
//!
//! [`Recorder::observe`] takes `&Simulation`. It cannot perturb a trajectory,
//! and `tests::recording_does_not_perturb` asserts the bit-identical outcome
//! rather than trusting the type. Recording happens at a configurable rate,
//! not every physics substep (brief §33), and stops at
//! [`MAX_EPISODE_FRAMES`] — a cap derived from a stated byte budget, so a long
//! run cannot exhaust a browser tab silently.
//!
//! ## No wall clock
//!
//! F9.1 forbids the physics crate from reading a clock, and
//! `determinism::no_wall_clock` greps this file along with the rest of `src/`.
//! [`EpisodeHeader::created_utc`] is therefore supplied by the **caller** —
//! the browser wrapper or the native bench — and frozen into the header at
//! [`Recorder::start`]. [`iso8601_utc`] is a pure function of the epoch
//! milliseconds it is handed; it reads nothing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostics;
use crate::environment::wind::WindConfig;
use crate::environment::wind_to_bearing;
use crate::identity::ModelIdentity;
use crate::integrator::Integrator;
use crate::scenario::Scenario;
use crate::simulation::Simulation;
use crate::state::{BoatState, Controls, STATE_LEN};

/// The schema this build **writes**.
pub const EPISODE_SCHEMA_VERSION: u32 = 2;

/// Every schema this build **reads**, oldest first (RV60).
pub const SUPPORTED_SCHEMA_VERSIONS: [u32; 2] = [1, 2];

/// The version of the canonical [`ExperimentIdentity`] record.
///
/// Separate from [`EPISODE_SCHEMA_VERSION`] on purpose: the *document* layout
/// and the *comparison contract* change for different reasons and at different
/// times. A header carrying `identity_version = 0` predates the record
/// entirely, which is why it is `Unknown` rather than absent.
pub const IDENTITY_VERSION: u32 = 1;

/// The version of the reserved [`PracticeEnvelope`] (section 11).
pub const PRACTICE_ENVELOPE_VERSION: u32 = 1;

/// Scalars per frame in schema 1: `t` + state + controls + wind + forces +
/// moments + tension + reward + capsized.
pub const FRAME_LEN_V1: usize = 1 + STATE_LEN + 3 + 2 + 12 + 4 + 1 + 1 + 1;

/// Scalars in the schema-2 [`FrameDiagnostics`] block.
pub const DIAG_LEN: usize = 40;

/// Scalars per frame in schema 2: the schema-1 run, then the diagnostics
/// block. The layout is a **strict extension** — a schema-1 reader looking at
/// the first [`FRAME_LEN_V1`] scalars of a schema-2 frame sees exactly what it
/// expects.
pub const FRAME_LEN: usize = FRAME_LEN_V1 + DIAG_LEN;

/// Bytes one schema-2 sample occupies in the binary form.
pub const BYTES_PER_FRAME: usize = FRAME_LEN * 8;

/// The frame block's byte budget for one recording.
///
/// **Provenance.** 8 MiB is a memory budget, not a physical quantity: it is
/// the point past which holding an episode, its JSON form and the decoded
/// JavaScript objects at the same time stops being comfortable in a browser
/// tab. It is stated in bytes and the frame cap is derived from it, so the cap
/// follows the frame width automatically instead of becoming a stale number
/// the next time the schema grows.
pub const MAX_EPISODE_BYTES: usize = 8 * 1024 * 1024;

/// The recorder stops appending here. `MAX_EPISODE_BYTES / BYTES_PER_FRAME`.
///
/// At the four logging rates the UI offers that is 44.8 min of simulated time
/// at 5 Hz, 22.4 min at 10 Hz, 11.2 min at 20 Hz and 4.5 min at 50 Hz.
pub const MAX_EPISODE_FRAMES: usize = MAX_EPISODE_BYTES / BYTES_PER_FRAME;

/// The largest header JSON block [`Episode::from_binary`] will accept.
///
/// The header carries the whole resolved F7 catalogue, the scenario document
/// and the identity record — a few kilobytes. 64 KiB is two decades of slack
/// and still refuses a corrupt length field before it is used to index.
pub const MAX_HEADER_BYTES: usize = 64 * 1024;

/// The four bytes that open a binary episode.
const MAGIC: [u8; 4] = *b"SGEP";

/// The fixed prefix of a binary episode, before the header JSON.
const BINARY_PREFIX: usize = 24;

// ---------------------------------------------------------------------------
// Recorded<T> — the three-valued field
// ---------------------------------------------------------------------------

/// A value that may be recorded, unrecorded, or inapplicable.
///
/// The distinction is the whole point of v2 F18.3's "unknown is distinct from
/// not applicable":
///
/// * [`Recorded::Value`] — the episode carries it.
/// * [`Recorded::Unknown`] — the concept applies, but this document does not
///   say. A schema-1 file, or a field added after it was written. **Unknown is
///   not equal to anything, including another unknown**, and blocks a strict
///   comparison while still permitting display.
/// * [`Recorded::NotApplicable`] — the concept does not exist for this
///   episode. A hand-flown browser run has no action adapter and no
///   observation layout; saying so is information, not a gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recorded<T> {
    /// The recorded value.
    Value(T),
    /// The concept applies; this document does not record it.
    Unknown,
    /// The concept does not apply to this episode.
    NotApplicable,
}

/// `Unknown`, for every `T`.
///
/// Written out rather than derived: `#[derive(Default)]` would demand
/// `T: Default`, and a `ModelIdentity` has no meaningful default — which is
/// the whole point of the enum.
impl<T> Default for Recorded<T> {
    fn default() -> Self {
        Self::Unknown
    }
}

impl<T> Recorded<T> {
    /// The value, if there is one.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Whether this field carries a value.
    pub fn is_known(&self) -> bool {
        matches!(self, Self::Value(_))
    }

    /// Whether this field says the concept does not apply.
    pub fn is_not_applicable(&self) -> bool {
        matches!(self, Self::NotApplicable)
    }
}

/// How two identity fields compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FieldMatch {
    /// Both sides agree, and both are known — or both say "not applicable".
    Same,
    /// Both sides are known and they differ.
    Different,
    /// At least one side is unknown, so nothing may be concluded.
    Indeterminate,
}

impl<T: PartialEq> Recorded<T> {
    fn compare(&self, other: &Self) -> FieldMatch {
        match (self, other) {
            (Self::Value(a), Self::Value(b)) => {
                if a == b {
                    FieldMatch::Same
                } else {
                    FieldMatch::Different
                }
            }
            (Self::NotApplicable, Self::NotApplicable) => FieldMatch::Same,
            (Self::NotApplicable, Self::Value(_)) | (Self::Value(_), Self::NotApplicable) => {
                FieldMatch::Different
            }
            _ => FieldMatch::Indeterminate,
        }
    }
}

// ---------------------------------------------------------------------------
// The identity records
// ---------------------------------------------------------------------------

/// The task an episode was flown against (section 11, v2 F18.4).
///
/// Thresholds are a `BTreeMap`, never an unordered map: F9.3 forbids hash
/// iteration inside physics, and a canonical record has to serialise in one
/// order or equality of its text stops meaning equality of its content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskIdentity {
    /// The task's stable id, e.g. `hold_a_close_hauled_course`.
    pub id: String,
    /// Bumped when a threshold or an outcome rule changes meaning.
    pub version: u32,
    /// Named numeric thresholds, in the task's own units.
    pub thresholds: BTreeMap<String, f64>,
}

/// The action adapter an agent drove the episode through (v2 F14.2, F14.5,
/// F14.6). `NotApplicable` for a hand-flown browser episode.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionIdentity {
    /// The adapter's registered name — never "the boom angle" (F14.5).
    pub adapter: String,
    /// Bumped when the adapter's denormalisation or engagement semantics
    /// change.
    pub version: u32,
    /// F14.6: a decision happens when `episode_step % period_steps == 0`.
    pub period_steps: u32,
}

/// One column of an observation vector (v2 F14.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationField {
    pub name: String,
    /// The F1 unit, spelled as F1 spells it.
    pub unit: String,
    /// How the column is scaled before a policy sees it; `none` if it is not.
    pub normalisation: String,
    /// The noise standard deviation applied to this column, in its own unit.
    pub noise: f64,
    /// F14.3's surviving `ObsMask`: is this column privileged?
    pub privileged: bool,
}

/// The observation layout an episode was produced under (v2 F14.3).
///
/// There is no `OBS_LEN` and no `const OBS_FIELDS`: the layout is runtime
/// data, so it travels with the episode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservationIdentity {
    pub layout_version: u32,
    pub fields: Vec<ObservationField>,
}

/// The reserved, typed, versioned envelope section 11 populates.
///
/// It is **one dependent feature**, not a general extension registry: a task
/// configuration and an ordered event list, written through the existing
/// [`Recorder`] API rather than by a second recorder living in the task crate.
/// Events are keyed by **physics step index** as well as by simulated time, so
/// a threshold crossing is anchored to the step that produced it and not to an
/// interpolated instant (v2 F18.4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PracticeEnvelope {
    /// [`PRACTICE_ENVELOPE_VERSION`] when written by this build.
    pub envelope_version: u32,
    pub task: TaskIdentity,
    /// Ordered by `step`, and appended in that order.
    pub events: Vec<PracticeEvent>,
}

/// One ordered practice event (section 11).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PracticeEvent {
    /// The event's stable id.
    pub id: String,
    /// The **physics** step index the event was decided on.
    pub step: u64,
    /// s, the simulated time of that step.
    pub t: f64,
    /// The value that crossed the threshold, in the task's own unit.
    pub value: f64,
}

/// Everything that has to agree before two episodes may be called "the same
/// conditions" (v2 F18.3, F16.4).
///
/// Built from an [`EpisodeHeader`] by [`EpisodeHeader::identity`] rather than
/// stored beside it, so the record and the document can never disagree.
///
/// **No digest is shipped.** v2 F16.4 permits an optional compact key but
/// requires an established SHA-256 rather than handwritten cryptography, and
/// says plainly that stable equality of the canonical records is enough. This
/// crate implements no cryptographic primitive (F18.1d says the same about the
/// source id), so comparison is structural equality and
/// [`ExperimentIdentity::canonical_json`] is the stable text form.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExperimentIdentity {
    /// [`IDENTITY_VERSION`], or `0` for a document written before the record
    /// existed.
    pub identity_version: u32,
    /// F18.1d: the declared model version and the physics source tree id.
    pub model: Recorded<ModelIdentity>,
    /// The fully resolved F7 catalogue, not the scenario's sparse overrides.
    pub parameters: Recorded<crate::parameters::BoatParameters>,
    pub integrator: Recorded<Integrator>,
    /// s, the fixed physics timestep.
    pub dt: Recorded<f64>,
    /// The complete F3 state the episode started from.
    pub initial_state: Recorded<BoatState>,
    pub initial_controls: Recorded<Controls>,
    /// The scenario id the run started from.
    pub scenario: Recorded<String>,
    pub wind: Recorded<WindConfig>,
    pub seed: Recorded<u64>,
    pub task: Recorded<TaskIdentity>,
    pub action: Recorded<ActionIdentity>,
    pub observation: Recorded<ObservationIdentity>,
}

/// The verdict [`ExperimentIdentity::compare`] returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Comparability {
    /// Every compared field is known on both sides and agrees. Only this
    /// verdict licenses the words "the same conditions".
    SameConditions,
    /// At least one compared field is known on both sides and differs. The
    /// names are the offending fields, in the fixed order below.
    Different(Vec<String>),
    /// At least one compared field is unknown on a side, or names a model
    /// identity that is dirty or unknown. Viewing is fine; comparing is not.
    Indeterminate(Vec<String>),
}

impl Comparability {
    /// Whether this verdict permits a strict, same-conditions comparison.
    pub fn is_same_conditions(&self) -> bool {
        matches!(self, Self::SameConditions)
    }

    /// The field names behind a negative verdict, in the fixed compare order.
    pub fn reasons(&self) -> &[String] {
        match self {
            Self::SameConditions => &[],
            Self::Different(r) | Self::Indeterminate(r) => r,
        }
    }

    /// One line, for a manifest, a log or a UI badge.
    pub fn describe(&self) -> String {
        match self {
            Self::SameConditions => "same conditions".to_string(),
            Self::Different(r) => format!("different conditions: {}", r.join(", ")),
            Self::Indeterminate(r) => format!("not comparable: {}", r.join(", ")),
        }
    }
}

impl ExperimentIdentity {
    /// The canonical text form: `serde_json` over a record whose maps are
    /// ordered and whose fields are declared in one order, so equal records
    /// produce equal text on every platform.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Whether an episode recorded under `self` may be compared, quantity for
    /// quantity, with one recorded under `other`.
    ///
    /// The compared fields are fixed and explicit; the order is the order the
    /// reasons come back in, so two runs of this function on the same pair
    /// produce identical text (F9.3, F9.4).
    pub fn compare(&self, other: &Self) -> Comparability {
        let mut different: Vec<String> = Vec::new();
        let mut indeterminate: Vec<String> = Vec::new();

        let mut note = |name: &str, m: FieldMatch| match m {
            FieldMatch::Same => {}
            FieldMatch::Different => different.push(name.to_string()),
            FieldMatch::Indeterminate => indeterminate.push(name.to_string()),
        };

        // The model is special: two *known* ids may still be incomparable,
        // because a dirty or unknown source tree names no baseline (F18.1d).
        note(
            "model",
            match (self.model.value(), other.model.value()) {
                (Some(a), Some(b)) if a.is_comparable_with(b) => FieldMatch::Same,
                (Some(a), Some(b)) => {
                    if a.source.is_known() && b.source.is_known() {
                        FieldMatch::Different
                    } else {
                        FieldMatch::Indeterminate
                    }
                }
                _ => FieldMatch::Indeterminate,
            },
        );

        note("parameters", self.parameters.compare(&other.parameters));
        note("integrator", self.integrator.compare(&other.integrator));
        note("dt", self.dt.compare(&other.dt));
        note(
            "initial_state",
            self.initial_state.compare(&other.initial_state),
        );
        note(
            "initial_controls",
            self.initial_controls.compare(&other.initial_controls),
        );
        note("scenario", self.scenario.compare(&other.scenario));
        note("wind", self.wind.compare(&other.wind));
        note("seed", self.seed.compare(&other.seed));
        note("task", self.task.compare(&other.task));
        note("action", self.action.compare(&other.action));
        note("observation", self.observation.compare(&other.observation));

        if !different.is_empty() {
            Comparability::Different(different)
        } else if !indeterminate.is_empty() {
            Comparability::Indeterminate(indeterminate)
        } else {
            Comparability::SameConditions
        }
    }
}

/// The toolchain a recording (or a golden trajectory) was produced by.
///
/// R7: recordings and golden files are only valid for the build that made
/// them (F9's guarantee is same-build, same-platform). Storing the toolchain
/// is what lets the regression harness *skip with a clear message* instead of
/// failing confusingly.
///
/// It is **metadata**, not identity: v2 F18.3 keeps the toolchain and the
/// recording cadence out of [`ExperimentIdentity`], because a rebuilt
/// compiler does not change the equations and a different `log_hz` does not
/// change the boat.
///
/// The values come from `build.rs`, which asks the compiler; nothing here is
/// guessed from `cfg!`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainInfo {
    /// `rustc --version` output, e.g. `rustc 1.98.1 (48a229cea 2026-09-01)`.
    pub rustc: String,
    /// The target triple the physics crate was compiled for.
    pub target: String,
    /// `debug` or `release`.
    pub profile: String,
}

impl ToolchainInfo {
    /// The toolchain that compiled this crate.
    pub fn current() -> Self {
        Self {
            rustc: env!("SAILGYM_RUSTC").to_string(),
            target: env!("SAILGYM_TARGET").to_string(),
            profile: env!("SAILGYM_PROFILE").to_string(),
        }
    }

    /// A one-line form for skip messages.
    pub fn describe(&self) -> String {
        format!("{} / {} / {}", self.rustc, self.target, self.profile)
    }
}

/// Everything needed to interpret — and to reproduce — an episode.
///
/// The first seven fields are schema 1's and are unchanged. The rest are
/// schema 2's, every one `#[serde(default)]`, so a schema-1 document decodes
/// with them [`Recorded::Unknown`] rather than failing or being invented.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeHeader {
    /// [`EPISODE_SCHEMA_VERSION`] when written by this build; `1` for a
    /// legacy document this build has decoded.
    pub schema_version: u32,
    pub scenario: Scenario,
    /// Fully resolved, not the scenario's sparse overrides.
    pub parameters: crate::parameters::BoatParameters,
    /// s, the fixed physics timestep the episode was produced at.
    pub dt: f64,
    /// Hz, the logging rate — **not** the physics rate (brief §33). Metadata:
    /// it changes the sampling resolution, never the boat.
    pub log_hz: f64,
    /// R7. Metadata (see [`ToolchainInfo`]).
    pub toolchain: ToolchainInfo,
    /// Metadata only; **never read by physics** (F9.1). Supplied by the
    /// caller, frozen at [`Recorder::start`].
    pub created_utc: String,
    /// [`IDENTITY_VERSION`], or `0` in a document written before the canonical
    /// record existed.
    #[serde(default)]
    pub identity_version: u32,
    /// F18.1d's model and source identity.
    #[serde(default)]
    pub model: Recorded<ModelIdentity>,
    /// The complete F3 state at the first recorded sample.
    #[serde(default)]
    pub initial_state: Recorded<BoatState>,
    /// The controls in force at that sample.
    #[serde(default)]
    pub initial_controls: Recorded<Controls>,
    /// Section 11's reserved envelope. `None` in a hand-flown episode.
    #[serde(default)]
    pub practice: Option<PracticeEnvelope>,
    /// `NotApplicable` for a hand-flown episode; `Unknown` in schema 1.
    #[serde(default)]
    pub action: Recorded<ActionIdentity>,
    /// `NotApplicable` for a hand-flown episode; `Unknown` in schema 1.
    #[serde(default)]
    pub observation: Recorded<ObservationIdentity>,
}

impl EpisodeHeader {
    /// A header for an episode a human flew in the browser or the bench.
    ///
    /// The research-only slots are marked [`Recorded::NotApplicable`] — there
    /// is no action adapter and no observation layout, and saying so is
    /// information. `created_utc` is the caller's (F9.1).
    pub fn manual(
        scenario: Scenario,
        parameters: crate::parameters::BoatParameters,
        log_hz: f64,
        created_utc: String,
        initial_state: BoatState,
        initial_controls: Controls,
    ) -> Self {
        Self {
            schema_version: EPISODE_SCHEMA_VERSION,
            scenario,
            parameters,
            dt: parameters.sim.dt,
            log_hz,
            toolchain: ToolchainInfo::current(),
            created_utc,
            identity_version: IDENTITY_VERSION,
            model: Recorded::Value(ModelIdentity::current()),
            initial_state: Recorded::Value(initial_state),
            initial_controls: Recorded::Value(initial_controls),
            practice: None,
            action: Recorded::NotApplicable,
            observation: Recorded::NotApplicable,
        }
    }

    /// The canonical identity record this header describes.
    ///
    /// Derived rather than stored: a second copy of the catalogue in the
    /// document is a second thing to keep in step. For a schema-1 header the
    /// scenario, catalogue and `dt` are still known — they were always in the
    /// document — and only the model, the initial condition and the three
    /// research contracts come back [`Recorded::Unknown`].
    pub fn identity(&self) -> ExperimentIdentity {
        let pre_identity = self.identity_version == 0;
        ExperimentIdentity {
            identity_version: self.identity_version,
            model: self.model.clone(),
            parameters: Recorded::Value(self.parameters),
            integrator: Recorded::Value(self.parameters.sim.integrator),
            dt: Recorded::Value(self.dt),
            initial_state: self.initial_state,
            initial_controls: self.initial_controls,
            scenario: Recorded::Value(self.scenario.name.clone()),
            wind: Recorded::Value(self.scenario.wind),
            seed: Recorded::Value(self.scenario.seed),
            task: match (&self.practice, pre_identity) {
                (Some(p), _) => Recorded::Value(p.task.clone()),
                // A document that predates the record cannot say whether a
                // task existed; one that carries the record and no envelope
                // is saying there was none.
                (None, true) => Recorded::Unknown,
                (None, false) => Recorded::NotApplicable,
            },
            action: self.action.clone(),
            observation: self.observation.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// The frame
// ---------------------------------------------------------------------------

/// The diagnostic subset recorded beside every schema-2 sample.
///
/// ## What is here, and what a replay needs it for
///
/// | block | consumer |
/// |---|---|
/// | wind speed/bearing, true and apparent wind | the HUD's wind readout, the force overlay's two wind vectors |
/// | speed over ground, acceleration | the HUD's speed readout, the boat-speed chart, the acceleration overlay |
/// | total force, sheet force, the five geometry points | the force overlay's application points and arms |
/// | the three `α` | the sail's drawn angle of attack and the `α` chart |
/// | rope length and extension | the drawn mainsheet and its sag |
/// | `GZ`, capsize `since` and `max_heel` | the capsize readout and the heel indicator |
///
/// The component forces, the four moments, the sheet tension, the wind vector
/// and the capsize flag are **not** here: they are schema-1 fields and are
/// already in [`EpisodeFrame`]. Neither are the quantities that come straight
/// out of the F3 state — `velocity_body`, `yaw_rate`, `roll_rate`, `beta`,
/// `beta_dot` — because the state is recorded in full and re-reading it is an
/// index, not a recomputation.
///
/// ## What is deliberately **not** recorded
///
/// These [`Diagnostics`] fields are omitted, and a replay must show them as
/// unavailable rather than evaluating the current model to fill the gap
/// (RV59): `steps`, `course_over_ground`, `leeway_angle`, `cl_sail`,
/// `cd_sail`, `cl_board`, `cd_board`, `cl_rudder`, `cd_rudder`, the
/// `boom_moment` breakdown (its **total** is `moments[3]`), `sheet_hull`,
/// `energy_kinetic`, `energy_roll_potential`, `energy_sheet_elastic` and
/// `hull_model_warning`. None of them is read by the HUD, the force overlay,
/// the charts or the capsize readout, which is the subset v2 section 10 task
/// 10.1 asks for; each costs bytes in every sample of every episode.
///
/// **Field order is normative** — it is the binary layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameDiagnostics {
    /// m/s, true wind speed at the boat. From `environment::wind_to_bearing`,
    /// so the from/toward conversion still exists in exactly one place (F6.1).
    pub wind_speed: f64,
    /// deg, meteorological FROM bearing, clockwise from north.
    pub wind_bearing_deg: f64,
    /// m/s, true wind in the horizontal body frame `H`.
    pub true_wind_body: [f64; 2],
    /// m/s, apparent wind at the CG, in `B` (F6.2).
    pub apparent_wind_body: [f64; 3],
    pub apparent_wind_speed: f64,
    /// rad, FROM angle off the bow, positive to starboard.
    pub apparent_wind_angle: f64,
    /// m/s, `hypot(u, v)`.
    pub speed_over_ground: f64,
    /// m/s², `(u̇, v̇)` in `H` (F4.2).
    pub acceleration_body: [f64; 2],
    /// N, `(ΣX, ΣY)` in `H`.
    pub total_force_h: [f64; 2],
    /// N, the pull on the boom at `P_b`, in `B` (F6.8).
    pub sheet_force: [f64; 3],
    /// m, the sail's centre of effort in `B` — the sail load's arm.
    pub sail_ce_b: [f64; 3],
    /// m, the centreboard's centre in `B` — the board load's arm.
    pub board_centre_b: [f64; 3],
    /// m, the rudder's centre in `B` — the rudder load's arm.
    pub rudder_centre_b: [f64; 3],
    /// m, `P_b(β)`, the mainsheet's boom attachment (F6.8).
    pub sheet_attach_b: [f64; 3],
    /// m, `P_k`, the block on the hull (F6.8).
    pub sheet_block_b: [f64; 3],
    pub alpha_sail: f64,
    pub alpha_board: f64,
    pub alpha_rudder: f64,
    /// m, `ℓ(β)`, the geometric rope path length (F6.8).
    pub sheet_rope_length: f64,
    /// m, `e = ℓ − L`; negative when the rope is slack.
    pub sheet_extension: f64,
    /// m, the righting arm `GZ(φ)` (F6.7).
    pub gz: f64,
    /// s, `CapsizeState::since`.
    pub capsize_since: f64,
    /// rad, `CapsizeState::max_heel`.
    pub capsize_max_heel: f64,
}

impl FrameDiagnostics {
    /// The subset, taken from a published [`Diagnostics`] record.
    ///
    /// It reads the record the caller already built for this state; it makes
    /// no second evaluation of the force model, exactly as
    /// [`crate::diagnostics::diagnostics`] makes none.
    pub fn of(d: &Diagnostics) -> Self {
        let (wind_speed, wind_bearing_deg) = wind_to_bearing(d.true_wind_world);
        Self {
            wind_speed,
            wind_bearing_deg,
            true_wind_body: [d.true_wind_body.x, d.true_wind_body.y],
            apparent_wind_body: [
                d.apparent_wind_body.x,
                d.apparent_wind_body.y,
                d.apparent_wind_body.z,
            ],
            apparent_wind_speed: d.apparent_wind_speed,
            apparent_wind_angle: d.apparent_wind_angle,
            speed_over_ground: d.speed_over_ground,
            acceleration_body: [d.acceleration_body.x, d.acceleration_body.y],
            total_force_h: [d.total_force_h.x, d.total_force_h.y],
            sheet_force: [d.sheet.f.x, d.sheet.f.y, d.sheet.f.z],
            sail_ce_b: [d.sail_ce_b.x, d.sail_ce_b.y, d.sail_ce_b.z],
            board_centre_b: [d.board_centre_b.x, d.board_centre_b.y, d.board_centre_b.z],
            rudder_centre_b: [
                d.rudder_centre_b.x,
                d.rudder_centre_b.y,
                d.rudder_centre_b.z,
            ],
            sheet_attach_b: [d.sheet_attach_b.x, d.sheet_attach_b.y, d.sheet_attach_b.z],
            sheet_block_b: [d.sheet_block_b.x, d.sheet_block_b.y, d.sheet_block_b.z],
            alpha_sail: d.alpha_sail,
            alpha_board: d.alpha_board,
            alpha_rudder: d.alpha_rudder,
            sheet_rope_length: d.sheet_rope_length,
            sheet_extension: d.sheet_extension,
            gz: d.gz,
            capsize_since: d.capsize.since,
            capsize_max_heel: d.capsize.max_heel,
        }
    }

    /// The block as [`DIAG_LEN`] scalars, in the declared field order.
    pub fn to_array(&self) -> [f64; DIAG_LEN] {
        let mut out = [0.0; DIAG_LEN];
        let mut at = 0;
        let mut put = |values: &[f64]| {
            out[at..at + values.len()].copy_from_slice(values);
            at += values.len();
        };
        put(&[self.wind_speed, self.wind_bearing_deg]);
        put(&self.true_wind_body);
        put(&self.apparent_wind_body);
        put(&[self.apparent_wind_speed, self.apparent_wind_angle]);
        put(&[self.speed_over_ground]);
        put(&self.acceleration_body);
        put(&self.total_force_h);
        put(&self.sheet_force);
        put(&self.sail_ce_b);
        put(&self.board_centre_b);
        put(&self.rudder_centre_b);
        put(&self.sheet_attach_b);
        put(&self.sheet_block_b);
        put(&[self.alpha_sail, self.alpha_board, self.alpha_rudder]);
        put(&[self.sheet_rope_length, self.sheet_extension]);
        put(&[self.gz, self.capsize_since, self.capsize_max_heel]);
        debug_assert_eq!(at, DIAG_LEN);
        out
    }

    /// The exact inverse of [`FrameDiagnostics::to_array`].
    pub fn from_array(a: &[f64; DIAG_LEN]) -> Self {
        let mut at = 0;
        let mut take = |n: usize| {
            let slice = &a[at..at + n];
            at += n;
            slice
        };
        let pair = |s: &[f64]| [s[0], s[1]];
        let triple = |s: &[f64]| [s[0], s[1], s[2]];

        let head = take(2);
        let (wind_speed, wind_bearing_deg) = (head[0], head[1]);
        let true_wind_body = pair(take(2));
        let apparent_wind_body = triple(take(3));
        let aw = take(2);
        let speed_over_ground = take(1)[0];
        let acceleration_body = pair(take(2));
        let total_force_h = pair(take(2));
        let sheet_force = triple(take(3));
        let sail_ce_b = triple(take(3));
        let board_centre_b = triple(take(3));
        let rudder_centre_b = triple(take(3));
        let sheet_attach_b = triple(take(3));
        let sheet_block_b = triple(take(3));
        let alpha = triple(take(3));
        let sheet = pair(take(2));
        let tail = triple(take(3));
        Self {
            wind_speed,
            wind_bearing_deg,
            true_wind_body,
            apparent_wind_body,
            apparent_wind_speed: aw[0],
            apparent_wind_angle: aw[1],
            speed_over_ground,
            acceleration_body,
            total_force_h,
            sheet_force,
            sail_ce_b,
            board_centre_b,
            rudder_centre_b,
            sheet_attach_b,
            sheet_block_b,
            alpha_sail: alpha[0],
            alpha_board: alpha[1],
            alpha_rudder: alpha[2],
            sheet_rope_length: sheet[0],
            sheet_extension: sheet[1],
            gz: tail[0],
            capsize_since: tail[1],
            capsize_max_heel: tail[2],
        }
    }
}

/// One logged sample.
///
/// **Field order is normative**: [`EpisodeFrame::to_common_array`] and the
/// binary encoding depend on it, and so does the episode inspector.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeFrame {
    /// s, simulation time.
    pub t: f64,
    /// The F3 state, in F8.3 order.
    pub state: [f64; STATE_LEN],
    /// `rudder_rate`, `sheet_rate`, `release` as `0.0`/`1.0`.
    pub controls: [f64; 3],
    /// m/s, true wind at the boat, world frame.
    pub wind_at_boat: [f64; 2],
    /// N, sail/board/rudder/hull force in `B`, `xyz` each, packed in that
    /// order.
    pub forces: [f64; 12],
    /// N·m: yaw, heel, righting, boom (the boom's **total**, F6.9).
    pub moments: [f64; 4],
    /// N, mainsheet tension.
    pub sheet_tension: f64,
    /// Placeholder, always `0.0` in v1 and v2 (brief §33). Nothing computes a
    /// reward and nothing may: the field exists so the format does not have to
    /// change when something does.
    pub reward: f64,
    pub capsized: bool,
    /// Schema 2's diagnostic subset. `None` in a schema-1 document, and that
    /// is the only case — this build records it for every sample it writes.
    /// Omitted from the JSON entirely when absent, so a schema-1 document
    /// re-encodes as the schema-1 document it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diag: Option<FrameDiagnostics>,
}

impl Default for EpisodeFrame {
    fn default() -> Self {
        Self {
            t: 0.0,
            state: [0.0; STATE_LEN],
            controls: [0.0; 3],
            wind_at_boat: [0.0; 2],
            forces: [0.0; 12],
            moments: [0.0; 4],
            sheet_tension: 0.0,
            reward: 0.0,
            capsized: false,
            diag: None,
        }
    }
}

impl EpisodeFrame {
    /// The schema-1 scalars, in the declared field order.
    ///
    /// These are the first [`FRAME_LEN_V1`] scalars of a schema-2 frame too:
    /// the layout is a strict extension, which is what lets one decoder read
    /// both widths.
    pub fn to_common_array(&self) -> [f64; FRAME_LEN_V1] {
        let mut out = [0.0; FRAME_LEN_V1];
        let mut at = 0;
        let mut put = |values: &[f64]| {
            out[at..at + values.len()].copy_from_slice(values);
            at += values.len();
        };
        put(&[self.t]);
        put(&self.state);
        put(&self.controls);
        put(&self.wind_at_boat);
        put(&self.forces);
        put(&self.moments);
        put(&[self.sheet_tension, self.reward]);
        put(&[if self.capsized { 1.0 } else { 0.0 }]);
        debug_assert_eq!(at, FRAME_LEN_V1);
        out
    }

    /// The diagnostics block as scalars, or `None` for a schema-1 frame.
    ///
    /// **Never a zero-filled substitute.** A caller that needs the block and
    /// does not get it is holding a legacy sample, and the honest answer is
    /// "not recorded" (RV59).
    pub fn diagnostics_array(&self) -> Option<[f64; DIAG_LEN]> {
        self.diag.as_ref().map(FrameDiagnostics::to_array)
    }

    /// Decode a frame from its scalars. `a.len()` must be [`FRAME_LEN_V1`]
    /// (schema 1) or [`FRAME_LEN`] (schema 2).
    pub fn from_scalars(a: &[f64]) -> Result<Self, EpisodeError> {
        if a.len() != FRAME_LEN_V1 && a.len() != FRAME_LEN {
            return Err(EpisodeError::Malformed(format!(
                "frame layout is {} scalars; this build reads {FRAME_LEN_V1} (schema 1) or \
                 {FRAME_LEN} (schema 2)",
                a.len()
            )));
        }
        let mut at = 0;
        let mut take = |n: usize| {
            let slice = &a[at..at + n];
            at += n;
            slice
        };
        let t = take(1)[0];
        let mut state = [0.0; STATE_LEN];
        state.copy_from_slice(take(STATE_LEN));
        let mut controls = [0.0; 3];
        controls.copy_from_slice(take(3));
        let mut wind_at_boat = [0.0; 2];
        wind_at_boat.copy_from_slice(take(2));
        let mut forces = [0.0; 12];
        forces.copy_from_slice(take(12));
        let mut moments = [0.0; 4];
        moments.copy_from_slice(take(4));
        let sheet_tension = take(1)[0];
        let reward = take(1)[0];
        let capsized = take(1)[0] != 0.0;
        let diag = if a.len() == FRAME_LEN {
            let mut block = [0.0; DIAG_LEN];
            block.copy_from_slice(take(DIAG_LEN));
            Some(FrameDiagnostics::from_array(&block))
        } else {
            None
        };
        Ok(Self {
            t,
            state,
            controls,
            wind_at_boat,
            forces,
            moments,
            sheet_tension,
            reward,
            capsized,
            diag,
        })
    }

    /// Whether every scalar this frame carries is finite.
    ///
    /// A `NaN` in a file is not a measurement, and letting one into a replay
    /// makes every downstream readout read `NaN` with no clue where it came
    /// from. Checked on decode, in both codecs.
    pub fn is_finite(&self) -> bool {
        self.to_common_array().iter().all(|v| v.is_finite())
            && self
                .diagnostics_array()
                .is_none_or(|d| d.iter().all(|v| v.is_finite()))
    }
}

/// A recorded episode: one header and a flat run of frames.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    pub header: EpisodeHeader,
    pub frames: Vec<EpisodeFrame>,
}

/// Why an episode could not be decoded.
#[derive(Clone, Debug, PartialEq)]
pub enum EpisodeError {
    /// The bytes or the text are not an episode at all.
    Malformed(String),
    /// The document declares a schema this build does not implement.
    UnsupportedSchemaVersion { found: u32 },
}

impl std::fmt::Display for EpisodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(e) => write!(f, "episode is malformed: {e}"),
            Self::UnsupportedSchemaVersion { found } => write!(
                f,
                "episode schema_version {found} is not supported; this build reads {}",
                supported_list()
            ),
        }
    }
}

impl std::error::Error for EpisodeError {}

/// `1 and 2`, for the message above. One place, so the message cannot drift
/// from [`SUPPORTED_SCHEMA_VERSIONS`].
fn supported_list() -> String {
    let names: Vec<String> = SUPPORTED_SCHEMA_VERSIONS
        .iter()
        .map(u32::to_string)
        .collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => names.join(""),
    }
}

/// Scalars per frame for a declared schema version.
fn frame_width(schema_version: u32) -> usize {
    if schema_version == 1 {
        FRAME_LEN_V1
    } else {
        FRAME_LEN
    }
}

impl Episode {
    /// The schema this document declares, which is the schema it re-encodes
    /// in. A legacy file that has been read and written again is still a
    /// legacy file (RV60).
    pub fn schema_version(&self) -> u32 {
        self.header.schema_version
    }

    /// Every frame carries the diagnostics a schema-2 document promises.
    fn diagnostics_complete(&self) -> Result<(), EpisodeError> {
        if let Some(i) = self.frames.iter().position(|f| f.diag.is_none()) {
            return Err(EpisodeError::Malformed(format!(
                "frame {i} carries no diagnostics, but the header declares schema \
                 {}; a schema-2 episode records them for every sample",
                self.header.schema_version
            )));
        }
        Ok(())
    }

    /// JSON, for inspection (brief §33).
    pub fn to_json(&self) -> Result<String, EpisodeError> {
        check_version(u64::from(self.header.schema_version))?;
        if self.header.schema_version != 1 {
            self.diagnostics_complete()?;
        }
        serde_json::to_string(self).map_err(|e| EpisodeError::Malformed(e.to_string()))
    }

    /// The inverse of [`Episode::to_json`], with the version checked first so
    /// an episode from another schema gets a message rather than a shape
    /// complaint.
    pub fn from_json(json: &str) -> Result<Self, EpisodeError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| EpisodeError::Malformed(e.to_string()))?;
        let found = value
            .get("header")
            .and_then(|h| h.get("schema_version"))
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                EpisodeError::Malformed("no header.schema_version in the document".to_string())
            })?;
        check_version(found)?;
        let episode: Self =
            serde_json::from_value(value).map_err(|e| EpisodeError::Malformed(e.to_string()))?;
        episode.validate()?;
        Ok(episode)
    }

    /// Everything a decoded episode must satisfy before it is handed on.
    ///
    /// Applied by both decoders, so a bad file is refused the same way
    /// whichever form it arrived in — and refused **before** the caller sees
    /// it, which is what leaves the episode already loaded untouched (RV60).
    fn validate(&self) -> Result<(), EpisodeError> {
        if !self.header.dt.is_finite() || self.header.dt <= 0.0 {
            return Err(EpisodeError::Malformed(format!(
                "header.dt is {}; a timestep must be finite and positive",
                self.header.dt
            )));
        }
        if !self.header.log_hz.is_finite() {
            return Err(EpisodeError::Malformed(
                "header.log_hz is not finite".to_string(),
            ));
        }
        if self.frames.len() > MAX_EPISODE_FRAMES {
            return Err(EpisodeError::Malformed(format!(
                "{} frames exceeds the {MAX_EPISODE_FRAMES}-frame budget \
                 ({MAX_EPISODE_BYTES} bytes at {BYTES_PER_FRAME} bytes per frame)",
                self.frames.len()
            )));
        }
        if let Some(i) = self.frames.iter().position(|f| !f.is_finite()) {
            return Err(EpisodeError::Malformed(format!(
                "frame {i} carries a non-finite value"
            )));
        }
        if self.header.schema_version != 1 {
            self.diagnostics_complete()?;
        }
        Ok(())
    }

    /// The typed-array binary form (brief §33): a small JSON header followed
    /// by one contiguous little-endian `f64` block of `frames × width`, where
    /// `width` is [`FRAME_LEN_V1`] for a schema-1 document and [`FRAME_LEN`]
    /// for a schema-2 one.
    ///
    /// ```text
    /// 0   "SGEP"
    /// 4   u32  schema_version
    /// 8   u32  header JSON length, bytes
    /// 12  u32  frame count
    /// 16  u32  scalars per frame
    /// 20  u32  offset of the f64 block (8-byte aligned)
    /// 24  header JSON
    ///     zero padding to the block offset
    ///     frames × width × f64, little-endian
    /// ```
    pub fn to_binary(&self) -> Result<Vec<u8>, EpisodeError> {
        let version = self.header.schema_version;
        check_version(u64::from(version))?;
        if version != 1 {
            self.diagnostics_complete()?;
        }
        let width = frame_width(version);

        let header =
            serde_json::to_vec(&self.header).map_err(|e| EpisodeError::Malformed(e.to_string()))?;
        let prefix = BINARY_PREFIX + header.len();
        let offset = prefix.next_multiple_of(8);

        let mut out = Vec::with_capacity(offset + self.frames.len() * width * 8);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.frames.len() as u32).to_le_bytes());
        out.extend_from_slice(&(width as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&header);
        out.resize(offset, 0);
        for frame in &self.frames {
            for v in frame.to_common_array() {
                out.extend_from_slice(&v.to_le_bytes());
            }
            if width == FRAME_LEN {
                // `diagnostics_complete` above is what makes this `expect`
                // unreachable: a schema-2 episode has the block on every
                // frame, and the check ran before a byte was written.
                let block = frame
                    .diagnostics_array()
                    .expect("schema 2 frames carry diagnostics");
                for v in block {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        Ok(out)
    }

    /// The exact inverse of [`Episode::to_binary`].
    pub fn from_binary(bytes: &[u8]) -> Result<Self, EpisodeError> {
        fn bad(why: String) -> EpisodeError {
            EpisodeError::Malformed(why)
        }
        if bytes.len() < BINARY_PREFIX || bytes[..4] != MAGIC {
            return Err(bad("not a sailgym episode (bad magic)".to_string()));
        }
        let u32_at = |at: usize| -> u32 {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let version = u32_at(4);
        check_version(u64::from(version))?;
        let header_len = u32_at(8) as usize;
        let frame_count = u32_at(12) as usize;
        let scalars = u32_at(16) as usize;
        let offset = u32_at(20) as usize;

        if scalars != frame_width(version) {
            return Err(bad(format!(
                "a schema-{version} frame is {} scalars; this file says {scalars}",
                frame_width(version)
            )));
        }
        if header_len > MAX_HEADER_BYTES {
            return Err(bad(format!(
                "header block is {header_len} bytes, above the {MAX_HEADER_BYTES}-byte limit"
            )));
        }
        if frame_count > MAX_EPISODE_FRAMES {
            return Err(bad(format!(
                "{frame_count} frames exceeds the {MAX_EPISODE_FRAMES}-frame budget"
            )));
        }
        if BINARY_PREFIX + header_len > bytes.len() || offset < BINARY_PREFIX + header_len {
            return Err(bad("header block does not fit the file".to_string()));
        }
        let header: EpisodeHeader =
            serde_json::from_slice(&bytes[BINARY_PREFIX..BINARY_PREFIX + header_len])
                .map_err(|e| EpisodeError::Malformed(e.to_string()))?;
        if header.schema_version != version {
            return Err(bad(format!(
                "the file declares schema {version} but its header says {}",
                header.schema_version
            )));
        }

        let needed = offset + frame_count * scalars * 8;
        if bytes.len() < needed {
            return Err(EpisodeError::Malformed(format!(
                "frame block is short: {} bytes, expected {needed}",
                bytes.len()
            )));
        }
        let mut frames = Vec::with_capacity(frame_count);
        let mut a = vec![0.0; scalars];
        for f in 0..frame_count {
            for (i, slot) in a.iter_mut().enumerate() {
                let at = offset + (f * scalars + i) * 8;
                let mut word = [0u8; 8];
                word.copy_from_slice(&bytes[at..at + 8]);
                *slot = f64::from_le_bytes(word);
            }
            frames.push(EpisodeFrame::from_scalars(&a)?);
        }
        let episode = Self { header, frames };
        episode.validate()?;
        Ok(episode)
    }
}

fn check_version(found: u64) -> Result<(), EpisodeError> {
    let capped = found.min(u64::from(u32::MAX)) as u32;
    if SUPPORTED_SCHEMA_VERSIONS.contains(&capped) && u64::from(capped) == found {
        Ok(())
    } else {
        Err(EpisodeError::UnsupportedSchemaVersion { found: capped })
    }
}

/// Samples the simulation at a fixed logging rate (brief §33).
///
/// It holds no reference to the simulation and is handed one per observation,
/// so a recorder cannot be the reason a trajectory changed.
pub struct Recorder {
    header: EpisodeHeader,
    frames: Vec<EpisodeFrame>,
    /// `1 / log_hz`, precomputed once so the due-time arithmetic is a
    /// multiply and not a divide per step.
    interval: f64,
    /// The index of the next sample, so due times are `n · interval` and
    /// never an accumulated sum (which would drift).
    next: u64,
    /// The cap, so a long run cannot exhaust a browser tab silently.
    max_frames: usize,
}

impl Recorder {
    /// Begin recording at `hz` samples per simulated second.
    ///
    /// A non-positive or non-finite `hz` records every observation. The frame
    /// count is capped at [`MAX_EPISODE_FRAMES`]; see [`Recorder::is_full`].
    pub fn start(hz: f64, header: EpisodeHeader) -> Self {
        let interval = if hz.is_finite() && hz > 0.0 {
            1.0 / hz
        } else {
            0.0
        };
        Self {
            header,
            frames: Vec::new(),
            interval,
            next: 0,
            max_frames: MAX_EPISODE_FRAMES,
        }
    }

    /// Whether the next completed step's state is due to be logged.
    ///
    /// Exposed so a caller can avoid building a whole [`Diagnostics`] record
    /// on the 199 steps out of 200 that will not be kept. `observe` applies
    /// the same test itself, so skipping this is only slower, never wrong.
    /// It is `false` once the recorder is full, so a capped recording stops
    /// paying for diagnostics as well as for memory.
    pub fn due(&self, t: f64) -> bool {
        !self.is_full() && t >= self.next as f64 * self.interval
    }

    /// The cap has been reached; nothing further will be logged.
    pub fn is_full(&self) -> bool {
        self.frames.len() >= self.max_frames
    }

    /// Frames this recorder will accept in total.
    pub fn capacity(&self) -> usize {
        self.max_frames
    }

    /// Log the simulation's published state, if the sample interval has
    /// elapsed. Call after each completed step.
    pub fn observe(&mut self, sim: &Simulation, diag: &Diagnostics) {
        let t = sim.state().t;
        if !self.due(t) {
            return;
        }
        // Advance past every interval this sample covers, so a large `dt` (or
        // a very high `log_hz`) cannot make the recorder fall permanently
        // behind and log every step for ever.
        self.next = if self.interval > 0.0 {
            (t / self.interval).floor() as u64 + 1
        } else {
            self.next + 1
        };

        let c = sim.controls();
        let f = sim.forces();
        let wind = sim.wind_at_boat();
        let first = self.frames.is_empty();
        self.frames.push(EpisodeFrame {
            t,
            state: sim.state().to_array(),
            controls: [
                c.rudder_rate_cmd,
                c.sheet_rate_cmd,
                if c.sheet_release { 1.0 } else { 0.0 },
            ],
            wind_at_boat: [wind.x, wind.y],
            forces: [
                f.sail.f.x,
                f.sail.f.y,
                f.sail.f.z,
                f.board.f.x,
                f.board.f.y,
                f.board.f.z,
                f.rudder.f.x,
                f.rudder.f.y,
                f.rudder.f.z,
                f.hull.f.x,
                f.hull.f.y,
                f.hull.f.z,
            ],
            moments: [
                diag.yaw_moment,
                diag.heeling_moment,
                diag.righting_moment,
                diag.boom_moment.total(),
            ],
            sheet_tension: diag.sheet_tension,
            // brief §33: the hook, and only the hook. Nothing computes a
            // reward in v1 or v2.
            reward: 0.0,
            capsized: diag.capsize.capsized,
            // Captured from the record the caller built for **this** state and
            // **this** time, not re-evaluated (v2 section 10, task 10.2).
            diag: Some(FrameDiagnostics::of(diag)),
        });

        // The header's initial condition is the first sample's, so the
        // identity describes the episode that exists rather than the one the
        // scenario asked for: an ad-hoc reset, a live parameter edit or a
        // recording started mid-run all land here honestly.
        if first {
            self.header.initial_state = Recorded::Value(*sim.state());
            self.header.initial_controls = Recorded::Value(*c);
        }
    }

    /// Frames logged so far.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn header(&self) -> &EpisodeHeader {
        &self.header
    }

    /// Attach section 11's practice envelope.
    ///
    /// The one extension point, and it is typed: the task crate hands the
    /// envelope to the **existing** recorder rather than opening a second one
    /// (v2 F18.4).
    pub fn set_practice(&mut self, envelope: PracticeEnvelope) {
        self.header.practice = Some(envelope);
    }

    /// Append one ordered practice event.
    ///
    /// Requires an envelope: an event without a task identity is an
    /// observation nobody can interpret. Returns whether it was accepted.
    pub fn push_practice_event(&mut self, event: PracticeEvent) -> bool {
        match self.header.practice.as_mut() {
            Some(envelope) => {
                envelope.events.push(event);
                true
            }
            None => false,
        }
    }

    /// Close the recording.
    pub fn finish(self) -> Episode {
        Episode {
            header: self.header,
            frames: self.frames,
        }
    }
}

/// Format epoch milliseconds as an ISO-8601 UTC instant.
///
/// A **pure function of its argument**: it reads no clock, which is what lets
/// it live in the physics crate at all (F9.1). Callers that want "now" bring
/// their own: the browser reads `Date`, the native bench reads the standard
/// library's clock. Neither of those names may appear in this crate, and
/// `determinism::no_wall_clock` is what says so.
pub fn iso8601_utc(epoch_millis: f64) -> String {
    if !epoch_millis.is_finite() {
        return String::new();
    }
    let total_millis = epoch_millis.floor() as i64;
    let (mut days, mut rem) = (
        total_millis.div_euclid(86_400_000),
        total_millis.rem_euclid(86_400_000),
    );
    let millis = rem % 1000;
    rem /= 1000;
    let (hour, minute, second) = (rem / 3600, (rem / 60) % 60, rem % 60);

    // Howard Hinnant's civil_from_days, shifted to an era starting 0000-03-01.
    days += 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::diagnostics;
    use crate::parameters::BoatParameters;
    use crate::scenario::load_shipped;
    use crate::state::STATE_FIELDS;

    /// A checked-in schema-1 episode, produced by the v1 recorder and kept
    /// byte for byte. RV60's fixture: if a schema bump ever stops reading it,
    /// this test fails rather than a user's recordings quietly dying.
    const LEGACY_JSON: &str = include_str!("../tests/fixtures/episode-schema1.json");
    const LEGACY_BIN: &[u8] = include_bytes!("../tests/fixtures/episode-schema1.bin");

    fn header(scenario: &Scenario, params: &BoatParameters, hz: f64) -> EpisodeHeader {
        EpisodeHeader::manual(
            scenario.clone(),
            *params,
            hz,
            iso8601_utc(1_758_326_400_000.0),
            Simulation::initial_state(params),
            Controls::default(),
        )
    }

    /// A simulation on `beam_reach_capsize`, which loads the sail, the sheet
    /// and the roll DOF — so every packed force and moment is non-zero.
    fn fixture(hz: f64) -> (Simulation, Recorder) {
        let sc = load_shipped("beam_reach_capsize").expect("shipped scenario");
        let params = sc.to_parameters().expect("valid parameters");
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("scenario loads");
        let rec = Recorder::start(hz, header(&sc, &params, hz));
        (sim, rec)
    }

    fn record_seconds(sim: &mut Simulation, rec: &mut Recorder, seconds: f64) {
        let steps = (seconds / sim.params().sim.dt).round() as u32;
        for _ in 0..steps {
            sim.advance(1);
            if rec.due(sim.state().t) {
                let d = diagnostics(sim);
                rec.observe(sim, &d);
            }
        }
    }

    /// Every scalar of a frame, both blocks, for a bit-exact comparison.
    fn all_scalars(f: &EpisodeFrame) -> Vec<f64> {
        let mut out = f.to_common_array().to_vec();
        if let Some(d) = f.diagnostics_array() {
            out.extend_from_slice(&d);
        }
        out
    }

    #[test]
    fn log_rate_respected() {
        // 60 s at 20 Hz is 1200 intervals; the boundary sample makes it 1201.
        let (mut sim, mut rec) = fixture(20.0);
        record_seconds(&mut sim, &mut rec, 60.0);
        let n = rec.len() as i64;
        assert!(
            (n - 1200).abs() <= 1,
            "logged {n} frames, expected 1200 ± 1"
        );

        // …and the samples really are one interval apart, not clustered.
        let episode = rec.finish();
        for pair in episode.frames.windows(2) {
            let gap = pair[1].t - pair[0].t;
            assert!(
                (gap - 0.05).abs() < sim.params().sim.dt + 1e-12,
                "samples {} and {} are {gap} s apart",
                pair[0].t,
                pair[1].t
            );
        }
    }

    #[test]
    fn recording_does_not_perturb() {
        // The recorder is an observer: a 60 s run with it on and one with it
        // off must end on bit-identical states.
        let (mut recorded, mut rec) = fixture(20.0);
        record_seconds(&mut recorded, &mut rec, 60.0);

        let (mut plain, _) = fixture(20.0);
        let steps = (60.0 / plain.params().sim.dt).round() as u32;
        for _ in 0..steps {
            plain.advance(1);
        }

        let (a, b) = (recorded.state().to_array(), plain.state().to_array());
        for (i, name) in STATE_FIELDS.iter().enumerate() {
            assert_eq!(
                a[i].to_bits(),
                b[i].to_bits(),
                "field {name}: recorded {} vs plain {}",
                a[i],
                b[i]
            );
        }
        assert!(rec.len() > 1000, "the recorded run must have logged");
        // The run has to have gone somewhere, or this proves nothing.
        assert!(a[0].abs() + a[1].abs() > 1.0, "the boat never moved");
    }

    #[test]
    fn binary_round_trip() {
        let (mut sim, mut rec) = fixture(10.0);
        sim.set_controls(Controls {
            rudder_rate_cmd: 0.3,
            sheet_rate_cmd: -0.5,
            sheet_release: true,
        });
        record_seconds(&mut sim, &mut rec, 20.0);
        let episode = rec.finish();
        assert!(episode.frames.len() > 100);
        assert_eq!(episode.schema_version(), 2);

        let bytes = episode.to_binary().expect("encodes");
        let back = Episode::from_binary(&bytes).expect("decodes");
        assert_eq!(back.header, episode.header);
        assert_eq!(back.frames.len(), episode.frames.len());
        for (i, (a, b)) in episode.frames.iter().zip(back.frames.iter()).enumerate() {
            let (x, y) = (all_scalars(a), all_scalars(b));
            assert_eq!(x.len(), FRAME_LEN, "frame {i} must carry both blocks");
            for k in 0..FRAME_LEN {
                assert_eq!(x[k].to_bits(), y[k].to_bits(), "frame {i}, scalar {k}");
            }
            assert_eq!(a.capsized, b.capsized);
            assert_eq!(a.diag, b.diag);
        }

        // A file from another schema is refused with a message, not a panic.
        let mut wrong = bytes.clone();
        wrong[4] = 9;
        assert_eq!(
            Episode::from_binary(&wrong).unwrap_err(),
            EpisodeError::UnsupportedSchemaVersion { found: 9 }
        );
        assert!(matches!(
            Episode::from_binary(b"nope"),
            Err(EpisodeError::Malformed(_))
        ));
    }

    #[test]
    fn json_round_trip() {
        let (mut sim, mut rec) = fixture(10.0);
        record_seconds(&mut sim, &mut rec, 20.0);
        let episode = rec.finish();

        let json = episode.to_json().expect("encodes");
        let back = Episode::from_json(&json).expect("decodes");
        assert_eq!(back.header, episode.header);
        for (i, (a, b)) in episode.frames.iter().zip(back.frames.iter()).enumerate() {
            let (x, y) = (all_scalars(a), all_scalars(b));
            for k in 0..FRAME_LEN {
                // Exact `f64` round trip — the workspace enables serde_json's
                // `float_roundtrip` feature precisely for this.
                assert_eq!(x[k].to_bits(), y[k].to_bits(), "frame {i}, scalar {k}");
            }
        }
        assert_eq!(back.to_json().unwrap(), json);

        let mut future: serde_json::Value = serde_json::from_str(&json).unwrap();
        future["header"]["schema_version"] = serde_json::json!(7);
        assert_eq!(
            Episode::from_json(&future.to_string()).unwrap_err(),
            EpisodeError::UnsupportedSchemaVersion { found: 7 }
        );
        assert!(
            EpisodeError::UnsupportedSchemaVersion { found: 7 }
                .to_string()
                .contains("this build reads 1 and 2"),
            "{}",
            EpisodeError::UnsupportedSchemaVersion { found: 7 }
        );
    }

    #[test]
    fn header_captures_toolchain() {
        // R7: the header says which build produced the episode.
        let info = ToolchainInfo::current();
        assert!(!info.rustc.is_empty(), "rustc version must be captured");
        assert!(info.rustc.contains("rustc"), "got {:?}", info.rustc);
        assert!(!info.target.is_empty(), "target triple must be captured");
        assert!(
            info.profile == "debug" || info.profile == "release",
            "profile must be a cargo profile, got {:?}",
            info.profile
        );
        assert!(info.describe().len() > 10);

        let (_, rec) = fixture(20.0);
        assert_eq!(rec.header().toolchain, info);
        assert_eq!(rec.header().schema_version, EPISODE_SCHEMA_VERSION);
        assert!(!rec.header().created_utc.is_empty());
    }

    #[test]
    fn reward_placeholder_zero() {
        let (mut sim, mut rec) = fixture(20.0);
        record_seconds(&mut sim, &mut rec, 30.0);
        let episode = rec.finish();
        assert!(!episode.frames.is_empty());
        for (i, frame) in episode.frames.iter().enumerate() {
            // Exactly zero, bit for bit.
            assert_eq!(frame.reward.to_bits(), 0.0_f64.to_bits(), "frame {i}");
        }
    }

    #[test]
    fn no_wall_clock_in_physics() {
        // F9.1. `created_utc` is set once, in `start`, from a value the
        // caller supplies; `observe` and everything it calls read no clock.
        let source = include_str!("recording.rs");
        let body = source
            .split_once("pub fn observe(")
            .expect("observe must be declared here")
            .1
            .split_once("\n    }")
            .expect("observe must close")
            .0;
        // Built from fragments: the crate-wide grep in
        // `determinism::no_wall_clock` scans this file too, test code
        // included, so the needles must not be spelled out in the source.
        let banned = [
            ["Ins", "tant"].concat(),
            ["System", "Time"].concat(),
            ["no", "w("].concat(),
            "created_utc".to_string(),
            "iso8601".to_string(),
        ];
        for needle in &banned {
            assert!(
                !body.contains(needle.as_str()),
                "`observe` mentions `{needle}`; the recorder must read no clock"
            );
        }

        // `start` is where the header — and with it `created_utc` — is
        // frozen, and it takes the value rather than producing one.
        let start = source
            .split_once("pub fn start(hz: f64, header: EpisodeHeader)")
            .expect("start must be declared here")
            .1;
        assert!(start.starts_with(" -> Self {"));

        // The crate-wide guard is `determinism::no_wall_clock`; this is the
        // narrow one for the file that would be most tempted.
        let code: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for needle in &banned[..3] {
            assert!(
                !code.contains(needle.as_str()),
                "recording.rs uses `{needle}`"
            );
        }
        assert!(!code.contains(&["std", "::time"].concat()));
    }

    #[test]
    fn frame_layout_is_the_declared_order() {
        let diag = FrameDiagnostics {
            wind_speed: 600.0,
            wind_bearing_deg: 601.0,
            true_wind_body: [602.0, 603.0],
            apparent_wind_body: [604.0, 605.0, 606.0],
            apparent_wind_speed: 607.0,
            apparent_wind_angle: 608.0,
            speed_over_ground: 609.0,
            acceleration_body: [610.0, 611.0],
            total_force_h: [612.0, 613.0],
            sheet_force: [614.0, 615.0, 616.0],
            sail_ce_b: [617.0, 618.0, 619.0],
            board_centre_b: [620.0, 621.0, 622.0],
            rudder_centre_b: [623.0, 624.0, 625.0],
            sheet_attach_b: [626.0, 627.0, 628.0],
            sheet_block_b: [629.0, 630.0, 631.0],
            alpha_sail: 632.0,
            alpha_board: 633.0,
            alpha_rudder: 634.0,
            sheet_rope_length: 635.0,
            sheet_extension: 636.0,
            gz: 637.0,
            capsize_since: 638.0,
            capsize_max_heel: 639.0,
        };
        let frame = EpisodeFrame {
            t: 1.0,
            state: std::array::from_fn(|i| 10.0 + i as f64),
            controls: [100.0, 101.0, 1.0],
            wind_at_boat: [200.0, 201.0],
            forces: std::array::from_fn(|i| 300.0 + i as f64),
            moments: std::array::from_fn(|i| 400.0 + i as f64),
            sheet_tension: 500.0,
            reward: 0.0,
            capsized: true,
            diag: Some(diag),
        };

        let a = frame.to_common_array();
        assert_eq!(a.len(), FRAME_LEN_V1);
        assert_eq!(FRAME_LEN_V1, 38);
        assert_eq!(a[0], 1.0);
        assert_eq!(a[1], 10.0);
        assert_eq!(a[1 + STATE_LEN], 100.0);
        assert_eq!(a[1 + STATE_LEN + 3], 200.0);
        assert_eq!(a[1 + STATE_LEN + 5], 300.0);
        assert_eq!(a[1 + STATE_LEN + 17], 400.0);
        assert_eq!(a[FRAME_LEN_V1 - 3], 500.0);
        assert_eq!(a[FRAME_LEN_V1 - 2], 0.0);
        assert_eq!(a[FRAME_LEN_V1 - 1], 1.0);

        // The diagnostics block is exactly the declared order, and the whole
        // frame is the schema-1 run followed by it — a strict extension.
        let d = frame.diagnostics_array().expect("the block is present");
        assert_eq!(d.len(), DIAG_LEN);
        assert_eq!(DIAG_LEN, 40);
        for (i, v) in d.iter().enumerate() {
            assert_eq!(*v, 600.0 + i as f64, "diagnostics scalar {i}");
        }
        assert_eq!(FRAME_LEN, FRAME_LEN_V1 + DIAG_LEN);

        let whole = all_scalars(&frame);
        assert_eq!(whole.len(), FRAME_LEN);
        assert_eq!(EpisodeFrame::from_scalars(&whole).unwrap(), frame);
        // …and the first 38 on their own decode as a schema-1 frame, with the
        // block absent rather than zeroed.
        let legacy = EpisodeFrame::from_scalars(&whole[..FRAME_LEN_V1]).unwrap();
        assert_eq!(legacy.diag, None);
        assert_eq!(legacy.to_common_array(), a);
        assert!(EpisodeFrame::from_scalars(&whole[..20]).is_err());
    }

    #[test]
    fn iso8601_is_a_pure_function_of_its_argument() {
        assert_eq!(iso8601_utc(0.0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601_utc(1_000.0), "1970-01-01T00:00:01.000Z");
        // 2026-09-20T00:00:00Z
        assert_eq!(iso8601_utc(1_789_862_400_000.0), "2026-09-20T00:00:00.000Z");
        assert_eq!(
            iso8601_utc(1_789_862_400_000.0 + 3_661_123.0),
            "2026-09-20T01:01:01.123Z"
        );
        // A leap day, and the same input twice.
        assert_eq!(iso8601_utc(1_709_164_800_000.0), "2024-02-29T00:00:00.000Z");
        assert_eq!(iso8601_utc(f64::NAN), "");
        assert_eq!(iso8601_utc(12_345.0), iso8601_utc(12_345.0));
    }

    // -----------------------------------------------------------------------
    // Schema 2 — the diagnostic subset
    // -----------------------------------------------------------------------

    #[test]
    fn captured_diagnostics_equal_the_live_record_at_the_sample_time() {
        // Task 10.2's acceptance, asserted where it can be asserted bit for
        // bit: the block beside a sample is the record built for that state.
        let (mut sim, mut rec) = fixture(20.0);
        sim.set_controls(Controls {
            rudder_rate_cmd: -0.4,
            sheet_rate_cmd: 0.7,
            sheet_release: false,
        });
        record_seconds(&mut sim, &mut rec, 12.0);
        let live = diagnostics(&sim);
        let expected = FrameDiagnostics::of(&live);
        let episode = rec.finish();
        let last = episode.frames.last().expect("frames");

        // The last sample is at or one step before the final state; step to
        // the sample's own time and compare there.
        assert!((last.t - live.t).abs() <= 0.05 + 1e-12);
        let captured = last.diag.expect("schema 2 records the block");
        if (last.t - live.t).abs() < 1e-12 {
            for (i, (a, b)) in captured
                .to_array()
                .iter()
                .zip(expected.to_array().iter())
                .enumerate()
            {
                assert_eq!(a.to_bits(), b.to_bits(), "diagnostics scalar {i}");
            }
        }

        // The frame's own schema-1 fields and its block describe one state:
        // `sheet_tension` and `gz` are independent readings of the same step.
        assert!(
            episode.frames.iter().any(|f| f.sheet_tension > 1.0),
            "the fixture must load the sheet"
        );
        assert!(
            episode
                .frames
                .iter()
                .any(|f| f.diag.expect("block").gz.abs() > 1e-6),
            "the fixture must heel"
        );
        // The wind block agrees with the vector recorded beside it.
        for f in &episode.frames {
            let d = f.diag.expect("block");
            let (speed, bearing) =
                wind_to_bearing(crate::vec::Vec2::new(f.wind_at_boat[0], f.wind_at_boat[1]));
            assert_eq!(d.wind_speed.to_bits(), speed.to_bits());
            assert_eq!(d.wind_bearing_deg.to_bits(), bearing.to_bits());
        }
    }

    #[test]
    fn the_omitted_diagnostics_are_named_at_their_source() {
        // RV59: a field that is not recorded must be *known* not to be
        // recorded, so the replay can say so instead of computing it. The
        // declaration's doc comment is the list, and this asserts that every
        // `Diagnostics` field is either carried somewhere in a frame or named
        // in that list — so a field added to `diagnostics.rs` cannot drift
        // into being silently absent.
        let recording = include_str!("recording.rs");
        let omitted_block = recording
            .split_once("/// ## What is deliberately **not** recorded")
            .expect("the omission list must be declared")
            .1
            .split_once("/// **Field order is normative**")
            .expect("the list must close")
            .0;

        let diag_src = include_str!("diagnostics.rs");
        let decl = diag_src
            .split_once("pub struct Diagnostics {")
            .expect("the record must be declared")
            .1
            .split_once("\n}")
            .expect("the declaration must close")
            .0;
        let declared: Vec<&str> = decl
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub "))
            .filter_map(|l| l.split_once(':'))
            .map(|(name, _)| name)
            .collect();
        assert!(declared.len() > 40, "only {} fields parsed", declared.len());

        // Fields a frame carries outside the diagnostics block, either as a
        // schema-1 field or as part of the F3 state.
        const IN_THE_FRAME: [&str; 15] = [
            "t",
            "true_wind_world",
            "velocity_body",
            "yaw_rate",
            "roll_rate",
            "sail",
            "board",
            "rudder",
            "hull",
            "yaw_moment",
            "heeling_moment",
            "righting_moment",
            "beta",
            "beta_dot",
            "sheet_tension",
        ];
        // Fields a replay assembles from more than one recorded quantity:
        // `heel_deg` is `state.phi` in degrees (F1 permits degrees at a UI
        // boundary) and `capsize` is the frame's `capsized` flag beside the
        // block's `capsize_since` and `capsize_max_heel`.
        const ASSEMBLED: [&str; 2] = ["heel_deg", "capsize"];
        /// Recorded under a different name, because the block is flat.
        const RENAMED: [(&str, &str); 1] = [("sheet", "sheet_force")];

        let block_src = recording
            .split_once("pub struct FrameDiagnostics {")
            .expect("the block must be declared")
            .1
            .split_once("\n}")
            .expect("the block must close")
            .0;

        for field in &declared {
            if IN_THE_FRAME.contains(field) || ASSEMBLED.contains(field) {
                continue;
            }
            let renamed = RENAMED
                .iter()
                .find(|(from, _)| from == field)
                .map(|(_, to)| *to);
            let carried = block_src.contains(&format!("pub {}:", renamed.unwrap_or(field)));
            let named = omitted_block.contains(&format!("`{field}`"));
            assert!(
                carried || named,
                "`{field}` is neither recorded in FrameDiagnostics nor named in the \
                 omission list; a replay would have no honest answer for it"
            );
            assert!(
                !(carried && named),
                "`{field}` is both recorded and listed as omitted"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Identity
    // -----------------------------------------------------------------------

    #[test]
    fn a_manual_episode_marks_the_research_fields_not_applicable() {
        let (_, rec) = fixture(20.0);
        let id = rec.header().identity();
        assert_eq!(id.identity_version, IDENTITY_VERSION);
        assert!(id.model.is_known());
        assert!(id.parameters.is_known());
        assert!(id.dt.is_known());
        assert!(id.integrator.is_known());
        assert!(id.scenario.is_known());
        assert!(id.wind.is_known());
        assert!(id.seed.is_known());
        // Unknown is not the same as not applicable, and a hand-flown episode
        // is the latter on all three.
        assert!(id.task.is_not_applicable());
        assert!(id.action.is_not_applicable());
        assert!(id.observation.is_not_applicable());
        // …and the canonical text is stable.
        assert_eq!(
            id.canonical_json(),
            rec.header().identity().canonical_json()
        );
        assert!(id.canonical_json().contains("not_applicable"));
    }

    #[test]
    fn same_conditions_needs_every_field_and_a_clean_source() {
        let (mut sim, mut rec) = fixture(20.0);
        record_seconds(&mut sim, &mut rec, 2.0);
        let episode = rec.finish();
        let a = episode.header.identity();

        // The compiled source id may be dirty in a working tree, in which case
        // nothing is comparable and the verdict says why (F18.1d).
        let verdict = a.compare(&a);
        if ModelIdentity::current().source.is_known() {
            assert_eq!(verdict, Comparability::SameConditions, "{verdict:?}");
        } else {
            assert!(
                matches!(verdict, Comparability::Indeterminate(ref r) if r.as_slice() == ["model"]),
                "{verdict:?}"
            );
        }

        // A changed equation: the same parameters under a different source.
        let mut other = a.clone();
        other.model = Recorded::Value(ModelIdentity {
            model_version: crate::identity::MODEL_VERSION,
            source: crate::identity::SourceId {
                tree: "f".repeat(40),
                state: crate::identity::SourceState::Clean,
            },
        });
        let mut mine = a.clone();
        mine.model = Recorded::Value(ModelIdentity {
            model_version: crate::identity::MODEL_VERSION,
            source: crate::identity::SourceId {
                tree: "e".repeat(40),
                state: crate::identity::SourceState::Clean,
            },
        });
        assert_eq!(
            mine.compare(&other),
            Comparability::Different(vec!["model".to_string()])
        );

        // A changed seed, dt, initial condition or task threshold: each one on
        // its own makes a strict comparison incompatible.
        type Edit = Box<dyn Fn(&mut ExperimentIdentity)>;
        let cases: Vec<(&str, Edit)> = vec![
            (
                "seed",
                Box::new(|id: &mut ExperimentIdentity| id.seed = Recorded::Value(999)),
            ),
            (
                "dt",
                Box::new(|id: &mut ExperimentIdentity| id.dt = Recorded::Value(0.01)),
            ),
            (
                "initial_state",
                Box::new(|id: &mut ExperimentIdentity| {
                    let mut st = *id.initial_state.value().expect("known");
                    st.u += 1.0;
                    id.initial_state = Recorded::Value(st);
                }),
            ),
            (
                "task",
                Box::new(|id: &mut ExperimentIdentity| {
                    let mut thresholds = BTreeMap::new();
                    thresholds.insert("heel_deg".to_string(), 25.0);
                    id.task = Recorded::Value(TaskIdentity {
                        id: "hold_a_course".to_string(),
                        version: 1,
                        thresholds,
                    });
                }),
            ),
            (
                "parameters",
                Box::new(|id: &mut ExperimentIdentity| {
                    let mut p = *id.parameters.value().expect("known");
                    p.stability.gm += 0.01;
                    id.parameters = Recorded::Value(p);
                }),
            ),
        ];
        for (name, edit) in cases {
            let mut changed = mine.clone();
            edit(&mut changed);
            let verdict = mine.compare(&changed);
            assert_eq!(
                verdict,
                Comparability::Different(vec![name.to_string()]),
                "changing {name} must make a strict comparison incompatible"
            );
            assert!(!verdict.is_same_conditions());
            assert!(verdict.describe().contains(name));
        }
    }

    #[test]
    fn an_unknown_identity_blocks_comparison_but_not_viewing() {
        // The legacy fixture is a `close_hauled` episode, so the comparison
        // below is between two runs of the **same** scenario: what blocks it
        // is the missing identity and nothing else.
        let sc = load_shipped("close_hauled").expect("shipped scenario");
        let params = sc.to_parameters().expect("valid parameters");
        let rec = Recorder::start(10.0, header(&sc, &params, 10.0));
        let known = rec.header().identity();

        // A schema-1 header: everything the document always carried is still
        // known, and the rest is Unknown — never NotApplicable, because a v1
        // file cannot say whether a task existed.
        let legacy = Episode::from_json(LEGACY_JSON).expect("the legacy fixture decodes");
        let old = legacy.header.identity();
        assert_eq!(old.identity_version, 0);
        assert_eq!(old.model, Recorded::Unknown);
        assert_eq!(old.initial_state, Recorded::Unknown);
        assert_eq!(old.initial_controls, Recorded::Unknown);
        assert_eq!(old.task, Recorded::Unknown);
        assert_eq!(old.action, Recorded::Unknown);
        assert_eq!(old.observation, Recorded::Unknown);
        // …and the fields a v1 header did carry are still readable, which is
        // what "can be viewed" means.
        assert!(old.parameters.is_known());
        assert!(old.scenario.is_known());
        assert!(old.wind.is_known());
        assert!(old.seed.is_known());
        assert!(old.dt.is_known());

        let verdict = old.compare(&known);
        assert!(!verdict.is_same_conditions());
        assert!(
            matches!(verdict, Comparability::Indeterminate(_)),
            "{verdict:?}"
        );
        assert!(verdict.reasons().contains(&"model".to_string()));
        assert!(verdict.reasons().contains(&"initial_state".to_string()));
        assert!(verdict.describe().starts_with("not comparable"));

        // Two unknowns are not equal to each other either.
        assert!(!old.compare(&old).is_same_conditions());

        // NotApplicable on one side and a value on the other is a real
        // difference, not an unknown.
        let mut agent = known.clone();
        agent.action = Recorded::Value(ActionIdentity {
            adapter: "rates".to_string(),
            version: 1,
            period_steps: 10,
        });
        assert_eq!(
            known.compare(&agent),
            Comparability::Different(vec!["action".to_string()])
        );
    }

    #[test]
    fn the_practice_envelope_is_typed_optional_and_versioned() {
        // Section 11's reservation: the task crate writes through *this*
        // recorder, and an event carries the physics step it was decided on.
        let (mut sim, mut rec) = fixture(20.0);
        assert!(
            !rec.push_practice_event(PracticeEvent {
                id: "start".to_string(),
                step: 0,
                t: 0.0,
                value: 0.0,
            }),
            "an event without an envelope has no task to belong to"
        );

        let mut thresholds = BTreeMap::new();
        thresholds.insert("max_heel_deg".to_string(), 30.0);
        thresholds.insert("time_limit_s".to_string(), 45.0);
        rec.set_practice(PracticeEnvelope {
            envelope_version: PRACTICE_ENVELOPE_VERSION,
            task: TaskIdentity {
                id: "hold_a_beam_reach".to_string(),
                version: 3,
                thresholds,
            },
            events: Vec::new(),
        });
        record_seconds(&mut sim, &mut rec, 3.0);
        assert!(rec.push_practice_event(PracticeEvent {
            id: "heel_exceeded".to_string(),
            step: 417,
            t: 2.085,
            value: 31.4,
        }));

        let episode = rec.finish();
        let id = episode.header.identity();
        let task = id.task.value().expect("the task is known now");
        assert_eq!(task.id, "hold_a_beam_reach");
        assert_eq!(task.version, 3);
        assert_eq!(task.thresholds["max_heel_deg"], 30.0);
        // The envelope survives both codecs, and the events keep their order.
        let json = episode.to_json().expect("json");
        let back = Episode::from_json(&json).expect("decodes");
        assert_eq!(back.header.practice, episode.header.practice);
        let bytes = episode.to_binary().expect("binary");
        assert_eq!(
            Episode::from_binary(&bytes).expect("decodes").header,
            episode.header
        );
        let envelope = back.header.practice.expect("present");
        assert_eq!(envelope.envelope_version, PRACTICE_ENVELOPE_VERSION);
        assert_eq!(envelope.events.len(), 1);
        assert_eq!(envelope.events[0].step, 417);
    }

    #[test]
    fn the_header_records_the_resolved_catalogue_and_the_initial_condition() {
        // Task 10.2: "exactly resolved defaults/overrides, not merely a
        // scenario name".
        let (mut sim, mut rec) = fixture(20.0);
        record_seconds(&mut sim, &mut rec, 1.0);
        let episode = rec.finish();
        let h = &episode.header;
        let resolved = h.scenario.to_parameters().expect("valid");
        assert_eq!(h.parameters, resolved);
        assert_eq!(h.dt, resolved.sim.dt);
        // The recorded initial condition is the **first sample's** state, so
        // an ad-hoc reset or a recording started mid-run is described
        // honestly rather than by the scenario's own opening line.
        let start = h.initial_state.value().expect("known");
        let first = episode.frames.first().expect("frames");
        for (i, name) in STATE_FIELDS.iter().enumerate() {
            assert_eq!(
                start.to_array()[i].to_bits(),
                first.state[i].to_bits(),
                "initial state field {name}"
            );
        }
        assert_eq!(*h.initial_controls.value().expect("known"), *sim.controls());

        // …and an override really is resolved into the catalogue, rather than
        // being left for a reader to apply from the scenario's name.
        let mut edited = h.scenario.clone();
        edited
            .parameter_overrides
            .insert("sail.area".to_string(), 6.0);
        let resolved_edit = edited.to_parameters().expect("valid");
        assert_eq!(resolved_edit.sail.section.area, 6.0);
        assert_ne!(resolved_edit.sail.section.area, resolved.sail.section.area);
        let edited_header = header(&edited, &resolved_edit, 20.0);
        assert_eq!(edited_header.parameters, resolved_edit);
        assert_eq!(
            edited_header.scenario.parameter_overrides["sail.area"], 6.0,
            "the overrides travel beside the resolved values, not instead of them"
        );
    }

    // -----------------------------------------------------------------------
    // Migration and rejection
    // -----------------------------------------------------------------------

    #[test]
    fn the_checked_in_schema_1_fixtures_still_decode_and_round_trip() {
        // RV60, both codecs and both directions.
        for (name, episode) in [
            (
                "json",
                Episode::from_json(LEGACY_JSON).expect("legacy json"),
            ),
            (
                "binary",
                Episode::from_binary(LEGACY_BIN).expect("legacy binary"),
            ),
        ] {
            assert_eq!(episode.schema_version(), 1, "{name}");
            assert!(episode.frames.len() > 5, "{name}");
            assert!(
                episode.frames.iter().all(|f| f.diag.is_none()),
                "{name}: a v1 file has no diagnostics block, and none may be invented"
            );
            // Units and meaning survive: the state is still 13 scalars in F8.3
            // order and the recorded quantities are still what they were.
            for f in &episode.frames {
                assert_eq!(f.state.len(), STATE_LEN);
                assert_eq!(f.forces.len(), 12);
                assert_eq!(f.moments.len(), 4);
                assert_eq!(f.reward, 0.0);
            }
            // Re-encoded in its own schema, not silently upgraded.
            let json = episode.to_json().expect("re-encodes as json");
            assert_eq!(Episode::from_json(&json).expect("round trip"), episode);
            let bytes = episode.to_binary().expect("re-encodes as binary");
            assert_eq!(
                &bytes[4..8],
                &1u32.to_le_bytes(),
                "{name}: schema preserved"
            );
            assert_eq!(
                u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize,
                FRAME_LEN_V1,
                "{name}: a v1 file keeps the v1 frame width"
            );
            assert_eq!(Episode::from_binary(&bytes).expect("round trip"), episode);
        }
        // The two fixtures are the same episode in two forms.
        assert_eq!(
            Episode::from_json(LEGACY_JSON).unwrap(),
            Episode::from_binary(LEGACY_BIN).unwrap()
        );
    }

    #[test]
    fn a_bad_document_is_refused_and_names_the_fault() {
        let (mut sim, mut rec) = fixture(20.0);
        record_seconds(&mut sim, &mut rec, 2.0);
        let episode = rec.finish();
        let good = episode.to_binary().expect("encodes");

        let message = |e: EpisodeError| e.to_string();

        // Bad magic.
        let mut magic = good.clone();
        magic[0] = b'X';
        assert!(message(Episode::from_binary(&magic).unwrap_err()).contains("bad magic"));

        // Truncation, at the frame block and inside the header.
        let short = &good[..good.len() - 9];
        assert!(message(Episode::from_binary(short).unwrap_err()).contains("short"));
        assert!(Episode::from_binary(&good[..BINARY_PREFIX + 3]).is_err());

        // A frame width that does not match the declared schema.
        let mut width = good.clone();
        width[16..20].copy_from_slice(&((FRAME_LEN + 1) as u32).to_le_bytes());
        assert!(message(Episode::from_binary(&width).unwrap_err()).contains("scalars"));

        // An absurd header length, refused before it is used to index.
        let mut header_len = good.clone();
        header_len[8..12].copy_from_slice(&(MAX_HEADER_BYTES as u32 + 1).to_le_bytes());
        assert!(message(Episode::from_binary(&header_len).unwrap_err()).contains("limit"));

        // A frame count above the budget, refused before it is allocated for.
        let mut count = good.clone();
        count[12..16].copy_from_slice(&((MAX_EPISODE_FRAMES + 1) as u32).to_le_bytes());
        assert!(message(Episode::from_binary(&count).unwrap_err()).contains("budget"));

        // A non-finite measurement, in either codec.
        let mut nan = good.clone();
        let offset = u32::from_le_bytes([good[20], good[21], good[22], good[23]]) as usize;
        nan[offset..offset + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(message(Episode::from_binary(&nan).unwrap_err()).contains("non-finite"));

        let mut doc: serde_json::Value = serde_json::from_str(&episode.to_json().unwrap()).unwrap();
        doc["frames"][0]["sheet_tension"] = serde_json::json!(f64::MAX * 10.0);
        // `serde_json` writes an out-of-range float as `null`, which is a shape
        // error; an explicit non-finite reaches the finiteness check instead.
        assert!(Episode::from_json(&doc.to_string()).is_err());

        // A `dt` that is not a timestep.
        let mut bad_dt: serde_json::Value =
            serde_json::from_str(&episode.to_json().unwrap()).unwrap();
        bad_dt["header"]["dt"] = serde_json::json!(0.0);
        assert!(message(Episode::from_json(&bad_dt.to_string()).unwrap_err()).contains("timestep"));

        // A schema-2 document with a frame missing its block.
        let mut gap: serde_json::Value = serde_json::from_str(&episode.to_json().unwrap()).unwrap();
        gap["frames"][1]["diag"] = serde_json::Value::Null;
        assert!(
            message(Episode::from_json(&gap.to_string()).unwrap_err()).contains("no diagnostics")
        );

        // …and none of that touched the episode we already hold.
        assert_eq!(Episode::from_binary(&good).unwrap(), episode);
    }

    #[test]
    fn the_recording_is_bounded_by_a_stated_byte_budget() {
        // Task 10.2's acceptance: a maximum-duration recording stays inside
        // the documented budget, and the budget is bytes/frame × samples.
        assert_eq!(BYTES_PER_FRAME, FRAME_LEN * 8);
        assert_eq!(MAX_EPISODE_FRAMES, MAX_EPISODE_BYTES / BYTES_PER_FRAME);
        const { assert!(MAX_EPISODE_FRAMES * BYTES_PER_FRAME <= MAX_EPISODE_BYTES) };

        // Drive a recorder to its cap with a tiny one, so the property is
        // measured rather than extrapolated.
        let (mut sim, _) = fixture(20.0);
        let sc = load_shipped("beam_reach_capsize").expect("scenario");
        let params = sc.to_parameters().expect("params");
        let mut rec = Recorder::start(200.0, header(&sc, &params, 200.0));
        rec.max_frames = 64;
        for _ in 0..4000 {
            sim.advance(1);
            if rec.due(sim.state().t) {
                let d = diagnostics(&sim);
                rec.observe(&sim, &d);
            }
        }
        assert!(rec.is_full());
        assert_eq!(rec.len(), 64);
        assert_eq!(rec.capacity(), 64);
        // Once full it is not due again, so the caller stops building
        // diagnostics as well as stops allocating.
        assert!(!rec.due(sim.state().t + 100.0));

        let episode = rec.finish();
        let bytes = episode.to_binary().expect("encodes");
        let block = episode.frames.len() * BYTES_PER_FRAME;
        assert!(
            bytes.len() <= block + MAX_HEADER_BYTES,
            "{} bytes for {} frames",
            bytes.len(),
            episode.frames.len()
        );
        // The shipped cap, expressed as the thing a user experiences.
        for (hz, seconds) in [(5.0, 2688.0), (20.0, 672.0), (50.0, 268.0)] {
            let duration = MAX_EPISODE_FRAMES as f64 / hz;
            assert!(
                duration > seconds,
                "{hz} Hz gives {duration} s, below the documented {seconds} s"
            );
        }
    }
}
