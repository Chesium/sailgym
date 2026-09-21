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
//! `episode_identity_json` and `episode_comparability_json`.
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
        if self.recorder.is_none() {
            return self.inner.advance(n);
        }
        for _ in 0..n {
            self.inner.advance(1);
            let t = self.inner.state().t;
            if self.recorder.as_ref().is_some_and(|r| r.due(t)) {
                let d = sailgym_physics::diagnostics::diagnostics(&self.inner);
                if let Some(r) = self.recorder.as_mut() {
                    r.observe(&self.inner, &d);
                }
            }
        }
        n
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
        self.inner
            .set_parameter(path, value)
            .map_err(|e| js_err("set_parameter", e))
    }

    /// Restore `BoatParameters::ilca7()` — the panel's "Reset to ILCA
    /// defaults" (brief §31). The boat state, the clock and the wind are
    /// untouched; only the catalogue moves.
    pub fn reset_parameters(&mut self) {
        self.inner.reset_parameters();
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
