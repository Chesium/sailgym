//! N independent episodes, across the boundary (v2 F16.5, F17.5; section 07
//! task 7.4).
//!
//! # Caller-owned buffers, and the GIL released around the work
//!
//! [`RawVecEnv::step_all`] takes five `numpy` arrays the caller allocated,
//! borrows them as plain Rust slices, **releases the GIL**, and hands the
//! slices to [`sailgym_env::VecEnv::step_all`]. Nothing on this side is
//! allocated per step, no Python object is created, and a Python thread runs
//! for the whole duration.
//!
//! Two things that sound alike and are not, stated once because F17.5 says
//! they are confused in practice:
//!
//! * **Releasing the GIL is not what makes rayon work.** `VecEnv` parallelises
//!   internally whether or not the caller holds the GIL. Releasing it is what
//!   lets *Python* code — a data-loader thread, a logger, another env's
//!   post-processing — run at the same time.
//! * **A stable buffer address is not an absence of allocation.** It proves
//!   the buffer is reused, which is what the zero-copy assertion is about;
//!   `python/tests/test_performance.py` measures retained and peak allocation
//!   separately because of it.
//!
//! # The observation buffer is `f32`, and that is not an F9.5 violation
//!
//! F17.5 in as many words: `obs_out: &mut [f32]` is an **output buffer**,
//! exactly like `sample_wind_grid`'s. F9.5 forbids `f32` *intermediates in
//! physics*, and there are none: rewards, actions and every state scalar
//! crossing here are `f64`.
//!
//! # Two masks, never their union
//!
//! `terminated` and `truncated` are separate `uint8` arrays because
//! `VecEnv::step_all` reports them separately and never computes their union
//! (RV34, F19.2). A caller that wants it computes it and knows it has.

use numpy::{PyReadonlyArray2, PyReadwriteArray1, PyReadwriteArray2, PyUntypedArrayMethods};
use pyo3::prelude::*;

use sailgym_env::VecEnv;

use crate::env::{contiguity_error, length_error};
use crate::spec::{value_error, Spec};

/// N independent episodes, stepped together.
#[pyclass(unsendable, module = "sailgym_core", name = "RawVecEnv")]
pub struct RawVecEnv {
    v: VecEnv,
    n: usize,
    obs_len: usize,
    action_dim: usize,
}

#[pymethods]
impl RawVecEnv {
    /// One independent episode per seed.
    ///
    /// `parallel` selects rayon. Both arms run the same code over the same
    /// data and section 06 asserts they are `to_bits()`-identical at
    /// N ∈ {1, 8, 64, 512} × six thread counts, so this is a throughput
    /// switch and never a semantic one (F16.5).
    #[new]
    #[pyo3(signature = (spec, seeds, parallel = true))]
    fn new(spec: &Spec, seeds: Vec<u64>, parallel: bool) -> PyResult<Self> {
        let mut v = VecEnv::new(spec.config(), spec.cadence, &seeds).map_err(value_error)?;
        v.set_parallel(parallel);
        let obs_len = v.obs_len();
        let action_dim = v.action_dim();
        Ok(Self {
            v,
            n: seeds.len(),
            obs_len,
            action_dim,
        })
    }

    /// Reset every slot, one seed each (F17.3).
    fn reset_all(&mut self, seeds: Vec<u64>) -> PyResult<()> {
        self.v.reset_all(&seeds).map_err(value_error)
    }

    /// Reset one slot, leaving every other slot exactly as it was.
    fn reset_one(&mut self, index: usize, seed: u64) -> PyResult<()> {
        self.v.reset_one(index, seed).map_err(value_error)
    }

    /// Fill `obs` with every slot's current observation, without stepping.
    /// What a caller needs after [`RawVecEnv::reset_all`].
    fn observe_all(&mut self, mut obs: PyReadwriteArray2<f32>) -> PyResult<()> {
        let out = mat_mut(&mut obs, "obs", self.n, self.obs_len)?;
        self.v.observe_all(out).map_err(value_error)
    }

    /// One decision period in every environment, with the GIL released.
    ///
    /// `actions` is `(N, action_dim)` `float64`, `obs` is `(N, obs_len)`
    /// `float32`, and `rewards`, `terminated` and `truncated` are `(N,)`
    /// `float64`, `uint8` and `uint8`. Every one is written in place.
    fn step_all(
        &mut self,
        py: Python<'_>,
        actions: PyReadonlyArray2<f64>,
        mut obs: PyReadwriteArray2<f32>,
        mut rewards: PyReadwriteArray1<f64>,
        mut terminated: PyReadwriteArray1<u8>,
        mut truncated: PyReadwriteArray1<u8>,
    ) -> PyResult<()> {
        let n = self.n;
        let a = mat(&actions, "actions", n, self.action_dim)?;
        let o = mat_mut(&mut obs, "obs", n, self.obs_len)?;
        let r = vec_mut(&mut rewards, "rewards", n)?;
        let te = vec_mut(&mut terminated, "terminated", n)?;
        let tr = vec_mut(&mut truncated, "truncated", n)?;
        let v = &mut self.v;
        // The whole point of the task. Nothing inside touches a Python
        // object: the five slices are plain Rust memory that happens to be
        // owned by `numpy`, and `VecEnv::step_all` is pure Rust.
        py.detach(move || v.step_all(a, o, r, te, tr))
            .map_err(value_error)
    }

    /// Copy the observation each slot ended its last episode on into `out`.
    ///
    /// Meaningful only where [`RawVecEnv::final_obs_valid`] is non-zero, which
    /// is only ever under `SameStep` — under `NextStep` the returned
    /// observation *is* the final one.
    fn final_obs(&self, mut out: PyReadwriteArray2<f32>) -> PyResult<()> {
        let dst = mat_mut(&mut out, "out", self.n, self.obs_len)?;
        dst.copy_from_slice(self.v.final_obs());
        Ok(())
    }

    /// One flag per slot: the corresponding row of [`RawVecEnv::final_obs`]
    /// was written by the last [`RawVecEnv::step_all`].
    fn final_obs_valid(&self, mut out: PyReadwriteArray1<u8>) -> PyResult<()> {
        let dst = vec_mut(&mut out, "out", self.n)?;
        dst.copy_from_slice(self.v.final_obs_valid());
        Ok(())
    }

    /// Physics steps executed in each slot's **current** episode, in index
    /// order.
    ///
    /// What a caller needs to key a scripted action off the *episode's own*
    /// decision counter rather than off a global call counter — the
    /// distinction section 06 §3.1 records as the difference between a
    /// returns test that means something and one that fails for an unrelated
    /// reason.
    #[getter]
    fn episode_steps(&self) -> Vec<u64> {
        (0..self.n)
            .map(|i| self.v.episode(i).map_or(0, sailgym_env::Episode::steps))
            .collect()
    }

    /// Every slot's outcome name, in index order.
    #[getter]
    fn outcomes(&self) -> Vec<String> {
        self.v
            .outcomes()
            .into_iter()
            .map(|o| o.as_str().to_string())
            .collect()
    }

    /// Every slot's termination reason, in index order; `None` where there is
    /// none.
    #[getter]
    fn termination_reasons(&self) -> Vec<Option<String>> {
        self.v
            .outcomes()
            .into_iter()
            .map(|o| o.reason().map(|r| r.as_str().to_string()))
            .collect()
    }

    /// Whether [`RawVecEnv::step_all`] uses rayon.
    fn set_parallel(&mut self, parallel: bool) {
        self.v.set_parallel(parallel);
    }

    #[getter]
    fn is_parallel(&self) -> bool {
        self.v.is_parallel()
    }

    #[getter]
    fn num_envs(&self) -> usize {
        self.n
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

/// A flat, contiguous `&[T]` out of an `(rows, cols)` read-only array.
fn mat<'a, T: numpy::Element>(
    array: &'a PyReadonlyArray2<'_, T>,
    name: &str,
    rows: usize,
    cols: usize,
) -> PyResult<&'a [T]> {
    check_shape(array.shape(), name, rows, cols)?;
    array.as_slice().map_err(|_| contiguity_error(name))
}

/// The same, writable.
fn mat_mut<'a, T: numpy::Element>(
    array: &'a mut PyReadwriteArray2<'_, T>,
    name: &str,
    rows: usize,
    cols: usize,
) -> PyResult<&'a mut [T]> {
    check_shape(array.shape(), name, rows, cols)?;
    array.as_slice_mut().map_err(|_| contiguity_error(name))
}

/// A flat, contiguous `&mut [T]` out of an `(n,)` array.
fn vec_mut<'a, T: numpy::Element>(
    array: &'a mut PyReadwriteArray1<'_, T>,
    name: &str,
    n: usize,
) -> PyResult<&'a mut [T]> {
    let got = array.shape()[0];
    if got != n {
        return Err(length_error(name, got, n));
    }
    array.as_slice_mut().map_err(|_| contiguity_error(name))
}

fn check_shape(shape: &[usize], name: &str, rows: usize, cols: usize) -> PyResult<()> {
    if shape != [rows, cols] {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "`{name}` has shape {shape:?} and this batch wants [{rows}, {cols}]"
        )));
    }
    Ok(())
}
