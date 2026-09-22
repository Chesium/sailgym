//! sailgym episode runner: `Simulation + Route + Agent → step / reset`,
//! `Outcome`, the autoreset convention, the decision log and `VecEnv`
//! (v2 `docs/v2/00-foundations.md` F14.1, F16.5, F17.4; section 06).
//!
//! Pure Rust. Four sailgym dependencies — `sailgym-physics`,
//! `sailgym-course`, `sailgym-agent` and `sailgym-task` — plus `serde` and
//! `rayon`. The arrows run **`env → {agent, course, task, physics}` and never
//! the other way** (F14.1), and
//! `vec_env::tests::physics_depends_on_no_v2_crate` asserts it with
//! `cargo tree` on every gate run: a reverse dependency would end the "builds
//! and tests on the host with plain `cargo test`" property that F8.1 exists
//! to protect.
//!
//! ## What is here
//!
//! | module | what it owns | task |
//! |---|---|---|
//! | [`outcome`] | `Outcome`, `TerminationReason`, `Bounds`, `AutoresetMode`, `Reward` | 6.1 |
//! | [`recording`] | `Decision`, `ResearchIdentity`, `ResearchEnvelope` | 6.1 |
//! | [`episode`] | one independent `Episode`: `reset`, `advance`, `step` | 6.2 |
//! | [`autoreset`] | what each convention means, and discounted returns | 6.3 |
//! | [`decision_log`] | the complete `(step, action)` record, and its replay | 6.4 |
//! | [`vec_env`] | N independent episodes, flat buffers, rayon | 6.5 |
//! | [`evaluate`] | the evaluation report | 6.7 |
//!
//! ## One independent episode first; `VecEnv` batches episodes
//!
//! Each [`Episode`](episode::Episode) owns its own `Simulation`, its own wind
//! field, its own seed, its own clock, its own tracker and its own reset
//! state. A [`VecEnv`](vec_env::VecEnv) is an ordered `Vec` of them and
//! nothing else: **an independent batch is not a shared-clock fleet**, and
//! `vec_env::tests::one_env_equals_a_single_episode` asserts that batching
//! changes no single-episode semantics (RV37). Resetting one slot leaves
//! every other slot bit-identical (RV38).
//!
//! There is **no boat-to-boat interaction of any kind** — no collisions, no
//! right-of-way, no wind shadow (`docs/v2/brief.md` §3). Two episodes may be
//! configured with the same wind and the same seed, which makes them equal,
//! not coupled. If shadowing is ever wanted the seam is a `WindField`
//! decorator and not a force term, and it would take the parallel path back
//! to a fixed index order (F16.5).
//!
//! ## The four traps, stated once
//!
//! 1. **Termination is not truncation.** `Outcome` is an enum and the batch
//!    API reports two masks, never their union (RV34). Conflating them
//!    biases every bootstrapped value — silently, in the returns.
//! 2. **The autoreset convention is pinned, recorded and *tested
//!    numerically*.** An off-by-one here shifts every bootstrapped value by
//!    one step and produces training curves that look merely mediocre rather
//!    than broken, which is the worst kind of bug. [`autoreset`] is the
//!    convention; `tests/returns.rs` is the proof, and it was demonstrated
//!    able to fail (RV33).
//! 3. **The capsize accumulator is carried, never recomputed.** F6.10 sets
//!    `capsized` after `|φ| > φ_capsize` holds *continuously* for
//!    `t_capsize`; it lives inside each episode's own `Simulation` and is
//!    read, not re-derived (RV36).
//! 4. **Cadence keys off the episode step counter**, never off a
//!    per-`advance` counter and never off elapsed time (F14.6). F9.7 —
//!    `advance(n) == n × advance(1)` — holds with an agent attached and is
//!    tested over six chunkings, not assumed.
//!
//! ## What is deliberately not here
//!
//! * **No Python.** Section 07. This crate has no `pyo3` and no binding.
//! * **No reward function.** A reward is an experiment parameter, not an
//!   environment constant: [`ZeroReward`](outcome::ZeroReward) is the only
//!   implementation that ships and everything else is configured.
//! * **No training loop.** `docs/v2/brief.md` S4 proposes the *environment*.
//! * **No physical coefficient.** No number in this crate reaches a force, a
//!   moment or an equation of motion, and
//!   `episode::tests::no_f7_literal_appears_in_the_env_crate` asserts on
//!   every gate run that none has been copied in (v1 brief §43, v2 F14.9).
//! * **No `Helm` and no setpoint action.** Task 5.6 is deferred and
//!   `ActionSpace::Setpoint` is still refused by `AgentSpec::validate`.
//! * **No WASM surface.** F8.2 is unchanged; no task in this section owns
//!   `crates/sailgym-wasm`, so the browser cannot start an episode.
//! * **No obstacles and no ray casting.** `docs/v2/brief.md` S6 is deferred
//!   and section 04 shipped none.

// One `pub mod` line per module, added by the task that owns the file: Rust
// has no way for a later task to declare its own module without touching the
// crate root, which is task 6.1's. Recorded in
// `docs/v2/progress/06-handoff.md` rather than quietly absorbed (F13.2), as
// sections 04, 05, 10 and 11 each recorded the same gap.
pub mod autoreset;
pub mod decision_log;
pub mod episode;
pub mod evaluate;
pub mod outcome;
pub mod recording;
pub mod vec_env;

pub use decision_log::DecisionLog;
pub use episode::{Episode, EpisodeConfig, Source, StepResult};
pub use evaluate::{evaluate, EpisodeSummary, EvalSuite, EvaluationReport};
pub use outcome::{AutoresetMode, Bounds, Outcome, Reward, RewardContext, TerminationReason};
pub use recording::{Decision, ResearchEnvelope, ResearchIdentity};
pub use vec_env::VecEnv;

/// Re-exported: `VecEnv::new` and `AgentSpec` both mention it, so a consumer
/// of this crate needs no second dependency for the one type in the
/// signature (F14.6).
pub use sailgym_agent::spec::Cadence;
