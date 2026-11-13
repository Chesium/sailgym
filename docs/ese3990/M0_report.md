# ESE3990 M0 report — benchmark freeze and branching audit

**Date:** 2026-11-13 · **Status:** implemented; contract **frozen** at sign-off (§6). Contract version 1.
Plan: [ESE3990_SailGym_Project_Plan.md](ESE3990_SailGym_Project_Plan.md) §6 M0. Code: [`experiments/ese3990/`](../../experiments/ese3990/README.md).

## 1. Inventory

| Item | Found |
|---|---|
| Earlier ESE3990 code (PPO, BPTT, pendulum) | None in the repository. Nothing from the October draft's milestones exists. |
| Nov 12 presentation feedback | Not yet received; expected **after Nov 19**. Fold any scope change into the first milestone after it arrives. |
| Python runtime | JAX 0.11.2, Gymnasium 1.3.0, `sailgym_core` working. Optax 0.2.9 and matplotlib 3.11.2 **added and locked** (`uv add`). |
| Python access before M0 | Observations and outcome only. No physical state, no RuleSailor, no branching. |

## 2. What landed

| Change | Files |
|---|---|
| `Simulation: Clone` (derive only; adds no behaviour) and a clone-determinism test | `crates/sailgym-physics/src/simulation.rs` |
| Read-only `CapsizeState::pending_since()` and its test | `crates/sailgym-physics/src/stability/capsize.rs` |
| `Episode::fork()` — a planning branch through the existing `step()` path | `crates/sailgym-env/src/episode.rs` |
| Six branch-versus-replay proofs | `crates/sailgym-env/tests/fork.rs` |
| `RawEnv.fork/observe_f64/research_state`, `Spec.bounds/route_json`, `route_waypoints`, `RuleSailor`, research-state names/units/version | `crates/sailgym-py/src/{env,spec,lib,research}.rs` |
| Task/condition configs, manifest generator, shared contract, baselines, evaluator, 20 tests | `experiments/ese3990/` |
| `/runs/` ignored | `.gitignore` |

No equation, coefficient, passage rule or termination rule was added or changed anywhere. No physical coefficient was touched.

## 3. Branching audit

`Episode` was inspected field by field. A fork copies everything the future depends on and drops only observers:

| `Episode` field | In a fork | Why |
|---|---|---|
| `sim` (state, params, controls, seeded wind, step count, cached forces, capsize accumulator incl. private `crossed_at`) | cloned | `Simulation` is now `Clone`; every field already was |
| `tracker`, `prev_pos`, `missed` | cloned | gate passage and the cut test read them |
| `step`, `outcome`, `seed`, `root_seed`, `index` | copied | cadence, budget and `Truncated` land where they would have |
| `rng`, `streams`, `scenario_rng` | cloned | observations identical to live |
| `config.reward` | `boxed_clone` | — |
| `sensors` | rebuilt by id | tier-0 sensors hold no per-call state (only mount config); test asserts identical observations |
| `adapter` (`Rate`) | copied | stateless unit struct |
| `source` | `manual` (live latch copied if manual) | branches are action-driven; no policy memory cloned |
| `recorder`, `task`, decision log | dropped | observers only; none feeds the state or the outcome |
| `config.autoreset` | forced `Disabled` | a branch never simulates through a reset |

**Evidence** (`cargo test -p sailgym-env --test fork`, all pass): each test compares the fork with deterministic reset/replay of the live prefix, bit-for-bit on state, observation, outcome, executed steps and capsize accumulator.

| Boundary | Test |
|---|---|
| Near-gate state ending in a finish (RuleSailor-driven prefix, fork 3 decisions before the gate) | `a_fork_near_the_gate_finishes_exactly_where_replay_does` |
| Fork of a policy-driven live episode | `a_fork_of_a_policy_driven_episode_is_manually_driven_and_exact` |
| Pending capsize crossing (public `since` still 0) → failure | `a_fork_during_a_pending_capsize_keeps_the_pending_timer` |
| Nearly exhausted, non-cadence-aligned budget (4-step terminal decision) | `a_fork_near_the_budget_truncates_on_the_named_step` |
| Candidate order; live episode untouched | `candidate_order_cannot_change_any_candidate_or_the_live_episode` |
| Fork while a `NextStep` reset is pending | `a_fork_of_an_episode_awaiting_a_next_step_reset_is_terminal` |

**Mutation check:** replacing the clone with a 13-scalar `Simulation::reset` (dropping the capsize timer) makes the pending-capsize test fail, so the suite does detect an incomplete branch. The same proof runs end to end through Python in `experiments/ese3990/tests/test_branch.py`.

**Cost** (this machine, 24 cores, release build, nominal R):

| Quantity | Measured |
|---|---|
| `fork()` | 4.5 µs |
| One branch decision (10 physics steps), driven from Python | 24.7 µs |
| Baseline planner work per live decision (K=64, I=3, H=40) | 7 680 branch decisions ≈ **190 ms from Python**; ≈ 130 ms at the native one-thread rate in `docs/v2/throughput.md` (0.578 M steps/s) |
| Real-time budget | 50 ms |

**Consequence:** exact branching is solved. The baseline planner will **not** meet 50 ms single-threaded. The remaining work is a native batch-rollout call that takes candidates as an array and steps them in Rust, which removes the Python overhead. It may also run candidates on several threads (`VecEnv` already uses rayon), since a ~4–9× speed-up would put it near the deadline. Otherwise the planner is labelled offline. This affects M3 cost only, not correctness.

## 4. Baseline outcome reports

Release forced off for every controller. R rows use the frozen **45 s** budget. RuleSailor's R results are identical under 90 s and 45 s. Raw reports: `runs/ese3990/m0/*.json` (regenerate with the evaluator).

| Task | Controller | Conditions | Finished | Failures | Finish time given success (s) |
|---|---|---|---|---|---|
| R | RuleSailor | nominal | 1/1 | — | 14.8 |
| R | RuleSailor | validation | 20/20 | — | 15.9 (14.7–16.6) |
| R | RuleSailor | test | 50/50 | — | 15.9 (14.6–17.1) |
| R | RuleSailor | shift | 20/20 | — | 17.1 (13.9–20.2) |
| R | smooth random | nominal × 50 seeds | **6/50** | 43 truncated, 1 mark missed | 39.7 (30.5–43.8) |
| R | smooth random | validation | 3/20 | 17 truncated | 38.3 (28.4–44.3) |
| R (90 s, superseded) | smooth random | nominal × 50 / validation | 27/50, 5/20 | mark missed, truncated | 61.9, 49.0 |
| U | RuleSailor | nominal | 1/1 | — | 62.6 |
| U | RuleSailor | validation | 13/20 | 5 mark missed, 2 truncated | 57.2 (53.1–59.9) |
| U | RuleSailor | test | 38/50 | 8 mark missed, 4 truncated | 71.5 (51.7–179.1) |
| U | RuleSailor | shift | 2/20 | 8 mark missed, 10 truncated | 105.6 |
| U | smooth random | nominal × 50 / validation | 0/50, 0/20 | truncated, a few out of bounds | — |

RuleSailor never requested release on any of these runs, so forcing release off did not change its behaviour here. Peak heel stayed at or below 36°.

Throughput of the explicit-reset Python collector: **≈ 29 500 real decisions/s** on one environment. A 500k-decision run spends about 17 s stepping the environment, so training cost will be dominated by network updates.

## 5. Frozen contract

As in `experiments/ese3990/configs/tasks.json` and plan §3:

* R: `(0,0) → (30,0)`, half-width 5 m, **45 s = 9 000 steps** (plan said 90 s; see §6); U: `(0,0) → (0,40)`, 180 s = 36 000 steps; bounds x∈[−80,120], y∈[−80,100]; cadence 10 (20 Hz).
* Reward and masks exactly as plan §3.4. The potential uses the distance to the **gate centre**.
* Conditions: validation 20 / test 50 inside the training range (4.5–5.5 m/s, ±5° bearing, heading 90±15°). Shift 20: speed 4 or 6 m/s (alternating), ±5° bearing, heading offset **15–30° in magnitude**. Both speed and heading are outside the training range. Training conditions come from a seeded sampler that rejects held-out documents.
* Actor features: 17 named columns (`sailing.FEATURE_NAMES`).

## 6. Decisions at sign-off (2026-11-13)

| Question | Decision | Evidence |
|---|---|---|
| R budget | **45 s** (was 90 s) | Smooth-random finished R in 54 % of nominal seeds and 25 % of validation conditions at 90 s, against 12 % and 15 % at 45 s. RuleSailor's worst case is 20.2 s (shift). |
| Shift set | Heading offset magnitude **15–30°**, speed 4 / 6 m/s, bearing ±5° | Speed and heading both lie outside the training range. |
| Shaping target | **Gate centre** | Smooth and identical for R and U. |
| Nov 12 feedback | Expected after Nov 19 | — |

Still open:

* **Core-crate edits.** The four Rust files in §2 are outside any v2 PRD's `Owns:` list; the plan names them as expected changes.
* **Gate step 3 on a dirty tree.** `sailgym-env`'s `recording::tests::incompatible_identities_refuse_an_action_resimulation` fails while `crates/sailgym-physics/src` has uncommitted edits. `build.rs` marks the model identity dirty, and that test does not pin a clean source, unlike the physics-crate tests. It is expected to pass once the edits are committed. Steps 1, 2, 4, 5, 6, 10 and 11 pass; the web steps 7–9 were not run.
* **U's reference is weak off-nominal.** This is information only: U is gated, and RuleSailor finishes nominal U as M0 requires.
* **Planner cost.** The next MPC task is a native batch-rollout call (§3). Correctness is already established.
