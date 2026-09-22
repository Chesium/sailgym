# v2 Section 07 — Handoff (`sailgym-py`: the Gymnasium binding)

**Written per F13.6.** Read this, `docs/v1/00-foundations.md` in full,
`docs/v2/00-foundations.md` F12′, F14, F16.4, F16.5, **F17** and F19, and
`docs/v2/prds/07-py.md` before touching the Python side, the spaces, the
seeding rule or the autoreset convention.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. **No file in `crates/sailgym-physics/` or
`crates/sailgym-wasm/` changed** — `git diff --name-only
crates/sailgym-physics/ crates/sailgym-wasm/` is empty, which is the
section's acceptance criterion 6 — so F3, F4, F5, F6, F7, F8 and F9 are
exactly as section 06 left them: `STATE_LEN` is still 13, the F8.3 snapshot
layout is unchanged, `parameters.rs` is byte identical, `scenarios/` is
untouched and F8.2's WASM surface is unchanged. **No episode can be started
from the browser**, and that is still a tracked debt, not an omission.

Status: **complete**; `scripts/check.sh` is green end to end at eleven steps
(§6). No task was deferred.

---

## 1. The implementation decision this section needed, and where it is recorded

The PRD opens **BLOCKED** on `docs/v2/brief.md` S4 having a recorded
implementation decision, answering `docs/v2/README.md` V-A. `brief.md` §5
says a changed normative contract needs the selected S rows, the exact F
deltas, the decision source and date, and the required validation recorded
"in the section handoff or this table". This is that record, in the same
place and the same shape sections 02–06 used.

| item | resolution |
|---|---|
| **decision source** | the human's dispatch of `docs/v2/prds/07-py.md` as the section PRD, **2026-09-22** — the same act `docs/v2/README.md` records as having resolved 02's and 03's deltas on 2026-09-21 and 04's, 05's and 06's on 2026-09-22 |
| **S rows selected** | **S4, the Python half.** The environment half was section 06's. S5 (multiple non-interacting boats) is **not** selected and neither is PettingZoo: `VectorEnv` batches independent episodes, which S5's own row says does not require live fleets |
| **D1 — F17.2–F17.5** | implemented. F17.2 is a fact about `crates/sailgym-py/Cargo.toml` and is greped (§4.1); F17.3 is §4.3; F17.4 is §4.4 and §5.2; F17.5 is §4.6, measured three ways |
| **D2 — `brief.md` S4** | the Python binding, in; training, a policy, an algorithm, a reward and PettingZoo, out. This table is the record, since no section-07 task owns `docs/v2/brief.md` |
| **D3 — no F12 change** | **none was requested and none was made.** `scripts/py-test.{sh,ps1}` gained `uv sync --frozen` and `maturin develop --release` **inside the script**; the chain is still eleven steps, `[ValidateRange(1, 11)]` did not move, and `scripts/check.sh` and `scripts/check.ps1` are **byte identical to their section-06 state** — `git diff scripts/check.sh scripts/check.ps1` is empty. That is F12′'s "step 11 delegates to a script" clause doing exactly the job it was written for |
| **validation performed** | **40 new Python tests** — 10 spaces, 11 env, 10 autoreset (8 functions, two of them parametrised), 7 performance, and 2 more in section 03's audit — on top of that audit's own 29, for **69**; the returns identity through the binding at **0 ULP** over two families of four episodes each at two discount factors; the single-env and `VectorEnv(N=1)` paths compared step for step in all four returned quantities under **both** conventions; N = 8 identical seeds giving eight identical rows and eight different seeds giving different ones; the serial and parallel arms compared through `numpy` buffers; the GIL release measured at a stated bound and **demonstrated able to fail**; five deliberate defects in all introduced, observed red and reverted; `cargo clippy --all-targets -- -D warnings` run with **no Python interpreter on `PATH`**; and five full runs of the eleven-step gate |

**No signature is claimed for anything beyond that.** This section makes no
physical claim of any kind: it adds a binding, and a binding is not
validation.

---

## 2. What landed, file by file

### Task 7.1 — the crate and the build (P-group S)

- **`crates/sailgym-py/Cargo.toml`** — new crate. `[lib] name =
  "sailgym_core"`, `crate-type = ["cdylib"]` and nothing else: no `rlib`,
  because nothing in the workspace depends on it, and no `#[cfg(test)]`
  module, because a test binary would have to link an interpreter that gate
  step 2 has just proved is not required. Dependencies: `sailgym-physics`,
  `sailgym-course`, `sailgym-agent`, `sailgym-env`, `serde_json`,
  **`pyo3 0.29` with `abi3-py312` and `extension-module`**, and
  **`numpy 0.29`**. `sailgym-wasm` is deliberately absent (F17.2, RV47).
- **`crates/sailgym-py/src/lib.rs`** — the crate document and the
  `#[pymodule]`. Three `mod` lines and five free functions.
- **`crates/sailgym-py/pyproject.toml`** — the `maturin` project.
  `name = "sailgym-core"`, `module-name = "sailgym_core"`,
  `dynamic = ["version"]` so the version comes from `Cargo.toml`, which
  takes it from the cargo workspace.
- **`Cargo.toml`** (workspace) — `crates/sailgym-py` in `members`.
  **`Cargo.lock`** — 18 new entries: `sailgym-py` itself plus pyo3's and
  rust-numpy's transitive set (`heck`, `matrixmultiply`, `ndarray`,
  `num-complex`, `num-integer`, `numpy`, `portable-atomic`,
  `portable-atomic-util`, `pyo3`, `pyo3-build-config`, `pyo3-ffi`,
  `pyo3-macros`, `pyo3-macros-backend`, `rawpointer`, `rustc-hash`, `syn`,
  `target-lexicon`). **None of them reaches `sailgym-physics`**: `cargo tree
  -p sailgym-physics --edges all` mentions no v2 crate, no `pyo3` and no
  `numpy`, and section 06's `physics_depends_on_no_v2_crate` still runs in
  gate step 3.
- **`pyproject.toml`** (root) — now a **`uv` workspace** with one member,
  `crates/sailgym-py`. `gymnasium==1.3.0` and `sailgym-core` joined the
  dependencies, `maturin>=1.9,<2` the dev group, `python/sailgym` the
  hatchling package list, and `sailgym`/`sailgym_core` the ruff
  first-party list. **`uv.lock`** — regenerated and committed: `cloudpickle
  3.1.2`, `farama-notifications 0.0.6`, **`gymnasium 1.3.0`**,
  `maturin 1.15.0`, `sailgym-core` and `typing-extensions 4.16.0`.
- **`python/sailgym/__init__.py`** — an `Owns:`-list gap; see §9.1.

### Task 7.2 — spaces from the sensor layout (P-group A)

- **`crates/sailgym-py/src/spec.rs`** — `Spec`, and the five module
  functions (`autoreset_modes`, `default_autoreset_mode`, `tier0_sensors`,
  `shipped_scenarios`, `termination_reasons`). Everything Python needs to
  build a space, and nothing Python could compute for itself.
- **`python/sailgym/spaces.py`** — `observation_space`, `action_space`,
  `ObservationContract`, `IncompatibleObservation`,
  `check_observation_compatible`.
- **`python/tests/test_spaces.py`** — 10 tests.
- **`python/tests/test_no_stray_constants.py`** — section 03's audit, with
  `python/sailgym` added as a **third** scope, and **two more tests**: the
  wrapper-scope pass and RV47's manifest grep. 3 tests became 5. See §4.1
  and §9.3.

### Task 7.3 — the single environment (P-group A)

- **`crates/sailgym-py/src/env.rs`** — `RawEnv`, and the `numpy`
  borrow helpers both modules share.
- **`python/sailgym/env.py`** — `SailgymEnv`, `as_u64`,
  `SINGLE_ENV_AUTORESET`.
- **`python/tests/test_env.py`** — 11 tests.

### Task 7.4 — the vector environment (P-group B)

- **`crates/sailgym-py/src/vector.rs`** — `RawVecEnv`, with the GIL
  released around `step_all`.
- **`python/sailgym/vector.py`** — `SailgymVectorEnv`,
  `pinned_autoreset_mode`, `gymnasium_autoreset_modes`.
- **`python/tests/test_autoreset.py`** — 8 test functions, 10 collected:
  the returns identity and the single-versus-vector agreement are each
  parametrised over the two live conventions.

### Task 7.5 — the performance assertions (P-group B)

- **`python/tests/test_performance.py`** — 7 tests, and the three
  non-negotiables with their bounds written down as documented constants.

### Task 7.6 — the script, and the docs (P-group S)

- **`scripts/py-test.sh`**, **`scripts/py-test.ps1`** — `uv sync --frozen`
  and `maturin develop --release` in front of `pytest`, with a `cargo`
  presence check beside the existing `uv` one.
- **`CLAUDE.md`** — the step-11 row, and `crates/sailgym-py/` and
  `python/sailgym/` in the layout block.
- **`docs/v2/README.md`** — the delivery table's 07 row, the status
  paragraph, the gate paragraph and V-E.
- **`docs/v2/progress/07-handoff.md`** — this file.

---

## 3. The autoreset convention, re-checked against the version this section pinned

F19.1 is explicit: *"Section 07 **must re-check them against whatever it
pins** and select the mode its version declares … Do **not** inherit
`NextStep` because this section defaulted to it."* Section 06 could not pin
Gymnasium — the binding that needs it is this one — so it read the release's
source and recorded what it found. This is the re-check.

| item | value |
|---|---|
| pinned in `uv.lock` | **`gymnasium 1.3.0`** — installed, imported, and exercised by 39 tests |
| the enum | `gymnasium.vector.AutoresetMode`, whose values are `NextStep`, `SameStep`, `Disabled` |
| how it was read | `[m.value for m in AutoresetMode]`, compared **sorted** against `sailgym_core.autoreset_modes()`; a mismatch raises `RuntimeError` rather than being translated |
| the declared default | `inspect.signature(gymnasium.vector.SyncVectorEnv.__init__).parameters["autoreset_mode"].default` → `AutoresetMode.NEXT_STEP` → `"NextStep"` |
| what the batch publishes | `metadata["autoreset_mode"]`, as `AutoresetMode(spec.autoreset_mode)` — Gymnasium's own enum value, not a string |
| the selection | `SailgymVectorEnv(...)` with no `autoreset` **selects** `pinned_autoreset_mode()`; a `Spec` that names another mode is honoured, and the test asserts both |

**Section 06's reading was right, and that is worth stating rather than
assuming.** The version it read as an sdist and did not install is the
version this section pinned, and the default it recorded from
`sync_vector_env.py:68` is the default `inspect` reports from the installed
package. Had the two differed, F19.1's instruction is to *select*, and the
code selects — `pinned_autoreset_mode()` is called, not `"NextStep"`.

One thing that is **not** inherited and is worth naming: `sailgym-env`'s own
`AutoresetMode::default()` is also `NextStep`, and the binding never reads
it. It is reported by `sailgym_core.default_autoreset_mode()` purely so the
test can print both and a reader can see that they agree by measurement
rather than by construction.

**A single `gymnasium.Env` defaults to `Disabled`**, which is neither of
those: Gymnasium's single-environment API has no autoreset and a terminated
env is reset by its caller. `SINGLE_ENV_AUTORESET` is checked against
`sailgym_core.autoreset_modes()` at import, so a rename on either side is an
`ImportError` with a message rather than a silent default.

---

## 4. The measured results

### 4.1 The boundary is a grep, not a promise

`python/tests/test_no_stray_constants.py` is section 03's audit with a third
scope and two more passes. Its own output, from the gate:

```
[audit] 8 files, 20 float literals, 14 inside documented named constants
[audit] 14 named constants cite a Rust source line
[audit] 6 files in wrapper scope declare 10 module-level names and not one number
[audit] 8 manifests: sailgym-py binds sailgym-env, names no sailgym-wasm, and nothing names it
[audit] 2 files under python/sailgym_jax/ carry none of ('pcg', 'seed', 'stratum', 'rng')
```

Three of the facts behind those five lines are new:

1. **`python/sailgym` is a third scope**, audited as **wrapper** scope
   alongside `sailgym_conformance`. `SCOPES` is now
   `WRAPPER_SCOPES + EXCEPTION_SCOPES`, and the citation pass — F17.1's
   "kernel constants cite the Rust source" — runs over the **exception**
   scope only, because that is the only scope the exception covers.
2. **A fifth pass: wrapper scope defines no number at all.** The literal scan
   permits a number on the right-hand side of a documented, cited
   module-level constant. That escape route is F17.1's *exception*, and the
   exception names one package; in wrapper scope there is nothing to except,
   because a `DT = 0.005` in `python/sailgym/` would be a second copy of `dt`
   however well it cited the first. §5.1's second demonstration is what that
   pass is for, and the first demonstration would have been caught without
   it.
3. **RV47's manifest grep.** `sailgym-py` declares `sailgym-env` and does not
   declare `sailgym-wasm`; and no other manifest in `crates/` names
   `sailgym-py`, so the binding is a leaf and no build acquires `pyo3` by
   accident.

The anti-vacuity floor moved with the scan: `MIN_FILES` went from 3 to 6 as
the file count went from 4 to 8, and each scope must now contribute at least
one file — the check that catches a deleted package the totals would hide.

### 4.2 The spaces are the layout Rust reports

Measured, on the tier-0 suite and the `rate` adapter:

| quantity | value | where it came from |
|---|---|---|
| `obs_len` | **18** | `ObsLayout::len()`, the ordered concatenation of five sensors' widths |
| `action_dim` | **3** | `Rate::DIM`, through `Episode::action_dim` |
| the action box | `[−1, 1]^3` | `Spec::action_low` / `action_high`, which are F14.5's bound stated in Rust |
| privileged columns | `[16]` | `guidance.leg_bearing_vs_wind`, F14.4's one derived column |
| bounded columns | 6 of 18 | the sensors' own `FieldSpec` bounds; the other 12 report `±inf`, substituted **in Rust** because an absent bound is a fact about the quantity |
| `dt` | 0.005 s | `BoatParameters::sim.dt` — a number `python/sailgym/` may not know |

The test that makes this more than Rust-against-Rust is
`test_a_configured_subset_of_sensors_shortens_the_layout`: a `Spec` built
with the first two sensor ids has a **shorter** layout whose names are a
prefix of the full one, and its space's shape follows. A length typed into
Python would be wrong there and only there.

`gymnasium.utils.env_checker.check_env` passes with **nothing skipped** — no
`skip_render_check`. It emits two warnings, that some observation bounds are
infinite; both are correct, and inventing a bound to quiet them would be
inventing a number.

### 4.3 Seeding: one `u64`, and `np_random` reaches nothing (F17.3, RV43)

| property | measurement |
|---|---|
| `reset(seed=k)` twice | the two observations are `np.testing.assert_array_equal`-identical |
| a different `k` | a different trajectory — asserted on `gybe`, the one shipped scenario whose wind field is drawn from the seed (`mode: gust`), so there is something for a seed to change |
| **1 000 draws from `np_random`** between `reset(seed=k)` and the first step | **400 observations bit identical**, compared with `tobytes()`; and the generator's `bit_generator.state` is asserted to have moved, so the draws really happened |
| a seed that is not a `u64` | `reset(seed=-1)` and `reset(seed=2**64)` raise `ValueError: … is not a u64`, **before** Gymnasium's own narrower check, so the message names the contract that was broken; `reset(seed=2**64 - 1)` is accepted |

The third row is the only test in the repository that would notice a second
generator leaking into the simulation. It compares bit patterns, so a
divergence of one ULP fails it.

### 4.4 Episode boundaries are Rust's (F17.4, F19.2, RV34, RV44)

* `terminated` and `truncated` arrive as two booleans out of `Outcome` and
  **their union is never computed** — not in `env.rs`, not in `vector.rs`,
  not in `env.py`, not in `vector.py`.
* `assert not (terminated and truncated)` holds on every step of both arms,
  and each flag is observed in the arm that produces it: a 15 m box gives
  `Terminated(OutOfBounds)`, a 2 000-step budget gives `Truncated` at
  **exactly** step 2 000, and `beam_reach_capsize` with the sheet hauled
  gives `Terminated(Capsized)`. Every reason is one of
  `sailgym_core.termination_reasons()`; Python neither invents nor renames
  one.
* **There is no `TimeLimit` anywhere under `python/`**, asserted by a grep
  over every `.py` file for the three ways the wrapper is actually used. The
  needles are assembled at run time so the test is subject to its own grep
  (§5.2).

### 4.5 The returns identity, through the binding — **0 ULP**

Two families, as section 06 used: one that always terminates (a 15 m box) and
one that always truncates (a 2 000-step budget), on `gybe`, from one seed,
with a script keyed off each episode's **own** decision counter. Four
complete episodes each.

```
returns [terminating] gamma=0.9 : [0.10089386407143498, 0.17829259241599096,
                                   0.39680772904946170, 0.35356167930067467]
returns [terminating] gamma=0.99: [8.091676430520007, 6.725887764720782,
                                   8.519973732857090, 7.801616976454261]
returns [terminating]: max |delta| between the two conventions = 0.0 (0 ULP)
returns [truncating]  gamma=0.9 : [0.10089386378597130, 0.17829259165100256,
                                   0.39680772887612653, 0.35356167887296090]
returns [truncating]  gamma=0.99: [8.087938177537119, 6.353778653697338,
                                   8.596201644231120, 7.655818005752095]
returns [truncating]:  max |delta| between the two conventions = 0.0 (0 ULP)
```

Compared with `float.hex()`, so the bound has nothing to loosen. Three things
make it an identity rather than a coincidence, and each is asserted:

1. **The four returns differ from each other.** `assert len(set(a)) == len(a)`
   — on a uniform-wind scenario every episode of a chain is the same episode
   and the identity would hold over four copies of one number. `gybe` is the
   only shipped scenario with a seeded field, which is why it is the one used.
2. **The split is one rule and it reads the data, not the mode.** A record
   that executed no physics is the neutral one `NextStep` inserts between two
   episodes; it belongs to neither. The assertion inside the splitter — that
   such a record carries no reward and no flag — is RV33's off-by-one stated
   directly.
3. **The reward is a function of the observation the period *ended* on.**
   That is the one quantity both conventions agree about: under `NextStep`
   the returned observation already is it, and under `SameStep` it arrives in
   `final_obs`. `RawEnv::step` reports `final_obs_valid` so Python can build
   it without knowing which convention it is under.

**The bound is tight enough to see a one-step shift**, measured rather than
asserted:

```
returns gamma=0.9 : a one-step shift moves a return by 0.039681
returns gamma=0.99: a one-step shift moves a return by 0.085200
```

Against a bound of zero, and the test requires `> 1e-3`, so it would still
notice if the reward scale shrank by an order of magnitude.

**The two paths agree step for step.** Under **both** conventions, 400 calls
of a single `SailgymEnv` and of a `SailgymVectorEnv(N=1)` produce identical
`(obs, reward, terminated, truncated)` — observations by `tobytes()`, rewards
by `float.hex()` — and the compared window is asserted to contain at least one
episode boundary, so the agreement about boundaries is not vacuous.

**N = 8 with identical seeds gives eight identical rows**, in all four
quantities, over 300 calls; and eight *different* seeds give different rows,
so the agreement is a property of the seeds and not of a buffer nobody wrote.
The serial and parallel arms are compared through the `numpy` buffers at
N = 8 and agree exactly — F16.5's Rust assertion re-made where a transposed
row or a shared buffer would show.

### 4.6 The three performance non-negotiables (F17.5)

Measured on this host (24 cores), in the release build `scripts/py-test.sh`
now produces:

| # | claim | measurement | bound |
|---|---|---|---|
| 1 | **zero-copy** | the observation array's `__array_interface__` data pointer is unchanged across **17** `step_all` calls; the buffer is filled with `NaN` beforehand and is finite afterwards, so Rust wrote into the caller's own memory; and it changes between calls, so the test would not pass on a no-op | exact |
| 2 | **GIL released** | a Python thread completes **0.90–1.13** of its solo work while Rust steps 4 096 environments — the best of three attempts, across several runs; individual attempts ranged 0.78–1.13 | `> 0.5` |
| 3 | **bounded memory** | 1 000 low-level `step_all` calls retain **2 Python blocks / 32 bytes** and peak at **120 bytes** of temporary allocation | `< 64` blocks, `< 4096` bytes |
| 3′ | the Gymnasium layer, **separately** | 1 000 `SailgymVectorEnv.step` calls retain **4 blocks / 336 bytes** and peak at **2 424 bytes** — the tuple and the `info` dict, which the standard requires | reported; retained held to `< 64` blocks |

A fourth number is reported and deliberately **not** pinned: with rayon using
all 24 cores the same Python thread completes **0.56–0.94** of its solo work,
at a wall-clock ratio of 0.87–1.01. That is a property of the host as much as
of the binding — every core is already busy — so only the progress figure is
held to a bound, and a loose one (`> 0.25`), because a held GIL would put it
at zero whatever the host.

**The wall-clock ratio is reported and not asserted, and §5.3 is why.** It is
the obvious way to measure overlap and it does not work.

---

## 5. Proven able to fail — five demonstrations, and their revert

Each was applied, measured and reverted; the suite is green afterwards (§6).

### 5.1 A catalogue value in wrapper scope (RV41) — twice

Section acceptance 2 requires the demonstration in `python/sailgym/`. It was
done **twice**, because the second is the one that justifies the new fifth
pass.

*A bare literal.* `SAIL_AREA = 7.06` — the F7 sail area — appended to
`python/sailgym/spaces.py`:

```
AssertionError: physical or contract literals loose in Python — move each one
into the bundle, give it a module-level named constant with a doc string
citing its Rust source line, or argue for it in EXEMPT:
  python/sailgym/spaces.py:192: 7.06   |SAIL_AREA = 7.06

AssertionError: a wrapper package defined a number of its own. F17.1 excepts
('python/sailgym_jax',) and nothing else: read it from Rust, or from the
identified bundle:
  python/sailgym/spaces.py: SAIL_AREA = [7.06]
```

*A documented, cited constant.* `DT = 0.005` with the doc string
`"s, the fixed physics timestep. parameters.rs:680"` — which is exactly what
section 03's rules permit in the exception scope:

```
AssertionError: a wrapper package defined a number of its own. F17.1 excepts
('python/sailgym_jax',) and nothing else: read it from Rust, or from the
identified bundle:
  python/sailgym/spaces.py: DT = [0.005]
```

Only the new pass caught the second one, and it is the more dangerous of the
two: a well-documented `dt` in a binding is a second copy of `dt` that a
reviewer would wave through. Both reverted, `diff` clean, audit green.

### 5.2 A `TimeLimit` wrapper (RV44)

`from gymnasium.wrappers import TimeLimit` appended to
`python/sailgym/vector.py`:

```
AssertionError: a TimeLimit wrapper under python/ would make the single-env
and vector paths disagree about episode boundaries (F17.4, RV44):
  sailgym/vector.py:294: from gymnasium.wrappers import TimeLimit
```

Reverted. The needles are assembled from two halves at run time
(`"Time" + "Limit"`) precisely so that the test file, which has to name the
wrapper in order to say it is absent, is scanned like every other file rather
than exempting itself.

### 5.3 The GIL held across `step_all` (RV42) — **and what it revealed**

`py.detach(move || v.step_all(…))` in `crates/sailgym-py/src/vector.rs`
replaced by a direct call. Rebuilt, re-measured:

```
[perf] GIL, serial arm, N=4096: batch 278.2 ms, python 278.2 ms, together 304.9 ms -> progress 0.033, ratio 0.548
[perf] GIL, serial arm, N=4096: batch 298.2 ms, python 298.2 ms, together 298.2 ms -> progress 0.073, ratio 0.500
[perf] GIL, serial arm, N=4096: batch 267.9 ms, python 267.9 ms, together 282.3 ms -> progress 0.034, ratio 0.527
[perf] GIL, serial arm: best progress 0.073 (bound 0.5), its ratio 0.500 (bound 0.9)

AssertionError: the Python thread completed only 0.073 of its solo work while
Rust stepped, in the best of 3 attempts; the GIL is being held across
step_all (RV42)
```

**Progress fell from 0.90–1.13 to 0.033–0.073**, which is the empty middle the
bound was drawn across: a factor of **13** between the two cases, with the
bound at 0.5 in the gap. Reverted.

**The finding.** The *wall-clock ratio* — the obvious overlap measurement,
and the one the PRD's phrase "a ratio against the serialised baseline" first
suggests — read **0.500 in the broken build and 0.540 in the correct one**.
It did not merely fail to notice; it scored the defect slightly *better*. The
reason is structural: the denominator contains a solo Python measurement
that, in the held-GIL case, never happens — the worker is blocked, contributes
no wall time, and the pair looks perfectly overlapped. So the ratio is now
**printed and not the load-bearing assertion**, the assertion is the Python
thread's *progress*, and `OVERLAP_BOUND`'s doc string carries this transcript
so nobody promotes it back.

### 5.4 Per-step allocation in the adapter (RV46)

One `dict(infos)` appended to a list per `SailgymVectorEnv.step`:

```
[perf] 1 000 Gymnasium VectorEnv steps: retained 38004 blocks / 2352600 bytes,
       peak temporary 2360720 bytes

AssertionError: 38004 blocks retained over 1 000 adapter steps: the tuples are
being kept, not merely created
```

38 004 blocks against a bound of 64 and a clean measurement of 4. Reverted.

### 5.5 A dependency on `sailgym-wasm` (RV47)

`sailgym-wasm = { path = "../sailgym-wasm" }` added to
`crates/sailgym-py/Cargo.toml`:

```
AssertionError: sailgym-py declares a dependency on sailgym-wasm. F17.2 keeps
the two boundaries apart deliberately (RV47)
```

Reverted. The grep reads the manifest's `[dependencies]` section rather than
the whole file, so the comment at the top of that manifest — which explains at
length *why* `sailgym-wasm` is absent, and therefore names it — does not trip
its own test.

---

## 6. The gate

Five full runs: a baseline and one after each of the four groups.

| run | tree | wall | result |
|---|---|---|---|
| baseline | `9c3ebc8`, clean, before any edit | 772 s | all eleven ok |
| after group S (7.1) | the crate, the `uv` workspace, `uv.lock`, `python/sailgym/__init__.py` | 772 s | all eleven ok |
| after group A (7.2, 7.3) | + `spec.rs`, `env.rs`, `spaces.py`, `env.py`, `test_spaces.py`, `test_env.py`, and the audit's third scope and wrapper-scope pass | 775 s | all eleven ok |
| after group B (7.4, 7.5) | + `vector.rs`, `vector.py`, `test_autoreset.py`, `test_performance.py` | 775 s | all eleven ok |
| after group S (7.6) | + `py-test.{sh,ps1}`, `CLAUDE.md`, `docs/v2/README.md`, and RV47's manifest grep (§9.3) | **778 s** | **all eleven ok, exit 0** |

**The baseline matters and it was measured**, not assumed: the PRD says to
compare no-change guards against this section's starting revision, and at
`9c3ebc8` with a clean tree the whole chain was already green in 772 s. No
pre-existing failure was found.

Two runs are not in the table because they are not results. The group-A run
was started, **aborted and restarted** after `skip_render_check=True` was
removed from `test_check_env_passes` (§4.2); and the final run was started,
aborted and restarted when RV47's manifest grep turned out never to have been
applied (§9.3) — so each row above measures the tree it names rather than a
tree that changed under it. §9.4 records the one place this discipline was not
kept, and one more is recorded here: the docstring of
`python/tests/test_no_stray_constants.py` was corrected while the final run
was in step 3, so steps 1–9 of that run saw the tree without the correction
and steps 10–11 saw it with. No step before 10 reads anything under
`python/`.

### 6.1 What the new work costs the chain

| step | before | after | why |
|---|---|---|---|
| 2 — `cargo clippy --all-targets` | 0 s | 0 s warm, 5 s cold | one more crate, and `pyo3`/`numpy` to check the first time |
| 3 — the five-crate test | 80–84 s | 80–84 s | **unchanged**: `sailgym-py` is not in step 3 and has no Rust tests. Its tests are the Python ones |
| 10 — `ruff` | 0 s | 0 s | six more files |
| 11 — `scripts/py-test.sh` | 4–5 s (29 tests) | **10–12 s** (69 tests) warm | `uv sync --frozen`, `maturin develop --release` against a warm `target/`, and 39 more tests of which `test_performance.py`'s GIL measurement alone is ~2 s |

The chain as a whole moved from **772 s to 778 s**, which is 0.8 %. Step 9,
the browser suite, still dominates every run at ~633 s.

**Step 11 from a pristine `target/`** — task 7.6's acceptance — was measured
in a copy of the tree with no `target/` and no `.venv`:

```
$ scripts/check.sh 11
[11/11] scripts/py-test.sh
[py-test] uv sync --frozen
[py-test] uv run maturin develop --release --manifest-path crates/sailgym-py/Cargo.toml
   Compiling sailgym-physics … sailgym-course … sailgym-task … sailgym-agent …
   Compiling sailgym-env … sailgym-py …
    Finished `release` profile [optimized] target(s) in 7.54s
[py-test] uv run --frozen pytest python
69 passed, 3 warnings in 9.73s
[11/11] ok in 18s
check: all steps passed in 18s
```

**18 s**, of which 7.5 s is `cargo`. The cargo registry and the `uv` cache
were warm; a network-cold figure is a property of the network and is not
claimed.

### 6.2 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` in this session. `scripts/check.ps1` is
**unchanged** — `git diff scripts/check.ps1` is empty, which is D3 — so there
is nothing in it to have broken. `scripts/py-test.ps1` **was** edited and is
therefore **unverified as an execution**; it is not unverified as text, since
it is the line-for-line mirror of `py-test.sh` and was diffed against it
clause by clause. Section 03 §6.3 unpacked PowerShell into a scratch
directory to verify its own edit; that was not repeated here, and it is
recorded rather than glossed.

---

## 7. No Rust outside the binding changed, and the clean checkout reproduces

```
$ git diff --name-only crates/sailgym-physics/ crates/sailgym-wasm/
(empty)
$ git status --porcelain crates/sailgym-physics/ crates/sailgym-wasm/ \
      crates/sailgym-agent/ crates/sailgym-course/ crates/sailgym-task/ \
      crates/sailgym-env/ crates/sailgym-bench/ scenarios/
(empty)
$ git diff --name-only scripts/check.sh scripts/check.ps1
(empty)
```

That is acceptance criterion 6 in its strongest form — **not one file**,
tracked or untracked, in any Rust crate but the new one — and criterion 7
beside it. `parameters.rs` is byte identical and `--test provenance` compares
the same F7 rows on every gate run.

**RV48, measured rather than argued.** A `PATH` was built from 2 291 symlinks
to `/usr/bin` with every `python*`, `pydoc*`, `uv` and `uvx` removed, and
gate step 2's command was run inside it:

```
$ env -i PATH=<no python anywhere> cargo clippy -p sailgym-py --all-targets -- -D warnings
python3: not found (exit 127)
uv: not found
    Checking sailgym-py v0.1.0 (/home/chesium/sailgym/crates/sailgym-py)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.08s
```

`abi3-py312` is what makes that work: without it `pyo3-build-config` asks an
interpreter for its configuration and fails when there is none. The PRD
offered "or record why not"; there is no why-not to record.

**`uv sync --frozen` from a clean checkout.** The tree was copied without
`.git`, `target/`, `.venv/`, `web/node_modules/` or any cache, and:

```
$ uv sync --frozen
 + gymnasium==1.3.0  + jax==0.11.2  + maturin==1.15.0  + numpy==2.5.3
 + pytest==9.1.1  + ruff==0.16.8  + sailgym==0.0.0  + sailgym-core==0.1.0  …
   20 packages in 7.75 s
$ scripts/check.sh 11
   … 69 passed … [11/11] ok in 18s
```

Two distributions come out of one tree, as §9.2 describes: `sailgym` (the
three importable Python packages) and `sailgym-core` (the compiled module).

---

## 8. Section acceptance criteria, one by one

| # | criterion | result |
|---|---|---|
| 1 | `scripts/check.sh` green, eleven steps, from a clean checkout after `uv sync --frozen` | **pass** — §6 and §7 |
| 2 | the stray-constants audit passes over `python/sailgym/`, and was demonstrated able to fail there | **pass** — §4.1 and §5.1, **twice**: a bare `7.06` and a documented, cited `DT = 0.005`. Both reverted |
| 3 | `check_env` passes; `terminated` and `truncated` are never both true | **pass** — §4.2 (with nothing skipped) and §4.4 |
| 4 | the returns identity holds through the binding, at the bound section 06 established | **pass** — §4.5; **0 ULP**, `float.hex()`, two families of four episodes at two discount factors, with the four returns asserted to differ from each other |
| 5 | all three performance assertions pass, with their measured values in the handoff | **pass** — §4.6, and §5.3 records that one of the two GIL metrics had to be demoted to a report because the demonstration showed it could not tell the two cases apart |
| 6 | `git diff --name-only crates/sailgym-physics/ crates/sailgym-wasm/` is empty | **pass** — §7 |
| 7 | no F12 change was made | **pass** — §1, D3. `git diff scripts/check.sh scripts/check.ps1` is empty |
| 8 | `docs/v2/progress/07-handoff.md` per F13.6, recording the measured numbers, the pinned Gymnasium version, and what a PettingZoo section would inherit | this file; §4 the numbers, §3 the version (**1.3.0**, pinned, installed and re-checked), §11.4 PettingZoo |

Task-level acceptance:

| task | criterion | result |
|---|---|---|
| 7.1 | `uv run maturin develop` succeeds; `uv run python -c "import sailgym"` succeeds; `scripts/check.sh 2` green; `cargo tree -p sailgym-physics` mentions no v2 crate | **pass**, all four. The abi3 requirement is not merely declared: §7 records `cargo clippy --all-targets -- -D warnings` completing with **no Python interpreter on `PATH`** |
| 7.2 | the space shape equals the Rust-reported layout length; the field-name list matches Rust's exactly, order included; incompatible observation metadata raises; the audit passes over `python/sailgym/` | **pass** — §4.1, §4.2, and four separate refusals (a different length, a renamed field, a bumped sensor version, and metadata merely missing) |
| 7.3 | `check_env` passes; `reset(seed=k)` twice gives identical observations; two different `np_random` draws with the same `k` give identical trajectories; `terminated` and `truncated` never both true | **pass** — §4.2, §4.3, §4.4 |
| 7.4 | discounted returns both ways agree to section 06's bound; single-env and `VectorEnv(N=1)` identical; N = 8 identical seeds, eight identical rows | **pass** — §4.5, and the agreement is asserted under **both** conventions rather than one |
| 7.5 | all three pass, with their bounds stated in the test and the measured values in the handoff | **pass** — §4.6; the bounds are documented module constants, not inline numbers |
| 7.6 | `scripts/check.sh 11` builds the extension and runs the suite from a clean `target/`; `scripts/check.sh` green | **pass** — §7 |

---

## 9. What deviated from the PRD

### 9.1 `python/sailgym/__init__.py` and `crates/sailgym-py/src/lib.rs` are in no task's `Owns:` list

Task 7.1's acceptance is `uv run python -c "import sailgym"`, so the package
has to exist at group S — but `python/sailgym/__init__.py` appears in no
`Owns:` list, and neither does the `mod` line each later task's Rust module
needs in the crate root. Both are the same gap sections 04, 05, 06, 10 and 11
each recorded: Rust has no way for task 7.4 to declare its own module without
editing task 7.1's file, and Python has no way for task 7.2 to add
`spaces.py` to a package with no `__init__.py`.

Both were staged by group, so that each gate run measured a tree containing
that group and nothing later: `lib.rs` carried `mod env; mod spec;` during
group A and gained `mod vector;` in group B, and `__init__.py` exported
`SailgymEnv` and the spaces in group A and gained `SailgymVectorEnv` in
group B. Recorded here rather than quietly absorbed (F13.2). It is now the
sixth section to hit this, and the PRD template is still where it should be
fixed.

### 9.2 The extension is `sailgym_core`, a separate distribution, and `sailgym` is pure Python

The PRD does not say how the compiled module and `python/sailgym/` should be
arranged, and the arrangement matters. Two distributions:

* **`sailgym`** — the root `hatchling` project, which already shipped
  `sailgym_conformance` and `sailgym_jax` and now also ships
  `python/sailgym/`;
* **`sailgym-core`** — the `maturin` project at `crates/sailgym-py/`, which
  ships the compiled `sailgym_core`.

The alternative — one `maturin` mixed project with
`python-source = "../../python"` and `module-name = "sailgym._core"` — would
have put a `..`-relative path in a build backend's configuration and made the
pure-Python half unbuildable without a Rust toolchain. Two build backends
cannot share one `pyproject.toml`, so the root file became a **`uv`
workspace** with the binding as its one member. A reader should know that
*distribution* names and *import* names differ here: `import sailgym` is the
root distribution's, and `import sailgym_core` is the binding's.

### 9.3 RV47's manifest grep lives in task 7.2's file and landed in group B

F17.2's "the binding does not bind `sailgym-wasm`" is task 7.1's subject, and
task 7.1 owns no test file. The grep went into
`python/tests/test_no_stray_constants.py`, which is where the Python side's
boundary rules already are — and that file is task 7.2's, which is group A's.
It was written after group B's gate run, so **neither the group-A nor the
group-B run saw it**; the final group-S run did, and so did a direct run of
the suite, and it was demonstrated able to fail (§5.5). Recorded rather than
glossed.

### 9.4 Task 7.6's two documentation files landed during group B

`CLAUDE.md` and `docs/v2/README.md` were edited while the group-B gate was
running, by a scripting slip. Neither file is read by any gate step, so the
group-B result is unaffected, and the final group-S run measured the tree
with the whole of 7.6 in it. Recorded because "the gate was green after group
B" means nothing if the tree was not what the gate saw.

### 9.5 The Gymnasium `Env` returns a copy and the `VectorEnv` returns its buffer

F17.5 asks for caller-owned buffers and the PRD asks for "a
standards-compliant public adapter". The two pull in opposite directions for
a single environment, whose observations are routinely kept by the code that
reads them. So `SailgymEnv.step` returns `self._obs.copy()` — one small
allocation, and the behaviour a Gymnasium user expects — while
`SailgymVectorEnv.step` returns the buffer itself and says so in its module
documentation. The low-level `RawVecEnv` returns `None` and writes into the
caller's arrays; that is the layer F17.5 governs, and §4.6 measures the two
separately.

### 9.6 The reward is a Python callable over the **end** observation

The package ships no reward, as the PRD requires. What it ships is the
*shape*: `reward_fn` is applied to the observation the decision period ended
on, which is the one quantity both autoreset conventions agree about. That
choice is what makes §4.5's identity exact rather than approximate, and it is
why `RawEnv::step` reports `final_obs_valid` and `RawVecEnv` exposes
`final_obs` / `final_obs_valid` at all. With no `reward_fn` the reward is
`sailgym-env`'s own, which is `ZeroReward`'s zero.

### 9.7 `Spec` is the shared configuration, and both paths take the same one

Not in the PRD. A `SailgymEnv` and a `SailgymVectorEnv` built from the same
`Spec` cannot disagree about the layout, the cadence, the bounds, the budget
or the convention — which removes by construction the failure 7.4's
step-for-step test exists to catch, and leaves that test measuring the
*runner* rather than the *configuration*.

### 9.8 The pyclasses are `unsendable`

pyo3 0.29 requires a `#[pyclass]` to be `Send + Sync`. `Episode` and `VecEnv`
are `Send` — section 06 made them so for rayon — but not `Sync`, and nothing
should make them so: each owns a `Simulation` and a mutable RNG. So all three
classes are `#[pyclass(unsendable)]`, which means a handle used from a thread
other than the one that made it raises rather than racing. That is the right
semantics for a simulation handle; the GIL-release path is unaffected,
because `py.detach` needs `&mut VecEnv: Send`, which holds.

### 9.9 `maturin develop --release`, not the default debug build

`scripts/py-test.{sh,ps1}` build the extension in release. A debug build
would have `test_performance.py` measuring the debug build's own overhead
rather than the binding's, and would take the rest of the suite from seconds
to minutes. The cost is one release compile, cached afterwards; §6 records
both the cold and the warm figure.

### 9.10 `py-test.{sh,ps1}` run `uv sync --frozen` before `maturin develop`

Not in the PRD, and necessary. `uv run` performs an implicit sync, and a sync
prunes packages the lock does not mention — so `uv run maturin develop`
followed by `uv run pytest` would install the freshly built extension and
then have the next command consider removing it. Syncing once, explicitly,
and then running both commands with `--no-sync` makes the order deterministic.

### 9.11 No task was delegated

Group A holds two tasks and group B two. The section agent executed them, as
sections 01, 02, 03, 04, 05, 06, 08, 09, 10 and 11 each recorded: the two
halves of each group are the two halves of one Rust crate, which has to
compile as a unit, and the coupling made splitting more expensive than
executing.

### 9.12 Group order was honoured, and the gate saw each group's own tree

S → gate → A → gate → B → gate → S(7.6) → gate, with §9.3 and §9.4 as the two
recorded exceptions. Later groups' sources were written before their groups
began but were kept **outside the repository** until their group.

---

## 10. Risks

**RV41 — a space shape or a field list typed in Python. Did not fire**, and
the audit that would catch it was extended, strengthened with a fifth pass
and **demonstrated able to fail twice** (§5.1). The stronger guard is
`test_a_configured_subset_of_sensors_shortens_the_layout`: a fixed length
would be wrong there and nowhere else.

**RV42 — the GIL held through `step_all`. Did not fire**, and §5.3 is the
proof rather than the claim. It also produced this section's one genuinely
surprising finding: the obvious wall-clock ratio **cannot tell the two cases
apart**, and scored the defect better than the fix. The assertion is the
Python thread's progress, and the ratio is now a report with its transcript
in its doc string.

**RV43 — `np_random` used for scenario sampling. Did not fire.** 1 000 draws
between `reset` and the first step leave 400 observations bit identical, and
the generator's state is asserted to have moved so the draws were real
(§4.3). Nothing in `python/sailgym/`, `sailgym_core` or `sailgym-env` reads
that generator; `super().reset(seed=seed)` is called because the API says
`reset` seeds it, and the value is never touched afterwards.

**RV44 — `TimeLimit` appears. Did not fire**, and the grep was demonstrated
able to find one (§5.2). The budget is `Spec(max_steps=…)`, enforced in Rust,
and `Truncated` is produced by the step budget and nothing else (F19.2).

**RV45 — `obs_digest` exposed but never checked. Did not fire.**
`check_observation_compatible` **raises**, and four different mismatches are
asserted to raise with a message naming what differs: a different length, a
renamed field, a bumped sensor version, and metadata that is merely missing
— which F16.4 says prevents strict comparison and is therefore not agreement.
The digest is section 05's canonical record rather than a hash, so equality of
the string is equality of the record.

**RV46 — per-step `info` dicts dominate the profile. Did not fire**, and §5.4
shows the assertion finding one: 38 004 retained blocks against a bound of 64
and a clean measurement of 4. The low-level loop retains 2 blocks and peaks
at 120 bytes over 1 000 steps.

**RV47 — the binding grows a shortcut through `sailgym-wasm`. Did not fire**,
and it is now a grep rather than a review item (§4.1): `sailgym-py` declares
`sailgym-env`, declares no `sailgym-wasm`, and no manifest in `crates/`
declares `sailgym-py`. Demonstrated able to fail (§5.5).

**RV48 — pyo3 needs an interpreter at build time. Did not fire, and it was
measured rather than argued.** `cargo clippy --all-targets -- -D warnings`
was run for the binding on a `PATH` built from 2 291 symlinks with every
`python*`, `pydoc*`, `uv` and `uvx` removed — `command -v python3` returns
127 — and it finished clean in 4.9 s. `abi3-py312` is what makes that work;
without it `pyo3-build-config` would need an interpreter to produce a
configuration.

**RV52 (from sections 04–06) — did not fire**, because nothing under
`crates/sailgym-physics/src` changed. The section-02 dirty-tree property
(F16.9.8) therefore never came near firing and the two section-11 browser
comparison specs passed in every gate run.

**R3 (v1) — sign-convention drift.** Not in play: this section introduces no
sign, no frame and no equation. The one numeric transformation it performs is
`v as f32` on the observation, and it is the **same** conversion
`sailgym-env`'s `vec_env` uses — deliberately, because §4.5's step-for-step
agreement would otherwise fail for a reason that has nothing to do with the
episode.

**R7 (v1) — build-sensitive artifacts.** In play, and stated: the figures in
§4.6 are properties of this host (24 cores), this build (release,
`pyo3 0.29.2`, `numpy 0.29.0`, CPython 3.12.3) and this machine's load. The
bounds are drawn to survive that; the numbers are not a contract.

---

## 11. What the next section must know

### 11.1 `docs/v2/00-foundations.md` still says F17.2–F17.5 are proposals, and no task here owns it

Sections 04, 05 and 06 each had a gate task that owned that file. **Section
07's task 7.6 does not** — its `Owns:` list is
`scripts/py-test.sh`, `scripts/py-test.ps1`, `CLAUDE.md`,
`docs/v2/README.md` — so F13.2 makes the edit *reported, not made*. What it
needs, in the shape F14.10, F15.5, F16.9, F17.6 and F19 already use:

* the delta table's **F17** row becomes *"F17.1 **implemented** by section 03;
  F17.2–F17.5 **implemented** by section 07; see F17.6 and F17.7"*;
* F17's preamble becomes *"F17.1 implemented by section 03 on 2026-09-21;
  F17.2–F17.5 implemented by section 07 on 2026-09-22. **F17.6 records what
  section 03 implemented and F17.7 what section 07 did.**"*;
* a new **F17.7**, whose content is this handoff's §9 (where the
  implementation departed) and §3 (the convention re-check), with the seven
  clauses worth promoting being: the two distributions (§9.2); the
  `unsendable` pyclasses (§9.8); the end-observation as the
  convention-independent reward input (§9.6); the shared `Spec` (§9.7); the
  copy-versus-buffer split between the two adapters (§9.5); `--release` in
  `py-test` (§9.9); and the wall-clock-ratio finding (§5.3), which is a
  measurement rule and not merely a test detail;
* F12′ needs **one sentence**, not an edit to the chain: *"Section 07 put
  `uv sync --frozen` and `maturin develop --release` inside
  `scripts/py-test.{sh,ps1}` on 2026-09-22 and amended F12 not at all, which
  is what this clause was written to make possible."*

`docs/v1/00-foundations.md` F12 is deliberately **not** edited: a v1 clause is
amended by a recorded v2 delta and never in place.

### 11.2 Gymnasium is pinned, and the pin is load-bearing

`gymnasium==1.3.0`, exactly, for the same reason `requires-python` is
`==3.12.*`: F17.4 makes the autoreset convention part of the contract, and a
floor would let it move under a recorded measurement. A version bump is a
deliberate act with two consequences to check, and
`test_the_pinned_gymnasium_convention_was_read_and_not_assumed` checks both:
the enum's spellings must still be the three Rust spells, and the declared
default must still be one this build implements. Neither is translated if it
differs — both raise.

### 11.3 The observation the period **ended** on is the quantity to build on

Under `NextStep` the returned observation is the final one; under `SameStep`
it is the reset one and the final one arrives separately. Anything that has
to be a function of "what the episode did" — a reward, a metric, a logged
feature — must be a function of the **end** observation, or it will disagree
with itself across a convention change. `SailgymEnv.end_observation` and
`RawVecEnv.final_obs` / `final_obs_valid` are that quantity; `reward_fn` is
the one consumer that ships.

### 11.4 What a PettingZoo section would inherit, and what it would not

**Inherited, unchanged:**

* the two-distribution layout and the `uv` workspace — a `ParallelEnv` is
  another module in `python/sailgym/`, not another project;
* `Spec` as the single configuration, and every space built from it;
* `ObservationContract` and `check_observation_compatible` — per-agent
  contracts are a dict of these, and the refusal rule is the same;
* the F17.3 seeding rule: one `u64` per *environment*, with per-agent
  streams derived below it in Rust, never in Python;
* the buffer discipline and the GIL release — `step_all` already takes
  `(N, ...)` arrays and would take `(N, A, ...)` ones;
* `test_performance.py`'s three measurements, including §5.3's warning about
  which of the two GIL metrics is load-bearing.

**Not inherited, and the work a PettingZoo section is:**

* `sailgym-env` has **no boat-to-boat interaction of any kind** — no
  collisions, no right-of-way, no wind shadow (`brief.md` §3, and section
  06's `lib.rs` says where the `WindField`-decorator seam would be). N
  independent boats are equal, not coupled, and `VectorEnv` is a batch of
  *episodes*, not a fleet. A `ParallelEnv` over independent boats would be
  an interface with nothing behind it;
* F16.5's parallel path returns to a fixed index order, or to an explicit
  two-phase sample-then-step, **the moment boats interact through a shared
  field**. That is a Rust change in `sailgym-env`, not a binding change;
* the autoreset convention is per-environment; Gymnasium's vector semantics
  say nothing about what happens when one agent of a parallel env terminates
  and another does not, and that is a decision to record, not to infer;
* `brief.md` **S5 is not selected**, so the multi-boat section needs its own
  recorded implementation decision before any of this is built.

The single-agent env stays the N = 1 specialisation of the same runtime, as
the PRD intends; nothing here would have to be rewritten for that to be true.

### 11.5 `Sensor` still needs `: Send`, and the repair is still one word

Section 06 §11.6's finding is untouched: `sailgym-agent`'s `Sensor` trait is
not declared `Send`, so `sailgym-env`'s `episode.rs` carries `make_sensor`,
`obs_layout_of` and `observe_into` as local copies of the registry's. This
section did not need to touch either crate and did not. The one-word repair —
`pub trait Sensor: Send` — still belongs to whoever owns `sailgym-agent`
next, and `the_env_suite_is_the_registrys_suite` is still what will say the
deletion was safe.

### 11.6 Three-quarters of the env's per-step overhead is still the force-cache refresh

Section 06 §7 measured it and reported it rather than making the repair, and
this section changed nothing about it. If a training run ever needs the
throughput, the repair is in `crates/sailgym-physics/src/simulation.rs` — a
lazy cache refresh, or an `advance` that yields each published state — and it
is still reported, not made. Measure first: 4.094 M steps/s is 20 472× real
time.

### 11.7 The repository root `README.md` still lies about the gate, and now about the layout too

No task in sections 11, 02, 03, 04, 05, 06 or 07 owns that file (F13.2), so
this is the seventh section to report it rather than fix it. Everything
section 06's handoff §12.10 lists is still true — the nine-step table spells
step 3 without `-p sailgym-task`, `-p sailgym-course`, `-p sailgym-agent` or
`-p sailgym-env`, step 4 without `--test conformance`, and the chain as nine
steps with no 10 or 11 — and this section adds **two rows to its layout
block** and nothing else:

```
crates/sailgym-py/        pyo3 binding over sailgym-env; binds the env and never sailgym-wasm (F17.2)
python/sailgym/           the Gymnasium environment; wrapper scope, so no equation and no layout of its own (F17.1)
```

### 11.8 There is still no WASM surface, no reward, no training and no CI

`Sim` is still single-boat, F8.2 is unchanged, and no episode can be started
from the browser. `ZeroReward` is still the only reward that ships. There is
still no `.github/`, so nothing runs the gate but a person — and now there
are two toolchains that have to be present for step 11, `uv` and `cargo`,
both of which `py-test.{sh,ps1}` check for by name.

---

## 12. Deliberate debts, as the PRD tracks them

| debt | state |
|---|---|
| No PettingZoo `ParallelEnv`; single-agent only | as designed. §11.4 records what it would inherit. `brief.md` S5 is not selected |
| No f32 training path, so F16.8's f32↔f64 divergence is still unmeasured for the env | unchanged. Every scalar crossing this boundary except the observation buffer is `f64`, and the buffer is an output (F17.5) |
| Reward is supplied by the caller; the package ships none | as designed. §9.6 records the *shape* that does ship |
| Gymnasium tuples and `info` dictionaries cost more than the reusable buffers | measured rather than argued: 4 retained blocks and 2 424 bytes of peak per 1 000 steps, against 2 and 120 for the low-level loop (§4.6) |
| No published wheel, no version policy | unchanged. The distribution name is `sailgym-core` and the version comes from the cargo workspace |
| **New:** the wall-clock overlap ratio is reported and not asserted | §5.3. Repaid by a measurement that can distinguish the two cases; the progress metric already is one |
| **New:** `uv sync --frozen` now needs a Rust toolchain | because `sailgym-core` is a workspace dependency of the root project. The gate needs `cargo` anyway; a pure-Python consumer of `sailgym_conformance` alone does not, and would have to install that package on its own |
| **New:** step 11 builds in release, so a cold `target/` costs a compile | §6 records both figures. Repaid by nothing; a debug build would make §4.6 measure the wrong thing |

---

## 13. Commands

```
uv sync --frozen                                   # the environment, from the lock
uv run maturin develop --release \
    --manifest-path crates/sailgym-py/Cargo.toml   # the extension
scripts/py-test.sh                                 # the suite      (gate step 11)
scripts/py-test.sh -s                              # ...with the measured numbers
SAILGYM_SKIP_MATURIN=1 scripts/py-test.sh          # ...without rebuilding
scripts/py-test.sh python/tests/test_autoreset.py -s
scripts/check.sh                                   # the gate, eleven steps
scripts/check.sh 2                                 # clippy, with or without python3
scripts/check.sh 10                                # ruff
scripts/check.sh 11                                # maturin + pytest
cargo tree -p sailgym-physics --edges all
```

`SAILGYM_WRITE_CONFORMANCE=1` still refreshes `docs/v2/conformance.md`'s
appended section, and still must not be set by the gate (section 03 §8.2).
