# v2 Section 06 — Handoff (`sailgym-env`: episodes, autoreset, decision log, `VecEnv`)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F16.5, F17.4 and
the new **F19**, F12′, F14.10 and F15.5, and `docs/v2/prds/06-env.md` before
starting anything that touches episodes, returns, batching or the Python
binding.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. **No file in `crates/sailgym-physics/` changed** —
`git diff --name-only crates/sailgym-physics/` is empty, which is the
section's acceptance criterion 6 — so F3, F4, F5, F6, F7, F8 and F9 are
exactly as section 05 left them: `STATE_LEN` is still 13, the F8.3 snapshot
layout is unchanged, `parameters.rs` is byte identical, `scenarios/` is
untouched and F8.2's WASM surface is unchanged. **No episode can be started
from the browser**, and that is a tracked debt, not an omission.

Status: **complete**; `scripts/check.sh` is green end to end (§6). No task
was deferred.

---

## 1. The implementation decision this section needed, and where it is recorded

The PRD opens **BLOCKED** on `docs/v2/brief.md` S4 having a recorded
implementation decision, answering `docs/v2/README.md` V-A. `brief.md` §5
says a changed normative contract needs the selected S rows, the exact F
deltas, the decision source and date, and the required validation recorded
"in the section handoff or this table". This is that record, in the same
place and the same shape sections 02, 03, 04 and 05 used.

| item | resolution |
|---|---|
| **decision source** | the human's dispatch of `docs/v2/prds/06-env.md` as the section PRD, 2026-09-22 — the same act `docs/v2/README.md` records as having resolved 02's and 03's deltas on 2026-09-21 and 04's and 05's on 2026-09-22 |
| **S rows selected** | **S4**, the **environment half**. The Python binding half of S4 is section 07's and nothing here touches it. S5 (multiple non-interacting boats) is **not** selected: `VecEnv` batches independent episodes, which S5's own row says does not require live fleets |
| **D1 — F16.5 and F17.4** | implemented. F16.5's argument is now a fact (§4.3); F17.4's autoreset convention is pinned, recorded and asserted numerically (§3). Where the implementation departed is recorded in the new **F19**, not left to be inferred |
| **D2 — step 3 gains `-p sailgym-env`** | implemented: `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent -p sailgym-env`, five sites moved together and **verified** (§6.1), step count unchanged so `[ValidateRange(1, 11)]` did not move |
| **D3 — `brief.md` S4** | the environment, in; the Python binding, the reward function and the training loop, out. This table is the record, since no section-06 task owns `docs/v2/brief.md` |
| **validation performed** | 46 tests in the new crate (43 unit + 3 integration), the two autoreset conventions agreeing **bit for bit** on discounted returns at two discount factors over two families of episodes, serial and parallel agreeing bit for bit at N ∈ {1, 8, 64, 512} × six thread counts, the F9.7 identity with an agent attached over six chunkings, a fixed seed reproduced across two processes, four demonstrations that the named guards can go red, and six full runs of the eleven-step gate |

**No signature is claimed for anything beyond that.** This section makes no
physical claim of any kind: it adds a runner, and a runner is not validation.

---

## 2. What landed, file by file

### Task 6.1 — contracts: the crate, `Outcome`, the research envelope (P-group S)

- **`crates/sailgym-env/Cargo.toml`** — new crate. Dependencies:
  `sailgym-physics`, `sailgym-task`, `sailgym-course`, `sailgym-agent`,
  `serde`, `serde_json` and **`rayon`**, which F16.5 permits here and in
  `sailgym-bench` and nowhere else. `sailgym-physics` with
  `features = ["testkit"]` as a **dev**-dependency only.
- **`src/lib.rs`** — the crate document: the four determinism traps stated
  once, what is here, and what is deliberately not.
- **`src/outcome.rs`** — `Outcome`, `TerminationReason`, `Bounds`,
  `AutoresetMode`, `Reward`, `RewardContext`, `ZeroReward`. 5 tests.
- **`src/recording.rs`** — `Decision`, `RewardIdentity`, `ResearchIdentity`,
  `ResearchEnvelope`, `EnvelopeError`, and the three-valued field rule
  applied from outside the crate that owns it. 6 tests.
- **`Cargo.toml`** (workspace) — `crates/sailgym-env` in `members`,
  `sailgym-env` in `workspace.dependencies`. **`Cargo.lock`** — exactly one
  new entry, the `sailgym-env` package itself (`git diff --stat Cargo.lock`
  reads `1 file changed, 14 insertions(+)`). **No new third-party crate
  entered the tree**: `rayon` was already in the lock for `sailgym-bench`.

### Task 6.2 — the episode runner (P-group A)

- **`src/episode.rs`** — `EpisodeConfig`, `EnvError`, `Source`,
  `StepResult`, `FinishedEpisode`, `Episode`, `make_sensor`,
  `obs_layout_of`, `observe_into`, `probe_route`, `manual_source`.
  10 tests, including the F9.7 identity over six chunkings, the cadence
  sweep, the two-process reproduction, one assertion per `Outcome` reason,
  the suite-equality guard, the F7-literal audit and the wall-clock grep.

### Task 6.3 — the autoreset convention (P-group A)

- **`src/autoreset.rs`** — `StepRecord`, `AutoresetError`,
  `split_episodes`, `discounted_return`, `discounted_returns`. 5 tests.
- **`tests/returns.rs`** — the numeric test that is the point of it, at two
  discount factors over two families of episodes, plus the permanent
  wrong-convention demonstration and the hand-checked discount arithmetic.
  3 tests.

### Task 6.4 — decision logging (P-group B)

- **`src/decision_log.rs`** — `DecisionLog`, `DecisionLogError`,
  `expected_count`, `validate`, `replay`. 5 tests, including the bit-exact
  replay, the index contract in both directions and the RV39 grep.

### Task 6.5 — `VecEnv` (P-group B)

- **`src/vec_env.rs`** — `VecEnv`, `VecError`, `step_all`, `observe_all`,
  `reset_all`, `reset_one`, `final_obs`, `final_obs_valid`, `set_parallel`.
  9 tests, including the bit-identity sweep, `N = 1` against a single
  `Episode`, the capsize accumulator, buffer refusal, RV38, the convention's
  effect on the final observation, and section acceptance 5's two greps.

### Task 6.6 — throughput, re-measured on the real runner (P-group C)

- **`crates/sailgym-bench/src/bin/env_bench.rs`** — the sweep, the
  bit-identity assertion, the two-cadence decomposition and the document
  writer.
- **`docs/v2/throughput.md`** — one **delimited** appended section.
- **`crates/sailgym-bench/Cargo.toml`** — one dependency line. **An
  `Owns:`-list gap**; see §11.2.

### Task 6.7 — the evaluation report (P-group C)

- **`src/evaluate.rs`** — `EvalSuite`, `EpisodeSummary`,
  `EvaluationReport`, `run_one`, `evaluate`. 3 tests.

### Task 6.8 — the gate (P-group S)

- **`scripts/check.sh`** (the `step_names` array, the `run_step` case **and**
  the comment that says why step 3 grows), **`scripts/check.ps1`** (`$Steps`
  and the same comment), **`CLAUDE.md`** (the step-3 row and the layout
  block), **`docs/v2/README.md`** (the delivery table, the status paragraph,
  the gate paragraph, V-E, the 07 row and the generated-outputs paragraph),
  **`docs/v2/00-foundations.md`** (the delta table, F12′ in four places, a
  pointer in F16.5, a pointer in F17.4 and the new **F19**).
- **`docs/v2/progress/06-handoff.md`** — this file.

---

## 3. The autoreset convention: what was checked, and what could not be

The PRD is explicit: *"Check the convention of the exact version pinned in
`uv.lock`. Do not assume it, and do not take it from this document."*

**There is no Gymnasium in `uv.lock`.** Section 03's Python workspace pins
`jax`, `numpy`, `pytest` and `ruff` and nothing else, because the binding
that needs Gymnasium is section 07's and this section ships no Python. So
the instruction could not be followed as written, and what was done instead
is recorded here rather than glossed.

| item | value |
|---|---|
| pinned in `uv.lock` | **nothing** — `grep gymnasium uv.lock` is empty, and `uv run python -c "import gymnasium"` is a `ModuleNotFoundError` |
| what was read | the **source** of the current release, `gymnasium 1.3.0`, fetched as an sdist from PyPI and read; **not installed, and not added to the lock** (no section-06 task owns `pyproject.toml` or `uv.lock`) |
| the enum and its values | `gymnasium/vector/vector_env.py:32-38` — `NEXT_STEP = "NextStep"`, `SAME_STEP = "SameStep"`, `DISABLED = "Disabled"` |
| the default | `gymnasium/vector/sync_vector_env.py:68` — `autoreset_mode: str \| AutoresetMode = AutoresetMode.NEXT_STEP` |
| the semantics | `gymnasium/vector/sync_vector_env.py:252-295`. `NEXT_STEP`: on the call after a boundary the sub-env is reset, the action is **ignored**, `reward = 0.0` and both flags are cleared. `SAME_STEP`: the boundary call returns the **reset** observation and puts the final one in `infos["final_obs"]` |
| where a consumer reads it | `metadata["autoreset_mode"]`, `vector_env.py:59` |
| the documented reference | <https://farama.org/Vector-Autoreset-Mode>, cited by `sync_vector_env.py:78` |

**What was implemented.** Both live conventions, named with Gymnasium's own
spellings so a log says which produced it in the vocabulary the other side
of the binding already uses, plus `Disabled`. `AutoresetMode::NextStep` is
the default here because it is Gymnasium's, and an environment that
disagreed with the default of the thing that will wrap it is a bug waiting
for section 07.

**What section 07 must do.** Pin Gymnasium, read
`metadata["autoreset_mode"]` from the version it pinned, and **select** the
matching `AutoresetMode` rather than inherit this one. That is a
configuration, not a rewrite, precisely because both are implemented and
`tests/returns.rs` proves they agree. The `docs/v2/README.md` row for 07 now
says so, and F19.1 records it.

### 3.1 The numeric test, and the number it produced

`crates/sailgym-env/tests/returns.rs` runs one fixed action sequence, from
one fixed seed, under both conventions, over **two families** of episodes —
one that always terminates (`Terminated(OutOfBounds)`, a box the boat always
leaves) and one that always truncates (a step budget that always runs out) —
splits the two flat streams with `autoreset::split_episodes` and compares
the discounted returns at **γ = 0.9 and γ = 0.99**.

```
returns [terminating] γ=0.9 : [1.2086063854505928, 1.1914937657707414,
                               1.1773512210636043, 1.1821946762786917]
returns [terminating] γ=0.99: [3.3645888346737864, 3.3676656388905086,
                               3.4237450822302984, 3.40567045409945]
returns [terminating]: max |Δ| between the two conventions = 0.000e0 (0 ULP)
returns [truncating]  γ=0.9 : [1.164814315770287,  1.1478922275606454,
                               1.135764195391569,  1.1398729731928323]
returns [truncating]  γ=0.99: [3.118617927346027,  3.0552359457697102,
                               2.990848742025989,  3.012967033378338]
returns [truncating]:  max |Δ| between the two conventions = 0.000e0 (0 ULP)
```

**The stated bound is zero ULP**, asserted with `to_bits()`, and that is not
luck. The two conventions differ in exactly one thing — whether the stream
carries a neutral record between two episodes — and the episodes themselves
are the same episodes, because the chain shares a root seed, the same
`STREAM_SCENARIO` draws and a script indexed by each episode's **own**
decision counter rather than by a global call counter. A global index would
have fed the two conventions different actions, because `NextStep` spends
one call per boundary on a reset, and the test would have failed for a
reason that has nothing to do with the convention. Anything looser than bit
identity would have hidden which of those four sentences was false.

The test also asserts, of the produced streams and not only of the split,
that `NextStep` carries exactly one extra record per boundary it steps past
and that **every one of those records is neutral** — `split_episodes`
refuses a reset record with a reward or a flag, so that assertion is the
`.expect` on the split.

### 3.2 The bound is tight enough to see a one-step shift — measured twice

**Permanently, in the suite.** `wrong_convention_moves_the_returns` reads a
`NextStep` stream as though it were a `SameStep` one, which is exactly the
off-by-one RV33 names: every episode after the first acquires the previous
boundary's neutral record at its front and slides one discount power later.

```
returns γ=0.9 : a one-step shift moves a return by up to 0.119149
returns γ=0.99: a one-step shift moves a return by up to 0.034237
```

Against a bound of zero, both are enormous. The test asserts `> 1e-3`, so it
would still notice if the reward scale shrank by two orders of magnitude.

**Deliberately, in the runner, twice — and reverted both times.**

*Shift A, staged.* `Episode::step` was changed so that under `NextStep` the
terminating call reports `reward = 0.0` and the **reset** call reports the
terminal reward instead: the reward arrives one call late.

```
---- the_two_autoreset_conventions_give_the_same_discounted_returns ----
every reset record is neutral: ResetRecordNotNeutral { index: 71,
  record: StepRecord { reward: -4.907044573600335,
                       terminated: false, truncated: false } }
---- wrong_convention_moves_the_returns ----
clean: ResetRecordNotNeutral { index: 71, … }
```

Two of the three tests went red, at the neutrality guard, naming the record
and its reward. Reverted.

*Shift B, and it was not staged first — it shipped in the first draft and
the test found it.* `Episode::step` returned `self.reward_acc` after the
`SameStep` branch had already called `begin`, which clears it. Every
terminating episode's last reward therefore read `0` under `SameStep` and
its true value under `NextStep`:

```
terminating: episode 0, step 70: -4.907044573600335 vs 0
terminating: episode 1, step 71: -4.967576606144427 vs 0
terminating: episode 2, step 77: -4.963908999735626 vs 0
terminating: episode 3, step 75: -4.980900249353896 vs 0
```

Re-staged afterwards to measure what it does to the returns rather than only
to detect it:

| γ | correct | with the defect | Δ |
|---|---|---|---|
| 0.9 | 1.2086063854505928 | 1.2116810352969682 | 3.07e-3 |
| 0.99 | 3.3645888346737864 | 5.792784194072645 | **2.434** |

At γ = 0.99 that is a **72 % change in the return of every episode**, in the
direction that makes a policy look better than it is — and at γ = 0.9 it is
3 parts in 1000, which is exactly the "merely mediocre rather than broken"
the PRD warns about. The fix is one line and a comment saying why, in
`episode.rs`. Reverted after measurement; the suite is green afterwards
(§4.1, §6).

---

## 4. The measured results

### 4.1 The crate

```
cargo test -p sailgym-env
  43 unit tests  (outcome 5, recording 6, episode 10, autoreset 5,
                  decision_log 5, vec_env 9, evaluate 3)
   3 integration tests  (tests/returns.rs)
```

### 4.2 The three properties the acceptance names directly

| property | measurement | result |
|---|---|---|
| F9.7 with an agent attached, in `sailgym-env` | `advance(3000)` against 3000 × `advance(1)` on `close_hauled`, a stub policy at cadence 10, over the **same six chunkings section 05 used** — {1, 3, 7, 10, 13, 200} — plus `manual` every step on `free_sail` over the same six | **bit identical** in all 13 state fields and in the decision log, in all twelve comparisons |
| cadence lands on episode steps | 1000 steps of `tack` at periods 7 and 10, over the same six chunkings | decisions at exactly `{0, 10, …, 990}` and `{0, 7, …, 994}`, in **all twelve** runs |
| a fixed seed reproduces a trajectory across two processes | the test binary re-executes **itself** with a marker environment variable and compares 13 state fields plus the whole decision log, as hex bit patterns, over 2000 steps of `gybe` chunked by 37 | identical |

### 4.3 Serial and parallel, bit for bit — F16.5 turned into a fact

```
vec_env: serial == parallel at N ∈ {1, 8, 64, 512} × [1, 2, 4, 8, 16, 24] threads
```

Twenty-four comparisons, each of three things: every slot's 13 state fields,
the whole `f32` observation buffer and the whole reward buffer, all with
`to_bits()`. The scenario is `gybe`, the one shipped scenario with a `Gust`
field, so a per-slot seed genuinely changes the trajectory — and the test
asserts that slot 0 and slot 1 **differ**, because agreement between two
identical boats would prove nothing.

`env_bench` makes the same assertion again at every N and every thread count
in the release build, before it prints a figure.

### 4.4 The capsize accumulator (RV36)

A single `beam_reach_capsize` episode, stepped one at a time, capsizes at a
definite step; the same configuration in a four-slot `VecEnv` driven by
`step_all` capsizes **at the same step**, with `terminated = 1` and
`truncated = 0`. The accumulator is F6.10's, read from each episode's own
`Simulation` and never recomputed; the test is the only one that would
notice if it were.

### 4.5 The decision log (task 6.4)

| property | measurement |
|---|---|
| replay reproduces the episode | a 1234-step `gybe` episode flown by a jittering policy at cadence 10, replayed through `manual` from its own log: all 13 state fields **bit identical**, and the replay's own log equal to the original's |
| the agreement is not vacuous | one logged action changed by hand moves the trajectory |
| the index contract | at periods 1, 7, 10 and 100 the logged steps are **exactly** `{k : 0 ≤ k < executed_steps, k % period == 0}`, and a fresh episode with zero steps has **zero** decisions |
| the contract refuses each way of breaking it | off-cadence, past-the-end, out-of-order, a gap, a wrong width, an out-of-range scalar and a zero cadence each produce their own named error, and a log that fails the contract is refused **before** anything is simulated |
| JSON round trip | every scalar `to_bits()`-identical, and the decoded log still replays |
| RV39 | a grep asserts `EpisodeFrame` mentions neither `action` nor `decision` |

### 4.6 The research envelope (task 6.1)

| property | measurement |
|---|---|
| round trip | a real schema-2 recording plus agent, action, observation, autoreset, route, bounds, budget and reward metadata, written and read back equal, with the nested document going through `Episode::to_json` / `from_json` |
| a legacy recording stays legacy | a hand-built schema-1 document wrapped by `viewing` comes back **schema 1**, frame for frame, with `diag: None` throughout |
| it cannot acquire agent metadata | all nine research fields are `Unknown` after `viewing`, asserted one by one |
| incompatible identities refuse resimulation | a different agent is `Different(["research.agent"])`; a different scenario is `Different(["recording.scenario"])` — section 10's half — and a legacy recording is `Indeterminate`, not `Different`. "Comparable but no log" is a **different** error from "not the same experiment" |
| the three-valued rule | all nine cases of section 10's `Recorded` rule pinned, applied from outside the crate that owns it |

### 4.7 The evaluation report (task 6.7)

The arithmetic is checked against hand-written summaries — finish rate 2/5,
mean finish time 25.0 s over the two that finished, one capsize, one mark
missed, one truncation, mean rudder 2.0 rad, mean sheet 2.0 m — and the
*measurement* against F7 rather than against a previous run of this code:
hold `rudder_rate_cmd = +1` and `sheet_rate_cmd = 0` for 40 steps and F4.3
integrates `δ̇r` at exactly `delta_r_rate_max`, so the realised rudder travel
is `40 · dt · rate = 0.418 rad` (measured to within 1e-9) and the realised
sheet travel is **exactly 0.0** — a number with no rounding in it, which is
the one worth asserting exactly. The mirror case, hauling with no rudder
command, moves the sheet and not the rudder, and never faster than
`sheet_haul_rate`.

The suite test sweeps three seeds against two courses, one on the boat's own
track and one 400 m off it, and the report separates the two axes: six
episodes, some finished and some truncated, with `route_index` recorded per
episode.

---

## 5. Proven able to fail — four demonstrations, and their revert

Each was applied, measured and reverted; the suite is green afterwards
(§4.1, §6). Two of them are in §3, because they are about the returns.

### 5.1 The autoreset reward carried one call late (RV33) — §3.2, shift A

`ResetRecordNotNeutral { index: 71, record: { reward: -4.907044573600335 } }`,
in two of the three returns tests. Reverted.

### 5.2 The terminal reward read after the `SameStep` reset (RV33) — §3.2, shift B

Found live, in the first draft, by the test that exists for it. Measured
again afterwards: **2.434** on a return of 3.365 at γ = 0.99, a 72 % error in
the direction that flatters a policy. Fixed, not reverted — the fix is the
one-line `let reward = self.reward_acc;` before the autoreset branch, with a
comment saying why.

### 5.3 Cadence keyed off a per-`advance` counter (RV26)

`Episode::advance` was changed to test `self.spec.cadence.decides_at(taken)`,
where `taken` counts iterations **within one `advance` call**.

```
test advance_n_equals_n_advance_1_with_an_agent_attached ... FAILED
  assertion `left == right` failed:  left: 3000  right: 300
test cadence_lands_on_episode_steps_whatever_the_chunking ... FAILED
test the_decision_indices_are_exactly_the_cadence_multiples_below_the_end ... FAILED
```

Three tests in two modules, including task 6.4's index contract, which is
the one that would notice if the F9.7 test were ever weakened. Reverted.

### 5.4 The capsize accumulator recomputed rather than carried (RV36)

`evaluate_outcome` was changed to test `|φ| > φ_capsize` directly instead of
reading `Simulation::capsize().capsized`.

```
test the_capsize_accumulator_survives_a_vectorised_step ... FAILED
  capsize fired on the very step the angle was exceeded (1775): the
  accumulator was recomputed rather than carried (RV36)
test outcome_transitions_are_asserted_one_per_reason ... FAILED
  assertion failed: ep.simulation().capsize().since > 0.0
```

**This demonstration changed the test, not only the code.** As first
written, `the_capsize_accumulator_survives_a_vectorised_step` compared the
batched path against a single episode — and a recomputation is wrong in
*both*, so the comparison stayed green and only the transition test went
red. The PRD calls this test "the only test that would notice", so it was
strengthened until it was: it now records the step at which `|φ|` first
exceeded `φ_capsize` and asserts that capsize fired **exactly** `t_capsize /
dt = 200` steps later — 1775 → 1975 — as well as at the same step under
`step_all`. With the defect in place it fires at 1775 and the test says so
by name. Reverted.

---

## 6. The gate

Six full runs: a baseline and one after each of the five groups. Step 9, the
browser suite, dominates every one of them.

| run | tree | wall | result |
|---|---|---|---|
| baseline | `beef35d`, clean, before any edit | 758 s | all eleven ok |
| after group S (6.1) | the crate, `outcome.rs`, `recording.rs`, the workspace | 757 s | all eleven ok |
| after group A (6.2, 6.3) | + `episode.rs`, `autoreset.rs`, `tests/returns.rs` | 757 s | all eleven ok |
| after group B (6.4, 6.5) | + `decision_log.rs`, `vec_env.rs` | 758 s | all eleven ok |
| after group C (6.6, 6.7) | + `evaluate.rs`, `env_bench.rs`, the throughput section | 755 s | all eleven ok |
| after group S (6.8) | + the gate edits, step 3 now five crates | 774 s | all eleven ok |

One further run is not in the table because it is not a result: the final
run was started, then **aborted and restarted** after a late doc-comment
edit to `episode.rs` and a one-sentence clarification in F19.4, so that
every row above measures the tree it names rather than a tree that changed
under it.

**The baseline matters and it was measured**, not assumed: the PRD says to
compare no-change guards against this section's starting revision, and at
`beef35d` with a clean tree the whole chain was already green. No
pre-existing failure was found.

**`cargo test -p sailgym-env` was run explicitly after every group**, because
until task 6.8 the gate did not run it: step 3 gained `-p sailgym-env` in the
last group, by design. One consequence is recorded rather than glossed: at
the end of group B the crate's own suite had exactly **one** red test,
`the_throughput_document_still_carries_the_env_section`, because the document
it guards is written by task 6.6 in group C. It is green from group C onward,
and it was never red at a point where the gate ran it.

**RV52 did not fire.** Section 05's handoff §7 warns that any section editing
`crates/sailgym-physics/src` must commit the edit and force `build.rs` to
re-run before the gate can be green. This section edits nothing under that
directory, so `SAILGYM_SOURCE_STATE` stayed clean, `ExperimentIdentity::compare`
kept returning `SameConditions` and section 10's browser comparison specs
passed in every run. **The repair section 05 proposed —
`cargo:rerun-if-changed=../../.git/HEAD` and `../../.git/index` in
`crates/sailgym-physics/build.rs` — is still unmade**, and still belongs to
whoever owns that crate next.

### 6.1 The five sites, moved together — and verified

v2 F12′ requires every site that spells the chain out to move together "or
the gate lies about itself". All five moved, and the result was **verified
rather than asserted**, as sections 03 and 05 did: both scripts were parsed
and their step-name lists diffed.

```
check.sh : 11 steps
check.ps1: 11 steps
 1 == cargo fmt --check
 2 == cargo clippy --all-targets -- -D warnings
 3 == cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent -p sailgym-env
 4 == cargo test -p sailgym-physics --test invariants --test no_shortcuts …
 5 == cargo test -p sailgym-physics --test regression
 6 == wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
 7 == pnpm --dir web typecheck
 8 == pnpm --dir web test:unit
 9 == pnpm --dir web test:e2e
10 == uv run ruff check python && uv run ruff format --check python
11 == scripts/py-test.sh
[ValidateRange(1, 11)]
both scripts' step-3 actions agree with their names
```

The `$E2E` splice in `check.ps1` is resolved to its non-`-Fast` form, which
is what the comparison should use.

| site | what changed |
|---|---|
| `scripts/check.sh` | the `step_names` array, the `run_step` case **and** the comment that says why step 3 grows |
| `scripts/check.ps1` | the `$Steps` entry and the same comment |
| `CLAUDE.md` | the step-3 table row, and `crates/sailgym-env/` in the layout block |
| `docs/v2/README.md` | the delivery table (06 and 07), the status paragraph, the gate paragraph, V-E and the generated-outputs paragraph |
| `docs/v2/00-foundations.md` | the delta table, F12′ in four places, a pointer in F16.5, a pointer in F17.4 and the new **F19** |

`[ValidateRange(1, 11)]` is **deliberately unchanged**: the step count did
not move, so RV17 does not arise.

Step 3 went from 66 s to **81 s** with `-p sailgym-env` in it. Most of that
is one test: `serial_and_parallel_agree_bit_for_bit` builds and steps
2 × (1 + 8 + 64 + 512) episodes across seven schedules in a debug build, and
it is the assertion section acceptance 2 names. The whole chain went from
758 s to 774 s, which is 2 %.

### 6.2 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` here, as in sections 01, 02, 04, 05, 08, 09,
10 and 11. `check.ps1` **was** edited, so the edit is **unverified as an
execution** — but it is not unverified as text: §6.1 parses both scripts and
diffs their step lists, and additionally checks that step 3's *action* in
`check.sh` matches its own *name*, which is the pair that could silently
disagree. Recorded rather than glossed.

---

## 7. Throughput, re-measured — and the §0 verdict in one sentence

**The observation pipeline did not change the verdict.** A `VecEnv` step
costs **1.44×** what a bare `Simulation` step costs, so the environment runs
at 0.70× the bare rate per core and still reaches **4.094 M steps/s** —
**20 472× real time**, at **409 k agent-decisions/s** — on 24 threads at
N = 4096, against section 02's 5.251 M steps/s for bare `Simulation`s on the
same machine.

`docs/v2/throughput.md` now carries both halves: section 02's tables above
the marker and section 06's below it, delimited so `env_bench` can replace
its own section in place. **`vec_bench --write` rewrites the whole document
from its own template and drops the appended section**; that is the same
hazard section 03 recorded for `conformance.md`, handled the same way —
`sailgym-env`'s `the_throughput_document_still_carries_the_env_section`
fails in gate step 3 when it has gone missing, and names the command that
puts it back.

**Where the 1.44× is, measured rather than argued.** The sweep runs a second
serial arm at cadence 100 as well as the cadence-10 one, and the two rates
solve for a per-step cost and a per-decision cost:

| quantity | measured |
|---|---|
| a bare `Simulation` step | 1750 ns |
| a `VecEnv` physics step | 2319 ns |
| one decision (18-column observation, agent, funnel) | 1947 ns |
| the decision's share of the 569 ns difference, at cadence 10 | **25 %** |

So three-quarters of the overhead is **not** the observation: it is that
`Outcome` is evaluated after **every** physics step, so the episode loop
calls `Simulation::advance(1)` rather than `advance(period)` — and `advance`
refreshes its cached force breakdown once per call, which costs one extra
force evaluation per step on top of RK2's two. That is a deliberate trade
and it buys three things a batched loop cannot have: a capsize resolved on
the step it happens, a mark passage tested at the physics rate rather than
at the sample rate (section 04's own warning), and a budget that truncates
on the step it names.

**If that ever needs repaying**, the cheapest honest repair is in the
physics crate and not here: a `Simulation::advance` that refreshes its cache
lazily, or an `advance_observed(n, &mut impl FnMut(&BoatState))` that yields
each published state without the refresh. Both are edits to a crate no
section-06 task owns, so they are **reported, not made** (F13.2). Nothing in
this section worked around it.

The figures exclude a controller, the recorder and the decision log, all of
which were switched off for the measurement and are named in the document as
excluded.

---

## 8. No physics changed, no coefficient was tuned, and no reward ships

```
$ git diff --name-only crates/sailgym-physics/
(empty)
$ git status --porcelain crates/sailgym-physics/ crates/sailgym-task/ \
      crates/sailgym-course/ crates/sailgym-agent/ crates/sailgym-wasm/ scenarios/
(empty)
```

That is section acceptance criterion 6, in its strongest form: **not one
file**, tracked or untracked. `parameters.rs` is byte identical and
`--test provenance` compares the same F7 rows on every gate run.

`crates/sailgym-env/` contains **no equation of motion, no coefficient and
no frame conversion**, and `no_f7_literal_appears_in_the_env_crate` asserts
on every gate run that no F7 value has been copied into it — the same audit
`sailgym-agent` runs, pointed at this crate. It fired once during
development, on `100.0` in a percentage in a log line, because `a_y` is
100 kg in the F7 catalogue; the log line was changed to print a fraction
rather than the audit being weakened. That choice is recorded in
`evaluate.rs` at the source.

**The numbers this crate does contain, and why none of them is a
coefficient:**

| number | where | what it is |
|---|---|---|
| `Cadence::EVERY_STEP`, `period_steps` | configuration | F14.6's decision rate; not physical (F14.9), declared by the agent and recorded in the header |
| `Bounds::Rect` corners | configuration | a sailing area, supplied by the caller. **The default is `Unbounded`** so the crate invents no distance |
| `max_steps` | configuration | a step budget. **The default is `None`** |
| `ZeroReward` | the only reward that ships | zero, everywhere |
| `PERIOD_STEPS = 10`, `SPARSE_PERIOD_STEPS = 100`, `POPULATIONS`, `TOTAL_STEPS` in `env_bench` | a benchmark's sweep | measurement settings, in `sailgym-bench`, reaching no force |
| thresholds in the tests | fixtures | every one derived from F7 at run time (`delta_r_rate_max`, `phi_capsize`, `t_capsize`, `sheet_haul_rate`) rather than copied |

**No reward function ships.** The PRD is explicit that a reward is an
experiment parameter and not an environment constant, so `sailgym-env`
provides the *shape* — called once per completed physics step, summed over a
decision period, pure in its context, identified by name and version in the
research envelope — and exactly one implementation, `ZeroReward`. The
reward used by `tests/returns.rs` is defined **in that test**, which is
where an experiment's reward belongs.

**No autopilot, no gain, no noise model.** `manual` and a test stub drive
every episode measured here; the rule sailor is still a later section, and
`CourseParams::DEFAULT_LOOKAHEAD` is **still untuned** for the third section
running, for the same reason section 04 and section 05 gave — tuning it
needs a controller to tune it against, and this section ships none.

The user's dispatch adds that a **visual** constant is governed by the same
discipline as a physical one. This section introduces no visual constant at
all: it has no UI.

---

## 9. Section acceptance criteria, one by one

| # | criterion | result |
|---|---|---|
| 1 | `scripts/check.sh` green, eleven steps | **pass** — §6; six runs, the last on the final tree |
| 2 | serial and parallel `step_all` agree bit for bit at every N and thread count | **pass** — §4.3; N ∈ {1, 8, 64, 512} × [1, 2, 4, 8, 16, 24], states **and** the observation buffer **and** the reward buffer, `to_bits()`; re-asserted by `env_bench` in release at N ∈ {1, 8, 64, 512, 4096} |
| 3 | the deliberate one-step autoreset shift was introduced, observed red, and reverted; the discrepancy magnitude is in the handoff | **pass** — §3.2, twice, with the magnitudes: 2.434 on a return of 3.365 at γ = 0.99 (72 %), 3.07e-3 at γ = 0.9, against a stated bound of **0 ULP** |
| 4 | F9.7 holds with an agent attached, in `sailgym-env`, over six chunkings | **pass** — §4.2; {1, 3, 7, 10, 13, 200}, for a stub policy and for `manual`, bit identical in all 13 state fields and in the decision log, demonstrated able to fail (§5.3) |
| 5 | `cargo tree -p sailgym-physics` mentions no v2 crate; `rayon` appears in `sailgym-env` and `sailgym-bench` and nowhere else, asserted by grep over every `Cargo.toml` | **pass** — `physics_depends_on_no_v2_crate` greps the tree for `sailgym-env`, `-agent`, `-course`, `-task`, `rayon` and `wasm-bindgen` and finds none; `rayon_appears_in_exactly_two_manifests` reads all eight manifests and asserts the declaring set is exactly `{sailgym-bench, sailgym-env}`. Both run in gate step 3 |
| 6 | `git diff --name-only crates/sailgym-physics/` is empty | **pass** — §8, and no untracked file there either |
| 7 | `docs/v2/throughput.md` carries both the bare-`Simulation` and the `VecEnv` figures, and the handoff states the §0 verdict | **pass** — §7; the verdict is the first sentence of it |
| 8 | `docs/v2/progress/06-handoff.md` per F13.6, naming the chosen autoreset convention, the Gymnasium version that determined it, and the schema-version migration | this file; §3 names the convention (`NextStep`, Gymnasium's default) and the version (**1.3.0, read as source, not pinned and not installed**), and §10 names the migration (**there is none**: `EPISODE_SCHEMA_VERSION` is untouched and the envelope is versioned independently) |

Task-level acceptance:

| task | criterion | result |
|---|---|---|
| 6.1 | nested recording and research metadata round-trip; known legacy recordings remain inspectable and cannot acquire fabricated agent metadata; incompatible identities refuse action-resimulation comparisons | pass — §4.6, three tests plus the nine-case field rule |
| 6.2 | `cargo test -p sailgym-env episode` — F9.7 over section 05's six chunkings; a fixed seed reproduced across two processes; `Outcome` transitions asserted one per reason | pass — §4.2 and the five-reason test: `Truncated` at the step the budget names, `Capsized` from the accumulator, `OutOfBounds` from a box the track is known to leave, `Finished` on a lookahead point at t > 1 s, and `MarkMissed` for **both** sided roundings, with the `Either` control that shows the detector reads the side clause and not only the plane |
| 6.3 | `cargo test -p sailgym-env --test returns`; the deliberate shift recorded with its discrepancy; the Gymnasium version and its documented convention named with a URL or a version string | pass — §3, §3.1, §3.2. The version could not be *pinned*, and that is recorded as the finding it is |
| 6.4 | `cargo test -p sailgym-env decision_log` — replay reproduces the episode bit for bit; the indices are exactly the cadence multiples below `executed_steps`, with zero decisions for zero steps; the log survives a JSON round trip | pass — §4.5, at four cadences, with each contract violation separately refused |
| 6.5 | `cargo test -p sailgym-env vec_env` — serial == parallel; `N = 1` equals a single `Episode`; the capsize accumulator survives; buffer mismatches rejected | pass — §4.3, §4.4, and `buffer_length_mismatches_are_rejected_not_truncated`, which also asserts that a **refused** call stepped nothing |
| 6.6 | `docs/v2/throughput.md` gains an env section with the host block, aggregate steps/s and agent-decisions/s at the largest N, and the ratio to the bare-`Simulation` figure; the handoff states the verdict | pass — §7 |
| 6.7 | `cargo test -p sailgym-env evaluate` — every metric computed over a scripted episode with a hand-checked expected value | pass — §4.7 |
| 6.8 | step 3 gains `-p sailgym-env`; step count unchanged | pass — §6.1 |

---

## 10. Risks

**RV33 — the autoreset off-by-one ships. Did not fire, and it very nearly
did.** §3.2's shift B was in the first draft of `Episode::step` and the
returns test found it before anything was committed, which is the strongest
evidence available that the test is load-bearing. The bound is zero ULP and
has nothing to loosen.

**RV34 — truncation and termination conflated. Did not fire.** `Outcome` is
an enum, `step_all` reports two masks and never their union, and
`no_done_flag_in_this_crate` greps the crate's own sources for the shapes
that would reintroduce it, with the scanner shown able to find one.

**RV35 — rayon changes a bit. Did not fire.** §4.3, twenty-four comparisons
in the test and thirty more in `env_bench`, all `to_bits()`.

**RV36 — the capsize accumulator recomputed. Did not fire**, and §5.4 is
both the proof and the admission that the test needed strengthening before
it was the proof.

**RV37 — `sailgym-env` built single-boat. Did not fire.** `VecEnv` is a
`Vec<Episode>` and `one_env_equals_a_single_episode` asserts N = 1 is
bit-identical to a single `Episode`, in the state and in the reward.

**RV38 — independent resets share a clock or a seed. Did not fire.**
`resetting_one_slot_leaves_the_others_untouched` resets slot 2 after three
batched steps and asserts slots 0, 1 and 3 are bit identical and still at
step 30. The autoreset seed chain is drawn from `STREAM_SCENARIO` and not
from `seed + index`, and `one_seed_reproduces_the_episode_and_the_chain_after_it`
asserts that two adjacent root seeds produce **non-overlapping** chains —
which a counter would not.

**RV39 — the decision log and the sampled frames merged. Did not fire**, and
a grep asserts `EpisodeFrame` mentions neither `action` nor `decision`.

**RV40 — `rayon` reaches `sailgym-physics` transitively. Did not fire**;
acceptance 5's two greps run in the gate.

**RV52 (from section 04 and 05) — did not fire**, because nothing under
`crates/sailgym-physics/src` changed. §6.

**R3 (v1) — sign-convention drift.** This section introduces no sign of its
own: every geometric decision is `sailgym-course`'s, reached through
`passage::passed_between` and `guidance`, including the missed-mark probe,
which exists precisely so that no second copy of F15.3's rule is written
here.

**R7 (v1) — build-sensitive goldens.** Not in play: this section compares
nothing against a committed golden. `sailgym-agent`'s golden comparison
still runs in step 3.

---

## 11. What deviated from the PRD

### 11.1 `src/lib.rs` takes one line per later module, and task 6.1 owns it

Rust has no way for task 6.2 to declare its own module without editing the
crate root, which is task 6.1's file. `lib.rs` therefore gained
`pub mod autoreset; pub mod episode;` during group A,
`pub mod decision_log; pub mod vec_env;` during group B and
`pub mod evaluate;` during group C. The file carries a comment saying so.
This is the same `Owns:`-list gap sections 04, 05, 10 and 11 recorded; it is
now the eighth section to hit one, and the PRD template is still where it
should be fixed.

### 11.2 `crates/sailgym-bench/Cargo.toml` is not in any `Owns:` list

Task 6.6 owns `crates/sailgym-bench/src/bin/env_bench.rs` and
`docs/v2/throughput.md` — but not the manifest of the crate that has to
build the binary. One line was added, `sailgym-env = { workspace = true }`,
with a comment naming this paragraph. There is no way to deliver task 6.6
without it.

### 11.3 `docs/v2/progress/06-handoff.md` appears in no `Owns:` list

F13.6 requires it and task 6.8 is the task that would own it. The same gap
sections 04 and 05 recorded.

### 11.4 `AutoresetMode` lives in `outcome.rs`, not `autoreset.rs`

The PRD's prose says the convention is "implemented in Rust, once, in
`episode.rs`"; task 6.3's `Owns:` names `autoreset.rs`. The `Owns:` list
wins (F13.2), so the *machinery* — what each convention does to a stream,
and the returns — is `autoreset.rs`'s, and `episode.rs` applies it. The
*enum* had to be visible to task 6.1's `recording.rs`, which is written in
group S before `autoreset.rs` exists, so it lives in `outcome.rs` beside
`Outcome` and `TerminationReason`: one module for the episode's boundary
vocabulary. The PRD's own requirement — "record it in the episode header as
a named enum value" — is what put it there.

### 11.5 `Decision` lives in `recording.rs`, `DecisionLog` in `decision_log.rs`

The same ordering constraint. A `Decision` is a *record* and records are
task 6.1's; the recorder, the index contract and the replay are task 6.4's.
The split is stated in both files.

### 11.6 `Sensor` is not `Send`, so the suite is built twice — and the two are asserted equal

The largest deviation, and the one with a repair attached. F16.5's parallel
path requires `Episode: Send`, so every value it owns must be `Send`.
`sailgym-agent`'s `Sensor` trait is **not** declared `Send`, and
`Box<dyn Sensor>` therefore is not — nor can it be coerced to
`Box<dyn Sensor + Send>` after the fact. No section-06 task owns
`crates/sailgym-agent` (F13.2).

The options were: `unsafe impl Send for Episode` (this repository contains
no `unsafe`, and the impl would silently cover any future non-`Send` field);
rebuild the sensors inside every parallel task (correct only while every
sensor is stateless, which the trait does not promise); or build a `Send`
suite here from the same five concrete types. The third was taken, and it
costs three repetitions: `make_sensor` against `SensorRegistry::tier0`,
`obs_layout_of` against `ObsLayout::of`, and `observe_into` against
`observation::observe`.

**None of the three is left to a comment.**
`the_env_suite_is_the_registrys_suite` builds the suite both ways and
asserts the registry's id list equals this crate's, the two `ObsLayout`s are
equal, the two `obs_digest`s are equal, and the two 18-column observation
vectors are equal **bit for bit** on a real state 137 steps into
`close_hauled` — with a guard that the vector is not all zeros, so agreeing
proves something. A sensor added to the registry and not here, or an
`observe` that changed, fails gate step 3.

**The repair is one word: `pub trait Sensor: Send` in
`crates/sailgym-agent/src/sensor/mod.rs`.** All five tier-0 sensors already
satisfy it. It is **reported, not made**, and the same will be true of
`Actuation` the day a second adapter arrives — this section holds the `rate`
adapter concretely (`Rate`, which is `Copy`) rather than behind
`Box<dyn Actuation>`, for exactly that reason.

### 11.7 A practice task is an observer and does not end an episode

The PRD says to reuse section 11's task outcomes rather than duplicate their
semantics. An `EpisodeConfig` may carry a `TaskSpec`; the `TaskRun` observes
every step and its verdict reaches `EpisodeSummary::task_outcome` and
`EvaluationReport::failed_manoeuvres`. It does **not** decide the episode
boundary, and `Outcome` has no variant for it. Two reasons: F17.4 makes the
episode boundary authoritative in one place, and section 11's own discipline
is that a task is an observer. A section that wants a task success to end an
episode should add a variant to `Outcome` deliberately, not discover that it
already did.

### 11.8 `Finished` means the route was completed, and only that

With no route there is no `Finished`: an episode ends by termination or by
the budget. That is a consequence of 11.7 and is stated in `outcome.rs`.

### 11.9 `VecEnv::new` takes the cadence as an argument

`EpisodeConfig` carries no cadence, because F14.6.3 makes the cadence the
**agent's** declaration, recorded in its `AgentSpec`. A `VecEnv` slot has no
policy to declare one — it is driven by `manual` — so the batch chooses it,
at the one place it builds its agents. Putting it in the configuration as
well would have created a second place for the number to disagree with
itself.

### 11.10 `Episode::step` refuses to start part-way through a decision period

Not in the PRD. `step()` advances exactly one cadence period and caches the
observation at the boundary it lands on; called from a non-boundary it would
cache at a non-decision step and the following decision would recompute,
which is how a noise model would quietly draw twice. Refusing is cheaper
than documenting.

### 11.11 No task was delegated

Group A holds two tasks, group B two and group C two. The section agent
executed them, as sections 01, 02, 04, 05, 08, 09, 10 and 11 each recorded.
It is now the ninth consecutive section to say this.

### 11.12 Group order was honoured, and the gate saw each group's own tree

S → gate → A → gate → B → gate → C → gate → S(6.8) → gate. Later groups'
sources were written before their groups began but were kept **outside the
repository** until their group, so each gate run measured a tree containing
that group and nothing later. Recorded because "the gate was green after
group A" means nothing if the group was not what the gate saw.

---

## 12. What the next section must know

1. **There is no schema migration, and that is deliberate.**
   `EPISODE_SCHEMA_VERSION` is still 2 and section 10's codec is untouched.
   `sailgym-env` wraps a recorded `Episode` in a `ResearchEnvelope` with its
   own `RESEARCH_ENVELOPE_VERSION = 1` and `RESEARCH_IDENTITY_VERSION = 1`,
   and writes and reads the nested document **through `Episode::to_json` /
   `from_json`**. A schema-1 file round-trips as a schema-1 file. If section
   07 needs a field in the envelope, bump `RESEARCH_ENVELOPE_VERSION`; if it
   needs one in the *recording*, that is section 10's crate and its rules.

2. **Pin Gymnasium and re-check the convention.** §3. Read
   `metadata["autoreset_mode"]` from the version section 07 pins and select
   the matching `AutoresetMode`; both live conventions are implemented and
   proved equal, so this is a configuration and not a rewrite. Do **not**
   inherit `NextStep` because this section defaulted to it.

3. **`obs_out: &mut [f32]` is the shape F17.5 asks for**, and `step_all`'s
   buffers are caller-owned, contiguous and validated against the **runtime**
   observation layout. What section 07 still has to do is release the GIL
   around `step_all` and avoid per-environment object churn; nothing here
   allocates per step except the decision log, which has an off switch
   (`EpisodeConfig::log_decisions`).

4. **`Sensor` needs `: Send`.** §11.6. One word in
   `crates/sailgym-agent/src/sensor/mod.rs`, after which `make_sensor`,
   `obs_layout_of` and `observe_into` in `episode.rs` can all be deleted in
   favour of `SensorRegistry::resolve`, `ObsLayout::of` and
   `observation::observe`. The guard test is what will tell you the deletion
   was safe. `Actuation` will want the same the day a second adapter lands.

5. **Three-quarters of the env's per-step overhead is the force-cache
   refresh, not the observation** (§7). The repair is in
   `crates/sailgym-physics/src/simulation.rs` — a lazy refresh, or an
   `advance` that yields each published state — and it is reported, not made.
   Measure before deciding it matters: 20 472× real time may be enough.

6. **RV52's repair is still unmade.** Section 05 §7 proposed
   `cargo:rerun-if-changed=../../.git/HEAD` and `../../.git/index` in
   `crates/sailgym-physics/build.rs`. This section did not need it and did
   not make it. The next section that edits `crates/sailgym-physics/src`
   will need it, or will have to commit and force a rebuild before the gate
   goes green.

7. **`vec_bench --write` drops section 06's part of `throughput.md`.** Run
   `cargo run --release -p sailgym-bench --bin env_bench -- --write
   docs/v2/throughput.md` afterwards. Gate step 3 fails until you do.

8. **There is no WASM surface and no reward.** `Sim` is still single-boat,
   F8.2 is unchanged, and no episode can be started from the browser. A
   reward is configured per experiment; `ZeroReward` is the only one that
   ships, and `tests/returns.rs` shows where an experiment's own belongs.

9. **`CourseParams::DEFAULT_LOOKAHEAD` is still untuned**, for the third
   section running. The first section with a controller owns it.

10. **The repository root `README.md` now lies about the gate in five
    ways.** Its nine-step table spells step 3 without `-p sailgym-task`
    (section 11's change), without `-p sailgym-course` (04's), without
    `-p sailgym-agent` (05's) and without `-p sailgym-env` (this one), step 4
    without `--test conformance` (02's), and the chain as nine steps with no
    10 or 11 (03's). No task in any of those sections owns that file
    (F13.2). The changes it needs:

    | line | should read |
    |---|---|
    | 194 | `\| 3 \| ` + "`cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent -p sailgym-env`" + ` \| The physics core, the practice evaluator, the course layer, the agent interface and the episode runner are correct **and build on the host with no WASM toolchain**. \|` |
    | 195 | step 4 with `--test conformance` |
    | the chain | eleven steps, with 10 and 11 |
    | the layout block | `crates/sailgym-task/`, `crates/sailgym-course/`, `crates/sailgym-agent/` and `crates/sailgym-env/` |

    `docs/v1/00-foundations.md` F12 is deliberately **not** edited: a v1
    clause is amended by a recorded v2 delta and never in place.

11. **Four `README.md` open items are still waiting for a human** — V-D,
    V-G, V-H, and the obstacle half of V-F. This section edited V-E to
    record what it did; it closed nothing, for the same reason sections 01,
    04, 05, 08, 09, 10 and 11 each left theirs.

---

## 13. Deliberate debts, as the PRD tracks them

| debt | state |
|---|---|
| No reward function ships; it is configured per experiment | as designed. `ZeroReward` is the only implementation; `tests/returns.rs` defines its own |
| No WASM fleet export; `Sim` is still single-boat | unchanged. F8.2 would need amending and no task owns `sailgym-wasm` |
| No wind shadow, so N boats are exactly independent | unchanged. `brief.md` §3 defers it; the `WindField`-decorator seam is recorded in `lib.rs` |
| `env_bench` measures `manual` and a stub, not a real controller | unchanged, and stated in the generated document |
| Held-out courses in 6.7 are hand-written, not generated | unchanged |
| **New:** the `Send` duplication in `episode.rs` | §11.6; repaid by one word in `sailgym-agent` |
| **New:** three-quarters of the per-step overhead is the physics force-cache refresh | §7; repaid in `sailgym-physics`, by whoever owns it, if measurement says it matters |
| **New:** the decision log allocates a `Vec<f64>` per decision only when read | the live store is flat (`Vec<u64>` + `Vec<f64>`), so a decision costs no allocation; `Episode::decisions()` builds the records on demand |

---

## 14. Commands

```
cargo test -p sailgym-env
cargo test -p sailgym-env episode
cargo test -p sailgym-env vec_env
cargo test -p sailgym-env decision_log
cargo test -p sailgym-env evaluate
cargo test -p sailgym-env --test returns -- --nocapture
cargo tree -p sailgym-physics --edges all
cargo run --release -p sailgym-bench --bin env_bench -- --write docs/v2/throughput.md
scripts/check.sh
scripts/check.sh 3
```

The throughput document is **generated**. Do not edit it by hand, and re-run
`env_bench` after any `vec_bench --write`.
