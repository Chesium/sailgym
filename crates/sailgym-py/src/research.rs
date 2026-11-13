//! Research access for the ESE3990 learning-versus-planning study
//! (`docs/ese3990/ESE3990_SailGym_Project_Plan.md` §3.3, §4.3, §4.4).
//!
//! Three narrow additions, none of which changes an existing contract:
//!
//! * **The research state** — a read-only, versioned vector of the 13 F3
//!   state scalars in [`STATE_FIELDS`] order, the current controls, the
//!   capsize observer's pending-crossing duration and the true wind at the
//!   boat. Names and units are reported from here, never typed in Python.
//!   It is a *readout*, not a restorable snapshot: restoring an episode means
//!   [`crate::env::RawEnv::fork`], which copies the whole causal state.
//! * **Route construction** — [`route_waypoints`] builds a route through
//!   `Route::waypoints` and returns the serialized `Route` that
//!   `Spec(route_json=…)` reads. Course documents and serialized routes are
//!   different schemas, so the conversion happens in Rust.
//! * **The rule sailor** — [`PyRuleSailor`] binds the existing controller's
//!   `reset`/`decide` contract unchanged. It is fed the episode's **`f64`**
//!   observation ([`crate::env::RawEnv::observe_f64`]), so it decides exactly
//!   as it does natively; any action restriction is the caller's.
//!
//! No equation, coefficient or frame conversion lives here.

use numpy::{PyReadonlyArray1, PyReadwriteArray1};
use pyo3::prelude::*;

use sailgym_agent::spec::Agent;
use sailgym_agent::{RuleSailor, Tunables};
use sailgym_course::{Route, Vec2};
use sailgym_env::Episode;
use sailgym_physics::environment::WindField;
use sailgym_physics::rng::Pcg32;
use sailgym_physics::state::{STATE_FIELDS, STATE_LEN};

use crate::env::as_mut_slice;
use crate::spec::{value_error, Spec};

/// Bumped on any change to the research-state layout or its meaning.
pub const RESEARCH_STATE_VERSION: u32 = 1;

/// The fields after the 13 state scalars, with their units.
const EXTRA_FIELDS: [(&str, &str); 7] = [
    ("rudder_rate_cmd", "1"),
    ("sheet_rate_cmd", "1"),
    ("sheet_release", "1"),
    ("capsize_pending", "s"),
    ("capsized", "1"),
    ("wind_x", "m/s"),
    ("wind_y", "m/s"),
];

/// F3's units, in [`STATE_FIELDS`] order.
const STATE_UNITS: [&str; STATE_LEN] = [
    "m", "m", "rad", "rad", "m/s", "m/s", "rad/s", "rad/s", "rad", "rad/s", "rad", "m", "s",
];

pub const RESEARCH_STATE_LEN: usize = STATE_LEN + EXTRA_FIELDS.len();

/// The research-state field names, in buffer order.
#[pyfunction]
pub fn research_state_names() -> Vec<String> {
    STATE_FIELDS
        .iter()
        .copied()
        .chain(EXTRA_FIELDS.iter().map(|(n, _)| *n))
        .map(str::to_string)
        .collect()
}

/// The research-state units, in buffer order.
#[pyfunction]
pub fn research_state_units() -> Vec<String> {
    STATE_UNITS
        .iter()
        .copied()
        .chain(EXTRA_FIELDS.iter().map(|(_, u)| *u))
        .map(str::to_string)
        .collect()
}

#[pyfunction]
pub fn research_state_version() -> u32 {
    RESEARCH_STATE_VERSION
}

/// Fill `out` with the research state of `ep`'s **current** state.
///
/// Under `AutoresetMode::Disabled` this is the state the last decision period
/// ended on, before any reset. `capsize_pending` is `t − t_crossing` while a
/// threshold crossing is pending (including after it has fired) and `0`
/// otherwise; it comes from the observer's own record, not from sampled heel.
pub(crate) fn fill(ep: &Episode, out: &mut [f64]) {
    let sim = ep.simulation();
    let st = sim.state();
    out[..STATE_LEN].copy_from_slice(&st.to_array());
    let c = sim.controls();
    let cap = sim.capsize();
    let w = sim.wind().sample(st.x, st.y, st.t);
    let extra = [
        c.rudder_rate_cmd,
        c.sheet_rate_cmd,
        if c.sheet_release { 1.0 } else { 0.0 },
        cap.pending_since().map_or(0.0, |t0| st.t - t0),
        if cap.capsized { 1.0 } else { 0.0 },
        w.x,
        w.y,
    ];
    out[STATE_LEN..].copy_from_slice(&extra);
}

/// The physics build this extension implements, as JSON
/// (`sailgym_physics::identity::ModelIdentity`): the model version and the
/// git tree id of `crates/sailgym-physics/src`, with its `clean`/`dirty`/
/// `unknown` state. An episode log records it so that a replay on a different
/// build can be refused.
#[pyfunction]
pub fn model_identity() -> PyResult<String> {
    serde_json::to_string(&sailgym_physics::identity::ModelIdentity::current()).map_err(value_error)
}

/// The serialized `Route` for an open course through `points` from `start`,
/// one gate of `2·half_width` per point (`Route::waypoints`). Pass the result
/// to `Spec(route_json=…)`.
#[pyfunction]
pub fn route_waypoints(
    start: (f64, f64),
    points: Vec<(f64, f64)>,
    half_width: f64,
) -> PyResult<String> {
    let pts: Vec<Vec2> = points.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let route =
        Route::waypoints(Vec2::new(start.0, start.1), &pts, half_width).map_err(value_error)?;
    serde_json::to_string(&route).map_err(value_error)
}

/// The rule sailor, bound unchanged.
#[pyclass(unsendable, module = "sailgym_core", name = "RuleSailor")]
pub struct PyRuleSailor {
    inner: RuleSailor,
    /// Handed to `decide`, which never draws from it.
    rng: Pcg32,
    action_dim: usize,
}

#[pymethods]
impl PyRuleSailor {
    /// The shipped tunables at `spec`'s cadence.
    #[new]
    fn new(spec: &Spec) -> Self {
        let inner = RuleSailor::with_tunables(Tunables::default(), spec.cadence);
        let action_dim = spec.action_dim_value();
        Self {
            inner,
            rng: Pcg32::seed_from_u64(0),
            action_dim,
        }
    }

    /// Resolve columns by name and forget the previous episode. Call it with
    /// `spec.obs_names` at every episode reset.
    fn reset(&mut self, names: Vec<String>) {
        self.inner.reset(&names, &mut self.rng);
    }

    /// Decide from the `f64` observation and write the normalised action
    /// (rudder rate, sheet rate, release sign) into `out`.
    fn decide(
        &mut self,
        obs: PyReadonlyArray1<f64>,
        mut out: PyReadwriteArray1<f64>,
    ) -> PyResult<()> {
        let o = obs
            .as_slice()
            .map_err(|_| crate::env::contiguity_error("obs"))?;
        let a = self.inner.decide(o, &mut self.rng);
        let dst = as_mut_slice(&mut out, "out", self.action_dim)?;
        dst.copy_from_slice(a.values());
        Ok(())
    }

    /// The current manoeuvre mode, e.g. `sailing` or `tacking`.
    #[getter]
    fn mode(&self) -> String {
        self.inner.mode().as_str().to_string()
    }

    /// Columns the controller needed and did not find at reset.
    #[getter]
    fn missing_fields(&self) -> Vec<String> {
        self.inner.missing_fields().to_vec()
    }

    #[getter]
    fn version(&self) -> u32 {
        self.inner.spec().version
    }
}
