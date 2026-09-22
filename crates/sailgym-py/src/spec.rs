//! The contract Python builds its spaces from (v2 F14.3, F14.5, F16.4, F17.1;
//! section 07 task 7.2).
//!
//! # Why every one of these getters exists
//!
//! `cross-stack.md` §1.2, quoted by the section PRD: spaces are built from
//! what the Rust core reports, never typed out in Python, *"because Python is
//! where that discipline usually collapses"*. So:
//!
//! | Python writes | it reads |
//! |---|---|
//! | `Box(low, high, shape=(n,))` | [`Spec::obs_low`], [`Spec::obs_high`], [`Spec::obs_len`] |
//! | the action box | [`Spec::action_low`], [`Spec::action_high`], [`Spec::action_dim`] |
//! | the field-name list | [`Spec::obs_names`] |
//! | the digest it refuses on | [`Spec::obs_digest`] |
//! | the autoreset vocabulary | [`autoreset_modes`] |
//! | `dt` | [`Spec::dt`] |
//!
//! Not one of those is a literal on the Python side, and RV41's grep — the
//! section-03 audit, extended to `python/sailgym/` — is what keeps it true.
//!
//! # `lo`/`hi` are `Option` in Rust and `±inf` here, and the mapping lives here
//!
//! [`FieldSpec`](sailgym_agent::sensor::FieldSpec) records an absent bound as
//! `None` rather than as `±f64::INFINITY`, because "unbounded" is a fact about
//! the quantity and `serde_json` writes a non-finite float as `null`. A
//! `gymnasium.spaces.Box` wants a number, so the substitution happens **here**,
//! once, in Rust — and not in Python, where it would be an invented value.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use sailgym_agent::actuation::rate::Rate;
use sailgym_agent::observation::{obs_digest, ObsLayout};
use sailgym_agent::spec::Cadence;
use sailgym_course::Route;
use sailgym_env::episode::{manual_source, Episode, EpisodeConfig, TIER0_SENSORS};
use sailgym_env::outcome::{AutoresetMode, Bounds, TerminationReason};
use sailgym_physics::scenario::{load_all_shipped, load_shipped, Scenario};

/// Turn any error with a `Display` into a Python `ValueError`.
///
/// Every refusal on this boundary is a `ValueError` carrying the Rust
/// message verbatim. A binding that swallowed the reason and raised its own
/// would be the second place the contract is stated.
pub(crate) fn value_error<E: std::fmt::Display>(e: E) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Parse an autoreset mode from Gymnasium's own spelling.
///
/// The three names are [`AutoresetMode::as_str`]'s, which section 06 took
/// verbatim from `gymnasium.vector.AutoresetMode`. Python passes the string it
/// read out of the version it pinned (F19.1); an unknown one is refused here
/// and never defaulted, because a silent default is how the two sides come to
/// disagree about episode boundaries.
fn parse_autoreset(name: &str) -> PyResult<AutoresetMode> {
    for mode in [
        AutoresetMode::NextStep,
        AutoresetMode::SameStep,
        AutoresetMode::Disabled,
    ] {
        if mode.as_str() == name {
            return Ok(mode);
        }
    }
    Err(PyValueError::new_err(format!(
        "unknown autoreset mode `{name}`; this build knows {:?}",
        autoreset_modes()
    )))
}

/// The autoreset conventions this build implements, in Gymnasium's spelling.
///
/// `python/sailgym/vector.py` cross-checks this list against
/// `gymnasium.vector.AutoresetMode` on the pinned version, so a Gymnasium
/// release that renames or adds a value fails gate step 11 with a message
/// instead of being silently ignored (F19.1, RV44).
#[pyfunction]
pub fn autoreset_modes() -> Vec<String> {
    [
        AutoresetMode::NextStep,
        AutoresetMode::SameStep,
        AutoresetMode::Disabled,
    ]
    .iter()
    .map(|m| m.as_str().to_string())
    .collect()
}

/// What `AutoresetMode::default()` is in Rust.
///
/// Reported so that Python can say **whether it agrees** with the default of
/// the Gymnasium it pinned, rather than inheriting either one.
#[pyfunction]
pub fn default_autoreset_mode() -> String {
    AutoresetMode::default().as_str().to_string()
}

/// The tier-0 sensor suite, in the order section 05 registered it.
#[pyfunction]
pub fn tier0_sensors() -> Vec<String> {
    TIER0_SENSORS.iter().map(|s| (*s).to_string()).collect()
}

/// The shipped scenario ids (v1 brief §32).
#[pyfunction]
pub fn shipped_scenarios() -> PyResult<Vec<String>> {
    Ok(load_all_shipped()
        .map_err(value_error)?
        .into_iter()
        .map(|s| s.name)
        .collect())
}

/// Every [`TerminationReason`] this build can report.
#[pyfunction]
pub fn termination_reasons() -> Vec<String> {
    [
        TerminationReason::Capsized,
        TerminationReason::OutOfBounds,
        TerminationReason::MarkMissed,
    ]
    .iter()
    .map(|r| r.as_str().to_string())
    .collect()
}

/// An episode configuration, plus everything Python needs to describe it.
///
/// Built once and shared by [`crate::env::RawEnv`] and
/// [`crate::vector::RawVecEnv`], so the single-env and vector paths cannot be
/// configured differently by accident — which is the mistake 7.4's
/// step-for-step agreement test exists to catch.
#[pyclass(unsendable, module = "sailgym_core", name = "Spec")]
pub struct Spec {
    pub(crate) config: EpisodeConfig,
    pub(crate) cadence: Cadence,
    layout: ObsLayout,
    action_dim: usize,
    dt: f64,
}

#[pymethods]
impl Spec {
    /// `scenario` is a shipped id (`"free_sail"`, `"tack"`, …) or the JSON
    /// text of a scenario document; anything beginning with `{` is read as
    /// JSON.
    ///
    /// Every other argument has a default that **invents nothing**, exactly as
    /// [`EpisodeConfig::new`] does: no route, no bounds, no budget, the tier-0
    /// suite, and `ZeroReward`. A configuration that wants a number supplies
    /// it.
    #[new]
    #[pyo3(signature = (
        scenario,
        *,
        sensors = None,
        max_steps = None,
        autoreset = None,
        cadence = 10,
        bounds = None,
        route_json = None,
        log_decisions = false,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        scenario: &str,
        sensors: Option<Vec<String>>,
        max_steps: Option<u64>,
        autoreset: Option<&str>,
        cadence: u32,
        bounds: Option<(f64, f64, f64, f64)>,
        route_json: Option<&str>,
        log_decisions: bool,
    ) -> PyResult<Self> {
        let scenario = if scenario.trim_start().starts_with('{') {
            Scenario::load(scenario).map_err(value_error)?
        } else {
            load_shipped(scenario).map_err(value_error)?
        };

        let mut config = EpisodeConfig::new(scenario);
        if let Some(ids) = sensors {
            config.sensors = ids;
        }
        config.max_steps = max_steps;
        config.autoreset = match autoreset {
            Some(name) => parse_autoreset(name)?,
            None => AutoresetMode::default(),
        };
        if let Some((min_x, min_y, max_x, max_y)) = bounds {
            config.bounds = Bounds::Rect {
                min: [min_x, min_y],
                max: [max_x, max_y],
            };
        }
        if let Some(text) = route_json {
            let route: Route = serde_json::from_str(text).map_err(value_error)?;
            config.route = Some(route);
        }
        config.log_decisions = log_decisions;

        let cadence = Cadence::new(cadence);
        cadence.validate().map_err(value_error)?;
        config.validate().map_err(value_error)?;

        // The layout and the action dimension are read off a probe episode
        // rather than derived a second time here. `VecEnv::new` does exactly
        // the same thing for exactly the same reason: a second derivation is
        // a second rule, and two rules drift.
        let probe = Episode::new(config.clone(), manual_source(cadence), 0).map_err(value_error)?;
        let layout = probe.layout().clone();
        let action_dim = probe.action_dim();
        let dt = probe.params().sim.dt;

        Ok(Self {
            config,
            cadence,
            layout,
            action_dim,
            dt,
        })
    }

    /// Scalars in one observation. **Runtime data** (F14.3), not a constant.
    #[getter]
    fn obs_len(&self) -> usize {
        self.layout.len()
    }

    /// Scalars in one action.
    #[getter]
    fn action_dim(&self) -> usize {
        self.action_dim
    }

    /// The fully qualified column names, in observation order.
    #[getter]
    fn obs_names(&self) -> Vec<String> {
        self.layout.names()
    }

    /// The F1 unit of each column, spelled as F1 spells it.
    #[getter]
    fn obs_units(&self) -> Vec<String> {
        self.layout
            .columns
            .iter()
            .map(|c| c.field.unit.clone())
            .collect()
    }

    /// Per-column lower bound, with an absent bound reported as `-inf`.
    #[getter]
    fn obs_low(&self) -> Vec<f64> {
        self.layout
            .columns
            .iter()
            .map(|c| c.field.lo.unwrap_or(f64::NEG_INFINITY))
            .collect()
    }

    /// Per-column upper bound, with an absent bound reported as `+inf`.
    #[getter]
    fn obs_high(&self) -> Vec<f64> {
        self.layout
            .columns
            .iter()
            .map(|c| c.field.hi.unwrap_or(f64::INFINITY))
            .collect()
    }

    /// The indices of the privileged columns — F14.3's surviving `ObsMask`.
    #[getter]
    fn obs_privileged(&self) -> Vec<usize> {
        self.layout.privileged_columns()
    }

    /// The action box's lower corner. F14.5: **always** `[−1, 1]^k`, and the
    /// two numbers come from here so that no Python file states the bound.
    #[getter]
    fn action_low(&self) -> Vec<f64> {
        vec![-1.0; self.action_dim]
    }

    /// The action box's upper corner (F14.5).
    #[getter]
    fn action_high(&self) -> Vec<f64> {
        vec![1.0; self.action_dim]
    }

    /// The adapter's registered name, as it appears in an experiment log.
    #[getter]
    fn action_adapter(&self) -> String {
        Rate::ID.to_string()
    }

    /// The observation identity (F16.4): the canonical record, not a hash.
    ///
    /// Section 05 chose canonical JSON over a second SHA-256 implementation;
    /// `python/sailgym/spaces.py` refuses on inequality of this string, which
    /// is the comparison F16.4 calls sufficient.
    #[getter]
    fn obs_digest(&self) -> String {
        obs_digest(&self.layout)
    }

    /// The complete canonical layout record. Identical text to
    /// [`Spec::obs_digest`] today, and named separately because F16.4 requires
    /// the **full record to travel beside any digest**: if a compact key is
    /// ever added, this is the thing it is a key for.
    #[getter]
    fn obs_layout_json(&self) -> String {
        self.layout.canonical_json()
    }

    /// The action half of the identity (F16.4): adapter, version, cadence.
    #[getter]
    fn action_identity_json(&self) -> PyResult<String> {
        let identity = sailgym_agent::actuation::action_identity(&Rate, self.cadence);
        serde_json::to_string(&identity).map_err(value_error)
    }

    /// Which autoreset convention this configuration steps under, in
    /// Gymnasium's spelling.
    #[getter]
    fn autoreset_mode(&self) -> String {
        self.config.autoreset.as_str().to_string()
    }

    /// F14.6: a decision happens when `episode_step % period_steps == 0`.
    #[getter]
    fn cadence_period_steps(&self) -> u32 {
        self.cadence.period_steps
    }

    #[getter]
    fn scenario_name(&self) -> String {
        self.config.scenario.name.clone()
    }

    /// The scenario's own `u64`. Not the episode seed: `reset(seed=k)` maps
    /// straight onto the Rust `u64` (F17.3) and overrides this.
    #[getter]
    fn scenario_seed(&self) -> u64 {
        self.config.scenario.seed
    }

    /// s, the fixed physics timestep (F7). Reported because a Python file may
    /// not define it (F17.1, section 03 §11.2).
    #[getter]
    fn dt(&self) -> f64 {
        self.dt
    }

    /// The step budget, in **physics** steps, or `None`.
    #[getter]
    fn max_steps(&self) -> Option<u64> {
        self.config.max_steps
    }

    #[getter]
    fn sensors(&self) -> Vec<String> {
        self.config.sensors.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "Spec(scenario={:?}, obs_len={}, action_dim={}, cadence={}, autoreset={:?})",
            self.config.scenario.name,
            self.layout.len(),
            self.action_dim,
            self.cadence.period_steps,
            self.config.autoreset.as_str(),
        )
    }
}

impl Spec {
    /// The configuration, for [`crate::env::RawEnv`] and
    /// [`crate::vector::RawVecEnv`].
    pub(crate) fn config(&self) -> EpisodeConfig {
        self.config.clone()
    }
}
