//! Thin `wasm_bindgen` wrapper over `sailgym-physics` (F8.1).
//!
//! There is no physics here and there never will be: every equation lives in
//! `sailgym-physics` (F8). This file only marshals strings and flat buffers
//! across the boundary. The method set is the subset of F8.2 that exists at
//! this milestone — `new`, `reset`, `set_controls`, `advance`, `snapshot`,
//! `set_parameter`, `parameters_json`, plus the section 03 wind methods
//! `sample_wind_grid`, `wind_at_boat`, `set_wind` and `wind_json`; the rest
//! arrive with the sections that specify them.
//!
//! The API is coarse-grained by construction (brief §24): per-force-component
//! and per-entity calls are forbidden.

use sailgym_physics::environment::wind::WindConfig;
use sailgym_physics::environment::{wind_to_bearing, WindField};
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls};
use wasm_bindgen::prelude::*;

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

/// The simulation handle JavaScript holds.
#[wasm_bindgen]
pub struct Sim {
    inner: Simulation,
    built: String,
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

        Ok(Sim {
            inner,
            built: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// Restart the simulation.
    ///
    /// Recognised keys, all optional: `seed`, `state` as the full
    /// thirteen-field F3 record, and `wind` as a full `WindConfig`. `{}`
    /// restarts at rest at the origin with the current seed and wind.
    pub fn reset(&mut self, scenario_json: &str) -> Result<(), JsValue> {
        let scenario: serde_json::Value =
            serde_json::from_str(scenario_json).map_err(|e| js_err("invalid scenario JSON", e))?;

        let seed = scenario
            .get("seed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_else(|| self.inner.seed());
        let state = match scenario.get("state") {
            Some(s) => serde_json::from_value::<BoatState>(s.clone())
                .map_err(|e| js_err("invalid initial state", e))?,
            None => Simulation::initial_state(self.inner.params()),
        };

        self.inner.reset(state, seed);
        if let Some(w) = scenario.get("wind") {
            self.inner.set_wind(parse_wind(w)?);
        }
        Ok(())
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
    pub fn advance(&mut self, n: u32) -> u32 {
        self.inner.advance(n)
    }

    /// Flat `f64` view of `BoatState`, layout = F8.3 field order.
    pub fn snapshot(&self) -> Box<[f64]> {
        Box::new(self.inner.state().to_array())
    }

    /// Live parameter editing (brief §31). Returns whether a reset is
    /// required.
    pub fn set_parameter(&mut self, path: &str, value: f64) -> Result<bool, JsValue> {
        self.inner
            .set_parameter(path, value)
            .map_err(|e| js_err("set_parameter", e))
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
