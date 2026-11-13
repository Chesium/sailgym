//! One episode, across the boundary (v2 F17.3, F17.4; section 07 task 7.3).
//!
//! # `reset(seed=k)` maps straight onto the Rust `u64`
//!
//! F17.3, and there is no arithmetic on the way: [`RawEnv::reset`] hands `k`
//! to [`Episode::reset`], which derives the wind field through `STREAM_WIND`,
//! the agent through `STREAM_AGENT` and the next episode's seed through
//! `STREAM_SCENARIO`. Gymnasium's `np_random` is seeded by the base class and
//! **never read**, here or in `python/sailgym/env.py`: two RNGs feeding one
//! episode is how a deterministic environment stops being reproducible, and
//! `python/tests/test_env.py` asserts that drawing from `np_random` a
//! different number of times leaves the trajectory bit-identical (RV43).
//!
//! # The episode boundary is Rust's (F17.4)
//!
//! [`RawEnv::step`] reports `terminated` and `truncated` as two booleans out
//! of [`sailgym_env::Outcome`], and **their union is never computed** here
//! (RV34). There is no `TimeLimit` wrapper anywhere under `python/`: only the
//! step budget produces `Truncated` (F19.2), and a wrapper-supplied truncation
//! would make the single-env and vector paths disagree about episode
//! boundaries — silently, in the returns.
//!
//! # `end_obs_valid`, and why the return tuple has six fields
//!
//! Both autoreset conventions agree about **the observation the decision
//! period ended on**; they disagree only about which array it arrives in.
//! Under `NextStep` the returned observation *is* the final one. Under
//! `SameStep` the returned observation is the **reset** one and the final one
//! is carried separately. [`RawEnv::step`] therefore fills `final_obs` and
//! reports `final_obs_valid`, and `python/sailgym/env.py` builds the one
//! quantity a reward can be a function of without depending on the convention.
//! That is what makes 7.4's returns identity an identity rather than an
//! approximation.

use numpy::{
    PyReadonlyArray1, PyReadonlyArray3, PyReadwriteArray1, PyReadwriteArray2, PyReadwriteArray3,
    PyUntypedArrayMethods,
};
use pyo3::prelude::*;

use sailgym_env::episode::{manual_source, Episode};
use sailgym_env::Outcome;

use crate::spec::{value_error, Spec};

/// Narrow an observation to the buffer's `f32`, exactly as
/// `sailgym_env::vec_env` does.
///
/// **The same conversion in both paths, deliberately.** 7.4 asserts that the
/// single-env and vector paths produce identical observation sequences; a
/// different rounding here would make that test fail for a reason that has
/// nothing to do with the episode.
pub(crate) fn narrow(src: &[f64], out: &mut [f32]) {
    for (o, v) in out.iter_mut().zip(src.iter()) {
        *o = *v as f32;
    }
}

/// One independent episode.
#[pyclass(unsendable, module = "sailgym_core", name = "RawEnv")]
pub struct RawEnv {
    ep: Episode,
    obs_len: usize,
    action_dim: usize,
}

#[pymethods]
impl RawEnv {
    #[new]
    fn new(spec: &Spec, seed: u64) -> PyResult<Self> {
        let ep =
            Episode::new(spec.config(), manual_source(spec.cadence), seed).map_err(value_error)?;
        let obs_len = ep.layout().len();
        let action_dim = ep.action_dim();
        Ok(Self {
            ep,
            obs_len,
            action_dim,
        })
    }

    /// Restart the whole chain from `seed` (F17.3).
    fn reset(&mut self, seed: u64) -> PyResult<()> {
        self.ep.reset(seed).map_err(value_error)
    }

    /// Fill `obs` with the current observation, without stepping.
    fn observe(&mut self, mut obs: PyReadwriteArray1<f32>) -> PyResult<()> {
        let out = as_mut_slice(&mut obs, "obs", self.obs_len)?;
        narrow(self.ep.observation(), out);
        Ok(())
    }

    /// One decision period.
    ///
    /// Returns `(reward, terminated, truncated, autoreset, steps,
    /// final_obs_valid)`. **There is no `done`** (RV34).
    fn step(
        &mut self,
        action: PyReadonlyArray1<f64>,
        mut obs: PyReadwriteArray1<f32>,
        mut final_obs: PyReadwriteArray1<f32>,
    ) -> PyResult<(f64, bool, bool, bool, u32, bool)> {
        let values = as_slice(&action, "action", self.action_dim)?;
        // Pushed even on an autoreset call, where `Episode::step` ignores it:
        // the bounds check is the funnel's (F14.5), and skipping it on the one
        // call that discards the action would make an out-of-range action
        // sometimes an error and sometimes not.
        self.ep.push_action(values).map_err(value_error)?;
        let r = self.ep.step().map_err(value_error)?;
        {
            let out = as_mut_slice(&mut obs, "obs", self.obs_len)?;
            narrow(self.ep.observation(), out);
        }
        {
            let out = as_mut_slice(&mut final_obs, "final_obs", self.obs_len)?;
            if r.final_obs_valid {
                narrow(self.ep.final_observation(), out);
            }
        }
        Ok((
            r.reward,
            r.terminated,
            r.truncated,
            r.autoreset,
            r.steps,
            r.final_obs_valid,
        ))
    }

    /// Fill `obs` with the current observation at full `f64` precision.
    ///
    /// For controllers bound from Rust (the rule sailor), which decide from
    /// the unnarrowed vector natively; feeding them the `f32` buffer would
    /// make the Python run a different run.
    fn observe_f64(&mut self, mut obs: PyReadwriteArray1<f64>) -> PyResult<()> {
        let out = as_mut_slice(&mut obs, "obs", self.obs_len)?;
        out.copy_from_slice(self.ep.observation());
        Ok(())
    }

    /// Fill `out` with the research state (`sailgym_core.research_state_names()`
    /// order). A readout, not a restorable snapshot; see [`RawEnv::fork`].
    fn research_state(&self, mut out: PyReadwriteArray1<f64>) -> PyResult<()> {
        let dst = as_mut_slice(&mut out, "out", crate::research::RESEARCH_STATE_LEN)?;
        crate::research::fill(&self.ep, dst);
        Ok(())
    }

    /// A planning branch: an independent environment continuing from exactly
    /// this point (`Episode::fork`). It is driven by actions, never resets
    /// (`Disabled`), keeps no log, and nothing it does reaches this one.
    fn fork(&self) -> PyResult<Self> {
        let ep = self.ep.fork().map_err(value_error)?;
        Ok(Self {
            ep,
            obs_len: self.obs_len,
            action_dim: self.action_dim,
        })
    }

    /// A batch of planning branches, all stepped in Rust (ESE3990 plan §4.3).
    ///
    /// `actions` is `(K, H, action_dim)`: candidate `k` forks this episode and
    /// runs its `H` decisions through the ordinary `step()` path, stopping at
    /// its first terminal outcome. For each decision it writes the research
    /// state the period ended on (`out_rs`, `(K, H, RESEARCH_STATE_LEN)`), the
    /// outcome code (`out_code`: 0 running, 1 finished, 2 terminated,
    /// 3 truncated) and the physics steps executed (`out_steps`). Decisions
    /// after a terminal one are not simulated: their steps are 0, their code
    /// and state repeat the terminal ones. This episode is never touched.
    fn rollout(
        &self,
        actions: PyReadonlyArray3<f64>,
        mut out_rs: PyReadwriteArray3<f64>,
        mut out_code: PyReadwriteArray2<u8>,
        mut out_steps: PyReadwriteArray2<u32>,
    ) -> PyResult<()> {
        let sh = actions.shape();
        let (k, h) = (sh[0], sh[1]);
        let n = crate::research::RESEARCH_STATE_LEN;
        if sh[2] != self.action_dim {
            return Err(length_error("actions[..., :]", sh[2], self.action_dim));
        }
        let want = |got: &[usize], dims: &[usize], name: &str| -> PyResult<()> {
            if got != dims {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "`{name}` has shape {got:?}, expected {dims:?}"
                )));
            }
            Ok(())
        };
        want(out_rs.shape(), &[k, h, n], "out_rs")?;
        want(out_code.shape(), &[k, h], "out_code")?;
        want(out_steps.shape(), &[k, h], "out_steps")?;
        let acts = actions
            .as_slice()
            .map_err(|_| contiguity_error("actions"))?;
        let rs = out_rs
            .as_slice_mut()
            .map_err(|_| contiguity_error("out_rs"))?;
        let code = out_code
            .as_slice_mut()
            .map_err(|_| contiguity_error("out_code"))?;
        let steps = out_steps
            .as_slice_mut()
            .map_err(|_| contiguity_error("out_steps"))?;
        if k == 0 || h == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "K and H must be positive",
            ));
        }
        let inputs = acts
            .chunks(h * self.action_dim)
            .zip(rs.chunks_mut(h * n))
            .zip(code.chunks_mut(h))
            .zip(steps.chunks_mut(h))
            .collect();
        let action_dim = self.action_dim;
        self.ep
            .for_each_fork(inputs, |ep, (((acts, rs), code), steps)| {
                for d in 0..h {
                    let mut ran = 0;
                    if !ep.outcome().is_terminal() {
                        ep.push_action(&acts[d * action_dim..(d + 1) * action_dim])?;
                        ran = ep.step()?.steps;
                    }
                    crate::research::fill(ep, &mut rs[d * n..(d + 1) * n]);
                    steps[d] = ran;
                    code[d] = match ep.outcome() {
                        Outcome::Running => 0,
                        Outcome::Finished { .. } => 1,
                        Outcome::Terminated(_) => 2,
                        Outcome::Truncated => 3,
                    };
                }
                Ok(())
            })
            .map_err(value_error)
    }

    /// The outcome's name: `running`, `finished`, `truncated`, or — for a
    /// termination — the reason itself (`capsized`, `out_of_bounds`,
    /// `mark_missed`; see [`RawEnv::termination_reason`]). `Outcome::as_str`
    /// spells a termination by its reason, so there is no `terminated` here.
    #[getter]
    fn outcome(&self) -> String {
        self.ep.outcome().as_str().to_string()
    }

    /// Why it terminated, or `None`.
    #[getter]
    fn termination_reason(&self) -> Option<String> {
        self.ep.outcome().reason().map(|r| r.as_str().to_string())
    }

    /// The simulated time at which the route was completed, or `None`.
    #[getter]
    fn finish_time(&self) -> Option<f64> {
        self.ep.outcome().finish_time()
    }

    /// Physics steps executed in the current episode.
    #[getter]
    fn steps(&self) -> u64 {
        self.ep.steps()
    }

    /// The `u64` the **current** episode was started from. After an autoreset
    /// this is the drawn seed, not the root one.
    #[getter]
    fn seed(&self) -> u64 {
        self.ep.seed()
    }

    /// The `u64` `reset` was last called with.
    #[getter]
    fn root_seed(&self) -> u64 {
        self.ep.root_seed()
    }

    /// How many autoresets have happened since `reset`.
    #[getter]
    fn episode_index(&self) -> u64 {
        self.ep.episode_index()
    }

    /// s, the simulated time within the current episode.
    #[getter]
    fn t(&self) -> f64 {
        self.ep.state().t
    }

    #[getter]
    fn obs_len(&self) -> usize {
        self.obs_len
    }

    #[getter]
    fn action_dim(&self) -> usize {
        self.action_dim
    }
}

/// A contiguous `&[T]` out of a read-only `numpy` array, with its length
/// checked against what Rust expects.
///
/// A non-contiguous array is **refused**, not silently copied: a caller who
/// handed in a slice of a larger array meant to see the result in that array,
/// and a copy would write the answer somewhere they are not looking.
pub(crate) fn as_slice<'a, T: numpy::Element>(
    array: &'a PyReadonlyArray1<'_, T>,
    name: &str,
    want: usize,
) -> PyResult<&'a [T]> {
    let got = array.shape()[0];
    if got != want {
        return Err(length_error(name, got, want));
    }
    array.as_slice().map_err(|_| contiguity_error(name))
}

/// The same, writable.
pub(crate) fn as_mut_slice<'a, T: numpy::Element>(
    array: &'a mut PyReadwriteArray1<'_, T>,
    name: &str,
    want: usize,
) -> PyResult<&'a mut [T]> {
    let got = array.shape()[0];
    if got != want {
        return Err(length_error(name, got, want));
    }
    array.as_slice_mut().map_err(|_| contiguity_error(name))
}

pub(crate) fn length_error(name: &str, got: usize, want: usize) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "`{name}` has {got} elements and this environment wants {want}"
    ))
}

pub(crate) fn contiguity_error(name: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "`{name}` is not C-contiguous; the batch API writes into the caller's \
         own memory (F17.5) and will not silently copy"
    ))
}
