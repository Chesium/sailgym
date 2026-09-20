# v2 Section 06 — `sailgym-env`: the episode runner, the autoreset convention, and `VecEnv`

**Planning revision (2026-09-20):** later than playable milestone 08–11; IDs are preserved, not dispatch order. Read the revised index/brief/F18 and dependency handoffs. All required scope/normative decisions remain proposed. Compare no-change guards against this section's starting revision, not the pre-08 baseline.

Source discussions: `../discussions/cross-stack.md` §§0, 1.4, 6, 8 points **1**
and **3**; `../discussions/unified-agent-interface.md` §§8, 9.

> **BLOCKED.** Not dispatchable until `../brief.md` S4 has a recorded implementation decision,
> answering `../README.md` V-A.

Read first: `../../v1/00-foundations.md` in full, `../README.md`, `../brief.md`,
`../00-foundations.md` (F14, F16.5, F17.4), `../progress/05-handoff.md`, and
`../throughput.md` — which section 02 measured precisely so this section is a
decision rather than a guess. Then this PRD.

## Goal

The episode runner: `Simulation + Route + Agent → step / reset`, with
termination distinguished from truncation, decisions logged, and a `VecEnv` that
steps N independent episodes under rayon.

Two of cross-stack §8's points land here. Point **1** is `VecEnv`. Point **3** —
pin the autoreset convention and test returns computed both ways — lands here
rather than in the Python binding, because F17.4 makes episode boundaries
authoritative in Rust and a convention decided in Python would be a convention
the vectorised path cannot see.

## Why this shape

**One independent episode first, VecEnv batches episodes.** Each episode owns its wind/seed, clock, outcome and reset state. An independent batch is not a shared-clock fleet. Build a live multi-boat episode only when S5 and its product need are selected; recorded ghosts do not need it.

Distinguish terminated and truncated in every batch result, retain final observations and define autoreset mode explicitly. A combined done mask may be derived, never replace those fields.

## What this section does **not** do

- No Python. Section 07.
- **No boat-to-boat interaction of any kind** — no collisions, no right-of-way,
  no wind shadow (`brief.md` §3). Independent episodes can use identical wind configurations/seeds but own their reset state. If
  shadowing is ever wanted, the seam is a `WindField` decorator, not a force
  term, and it would take the parallel path back to a fixed index order.
- No reward function beyond what evaluation needs. A reward is an experiment
  parameter, not an environment constant, and it is configured, not compiled in.
- No training loop. `brief.md` S4 proposes the *environment*, not a training
  programme, and this section keeps that line.
- No physical coefficient added or changed anywhere.

## Normative deltas

### D1 — F16.5 (parallelism) and F17.4 (episode boundaries). **Open; blocking.**

`../00-foundations.md` F16.5 permits rayon across independent boats in
`sailgym-env` and `sailgym-bench`, and **only** there; F9.6 is unchanged and
still forbids parallelism inside a single step. F17.4 puts `Outcome` and the
autoreset convention in Rust.

### D2 — F12′ step 3 gains `-p sailgym-env`. **Open.** As section 04's D2.

### D3 — `brief.md` S4. **Open; record the implementation decision.**

## The autoreset convention, stated once

Vector APIs differ over whether reset observations appear on the same or next step and how final observations are returned. The implementation must verify the selected pinned API, not assume a historical version behavior.

**Check the convention of the exact version pinned in `uv.lock`. Do not assume
it, and do not take it from this document.** Then:

1. Pick one convention and implement it in Rust, once, in `episode.rs`.
2. Record it in the episode header as a named enum value, so a log says which
   convention produced it.
3. Test it **numerically**: compute discounted returns for a fixed action
   sequence under both conventions and assert they agree.

The third step is the one that matters and the one most likely to be skipped. An
off-by-one here shifts every bootstrapped value by one step and produces
training curves that look merely *mediocre* rather than broken — which is the
worst kind of bug, because it survives review, survives a demo, and is
indistinguishable from "the task is hard" for as long as anyone is willing to
keep tuning.

## Tasks

### 6.1 — Contracts: the crate, `Outcome`, the episode header

**Owns:** `crates/sailgym-env/Cargo.toml`, `crates/sailgym-env/src/lib.rs`,
`crates/sailgym-env/src/outcome.rs`, `crates/sailgym-env/src/recording.rs`, `Cargo.toml`, `Cargo.lock`
**P-group: S**

```rust
pub enum Outcome {
    Running,
    Finished { time: f64 },
    Terminated(TerminationReason),   // Capsized, OutOfBounds, MarkMissed
    Truncated,                       // step budget only
}
```

An enum, not a bool. Gymnasium distinguishes task termination from time-limit
truncation, and conflating them biases value bootstrapping — silently, in the
returns, which is where this section's whole risk lives.

Reuse section 10's complete identity and section 11's task outcomes; do not duplicate their semantics. The env crate owns a versioned research envelope around the existing recorded Episode, adding agent/action/observation/cadence/route/autoreset metadata and a complete decision log. It does not mutate physics recording types from a task without ownership.

Do not assume the current schema is still 1 or automatically bump it to 2. Preserve section 10's legacy-viewing policy; version the research envelope independently and validate the nested recording through the existing codec. The research envelope lives in the recording module owned by 6.1.

Acceptance: nested recording and research metadata round-trip; known legacy recordings remain inspectable and cannot acquire fabricated agent metadata. Incompatible identities refuse action-resimulation comparisons.

### 6.2 — The episode runner

**Owns:** `crates/sailgym-env/src/episode.rs`
**P-group: A**

A single independent Episode; VecEnv in 6.5 owns an ordered Vec of Episodes. `reset(seed: u64)`, `step()`, cadence
from F14.6, zero-order hold between decisions, `Outcome` evaluated every step.

The scenario, if randomised per episode, derives from the same `u64` through
`STREAM_SCENARIO` — which `rng.rs` has reserved for exactly this since v1.
There is no second RNG.

Acceptance: `cargo test -p sailgym-env episode` — F9.7 holds
(`advance(n) == n × advance(1)`) with an agent attached, bit-for-bit, over the
same six chunkings section 05 used; a fixed seed reproduces a trajectory
bit-for-bit across two processes; `Outcome` transitions are asserted one per
reason.

### 6.3 — The autoreset convention

**Owns:** `crates/sailgym-env/src/autoreset.rs`,
`crates/sailgym-env/tests/returns.rs`
**P-group: A**

The convention above, plus the numeric test that is the point of it:

- A fixed action sequence, a fixed seed, an episode that terminates partway.
- Discounted returns computed under both conventions, at two discount factors.
- Assert they agree to a stated numeric bound, and assert the bound is tight
  enough to catch a one-step shift — which is checked by **deliberately
  introducing the shift and observing the test go red**, then reverting.

Acceptance: `cargo test -p sailgym-env --test returns`; the deliberate-shift
demonstration recorded in the handoff with the observed discrepancy; the
installed Gymnasium version and its documented convention named in the handoff,
with the URL or the version string that established it.

### 6.4 — Decision logging

**Owns:** `crates/sailgym-env/src/decision_log.rs`
**P-group: B**

The existing recorder samples at `log_hz` and would miss intervening actions — a
real gap for RL, where the action sequence **is** the artifact. Since decisions
are at a fixed cadence with zero-order hold, `(step_index, action)` pairs are a
complete and small record.

A `decisions` array in the research envelope alongside the nested recorded Episode. The sampled frames stay
for inspection. Do not try to make one array serve both; they have different
rates, different consumers and different lifetimes.

Acceptance: `cargo test -p sailgym-env decision_log` — replaying the logged
decisions reproduces the episode bit-for-bit; decision indices are exactly those k with `0 <= k < executed_steps` and `k % period_steps == 0` (zero decisions for zero steps); the log survives a JSON round-trip.

### 6.5 — `VecEnv`

**Owns:** `crates/sailgym-env/src/vec_env.rs`
**P-group: B**

```rust
fn step_all(&mut self, actions: &[f64], obs_out: &mut [f32],
            rewards: &mut [f64], terminated: &mut [u8], truncated: &mut [u8]);
// Also expose final observations and validity masks for the chosen autoreset mode.
```

Flat caller-provided buffers, no per-env allocation — the pattern
`sample_wind_grid` already proves. Batch shapes and strides are validated against the runtime observation layout. F8.3 is the physical state order, not the configurable observation order. rayon across envs (F16.5), explicit per-episode autoreset, separate termination/truncation masks.

`obs_out: &mut [f32]` does **not** violate F9.5, which forbids f32
*intermediates in physics*. This is an output buffer, exactly like the wind
grid's, and the doc comment says so at the source so nobody 'fixes' it.

One piece of per-env state that a port drops and no short test notices: F6.10
requires `capsized` to be set after `|φ| > φ_capsize` holds **continuously** for
`t_capsize`. That accumulator must be carried through the vectorised step, not
recomputed from the current state.

Acceptance: `cargo test -p sailgym-env vec_env` —

- **Serial and parallel agree bit-for-bit**, `to_bits()`, at N ∈ {1, 8, 64, 512}
  and every thread count. This is section 02 task 2.7's assertion re-run on the
  real runner, and it is what turns F16.5's argument into a fact.
- `VecEnv` with N = 1 equals a single `Episode`, bit-for-bit.
- The capsize accumulator survives a vectorised step: an env that would capsize
  at step k does so at step k under `step_all`, not earlier and not never.
- Buffer length mismatches are rejected, not truncated.

### 6.6 — Throughput, re-measured on the real runner

**Owns:** `crates/sailgym-bench/src/bin/env_bench.rs`, `docs/v2/throughput.md`
**P-group: C**

Section 02 measured bare `Simulation`s and could not see the cost of
observation, cadence or guidance. This re-measures on `VecEnv`, appending to the
same generated document, so §0's question is answered against the thing that
will actually run.

Acceptance: `docs/v2/throughput.md` gains an env section with the host block,
aggregate steps/s and agent-decisions/s at the largest N, and the ratio to the
bare-`Simulation` figure from section 02. The handoff states in one sentence
whether the observation pipeline changed the verdict.

### 6.7 — Evaluation report

**Owns:** `crates/sailgym-env/src/evaluate.rs`
**P-group: C**

Finish rate, completion time, capsizes, missed marks, failed manoeuvres and
realised control effort, on held-out wind seeds and held-out courses. Reward
alone will not tell you a policy learned to cut a mark — and section 04's
passage rule is what makes "cut a mark" detectable at all.

Acceptance: `cargo test -p sailgym-env evaluate` — every metric computed over a
scripted episode with a hand-checked expected value.

### 6.8 — The gate

**Owns:** `scripts/check.sh`, `scripts/check.ps1`, `CLAUDE.md`,
`docs/v2/00-foundations.md`, `docs/v2/README.md`
**P-group: S**

D2. Add sailgym-env to the existing step-3 crate list, retaining physics, task, course and agent coverage. Step count unchanged.

## Section acceptance criteria

1. `scripts/check.sh` green, eleven steps.
2. Serial and parallel `step_all` agree bit-for-bit at every N and thread count.
3. The deliberate one-step autoreset shift was introduced, observed red, and
   reverted; the discrepancy magnitude is in the handoff.
4. F9.7 holds with an agent attached, in `sailgym-env`, over six chunkings.
5. `cargo tree -p sailgym-physics` mentions no v2 crate; `rayon` appears in
   `sailgym-env` and `sailgym-bench` and nowhere else — asserted by grep over
   every `Cargo.toml`.
6. `git diff --name-only crates/sailgym-physics/` is empty.
7. `docs/v2/throughput.md` carries both the bare-`Simulation` and the `VecEnv`
   figures, and the handoff states the §0 verdict.
8. `docs/v2/progress/06-handoff.md` per F13.6, naming the chosen autoreset
   convention, the Gymnasium version that determined it, and the schema-version
   migration.

## Risks

| # | Risk | Mitigation | Fires when |
|---|---|---|---|
| **RV33** | The autoreset off-by-one ships, and every bootstrapped value is shifted by one step for months. | 6.3's numeric test, plus the deliberate-shift demonstration that proves the test can see it. | the returns test is removed, or its bound is widened |
| **RV34** | Truncation and termination are conflated, biasing value bootstrapping. | `Outcome` is an enum (6.1); Gymnasium's `TimeLimit` is not used (F17.4). | a `bool done` appears in any signature |
| **RV35** | rayon changes a bit, and the parallel path is not the environment the serial path describes. | 6.5's `to_bits()` assertion at four N and every thread count — the argument is not trusted, the test is. | the bit-identity test is relaxed to a tolerance |
| **RV36** | The capsize accumulator is recomputed rather than carried, so capsize never fires under `step_all`. | F6.10; 6.5's dedicated assertion, which is the only test that would notice. | an env capsizes at a different step under `step_all` |
| **RV37** | `sailgym-env` is built single-boat and a fleet has to work around it. | one Episode per VecEnv slot; 6.5 asserts N = 1 equals a single `Episode`. | batching changes single-episode semantics |
| **RV38** | Independent episode resets accidentally share a clock or seed. | 6.2 gives every episode its own state; reset one slot and assert others are unchanged. | resetting one slot changes another |
| **RV39** | The decision log and the sampled frames are merged "to simplify", and the action sequence stops being complete. | 6.4 states they are separate and why. | `frames` gains an action column |
| **RV40** | `rayon` reaches `sailgym-physics` through a transitive dependency. | Criterion 5's grep over every `Cargo.toml`, plus `cargo tree`. | the grep finds it |

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| No reward function ships; it is configured per experiment | by design — a reward is an experiment parameter | never |
| No WASM fleet export; `Sim` is still single-boat | scope; F8.2 would need amending and no task owns `sailgym-wasm` | the fleet/ghost-race section |
| No wind shadow, so N boats are exactly independent | `brief.md` §3 defers it; the `WindField`-decorator seam is recorded | only with a new scope ruling |
| `env_bench` measures `manual` and a stub agent, not a real controller | scope — no controller exists yet | the rule-sailor section |
| Held-out courses in 6.7 are hand-written, not generated | scope | whenever a course generator exists |
