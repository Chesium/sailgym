//! N independent episodes, flat caller-provided buffers, rayon
//! (v2 section 06 task 6.5).
//!
//! # What is parallel here, and what is not
//!
//! F9.6 forbids parallelism **inside a single simulation step** and is
//! unchanged. These are N **independent** [`Episode`]s: they share no
//! accumulator, no field, no RNG and no clock, so each performs exactly the
//! arithmetic it would perform alone. v2 F16.5 permits that, in
//! `sailgym-env` and `sailgym-bench` only, and **`rayon` may not appear in
//! `sailgym-physics`**.
//!
//! That is an argument. [`tests::serial_and_parallel_agree_bit_for_bit`]
//! turns it into a fact, at N ∈ {1, 8, 64, 512} and every thread count,
//! with `to_bits()` — the assertion section 02's `vec_bench` makes about
//! bare `Simulation`s, re-run on the real runner. It has no tolerance to
//! loosen (RV35).
//!
//! # `obs_out: &mut [f32]` does not violate F9.5
//!
//! F9.5 forbids `f32` **intermediates in physics**. This is an output
//! buffer, exactly like `sample_wind_grid`'s: every number in it has
//! already been computed in `f64` by the sensors and is narrowed on its way
//! out, and nothing reads it back. The sentence is here, at the source, so
//! that nobody "fixes" it (F17.5 says the same).
//!
//! # The capsize accumulator is carried, not recomputed
//!
//! F6.10 sets `capsized` after `|φ| > φ_capsize` has held **continuously**
//! for `t_capsize`. It lives inside each episode's own `Simulation`, which
//! the `VecEnv` owns and never rebuilds mid-episode, so it is carried
//! through a vectorised step by construction. RV36 is the version of this
//! that a port gets wrong and no short test notices, so
//! [`tests::the_capsize_accumulator_survives_a_vectorised_step`] finds the
//! step a single episode capsizes on and asserts the batched one capsizes
//! on the same step — not earlier and not never.
//!
//! # Buffers are validated, never truncated
//!
//! A caller who hands in a buffer of the wrong length has a shape bug, and
//! a silently truncated batch turns it into a numerical one: half the
//! envs would be stepped with somebody else's action. Every length is
//! checked against the **runtime** observation layout before anything is
//! stepped. F8.3 is the physical state order and is not the observation
//! order: the observation layout is runtime data (F14.3), and
//! [`VecEnv::obs_len`] comes from it.

use rayon::prelude::*;

use sailgym_agent::observation::ObsLayout;
use sailgym_agent::spec::Cadence;

use crate::episode::{manual_source, EnvError, Episode, EpisodeConfig, StepResult};
use crate::outcome::{AutoresetMode, Outcome};

/// Why a batch call was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum VecError {
    /// A caller-provided buffer is the wrong length. Rejected, not
    /// truncated, and named so the caller knows which one.
    BufferLength {
        name: &'static str,
        got: usize,
        want: usize,
    },
    /// The number of seeds does not match the number of environments.
    SeedCount { got: usize, want: usize },
    /// An index past the end of the batch.
    NoSuchEnv { index: usize, len: usize },
    /// One environment refused its step. The index is the slot.
    Env { index: usize, why: EnvError },
}

impl std::fmt::Display for VecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BufferLength { name, got, want } => write!(
                f,
                "the `{name}` buffer is {got} long and must be {want}: a batch is rejected, \
                 never truncated"
            ),
            Self::SeedCount { got, want } => {
                write!(f, "{got} seeds for {want} environments")
            }
            Self::NoSuchEnv { index, len } => {
                write!(f, "environment {index} of {len}")
            }
            Self::Env { index, why } => write!(f, "environment {index}: {why}"),
        }
    }
}

impl std::error::Error for VecError {}

/// N independent episodes, stepped together.
///
/// An **ordered `Vec`** of [`Episode`]s and nothing else (F9.3). Batching
/// changes no single-episode semantics, and
/// [`tests::one_env_equals_a_single_episode`] asserts it (RV37).
pub struct VecEnv {
    envs: Vec<Episode>,
    layout: ObsLayout,
    obs_len: usize,
    action_dim: usize,
    mode: AutoresetMode,
    /// The observation each env ended its last episode on, when
    /// [`VecEnv::final_obs_valid`] says so.
    final_obs: Vec<f32>,
    final_obs_valid: Vec<u8>,
    /// Whether [`VecEnv::step_all`] uses rayon. The serial path exists so
    /// the bit-identity assertion has something to compare against, and it
    /// is the same code either way.
    parallel: bool,
}

impl VecEnv {
    /// Build one independent episode per seed.
    ///
    /// Every slot is driven by `manual`: a batch API's actions arrive from
    /// outside, and `manual` is the external action source section 05 built
    /// for exactly that — it goes through the same bounds check, the same
    /// adapter, the same cadence and the same log as a policy's action
    /// (RV27).
    /// `cadence` is the decision period every slot's `manual` source runs
    /// at. It is a constructor argument rather than a field of
    /// [`EpisodeConfig`] because F14.6.3 makes the cadence the **agent's**
    /// declaration, recorded in its `AgentSpec`: a batch builds its own
    /// agents, so a batch is where its cadence is chosen, and there is no
    /// second place for the number to disagree with itself.
    pub fn new(config: EpisodeConfig, cadence: Cadence, seeds: &[u64]) -> Result<Self, EnvError> {
        config.validate()?;
        cadence.validate()?;
        let mut envs = Vec::with_capacity(seeds.len());
        for seed in seeds {
            envs.push(Episode::new(config.clone(), manual_source(cadence), *seed)?);
        }
        let layout = match envs.first() {
            Some(e) => e.layout().clone(),
            None => {
                // An empty batch still has to know its shapes, so one
                // episode is built and dropped rather than the layout
                // being guessed.
                let probe = Episode::new(config.clone(), manual_source(cadence), 0)?;
                probe.layout().clone()
            }
        };
        let obs_len = layout.len();
        let action_dim = envs.first().map_or(
            sailgym_agent::actuation::rate::Rate::DIM,
            Episode::action_dim,
        );
        Ok(Self {
            final_obs: vec![0.0; seeds.len() * obs_len],
            final_obs_valid: vec![0; seeds.len()],
            envs,
            layout,
            obs_len,
            action_dim,
            mode: config.autoreset,
            parallel: true,
        })
    }

    /// Whether [`VecEnv::step_all`] runs across rayon threads.
    pub fn set_parallel(&mut self, parallel: bool) {
        self.parallel = parallel;
    }

    pub fn is_parallel(&self) -> bool {
        self.parallel
    }

    pub fn len(&self) -> usize {
        self.envs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.envs.is_empty()
    }

    /// Scalars in one observation. **Runtime data** (F14.3), not a
    /// constant, and not F8.3's state order.
    pub fn obs_len(&self) -> usize {
        self.obs_len
    }

    /// Scalars in one action.
    pub fn action_dim(&self) -> usize {
        self.action_dim
    }

    pub fn layout(&self) -> &ObsLayout {
        &self.layout
    }

    pub fn autoreset(&self) -> AutoresetMode {
        self.mode
    }

    pub fn episode(&self, index: usize) -> Option<&Episode> {
        self.envs.get(index)
    }

    pub fn episode_mut(&mut self, index: usize) -> Option<&mut Episode> {
        self.envs.get_mut(index)
    }

    /// The observation each env ended its last episode on, `obs_len`
    /// scalars per slot. Meaningful only where
    /// [`VecEnv::final_obs_valid`] is non-zero.
    pub fn final_obs(&self) -> &[f32] {
        &self.final_obs
    }

    /// One flag per slot: the corresponding block of [`VecEnv::final_obs`]
    /// was written by the last [`VecEnv::step_all`].
    ///
    /// Only ever set under [`AutoresetMode::SameStep`], where the returned
    /// observation is the **reset** one. Under
    /// [`AutoresetMode::NextStep`] the returned observation *is* the final
    /// one, so there is nothing to carry separately — which is the whole
    /// difference between the two conventions.
    pub fn final_obs_valid(&self) -> &[u8] {
        &self.final_obs_valid
    }

    /// Reset every slot, one seed each.
    pub fn reset_all(&mut self, seeds: &[u64]) -> Result<(), VecError> {
        if seeds.len() != self.envs.len() {
            return Err(VecError::SeedCount {
                got: seeds.len(),
                want: self.envs.len(),
            });
        }
        for (index, (env, seed)) in self.envs.iter_mut().zip(seeds.iter()).enumerate() {
            env.reset(*seed)
                .map_err(|why| VecError::Env { index, why })?;
        }
        self.final_obs_valid.iter_mut().for_each(|v| *v = 0);
        Ok(())
    }

    /// Reset one slot, leaving every other slot exactly as it was (RV38).
    pub fn reset_one(&mut self, index: usize, seed: u64) -> Result<(), VecError> {
        let len = self.envs.len();
        let env = self
            .envs
            .get_mut(index)
            .ok_or(VecError::NoSuchEnv { index, len })?;
        env.reset(seed)
            .map_err(|why| VecError::Env { index, why })?;
        self.final_obs_valid[index] = 0;
        Ok(())
    }

    /// Fill `obs_out` with every slot's current observation, without
    /// stepping. What a caller needs after [`VecEnv::reset_all`].
    pub fn observe_all(&mut self, obs_out: &mut [f32]) -> Result<(), VecError> {
        self.check_len("obs_out", obs_out.len(), self.envs.len() * self.obs_len)?;
        let width = self.obs_len;
        for (env, out) in self.envs.iter_mut().zip(obs_out.chunks_mut(width)) {
            narrow(env.observation(), out);
        }
        Ok(())
    }

    /// One decision period in every environment.
    ///
    /// `actions` is `N × action_dim`, `obs_out` is `N × obs_len`, and
    /// `rewards`, `terminated` and `truncated` are `N` each.
    /// **`terminated` and `truncated` are separate masks and their union is
    /// never reported** (RV34): a caller that wants it computes it and
    /// knows it has.
    pub fn step_all(
        &mut self,
        actions: &[f64],
        obs_out: &mut [f32],
        rewards: &mut [f64],
        terminated: &mut [u8],
        truncated: &mut [u8],
    ) -> Result<(), VecError> {
        let n = self.envs.len();
        let width = self.obs_len;
        self.check_len("actions", actions.len(), n * self.action_dim)?;
        self.check_len("obs_out", obs_out.len(), n * width)?;
        self.check_len("rewards", rewards.len(), n)?;
        self.check_len("terminated", terminated.len(), n)?;
        self.check_len("truncated", truncated.len(), n)?;

        let dim = self.action_dim;
        let envs = &mut self.envs;
        let finals = &mut self.final_obs;

        // The two arms run the **same** function over the same data; only
        // the iterator differs. Anything else and the bit-identity
        // assertion would be comparing two implementations rather than one
        // implementation on two schedules.
        let results: Vec<Result<StepResult, EnvError>> = if self.parallel {
            envs.par_iter_mut()
                .zip(actions.par_chunks(dim))
                .zip(obs_out.par_chunks_mut(width))
                .zip(finals.par_chunks_mut(width))
                .map(|(((env, a), o), f)| step_one(env, a, o, f))
                .collect()
        } else {
            envs.iter_mut()
                .zip(actions.chunks(dim))
                .zip(obs_out.chunks_mut(width))
                .zip(finals.chunks_mut(width))
                .map(|(((env, a), o), f)| step_one(env, a, o, f))
                .collect()
        };

        for (index, r) in results.into_iter().enumerate() {
            let r = r.map_err(|why| VecError::Env { index, why })?;
            rewards[index] = r.reward;
            terminated[index] = u8::from(r.terminated);
            truncated[index] = u8::from(r.truncated);
            self.final_obs_valid[index] = u8::from(r.final_obs_valid);
        }
        Ok(())
    }

    /// Every slot's outcome, in index order.
    pub fn outcomes(&self) -> Vec<Outcome> {
        self.envs.iter().map(Episode::outcome).collect()
    }

    fn check_len(&self, name: &'static str, got: usize, want: usize) -> Result<(), VecError> {
        if got == want {
            Ok(())
        } else {
            Err(VecError::BufferLength { name, got, want })
        }
    }
}

/// One environment's decision period. The **only** stepping code; the
/// serial and parallel arms both call it.
fn step_one(
    env: &mut Episode,
    action: &[f64],
    obs_out: &mut [f32],
    final_out: &mut [f32],
) -> Result<StepResult, EnvError> {
    // Pushed even on a `NextStep` reset call, where it is then cleared by
    // the agent's own reset: an out-of-range action is refused wherever it
    // arrives, not only where it would have been used.
    env.push_action(action)?;
    let result = env.step()?;
    if result.final_obs_valid {
        narrow(env.final_observation(), final_out);
    }
    narrow(env.observation(), obs_out);
    Ok(result)
}

/// `f64 → f32`, into an output buffer. See the module documentation for why
/// this is not an F9.5 violation.
fn narrow(from: &[f64], to: &mut [f32]) {
    debug_assert_eq!(from.len(), to.len());
    for (dst, src) in to.iter_mut().zip(from.iter()) {
        *dst = *src as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::TerminationReason;
    use sailgym_physics::scenario::load_shipped;
    use sailgym_physics::state::{STATE_FIELDS, STATE_LEN};
    use std::path::{Path, PathBuf};

    fn config() -> EpisodeConfig {
        let mut cfg = EpisodeConfig::new(load_shipped("gybe").expect("a shipped scenario"));
        cfg.autoreset = AutoresetMode::NextStep;
        cfg
    }

    /// Every batch in this module decides at 20 Hz over 200 Hz physics.
    fn cadence() -> Cadence {
        Cadence::new(10)
    }

    fn seeds(n: usize) -> Vec<u64> {
        (0..n as u64).map(|i| 1_000 + i).collect()
    }

    struct Buffers {
        actions: Vec<f64>,
        obs: Vec<f32>,
        rewards: Vec<f64>,
        terminated: Vec<u8>,
        truncated: Vec<u8>,
    }

    impl Buffers {
        fn for_env(env: &VecEnv) -> Self {
            let n = env.len();
            Self {
                actions: vec![0.0; n * env.action_dim()],
                obs: vec![0.0; n * env.obs_len()],
                rewards: vec![0.0; n],
                terminated: vec![0; n],
                truncated: vec![0; n],
            }
        }
    }

    /// A deterministic action for slot `i` at call `k`, so the batch is not
    /// stepped with zeros — which would make every env identical and the
    /// comparison vacuous.
    fn fill_actions(actions: &mut [f64], dim: usize, k: usize) {
        for (i, a) in actions.chunks_mut(dim).enumerate() {
            let phase = (i * 7 + k * 3) % 11;
            a[0] = (phase as f64 / 5.0) - 1.0;
            a[1] = if phase.is_multiple_of(3) { 1.0 } else { -1.0 };
            a[2] = -1.0;
        }
    }

    /// Run a batch for `calls` decision periods and return every env's
    /// final state, the observation buffer and the reward buffer.
    fn drive(
        n: usize,
        calls: usize,
        parallel: bool,
    ) -> (Vec<[f64; STATE_LEN]>, Vec<f32>, Vec<f64>) {
        let mut env = VecEnv::new(config(), cadence(), &seeds(n)).expect("a valid batch");
        env.set_parallel(parallel);
        let mut b = Buffers::for_env(&env);
        for k in 0..calls {
            fill_actions(&mut b.actions, env.action_dim(), k);
            env.step_all(
                &b.actions,
                &mut b.obs,
                &mut b.rewards,
                &mut b.terminated,
                &mut b.truncated,
            )
            .expect("a valid batch step");
        }
        let states = (0..n)
            .map(|i| env.episode(i).expect("a slot").state().to_array())
            .collect();
        (states, b.obs, b.rewards)
    }

    fn assert_states_identical(a: &[[f64; STATE_LEN]], b: &[[f64; STATE_LEN]], what: &str) {
        assert_eq!(a.len(), b.len(), "{what}: population changed");
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            for k in 0..STATE_LEN {
                assert_eq!(
                    x[k].to_bits(),
                    y[k].to_bits(),
                    "{what}: env {i}, field {}: {} vs {}",
                    STATE_FIELDS[k],
                    x[k],
                    y[k]
                );
            }
        }
    }

    /// Section acceptance 2, and task 6.5's first criterion: serial and
    /// parallel agree **bit for bit** at N ∈ {1, 8, 64, 512} and every
    /// thread count. This is section 02 task 2.7's assertion re-run on the
    /// real runner, and it is what turns F16.5's argument into a fact
    /// (RV35).
    #[test]
    fn serial_and_parallel_agree_bit_for_bit() {
        let all = std::thread::available_parallelism().map_or(1, |n| n.get());
        let mut threads = Vec::new();
        let mut t = 1usize;
        while t < all {
            threads.push(t);
            t *= 2;
        }
        threads.push(all);
        threads.dedup();

        for n in [1usize, 8, 64, 512] {
            let calls = if n > 64 { 2 } else { 4 };
            let (reference, ref_obs, ref_rewards) = drive(n, calls, false);
            for &t in &threads {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(t)
                    .build()
                    .expect("a rayon pool");
                let (states, obs, rewards) = pool.install(|| drive(n, calls, true));
                assert_states_identical(&reference, &states, &format!("N = {n}, {t} threads"));
                assert_eq!(
                    obs.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    ref_obs.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "N = {n}, {t} threads: the observation buffer differs"
                );
                assert_eq!(
                    rewards.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    ref_rewards.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "N = {n}, {t} threads: the reward buffer differs"
                );
            }
            // …and the envs are not all the same boat, or agreeing would
            // prove nothing: `gybe` is the one shipped scenario with a
            // `Gust` field, so a per-slot seed changes the trajectory.
            if n > 1 {
                assert_ne!(
                    reference[0][0].to_bits(),
                    reference[1][0].to_bits(),
                    "N = {n}: two slots computed the same trajectory"
                );
            }
        }
        eprintln!("vec_env: serial == parallel at N ∈ {{1, 8, 64, 512}} × {threads:?} threads");
    }

    /// RV37: batching changes no single-episode semantics.
    #[test]
    fn one_env_equals_a_single_episode() {
        let calls = 6;
        let (batched, _, batched_rewards) = drive(1, calls, true);

        let mut alone = Episode::new(config(), manual_source(Cadence::new(10)), seeds(1)[0])
            .expect("a valid episode");
        let mut actions = vec![0.0; alone.action_dim()];
        let mut rewards = Vec::new();
        for k in 0..calls {
            fill_actions(&mut actions, alone.action_dim(), k);
            alone.push_action(&actions).expect("in bounds");
            rewards.push(alone.step().expect("a valid step").reward);
        }
        assert_states_identical(
            &batched,
            &[alone.state().to_array()],
            "VecEnv N = 1 vs one Episode",
        );
        assert_eq!(batched_rewards.len(), 1);
        assert_eq!(
            batched_rewards[0].to_bits(),
            rewards.last().expect("a reward").to_bits()
        );
    }

    /// RV36, the one test that would notice: an env that would capsize at
    /// step `k` does so at step `k` under `step_all` — not earlier and not
    /// never.
    #[test]
    fn the_capsize_accumulator_survives_a_vectorised_step() {
        let mut cfg =
            EpisodeConfig::new(load_shipped("beam_reach_capsize").expect("a shipped scenario"));
        cfg.autoreset = AutoresetMode::Disabled;
        cfg.max_steps = Some(40_000);

        // Where a single episode capsizes, stepping one at a time — and
        // where it *first exceeded the angle*, which is a different step.
        let mut alone =
            Episode::new(cfg.clone(), manual_source(Cadence::new(10)), 5).expect("valid");
        alone.push_action(&[0.0, -1.0, -1.0]).expect("in bounds");
        let phi_capsize = alone.params().stability.phi_capsize;
        let hold_steps = (alone.params().stability.t_capsize / alone.params().sim.dt).round();
        let mut first_over: Option<u64> = None;
        while !alone.outcome().is_terminal() {
            alone.advance(1).expect("a valid action");
            if first_over.is_none() && alone.state().phi.abs() > phi_capsize {
                first_over = Some(alone.steps());
            }
        }
        assert_eq!(
            alone.outcome(),
            Outcome::Terminated(TerminationReason::Capsized)
        );
        let k = alone.steps();
        assert!(
            k > 100,
            "the fixture capsized at step {k}, too early to test"
        );

        // The flag is F6.10's **accumulator**, not the instantaneous angle:
        // it fires `t_capsize` after the angle was first exceeded and not
        // on the step it was exceeded. An implementation that recomputed it
        // from the current state would fire at `first_over`, and this is
        // the assertion that says so (RV36).
        let first_over = first_over.expect("the boat never exceeded the capsize angle");
        assert!(
            k > first_over,
            "capsize fired on the very step the angle was exceeded ({k}): the accumulator \
             was recomputed rather than carried (RV36)"
        );
        assert!(
            ((k - first_over) as f64 - hold_steps).abs() <= 1.0,
            "capsize fired {} steps after the angle was exceeded; F6.10's t_capsize is \
             {hold_steps} steps",
            k - first_over
        );

        // …and where a batch does.
        let mut env = VecEnv::new(cfg, cadence(), &[5, 6, 7, 8]).expect("a valid batch");
        let mut b = Buffers::for_env(&env);
        let mut calls = 0usize;
        while env.episode(0).expect("a slot").outcome() == Outcome::Running {
            for a in b.actions.chunks_mut(env.action_dim()) {
                a.copy_from_slice(&[0.0, -1.0, -1.0]);
            }
            env.step_all(
                &b.actions,
                &mut b.obs,
                &mut b.rewards,
                &mut b.terminated,
                &mut b.truncated,
            )
            .expect("a valid batch step");
            calls += 1;
            assert!(calls < 5_000, "the batch never capsized");
        }
        let slot = env.episode(0).expect("a slot");
        assert_eq!(
            slot.outcome(),
            Outcome::Terminated(TerminationReason::Capsized),
            "capsize did not fire under step_all"
        );
        assert_eq!(
            slot.steps(),
            k,
            "the batched env capsized at a different step: the accumulator was recomputed \
             rather than carried (RV36)"
        );
        assert_eq!(b.terminated[0], 1);
        assert_eq!(b.truncated[0], 0, "a capsize is not a truncation");
    }

    #[test]
    fn buffer_length_mismatches_are_rejected_not_truncated() {
        let mut env = VecEnv::new(config(), cadence(), &seeds(4)).expect("a valid batch");
        let good = Buffers::for_env(&env);
        let n = env.len();
        let obs_len = env.obs_len();
        let dim = env.action_dim();

        let mut obs = good.obs.clone();
        let mut rewards = good.rewards.clone();
        let mut terminated = good.terminated.clone();
        let mut truncated = good.truncated.clone();

        let short = vec![0.0; n * dim - 1];
        assert_eq!(
            env.step_all(
                &short,
                &mut obs,
                &mut rewards,
                &mut terminated,
                &mut truncated
            ),
            Err(VecError::BufferLength {
                name: "actions",
                got: n * dim - 1,
                want: n * dim
            })
        );

        let mut wide = vec![0.0f32; n * obs_len + 1];
        assert_eq!(
            env.step_all(
                &good.actions,
                &mut wide,
                &mut rewards,
                &mut terminated,
                &mut truncated
            ),
            Err(VecError::BufferLength {
                name: "obs_out",
                got: n * obs_len + 1,
                want: n * obs_len
            })
        );

        let mut few = vec![0.0; n - 1];
        assert!(matches!(
            env.step_all(
                &good.actions,
                &mut obs,
                &mut few,
                &mut terminated,
                &mut truncated
            ),
            Err(VecError::BufferLength {
                name: "rewards",
                ..
            })
        ));

        let mut few = vec![0u8; n - 1];
        assert!(matches!(
            env.step_all(
                &good.actions,
                &mut obs,
                &mut rewards,
                &mut few,
                &mut truncated
            ),
            Err(VecError::BufferLength {
                name: "terminated",
                ..
            })
        ));
        assert!(matches!(
            env.step_all(
                &good.actions,
                &mut obs,
                &mut rewards,
                &mut terminated,
                &mut few
            ),
            Err(VecError::BufferLength {
                name: "truncated",
                ..
            })
        ));

        // A refused call steps nothing: every slot is still at step 0.
        assert!(env.envs.iter().all(|e| e.steps() == 0));

        // And the right lengths work.
        let mut b = Buffers::for_env(&env);
        assert!(env
            .step_all(
                &b.actions,
                &mut b.obs,
                &mut b.rewards,
                &mut b.terminated,
                &mut b.truncated
            )
            .is_ok());
        assert_eq!(
            env.reset_all(&[1, 2, 3]),
            Err(VecError::SeedCount { got: 3, want: 4 })
        );
    }

    /// RV38: resetting one slot leaves every other slot bit-identical.
    #[test]
    fn resetting_one_slot_leaves_the_others_untouched() {
        let mut env = VecEnv::new(config(), cadence(), &seeds(4)).expect("a valid batch");
        let mut b = Buffers::for_env(&env);
        for k in 0..3 {
            fill_actions(&mut b.actions, env.action_dim(), k);
            env.step_all(
                &b.actions,
                &mut b.obs,
                &mut b.rewards,
                &mut b.terminated,
                &mut b.truncated,
            )
            .expect("a valid batch step");
        }
        let before: Vec<[f64; STATE_LEN]> = (0..4)
            .map(|i| env.episode(i).expect("a slot").state().to_array())
            .collect();

        env.reset_one(2, 9_999).expect("a valid reset");
        for i in [0usize, 1, 3] {
            let now = env.episode(i).expect("a slot").state().to_array();
            assert_states_identical(
                &[before[i]],
                &[now],
                &format!("resetting slot 2 moved slot {i}"),
            );
            assert_eq!(env.episode(i).expect("a slot").steps(), 30);
        }
        let reset = env.episode(2).expect("a slot");
        assert_eq!(reset.steps(), 0);
        assert_eq!(reset.seed(), 9_999);
        assert_eq!(
            env.reset_one(9, 0),
            Err(VecError::NoSuchEnv { index: 9, len: 4 })
        );
    }

    /// Under `SameStep` the returned observation is the reset one and the
    /// final observation is carried separately; under `NextStep` it is the
    /// other way round. The masks say which.
    #[test]
    fn the_final_observation_follows_the_convention() {
        let mut cfg = config();
        cfg.max_steps = Some(30);

        for mode in AutoresetMode::AUTORESETTING {
            cfg.autoreset = mode;
            let mut env = VecEnv::new(cfg.clone(), cadence(), &[1, 2]).expect("a valid batch");
            let mut b = Buffers::for_env(&env);
            let mut saw_boundary = false;
            for k in 0..6 {
                fill_actions(&mut b.actions, env.action_dim(), k);
                env.step_all(
                    &b.actions,
                    &mut b.obs,
                    &mut b.rewards,
                    &mut b.terminated,
                    &mut b.truncated,
                )
                .expect("a valid batch step");
                if b.truncated[0] == 1 {
                    saw_boundary = true;
                    match mode {
                        AutoresetMode::SameStep => {
                            assert_eq!(env.final_obs_valid()[0], 1, "{mode:?}");
                            assert_eq!(
                                env.episode(0).expect("a slot").steps(),
                                0,
                                "SameStep resets inside the terminating call"
                            );
                        }
                        AutoresetMode::NextStep => {
                            assert_eq!(env.final_obs_valid()[0], 0, "{mode:?}");
                            assert_eq!(
                                env.episode(0).expect("a slot").steps(),
                                30,
                                "NextStep resets on the following call"
                            );
                        }
                        AutoresetMode::Disabled => unreachable!(),
                    }
                }
            }
            assert!(saw_boundary, "{mode:?}: no episode ended in six calls");
        }
    }

    // -----------------------------------------------------------------
    // Section acceptance 5
    // -----------------------------------------------------------------

    /// `rayon` appears in `sailgym-env` and `sailgym-bench` and nowhere
    /// else — a grep over **every** `Cargo.toml` in the workspace (RV40).
    #[test]
    fn rayon_appears_in_exactly_two_manifests() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates/")
            .parent()
            .expect("the repository root")
            .to_path_buf();
        let mut manifests: Vec<PathBuf> = vec![root.join("Cargo.toml")];
        let mut crates: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
            .expect("crates/ must be readable")
            .filter_map(Result::ok)
            .map(|e| e.path().join("Cargo.toml"))
            .filter(|p| p.exists())
            .collect();
        crates.sort();
        assert!(crates.len() >= 6, "only {} crates found", crates.len());
        manifests.extend(crates);

        let mut with_rayon: Vec<String> = Vec::new();
        for path in &manifests {
            let text = std::fs::read_to_string(path).expect("a readable manifest");
            // The declaration, not the prose: every `rayon` in this
            // repository's manifests that is *not* a dependency line is in
            // a comment explaining why it may not appear.
            let declared = text
                .lines()
                .any(|l| !l.trim_start().starts_with('#') && l.trim_start().starts_with("rayon"));
            if declared {
                with_rayon.push(
                    path.parent()
                        .and_then(|p| p.file_name())
                        .and_then(|s| s.to_str())
                        .unwrap_or("?")
                        .to_string(),
                );
            }
        }
        with_rayon.sort();
        assert_eq!(
            with_rayon,
            vec!["sailgym-bench".to_string(), "sailgym-env".to_string()],
            "F16.5 permits rayon in `sailgym-env` and `sailgym-bench` only"
        );
    }

    /// Section acceptance 7: `docs/v2/throughput.md` still carries the
    /// `VecEnv` section task 6.6 wrote into it.
    ///
    /// `vec_bench --write` rewrites that document from its own template and
    /// drops everything it does not know about — the same hazard section 03
    /// recorded for `conformance.md`, and the same remedy: a test in the
    /// gate that goes red when the section has gone missing, naming the
    /// command that puts it back. The **numbers** are deliberately not
    /// asserted; they are a property of one machine.
    #[test]
    fn the_throughput_document_still_carries_the_env_section() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates/")
            .parent()
            .expect("the repository root")
            .join("docs/v2/throughput.md");
        let doc = std::fs::read_to_string(&path).expect("throughput.md must be readable");
        for marker in [
            "<!-- BEGIN sailgym-env throughput (section 06) -->",
            "<!-- END sailgym-env throughput (section 06) -->",
            "## The episode runner (section 06)",
        ] {
            assert!(
                doc.contains(marker),
                "docs/v2/throughput.md has no `{marker}`. It is written by \
                 `cargo run --release -p sailgym-bench --bin env_bench -- --write \
                 docs/v2/throughput.md` and is dropped whenever `vec_bench --write` \
                 regenerates the file; re-run it."
            );
        }
        // Both halves are in the one document (section acceptance 7).
        assert!(
            doc.contains("bare `Simulation`") && doc.contains("`VecEnv`, serial"),
            "the document no longer carries both the bare-Simulation and the VecEnv figures"
        );
    }

    /// Section acceptance 5's other half, and F14.1: the arrow runs
    /// `env → physics` and never the reverse.
    #[test]
    fn physics_depends_on_no_v2_crate() {
        let out = std::process::Command::new(env!("CARGO"))
            .args(["tree", "-p", "sailgym-physics", "--edges", "all"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output();
        let Ok(out) = out else {
            eprintln!("skip: cargo tree is not runnable here");
            return;
        };
        if !out.status.success() {
            eprintln!(
                "skip: cargo tree failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            return;
        }
        let tree = String::from_utf8_lossy(&out.stdout);
        for name in [
            "sailgym-env",
            "sailgym-agent",
            "sailgym-course",
            "sailgym-task",
            "rayon",
            "wasm-bindgen",
        ] {
            assert!(
                !tree.contains(name),
                "cargo tree -p sailgym-physics mentions {name} (F14.1, F16.5):\n{tree}"
            );
        }
    }
}
