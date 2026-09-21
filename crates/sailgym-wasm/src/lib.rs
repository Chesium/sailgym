//! Thin `wasm_bindgen` wrapper over `sailgym-physics` (F8.1).
//!
//! There is no physics here and there never will be: every equation lives in
//! `sailgym-physics` (F8). This file only marshals strings and flat buffers
//! across the boundary. The method set is the subset of F8.2 that exists at
//! this milestone — `new`, `reset`, `set_controls`, `advance`, `snapshot`,
//! `set_parameter`, `parameters_json`, `parameter_meta_json`, `diagnostics`,
//! plus the section 03 wind methods `sample_wind_grid`, `wind_at_boat`,
//! `set_wind` and `wind_json`, and the section 09 scenario and recording
//! methods `scenarios_json`, `scenario_json`, `restart`, `start_recording`,
//! `stop_recording`, `is_recording`, `recorded_frames`, `episode_to_binary`,
//! `episode_from_binary` and `episode_from_json`, plus the v2 section 10
//! inspection methods `recording_capacity`, `recording_full`,
//! `episode_identity_json` and `episode_comparability_json`, plus the v2
//! section 11 practice methods `practice_tasks_json`, `start_practice`,
//! `retry_practice`, `cancel_practice` and `practice_state_json`.
//!
//! ## Section 11 keeps the scoring in Rust, and outside the physics crate
//!
//! A practice attempt is evaluated by `sailgym-task`, once per **completed
//! physics step**, inside the existing [`Sim::advance`] loop — there is no
//! second clock and no second stepping path (v2 F18.4). The evaluator is an
//! observer: it takes a value and returns an outcome, and
//! `sailgym-task`'s `task_evaluation_does_not_perturb_the_physics` compares
//! all thirteen state scalars bit for bit to say so.
//!
//! The attempt also owns its **initial contract**: the resolved catalogue,
//! the complete F3 state, the controls, the wind configuration and the seed,
//! frozen at [`Sim::start_practice`] and restored verbatim by
//! [`Sim::retry_practice`]. That is what makes a retry the same experiment
//! (RV63), and it is deliberately **not** [`Sim::restart`], which keeps a
//! live brief §31 parameter edit. A parameter, catalogue or wind change while
//! an attempt is running ends it as `conditions_changed`, because the
//! conditions it was started under no longer hold.
//!
//! ## Section 10 keeps the codec, the identity and the cap in Rust
//!
//! The episode codec was already the core's. v2 section 10 adds two more
//! things that a browser must not decide for itself: the canonical
//! [`ExperimentIdentity`](sailgym_physics::recording::ExperimentIdentity) —
//! whether two episodes describe the same conditions is a physics question,
//! not a UI one — and the recording cap, which is `Recorder`'s and is only
//! *reported* here so the page can show it before memory runs out. Both cross
//! the boundary as JSON strings, coarse-grained, one call each (brief §24).
//!
//! ## The one thing this file reads that the core may not
//!
//! `EpisodeHeader::created_utc` is metadata and never reaches the physics
//! (F9.1), so the physics crate is forbidden from producing it. The browser's
//! `Date.now()` is bound below and formatted by the core's pure
//! `recording::iso8601_utc`, which takes the millisecond count as an
//! argument. That is the whole of the wall clock in sailgym's browser build.
//!
//! The API is coarse-grained by construction (brief §24): per-force-component
//! and per-entity calls are forbidden.

use std::collections::BTreeMap;

use sailgym_physics::environment::wind::WindConfig;
use sailgym_physics::environment::{wind_to_bearing, WindField};
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::recording::{
    iso8601_utc, Comparability, Episode, EpisodeHeader, Recorder, BYTES_PER_FRAME,
    MAX_EPISODE_FRAMES,
};
use sailgym_physics::scenario::{
    load_all_shipped, CameraSuggestion, InitialState, Scenario, SCENARIO_SCHEMA_VERSION,
};
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls};
use sailgym_task::{StepObservation, TaskId, TaskRun, TaskSpec, TASK_IDS};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `Date.now()`. The only clock in the browser build, and it feeds
    /// nothing but `EpisodeHeader::created_utc` (F9.1).
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now_ms() -> f64;
}

/// Installs the panic hook so a Rust panic surfaces as a JS console error
/// rather than an opaque `unreachable` trap. `wasm_bindgen(start)` makes the
/// glue call it during module init, so JS never has to.
///
/// `js_name` is required, not cosmetic: `wasm-bindgen-test` emits its own
/// `main` export into the integration-test module, and two crates exporting
/// `main` is a hard `wasm-bindgen` error ("the name `main` is exported by
/// multiple crates in this build"). The Rust signature stays `pub fn main()`
/// exactly as task 1.2 specifies; only the JS-side symbol is renamed.
#[wasm_bindgen(start, js_name = sailgymStart)]
pub fn main() {
    console_error_panic_hook::set_once();
}

fn js_err(context: &str, e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{context}: {e}"))
}

/// Deserialise and validate a `WindConfig`. Validation happens here rather
/// than inside the field so that a bad configuration is a JS exception at the
/// call site, not a `NaN` that spreads silently through the visualization.
fn parse_wind(value: &serde_json::Value) -> Result<WindConfig, JsValue> {
    let cfg: WindConfig =
        serde_json::from_value(value.clone()).map_err(|e| js_err("invalid wind", e))?;
    cfg.validate().map_err(|e| js_err("invalid wind", e))?;
    Ok(cfg)
}

/// A scenario document describing a run that did **not** come from one of the
/// six shipped files — an ad-hoc `reset({ state, wind })` from a test fixture.
///
/// The recording header has to say honestly where the episode began, and
/// `EpisodeHeader` carries the fully resolved `BoatParameters` alongside this,
/// so the empty `parameter_overrides` costs nothing: the catalogue that was
/// actually in force is recorded either way.
fn ad_hoc_scenario(inner: &Simulation) -> Scenario {
    Scenario {
        schema_version: SCENARIO_SCHEMA_VERSION,
        name: "custom".to_string(),
        description: "An ad-hoc initial condition, not one of the six shipped scenarios."
            .to_string(),
        seed: inner.seed(),
        parameter_overrides: BTreeMap::new(),
        initial_state: InitialState::from_boat_state(inner.state()),
        wind: *inner.wind().config(),
        camera: CameraSuggestion::default(),
        initial_controls: None,
    }
}

/// Where a practice attempt stands (v2 section 11).
///
/// `Finished` is the evaluator's verdict — succeeded, failed or timed out,
/// which [`TaskRun::outcome`] carries. The other two are the browser's:
/// something ended the attempt that was not the boat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttemptStatus {
    /// Being evaluated on every step.
    Active,
    /// The evaluator reached a terminal outcome.
    Finished,
    /// A reset, a restart or a scenario change ended it. There is no result.
    Cancelled,
    /// A parameter, catalogue or wind change ended it. The conditions it was
    /// started under no longer hold, so no result it could produce would be
    /// comparable with another attempt (RV63).
    ConditionsChanged,
}

impl AttemptStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Finished => "finished",
            Self::Cancelled => "cancelled",
            Self::ConditionsChanged => "conditions_changed",
        }
    }
}

/// The initial contract an attempt was started under, frozen.
///
/// **This is what "retry restores the exact conditions" means.** Every field
/// is a resolved value, not a document that would be resolved again: the
/// catalogue is the one that was in force, the state is the complete F3
/// state, and the wind is the configuration plus the seed it was built from.
/// Re-deriving any of them from the scenario at retry time would re-apply
/// `parameter_overrides` to whatever the catalogue happens to be now, which
/// is the defect RV63 names.
#[derive(Clone)]
struct Conditions {
    scenario: Scenario,
    params: BoatParameters,
    state: BoatState,
    controls: Controls,
    wind: WindConfig,
    seed: u64,
    log_hz: f64,
}

/// One practice attempt: the frozen contract, the evaluator and its status.
struct Attempt {
    spec: TaskSpec,
    run: TaskRun,
    conditions: Conditions,
    status: AttemptStatus,
}

/// The simulation handle JavaScript holds.
#[wasm_bindgen]
pub struct Sim {
    inner: Simulation,
    built: String,
    /// The scenario the current run started from, for the recording header.
    scenario: Scenario,
    /// The document the last [`Sim::reset`] was given, verbatim.
    ///
    /// [`Sim::restart`] replays it. Kept as text rather than as the parsed
    /// `Scenario` because the browser's ad-hoc fixtures carry a full
    /// thirteen-field F3 state, and `InitialState` — being the human-facing
    /// form (F1) — cannot express `v`, `r`, `p`, `β̇` or `δr`.
    reset_document: String,
    /// `Some` while recording (brief §33). An observer: see
    /// `sailgym_physics::recording`.
    recorder: Option<Recorder>,
    /// `Some` once a practice challenge has been started, and until it is
    /// cancelled. A finished attempt is kept so the page can show the result.
    attempt: Option<Attempt>,
}

#[wasm_bindgen]
impl Sim {
    /// Construct from a JSON configuration document.
    ///
    /// Recognised keys, both optional:
    ///
    /// ```json
    /// { "seed": 0,
    ///   "parameters": { ... the full F7 catalogue ... },
    ///   "wind": { ... the full WindConfig ... } }
    /// ```
    ///
    /// `{}` yields the ILCA 7 defaults, the default wind and seed 0.
    /// Scenario-shaped documents arrive in section 09.
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<Sim, JsValue> {
        let config: serde_json::Value =
            serde_json::from_str(config_json).map_err(|e| js_err("invalid config JSON", e))?;

        let seed = config
            .get("seed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let params = match config.get("parameters") {
            Some(p) => serde_json::from_value::<BoatParameters>(p.clone())
                .map_err(|e| js_err("invalid parameters", e))?,
            None => BoatParameters::ilca7(),
        };
        params
            .validate()
            .map_err(|e| js_err("invalid parameters", e))?;

        let mut inner = Simulation::new(params, seed);
        if let Some(w) = config.get("wind") {
            inner.set_wind(parse_wind(w)?);
        }

        let scenario = match config.get("scenario") {
            Some(doc) => {
                let sc =
                    Scenario::load(&doc.to_string()).map_err(|e| js_err("invalid scenario", e))?;
                inner
                    .load_scenario(&sc)
                    .map_err(|e| js_err("invalid scenario", e))?;
                sc
            }
            None => ad_hoc_scenario(&inner),
        };

        Ok(Sim {
            inner,
            built: env!("CARGO_PKG_VERSION").to_string(),
            scenario,
            reset_document: "{}".to_string(),
            recorder: None,
            attempt: None,
        })
    }

    /// Restart the simulation.
    ///
    /// Two document shapes are accepted, told apart by `schema_version`:
    ///
    /// * **A full scenario** (section 09) — everything `Scenario` carries:
    ///   parameter overrides, initial state, wind, seed and the controls in
    ///   force at `t = 0`. This is what `scenarios_json()` returns and what
    ///   the picker hands back.
    /// * **An ad-hoc state** — the section 02–08 shape, `{ seed?, state?,
    ///   wind? }`, still supported because the browser test fixtures are
    ///   written in it. The stored scenario is refreshed from the resulting
    ///   state so a recording started afterwards still describes its own
    ///   initial condition honestly.
    ///
    /// An in-progress recording is discarded: its header describes a run that
    /// no longer exists (brief §33).
    pub fn reset(&mut self, scenario_json: &str) -> Result<(), JsValue> {
        // A reset replaces the run a practice attempt was being flown in, so
        // the attempt is over. It is a *cancellation*, not a result: there is
        // nothing to compare and nothing to show (v2 section 11).
        self.end_attempt(AttemptStatus::Cancelled);
        self.apply(scenario_json, true)?;
        self.reset_document = scenario_json.to_string();
        Ok(())
    }

    /// Restart from the document the last [`Sim::reset`] was given,
    /// **keeping the parameter catalogue currently in force**.
    ///
    /// This is what the Reset button and the `R` key do, and the distinction
    /// from [`Sim::reset`] is deliberate. brief §31 makes the whole F7
    /// catalogue live-editable and section 08 made a `sim.*` edit
    /// *reset-required*; a reset that re-applied the scenario's parameters
    /// would throw away the very edit it exists to make good. Choosing a
    /// scenario in the picker is the other case — its `parameter_overrides`
    /// are the point of choosing it — and that goes through `reset`.
    pub fn restart(&mut self) -> Result<(), JsValue> {
        // Same reasoning as `reset`, and the reason a retry does **not** go
        // through here: `restart` keeps the parameter catalogue currently in
        // force, which is exactly what an attempt's frozen contract must not
        // do (RV63). [`Sim::retry_practice`] is the practice path.
        self.end_attempt(AttemptStatus::Cancelled);
        let document = std::mem::take(&mut self.reset_document);
        let outcome = self.apply(&document, false);
        self.reset_document = document;
        outcome
    }

    /// The body of [`Sim::reset`] and [`Sim::restart`]. `with_parameters`
    /// decides whether a scenario document's `parameter_overrides` are
    /// applied.
    fn apply(&mut self, scenario_json: &str, with_parameters: bool) -> Result<(), JsValue> {
        let document: serde_json::Value =
            serde_json::from_str(scenario_json).map_err(|e| js_err("invalid scenario JSON", e))?;
        self.recorder = None;

        if document.get("schema_version").is_some() {
            let sc = Scenario::load(scenario_json).map_err(|e| js_err("invalid scenario", e))?;
            let outcome = if with_parameters {
                self.inner.load_scenario(&sc)
            } else {
                self.inner.restart_scenario(&sc)
            };
            outcome.map_err(|e| js_err("invalid scenario", e))?;
            self.scenario = sc;
            return Ok(());
        }

        let seed = document
            .get("seed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_else(|| self.inner.seed());
        let state = match document.get("state") {
            Some(s) => serde_json::from_value::<BoatState>(s.clone())
                .map_err(|e| js_err("invalid initial state", e))?,
            None => Simulation::initial_state(self.inner.params()),
        };

        self.inner.reset(state, seed);
        if let Some(w) = document.get("wind") {
            self.inner.set_wind(parse_wind(w)?);
        }
        self.scenario = ad_hoc_scenario(&self.inner);
        Ok(())
    }

    // --- scenarios (section 09) -------------------------------------------

    /// All six shipped scenarios (brief §32) as one JSON array of documents.
    ///
    /// Coarse-grained (brief §24): the whole catalogue in a single call, and
    /// the documents are the core's own, so the browser never restates a
    /// scenario's contents.
    pub fn scenarios_json(&self) -> Result<JsValue, JsValue> {
        let all = load_all_shipped().map_err(|e| js_err("shipped scenarios", e))?;
        let json = serde_json::to_string(&all).map_err(|e| js_err("scenarios_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// The scenario the current run started from, as a JSON string.
    pub fn scenario_json(&self) -> Result<JsValue, JsValue> {
        let json = serde_json::to_string(&self.scenario).map_err(|e| js_err("scenario_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    // --- recording (brief §33, section 09) --------------------------------

    /// Begin recording at `hz` samples of **simulated** time per second.
    ///
    /// The current state is logged immediately, so frame 0 of the episode is
    /// the state the recording was started from. The recorder is an observer
    /// and cannot change a trajectory.
    pub fn start_recording(&mut self, hz: f64) {
        self.begin_recording(hz);
    }

    /// The body of [`Sim::start_recording`], reused by the practice path.
    ///
    /// When an attempt is in progress the reserved `PracticeEnvelope` is
    /// attached **before** the first sample, because `push_practice_event`
    /// refuses an event with no envelope and because a recording stopped
    /// mid-attempt should still say which task it was flown against
    /// (`docs/v2/recording-format.md` §6). No second recorder and no second
    /// `Episode` type is created: section 10 reserved this slot and this is
    /// the one thing that fills it.
    fn begin_recording(&mut self, hz: f64) {
        let header = EpisodeHeader::manual(
            self.scenario.clone(),
            *self.inner.params(),
            hz,
            // The one clock reading in the browser build. Metadata only: the
            // core formats it and nothing in the physics ever sees it (F9.1).
            iso8601_utc(date_now_ms()),
            *self.inner.state(),
            *self.inner.controls(),
        );
        let mut rec = Recorder::start(hz, header);
        if let Some(a) = self.attempt.as_ref() {
            rec.set_practice(a.run.envelope());
        }
        let d = sailgym_physics::diagnostics::diagnostics(&self.inner);
        rec.observe(&self.inner, &d);
        self.recorder = Some(rec);
    }

    /// Whether a recording is in progress.
    pub fn is_recording(&self) -> bool {
        self.recorder.is_some()
    }

    /// Frames logged so far, for the record button's readout.
    pub fn recorded_frames(&self) -> u32 {
        self.recorder.as_ref().map_or(0, |r| r.len() as u32)
    }

    /// The frames a recording will accept in total
    /// (`recording::MAX_EPISODE_FRAMES`).
    ///
    /// Reported so the page can show the bound *before* it is reached rather
    /// than growing until the tab dies. The cap itself is the core's and is
    /// derived from a stated byte budget; see `docs/v2/recording-format.md` §8.
    /// Available whether or not a recording is in progress, so the readout can
    /// quote it from the moment the page loads.
    pub fn recording_capacity(&self) -> u32 {
        self.recorder
            .as_ref()
            .map_or(MAX_EPISODE_FRAMES, Recorder::capacity) as u32
    }

    /// Bytes one recorded sample occupies in the binary form
    /// (`recording::BYTES_PER_FRAME`), so the page's size readout is
    /// `bytes per frame × samples` and not a number somebody guessed.
    pub fn recording_bytes_per_frame(&self) -> u32 {
        BYTES_PER_FRAME as u32
    }

    /// The cap has been reached; nothing further is being logged.
    pub fn recording_full(&self) -> bool {
        self.recorder.as_ref().is_some_and(Recorder::is_full)
    }

    /// The canonical [`ExperimentIdentity`] of an episode document, as JSON.
    ///
    /// The record is built by the core from the episode's own header, so a
    /// schema-1 file comes back with `model`, `initial_state`,
    /// `initial_controls`, `task`, `action` and `observation` as `"unknown"` —
    /// which is the honest answer and is what stops the page calling it a
    /// same-conditions experiment (v2 F18.3).
    pub fn episode_identity_json(&self, episode_json: &str) -> Result<JsValue, JsValue> {
        let episode = Episode::from_json(episode_json).map_err(|e| js_err("episode", e))?;
        let json = serde_json::to_string(&episode.header.identity())
            .map_err(|e| js_err("episode_identity_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Whether two episode documents may be compared quantity for quantity.
    ///
    /// `{ "verdict": "same_conditions" | "different" | "indeterminate",
    ///    "reasons": [...], "describe": "..." }`. The decision is the core's
    /// (`ExperimentIdentity::compare`); this only shapes it for the page, and
    /// only `same_conditions` licenses the phrase.
    pub fn episode_comparability_json(
        &self,
        a_json: &str,
        b_json: &str,
    ) -> Result<JsValue, JsValue> {
        let a = Episode::from_json(a_json).map_err(|e| js_err("episode a", e))?;
        let b = Episode::from_json(b_json).map_err(|e| js_err("episode b", e))?;
        let verdict = a.header.identity().compare(&b.header.identity());
        let kind = match verdict {
            Comparability::SameConditions => "same_conditions",
            Comparability::Different(_) => "different",
            Comparability::Indeterminate(_) => "indeterminate",
        };
        let json = serde_json::to_string(&serde_json::json!({
            "verdict": kind,
            "reasons": verdict.reasons(),
            "describe": verdict.describe(),
        }))
        .map_err(|e| js_err("episode_comparability_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Close the recording and return the episode as a JSON **string**.
    pub fn stop_recording(&mut self) -> Result<JsValue, JsValue> {
        let rec = self
            .recorder
            .take()
            .ok_or_else(|| JsValue::from_str("stop_recording: no recording is in progress"))?;
        let json = rec
            .finish()
            .to_json()
            .map_err(|e| js_err("stop_recording", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Encode an episode JSON document as the typed-array binary form
    /// (brief §33). The codec lives in the core; this only marshals.
    pub fn episode_to_binary(&self, episode_json: &str) -> Result<Box<[u8]>, JsValue> {
        let episode = Episode::from_json(episode_json).map_err(|e| js_err("episode", e))?;
        let bytes = episode.to_binary().map_err(|e| js_err("episode", e))?;
        Ok(bytes.into_boxed_slice())
    }

    /// Decode the binary form back to an episode JSON document.
    pub fn episode_from_binary(&self, bytes: &[u8]) -> Result<JsValue, JsValue> {
        let episode = Episode::from_binary(bytes).map_err(|e| js_err("episode", e))?;
        let json = episode.to_json().map_err(|e| js_err("episode", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Validate and normalise an episode JSON document.
    ///
    /// The import path calls this so that an episode from another schema is
    /// rejected by the **core**, with the core's message, rather than by a
    /// version check written a second time in TypeScript (F8).
    pub fn episode_from_json(&self, episode_json: &str) -> Result<JsValue, JsValue> {
        let episode = Episode::from_json(episode_json).map_err(|e| js_err("episode", e))?;
        let json = episode.to_json().map_err(|e| js_err("episode", e))?;
        Ok(JsValue::from_str(&json))
    }

    // --- guided practice (v2 section 11, F18.4) ---------------------------

    /// The three shipped challenges, as one JSON array (brief §24).
    ///
    /// ```json
    /// [{ "id": "get_moving", "version": 1, "scenario": "free_sail",
    ///    "time_limit_s": 45.0, "highlight_event": "speed_reached",
    ///    "metric": { "id": "top_speed", "unit": "m/s" },
    ///    "thresholds": { "target_speed_mps": 1.2, … } }]
    /// ```
    ///
    /// Every number a challenge is judged by is in here, which is what makes
    /// the thresholds *visible* — the page shows them and restates none of
    /// them. A challenge is a shipped scenario plus a task; `scenario` names
    /// the one it is set on, and section 11 ships no new scenario.
    pub fn practice_tasks_json(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<serde_json::Value> = TASK_IDS
            .iter()
            .map(|id| {
                let spec = TaskSpec::shipped(*id);
                let (metric, unit) = spec.metric();
                serde_json::json!({
                    "id": id.as_str(),
                    "version": spec.version(),
                    "scenario": id.scenario(),
                    "time_limit_s": spec.time_limit_s(),
                    "highlight_event": id.highlight_event(),
                    "metric": { "id": metric, "unit": unit },
                    "thresholds": spec.thresholds(),
                })
            })
            .collect();
        let json = serde_json::to_string(&rows).map_err(|e| js_err("practice_tasks_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Begin an attempt at `task_id`, recording at `log_hz`.
    ///
    /// Four things happen, in this order, and none of them is optional:
    ///
    /// 1. the challenge's shipped scenario is **loaded** — `load_scenario`,
    ///    not `restart_scenario`, so the scenario's own `parameter_overrides`
    ///    are applied to the ILCA catalogue and any live brief §31 edit is
    ///    discarded. An attempt starts from the boat the scenario describes;
    ///    2. the resolved conditions are **frozen** ([`Conditions`]);
    /// 3. the evaluator is started from the resolved initial observation;
    /// 4. recording begins, with the practice envelope attached.
    ///
    /// A previous attempt is cancelled. Free sail is what you get by not
    /// calling this, and [`Sim::cancel_practice`] is how you get back to it.
    pub fn start_practice(&mut self, task_id: &str, log_hz: f64) -> Result<(), JsValue> {
        let id = TaskId::parse(task_id)
            .ok_or_else(|| JsValue::from_str(&format!("unknown practice task `{task_id}`")))?;
        let spec = TaskSpec::shipped(id);
        self.attempt = None;
        self.recorder = None;

        let scenario = sailgym_physics::scenario::load_shipped(id.scenario())
            .map_err(|e| js_err("practice scenario", e))?;
        self.inner
            .load_scenario(&scenario)
            .map_err(|e| js_err("practice scenario", e))?;
        self.scenario = scenario.clone();
        self.reset_document =
            serde_json::to_string(&scenario).map_err(|e| js_err("practice scenario", e))?;

        let conditions = Conditions {
            scenario,
            params: *self.inner.params(),
            state: *self.inner.state(),
            controls: *self.inner.controls(),
            wind: *self.inner.wind().config(),
            seed: self.inner.seed(),
            log_hz,
        };
        let run = TaskRun::start(spec, &StepObservation::of(&self.inner))
            .map_err(|e| js_err("practice task", e))?;
        self.attempt = Some(Attempt {
            spec,
            run,
            conditions,
            status: AttemptStatus::Active,
        });
        self.begin_recording(log_hz);
        Ok(())
    }

    /// Start the same challenge again under **exactly** the conditions the
    /// last attempt was started under.
    ///
    /// Not a reset and not a restart: the frozen [`Conditions`] are written
    /// back one field at a time — catalogue, wind configuration, seed, the
    /// complete F3 state, the controls — so a live parameter edit made during
    /// the attempt cannot survive into the retry (RV63). `Simulation::reset`
    /// clears the step counter, the capsize report and the controls and
    /// rebuilds the wind field from the seed, which is what makes the second
    /// attempt bit-reproducible against the first (brief §34).
    ///
    /// The recorder is replaced, the evaluator is rebuilt from the same
    /// frozen specification, and every event, timer and success flag from the
    /// previous attempt is gone with it.
    pub fn retry_practice(&mut self) -> Result<(), JsValue> {
        let Some(previous) = self.attempt.as_ref() else {
            return Err(JsValue::from_str("retry_practice: no attempt to retry"));
        };
        let spec = previous.spec;
        let conditions = previous.conditions.clone();
        self.attempt = None;
        self.recorder = None;

        self.inner
            .set_parameters(conditions.params)
            .map_err(|e| js_err("retry_practice", e))?;
        // The configuration first, then the seeded reset: `Simulation::reset`
        // rebuilds the field from the configuration in force and the seed it
        // is given, so this order is what reproduces the same wind.
        self.inner.set_wind(conditions.wind);
        self.inner.reset(conditions.state, conditions.seed);
        self.inner.set_controls(conditions.controls);
        self.scenario = conditions.scenario.clone();
        self.reset_document =
            serde_json::to_string(&conditions.scenario).map_err(|e| js_err("retry_practice", e))?;

        let run = TaskRun::start(spec, &StepObservation::of(&self.inner))
            .map_err(|e| js_err("practice task", e))?;
        let log_hz = conditions.log_hz;
        self.attempt = Some(Attempt {
            spec,
            run,
            conditions,
            status: AttemptStatus::Active,
        });
        self.begin_recording(log_hz);
        Ok(())
    }

    /// Abandon practice and go back to free sail.
    ///
    /// The in-progress recording goes with it: its header describes a run
    /// nobody finished, and keeping it would leave a half-scored episode on
    /// the page (brief §33 takes the same view of a reset).
    pub fn cancel_practice(&mut self) {
        self.attempt = None;
        self.recorder = None;
    }

    /// The attempt, as JSON, or `{ "active": false }` when there is none.
    ///
    /// ```json
    /// { "active": true, "status": "active",
    ///   "report": { "task": …, "outcome": …, "elapsed_s": …, "metric": …,
    ///               "progress": …, "events": […], "highlight": … } }
    /// ```
    ///
    /// The whole record in one call (brief §24). Every number in it is the
    /// evaluator's; the page formats and never recomputes.
    pub fn practice_state_json(&self) -> Result<JsValue, JsValue> {
        let value = match self.attempt.as_ref() {
            None => serde_json::json!({ "active": false }),
            Some(a) => serde_json::json!({
                "active": true,
                "status": a.status.as_str(),
                "report": a.run.report(),
            }),
        };
        let json = serde_json::to_string(&value).map_err(|e| js_err("practice_state_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Set the control rates. Rates, never absolute angles (brief §12, §13).
    pub fn set_controls(&mut self, rudder_rate: f64, sheet_rate: f64, release: bool) {
        self.inner.set_controls(Controls {
            rudder_rate_cmd: rudder_rate,
            sheet_rate_cmd: sheet_rate,
            sheet_release: release,
        });
    }

    /// Advance exactly `n` fixed steps of `dt`. Returns steps actually taken.
    ///
    /// While recording, the steps are issued one at a time so the recorder
    /// can see every completed state; F9.7 makes that bit-identical to the
    /// batched call, and `recording::tests::recording_does_not_perturb`
    /// asserts it. The full `Diagnostics` record is built only on the steps
    /// the sample interval actually keeps — at 20 Hz and `dt = 0.005` that is
    /// one step in ten, and none at all once the recorder is full, because
    /// `Recorder::due` is `false` from then on.
    pub fn advance(&mut self, n: u32) -> u32 {
        if self.recorder.is_none() && !self.attempt_is_active() {
            return self.inner.advance(n);
        }
        for _ in 0..n {
            self.inner.advance(1);
            self.observe_step();
        }
        n
    }

    /// End an **active** attempt for a reason that is not the boat's.
    ///
    /// A finished, cancelled or conditions-changed attempt is left alone: the
    /// first already has a result and the other two already have none, and a
    /// second cancellation must not overwrite either. Nothing is thrown away
    /// here — the attempt is still readable through
    /// [`Sim::practice_state_json`], which is how the page explains why the
    /// result it is looking at cannot be compared.
    fn end_attempt(&mut self, status: AttemptStatus) {
        if let Some(a) = self.attempt.as_mut() {
            if a.status == AttemptStatus::Active {
                a.status = status;
            }
        }
    }

    /// Whether a practice attempt is being evaluated.
    fn attempt_is_active(&self) -> bool {
        self.attempt
            .as_ref()
            .is_some_and(|a| a.status == AttemptStatus::Active)
    }

    /// Everything that watches a **completed** physics step: the practice
    /// evaluator, then the recorder.
    ///
    /// `self` is destructured so the evaluator can read `inner` while the
    /// attempt and the recorder are held mutably; they are disjoint fields
    /// and the borrow checker is told so once, here, rather than the code
    /// being shaped around it.
    ///
    /// Order matters in exactly one way: the task is evaluated first, so an
    /// event decided on this step is already in the envelope when the frame
    /// beside it is logged. Neither can change the state — the evaluator
    /// takes a value and the recorder takes `&Simulation` — so nothing else
    /// about the order is observable (`recording_does_not_perturb`,
    /// `task_evaluation_does_not_perturb_the_physics`).
    fn observe_step(&mut self) {
        let Self {
            inner,
            attempt,
            recorder,
            ..
        } = self;

        if let Some(a) = attempt {
            if a.status == AttemptStatus::Active {
                let before = a.run.events().len();
                let outcome = a.run.observe(&StepObservation::of(inner));
                // The new events, in the order the evaluator appended them.
                // `push_practice_event` refuses one with no envelope, which
                // is why `begin_recording` sets the envelope first.
                if let Some(rec) = recorder.as_mut() {
                    for event in &a.run.events()[before..] {
                        rec.push_practice_event(event.clone());
                    }
                }
                if outcome.is_terminal() {
                    a.status = AttemptStatus::Finished;
                }
            }
        }

        if let Some(rec) = recorder.as_mut() {
            let t = inner.state().t;
            if rec.due(t) {
                let d = sailgym_physics::diagnostics::diagnostics(inner);
                rec.observe(inner, &d);
            }
        }
    }

    /// Flat `f64` view of `BoatState`, layout = F8.3 field order.
    pub fn snapshot(&self) -> Box<[f64]> {
        Box::new(self.inner.state().to_array())
    }

    /// Current-state diagnostics as a JSON string, like `parameters_json`.
    pub fn diagnostics(&self) -> Result<JsValue, JsValue> {
        let d = sailgym_physics::diagnostics::diagnostics(&self.inner);
        let json = serde_json::to_string(&d).map_err(|e| js_err("diagnostics", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Live parameter editing (brief §31). Returns whether a reset is
    /// required.
    pub fn set_parameter(&mut self, path: &str, value: f64) -> Result<bool, JsValue> {
        let reset_required = self
            .inner
            .set_parameter(path, value)
            .map_err(|e| js_err("set_parameter", e))?;
        // Only once the edit has actually been accepted: a rejected edit
        // changes nothing, so it cannot have changed an attempt's conditions
        // either (section 08 handoff §2.1).
        self.end_attempt(AttemptStatus::ConditionsChanged);
        Ok(reset_required)
    }

    /// Restore `BoatParameters::ilca7()` — the panel's "Reset to ILCA
    /// defaults" (brief §31). The boat state, the clock and the wind are
    /// untouched; only the catalogue moves.
    pub fn reset_parameters(&mut self) {
        self.inner.reset_parameters();
        self.end_attempt(AttemptStatus::ConditionsChanged);
    }

    /// The whole F7 catalogue, as a JSON **string**.
    ///
    /// A string rather than a structured object keeps this crate free of
    /// `js-sys`/`serde-wasm-bindgen`; the caller does one `JSON.parse`.
    pub fn parameters_json(&self) -> Result<JsValue, JsValue> {
        let json =
            serde_json::to_string(self.inner.params()).map_err(|e| js_err("parameters_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// The catalogue's **metadata** — one record per editable leaf, with its
    /// dotted path, F7 tag, unit and doc comment — as a JSON string.
    ///
    /// A sibling of `parameters_json` rather than a key inside it, for two
    /// reasons. The values change on every edit and this does not, so the
    /// panel fetches it once and re-reads only the values; and three callers
    /// (`useSimulation`, `sheet.spec.ts`, the `wasm-bindgen-test` in
    /// `boundary.rs`) already parse `parameters_json()` as the plain
    /// `BoatParameters` tree, which it stays. Still coarse-grained
    /// (brief §24): the whole catalogue in one call, never a field at a time.
    ///
    /// The records are derived from `parameters.rs`'s own source, so the
    /// parameter panel is generated rather than written (brief §31).
    pub fn parameter_meta_json(&self) -> Result<JsValue, JsValue> {
        let json = serde_json::to_string(&sailgym_physics::parameters::catalogue())
            .map_err(|e| js_err("parameter_meta_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    // --- wind (section 03) ------------------------------------------------

    /// Fill `out` with `[wx, wy]` pairs for an `nx × ny` grid whose node
    /// `(i, j)` sits at `(x0 + i·dx, y0 + j·dy)`, row-major.
    ///
    /// **One call per animation frame, for the whole grid** (brief §19). The
    /// per-point `sample` is deliberately *not* exported to JavaScript: the
    /// batched call is the only way across the boundary, which is what keeps
    /// the visualization from making thousands of boundary crossings a frame
    /// (brief §24).
    ///
    /// `out.len()` must be `2·nx·ny`.
    #[allow(clippy::too_many_arguments)] // signature is normative, F8.2
    pub fn sample_wind_grid(
        &self,
        x0: f64,
        y0: f64,
        dx: f64,
        dy: f64,
        nx: u32,
        ny: u32,
        t: f64,
        out: &mut [f32],
    ) {
        self.inner
            .wind()
            .sample_grid(x0, y0, dx, dy, nx as usize, ny as usize, t, out);
    }

    /// True wind at the boat, for the HUD: `[wx, wy, speed, bearing_deg]`.
    ///
    /// The bearing is the meteorological FROM direction and comes from
    /// `environment::wind_to_bearing`. It is returned here, rather than being
    /// derived in TypeScript, because that conversion exists in exactly one
    /// place (F6.1, F8) — a second implementation is how the from/toward flip
    /// gets in.
    pub fn wind_at_boat(&self) -> Box<[f64]> {
        let w = self.inner.wind_at_boat();
        let (speed, bearing_deg) = wind_to_bearing(w);
        Box::new([w.x, w.y, speed, bearing_deg])
    }

    /// Replace the wind configuration while the simulation is running.
    ///
    /// The boat state, the parameters and the seed are untouched, so the mode
    /// can be switched without a reload or a reset.
    pub fn set_wind(&mut self, wind_json: &str) -> Result<(), JsValue> {
        let value: serde_json::Value =
            serde_json::from_str(wind_json).map_err(|e| js_err("invalid wind JSON", e))?;
        self.inner.set_wind(parse_wind(&value)?);
        self.end_attempt(AttemptStatus::ConditionsChanged);
        Ok(())
    }

    /// The current wind configuration, as a JSON **string** (see
    /// `parameters_json` for why a string).
    pub fn wind_json(&self) -> Result<JsValue, JsValue> {
        let json = serde_json::to_string(self.inner.wind().config())
            .map_err(|e| js_err("wind_json", e))?;
        Ok(JsValue::from_str(&json))
    }

    /// Fixed physics timestep, seconds. The browser clock needs it to convert
    /// wall time into whole steps, and must not hard-code it (F8).
    pub fn dt(&self) -> f64 {
        self.inner.params().sim.dt
    }

    /// The `sailgym-wasm` crate version this module was built from.
    pub fn version(&self) -> String {
        self.built.clone()
    }
}
