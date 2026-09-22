//! `sailgym_core`: the pyo3 extension under `python/sailgym/`
//! (v2 `docs/v2/00-foundations.md` F17.2–F17.5; section 07).
//!
//! # What this crate is for
//!
//! Everything the Python side needs to know about the simulation comes from
//! here, and **nothing is typed out in Python**: the observation layout, its
//! field names, its per-column bounds, the action dimension, the action box,
//! the observation digest, the autoreset vocabulary, `dt`, and the episode
//! boundary itself. F17.1 states the rule and section 03 built the audit;
//! `python/tests/test_no_stray_constants.py` now points it at
//! `python/sailgym/` as well, so a shape retyped on the Python side is a red
//! gate step 11 and not a comment nobody reads.
//!
//! # The two boundaries stay apart (F17.2, RV47)
//!
//! This crate binds [`sailgym_env`]. It does **not** bind, wrap or reuse
//! `sailgym-wasm`, which is not in its manifest and never will be: the
//! browser's boundary is coarse by design (F8.2, brief §24) and a training
//! loop calling at 20 Hz × N envs wants the opposite.
//!
//! # The three performance non-negotiables (F17.5)
//!
//! 1. **Caller-owned buffers.** [`vector::RawVecEnv::step_all`] writes into
//!    `numpy` arrays the caller allocated, in the shape `sample_wind_grid`
//!    established in v1. Nothing is allocated per step on this side of the
//!    boundary.
//! 2. **The GIL is released** for the duration of `step_all`, so a Python
//!    thread runs while Rust steps. `python/tests/test_performance.py`
//!    measures the overlap against a stated bound rather than trusting this
//!    sentence.
//! 3. **No per-environment object churn.** The low-level API returns `None`
//!    and writes into buffers; the Gymnasium adapters above it build the
//!    tuples and `info` dictionaries the standard requires, and their cost is
//!    measured **separately**.
//!
//! # `f32` here is an output buffer, not a physical intermediate
//!
//! F9.5 forbids `f32` intermediates **in physics**. The observation buffers
//! are `f32` for exactly the reason `sample_wind_grid`'s are, and F17.5 says
//! so in as many words. Every reward, every action and every state scalar
//! that crosses this boundary is `f64`.
//!
//! # What is deliberately not here
//!
//! * **No equation, no coefficient, no frame conversion.** This crate holds
//!   none, and neither does `python/sailgym/`.
//! * **No reward.** `sailgym-env` ships `ZeroReward` and nothing else; a
//!   reward is an experiment parameter (section 06 §8). The Python adapters
//!   take a `reward_fn` and apply it to the observation the decision period
//!   **ended on**, which is the one quantity both autoreset conventions
//!   agree about.
//! * **No training, no policy, no algorithm.** `docs/v2/brief.md` S4
//!   proposes the environment.
//! * **No PettingZoo.** When multi-boat arrives, `ParallelEnv` is the right
//!   interface and this stays its N = 1 specialisation.

use pyo3::prelude::*;

// One `mod` line per later task's file. Rust has no way for task 7.2, 7.3 or
// 7.4 to declare its own module without editing the crate root, which is task
// 7.1's file — the same `Owns:`-list gap sections 04, 05, 06, 10 and 11 each
// recorded, and it is recorded again in `docs/v2/progress/07-handoff.md`
// rather than quietly absorbed (F13.2).
mod env;
mod spec;
mod vector;

/// The extension module. Named `sailgym_core` because it is its own
/// distribution: `python/sailgym/` is the package a user imports and this is
/// the compiled core it calls.
#[pymodule]
fn sailgym_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<spec::Spec>()?;
    m.add_class::<env::RawEnv>()?;
    m.add_class::<vector::RawVecEnv>()?;
    m.add_function(wrap_pyfunction!(spec::autoreset_modes, m)?)?;
    m.add_function(wrap_pyfunction!(spec::default_autoreset_mode, m)?)?;
    m.add_function(wrap_pyfunction!(spec::tier0_sensors, m)?)?;
    m.add_function(wrap_pyfunction!(spec::shipped_scenarios, m)?)?;
    m.add_function(wrap_pyfunction!(spec::termination_reasons, m)?)?;
    Ok(())
}
