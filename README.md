# sailgym

A browser sailing simulator for an ILCA 7 / Laser Standard dinghy, in which the
sailing behaviour is not scripted — it falls out of the forces.

The physics is written from scratch in Rust, compiled to WebAssembly, and driven
by a React/TypeScript front end. You steer with the tiller and trim with the
mainsheet, and that is all you control. The boom angle, the heel, the leeway,
the tack, the gybe and the capsize are consequences.

Around that core, v2 added two things. One is a learning loop in the browser:
try a short challenge, inspect the replay, and retry under exactly the same
conditions. The other is a headless research stack: courses, an agent
interface, an episode runner and a Gymnasium binding, all over the same physics.

`docs/v1/brief.md` is the original specification and `docs/v2/README.md` the
current milestone plan. Both are worth reading if you want to know why anything
here is the way it is; what follows is what the code actually does.

**Status (2026-10-08, `f4c3ff2`).**

- **v1** — all ten milestones (M0–M9) landed. `docs/v1/acceptance.md` walks the
  brief's fourteen success criteria with a verdict on each.
- **v2, the playable half** — sections 01 and 08–11 shipped: a low-poly 3-D boat
  projected into the SVG, corrected stability and mainsheet models, touch
  controls, truthful replay with experiment identity, and three guided-practice
  challenges.
- **v2, the research half** — sections 02–07 shipped: a cross-stack conformance
  bundle, a JAX port of the wind field, `sailgym-course`, `sailgym-agent`,
  `sailgym-env` and the Python/Gymnasium binding.
- **v2 section 12** shipped: **waypoint courses in the browser**. A numbered
  overlay with gate bars and leg lines, three course challenges, and a
  rule-sailor baseline you can race as a translucent ghost or watch as a replay.
  This is the first section to connect the research crates to the page: the
  controller, the course layer and the episode runner all run behind one
  coarse-grained `run_baseline` call.
  [`docs/v3/discussions/core-features-not-in-web.md`](docs/v3/discussions/core-features-not-in-web.md)
  tracks what is still core-only.

The gate has eleven steps and is green at the last recorded section handoff.

---

## What it simulates

A reduced **4-DOF marine dynamics model** — surge, sway, yaw and roll (brief §6)
— plus the boom as a dynamic degree of freedom of its own, and the mainsheet as
a rope that can pull but never push.

| Subsystem | What it does |
|---|---|
| **Wind field** | Uniform, spatially varying, or gusty. Procedural, divergence-free, and deterministic from a seed. The visualization samples the *same* field the physics does. |
| **Apparent wind** | Built from the true wind, the boat's velocity and its rotation at the sail's centre of effort — so the sail unloads correctly during a fast tack. |
| **Sail** | A continuous lift/drag model defined over the whole ±180°, covering attached flow, stall, deep stall and reversed loading. Applied at a centre of effort, so the yaw moment, the heeling moment and the boom torque all come out of the geometry. |
| **Boom** | `I_b β̈ = M_aero + M_sheet + M_damping + M_limits`. There is no `portTack`/`starboardTack` state anywhere in the codebase. |
| **Mainsheet** | A unilateral tension element: zero whenever the rope is slack (`e ≤ 0`), `max(0, k·e + c·ė)` when it is taut (v2 F18.1b). It generates boom torque through rope geometry; the boom angle is never assigned. Fully hauled, the sheet stops at the rig's own shortest rope path, so there is no hidden preload. |
| **Centreboard & rudder** | Finite lifting surfaces in water, using each surface's own local flow. Leeway resistance, yaw damping, speed-dependent rudder authority and rudder stall all emerge; none is coded as a special case. |
| **Hull** | A reduced empirical resistance model, linear plus quadratic, parameterised so towing-tank data can replace it. |
| **Roll & capsize** | A four-harmonic righting arm `GZ(φ)` whose peak sits exactly at the declared peak angle. It is negative from the vanishing angle all the way to inversion, and stable upside down (v2 F18.1a). The boat can pass dynamically through 90° of heel; `capsized` is *reported*, never acted on. |

One boat runs natively at roughly **3 000× real time** on one core. A batch of
4 096 independent boats on 24 threads reaches **5.25 M steps/s**
(`docs/v2/throughput.md`).

### What it is not

**This prototype does not claim quantitative ILCA accuracy**, and brief §36 says
so explicitly. Most coefficients are physically motivated estimates rather than
measurements. Every one of them is tagged KNOWN / ASSUMED / TUNABLE / DEFERRED
in `docs/v1/parameters.md`; the two that v2 changed are recorded in
`docs/v2/physics-validation.md` and `docs/v2/progress/08-handoff.md`.

Still out of scope: currents, waves, heave and pitch, sail cloth, mast bend,
traveler and vang, hiking or any sailor movement, interacting boats, shorelines
and obstacles, and RL *training*. The environment exists, but no reward,
policy or training loop ships. `docs/v2/discussions/deferred-features.md` gives
the disposition of every item.

---

## Workspace

| Crate / package | What it is | Reached from |
|---|---|---|
| `crates/sailgym-physics` | The pure Rust core: state, forces, integrator, wind, scenarios, recording codec, experiment identity. No browser or Python dependency. | everything |
| `crates/sailgym-task` | The practice evaluator: three skills and three waypoint courses, scored on every physics step, outside the physics crate. | web, env |
| `crates/sailgym-course` | Routes, marks, gates, ordered passage, guidance, and the shipped waypoint courses in `courses/`. | env, Python |
| `crates/sailgym-agent` | Sensors, the observation layout, the `[−1, 1]^k` action funnel, cadence, the `Agent` contract, and the rule sailor in `src/pilot/`. | env, Python, web |
| `crates/sailgym-env` | The episode runner: `Outcome`, termination vs truncation, autoreset, decision log, `VecEnv`, evaluation report. | Python, bench |
| `crates/sailgym-wasm` | The thin `wasm_bindgen` wrapper the web app talks to. Since v2 section 12 it also calls `sailgym-course`, `sailgym-agent` and `sailgym-env`, for `run_baseline` and the course block. | web |
| `crates/sailgym-py` | The pyo3 binding over `sailgym-env`, built as `sailgym_core`. | Python |
| `crates/sailgym-bench` | Native benchmarks and the golden, conformance and throughput generators. | CLI |
| `web/` | Vite + React + TypeScript app and its Playwright/vitest suites. | browser |
| `python/sailgym` | The Gymnasium `Env` and `VectorEnv`: wrapper scope, no equations. | Python |
| `python/sailgym_conformance`, `python/sailgym_jax` | The conformance-bundle loader and the JAX verification port of the wind field. | gate step 11 |

The dependency arrows run one way: `env → {agent, course, task, physics}`,
`agent → {course, physics}`, `task → {course, physics}`,
`course → physics`. `sailgym-physics` depends on none of them, and the gate
checks that with `cargo tree`.

---

## Quick start

### Prerequisites

| | |
|---|---|
| Rust | stable, with the `wasm32-unknown-unknown` target and the `rustfmt` and `clippy` components — `rust-toolchain.toml` pins all of it, so `rustup` will do the right thing on first build |
| [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) | `cargo install wasm-pack` |
| Node + [`pnpm`](https://pnpm.io/) | any recent version |
| [`uv`](https://docs.astral.sh/uv/) | only for the Python side (gate steps 10–11). `uv` installs Python 3.12, `maturin`, JAX and Gymnasium 1.3.0 from the committed `uv.lock` |
| Shell | PowerShell 7 (`pwsh`) on Windows, `bash` elsewhere |
| Browsers | `pnpm --dir web exec playwright install chromium firefox` — the `msedge` test project uses whatever Edge is installed on the machine |

### Build and run

```sh
pnpm --dir web install
scripts/build-wasm.sh          # or: pwsh scripts/build-wasm.ps1
pnpm --dir web dev             # http://localhost:5173
```

`web/src/wasm/` is generated by the build script and is not committed. Re-run
the build script whenever you change anything under `crates/`; the Vite dev
server picks up the new package on reload.

For a production build, `pnpm --dir web build` then `pnpm --dir web preview`.

---

## Operating it

### Controls

| Input | Effect |
|---|---|
| `A` / `←` and `D` / `→` | Tiller. These command a rudder **rate**, not an angle; released, the tiller returns to neutral. |
| **Drag down / drag up** on the boat (mouse or pen) | Haul in / ease the mainsheet. The drag commands a payout rate and holds it while the button is down, so you can haul and hold. |
| `Space` | Release the sheet — let it run. |
| `P` | Pause / resume. |
| `.` | Single physics step while paused. |
| `R` | Reset to the current scenario's initial condition. |
| `M` | Switch between Sail Mode and Debug Mode. |
| Wheel | Zoom. |
| Middle-drag, or `Shift`+drag | Pan. A plain left-drag is always the mainsheet. |

**On-screen and touch controls** (v2 section 09) sit beside the world view.
There is a helm pad and a mainsheet pad, each taking one finger and sending a
normalised rate, plus a hold-to-release button and a Reset button. Under them
are the boat's *actual* rudder angle and sheet length, and the full-travel
times, which come from the Rust parameter catalogue. Keyboard, mouse and touch
all compose into one `Controls` value at one call site. A finger on the world
view itself does nothing yet: there is no touch pan or pinch-zoom. The layout
follows the measured viewport and works down to 360 × 640.

The header holds the clock speed (0.25×, 1×, 2×, 4×), the camera mode
(follow / north-up), the wind mode (uniform / spatial / gust) and the scenario
picker.

### The two modes

**Sail Mode** is the default and shows only brief §29's readouts: wind, boat
speed, heading, heel, rudder, sheet and capsize state.

**Debug Mode** (`M`) adds brief §30's instrumentation list: sixteen toggleable
force and moment overlays drawn at their application points, eight time-series
charts, a numeric readout of every diagnostic field, and a collapsible
parameter panel.

The panel is generated from the Rust catalogue, so every parameter the core has
appears in it with its unit and its provenance tag, and editing one takes effect
on the next physics step. The wind's speed, direction and variation are in the
same panel. Rust validates every edit before committing it, and a value that
would produce an unphysical righting curve is refused with a message rather than
silently accepted. **Reset to ILCA defaults** restores the whole catalogue.

### Guided practice

The practice panel (v2 section 11) offers three challenges, each a shipped
scenario plus a deterministic evaluator in `sailgym-task` that watches every
physics step:

| Challenge | Scenario | Goal (thresholds shipped by `TaskSpec::shipped`) |
|---|---|---|
| Get moving | `free_sail` | hold 1.2 m/s of forward speed for 3 s, within 45 s |
| Complete a tack | `tack` | cross head to wind and settle more than 35° onto the other tack for 1.5 s, still making 0.4 m/s, within 45 s |
| Recover from excessive heel | `sheet_release_recovery` | get the heel back under 20° and hold it for 2 s, within 30 s; reaching 75° with the sheet still in, or capsizing, ends the attempt |

An attempt ends as succeeded, failed (with a reason: backward drift, wrong way,
repeated jitter, late release or capsize) or timed out. It is recorded as it
runs. **Inspect** opens the replay at the moment the challenge is about.
**Retry** restores the exact initial contract: parameters, state, controls,
wind and seed. The panel remembers the last two attempts and compares them only
when Rust's `ExperimentIdentity` says they ran under the same conditions. No
threshold or rule lives in TypeScript, and a parameter or wind edit during an
attempt ends it as `conditions_changed`.

### Scenarios

Six ship with the app (brief §32), and none of them scripts an outcome — they
set an initial condition and a wind field, and what happens next is up to you:

| Scenario | Set up for |
|---|---|
| `beam_reach_capsize` | Beam-on wind (9 m/s) with the sheet hard in. Oversheeting, and going over. |
| `sheet_release_recovery` | Byte-identical to the above. The only difference is what the sailor does. |
| `close_hauled` | Steady upwind sailing. |
| `tack` | Poised to tack through the wind. |
| `gybe` | Downwind, for controlled and uncontrolled gybe experiments. |
| `free_sail` | The sandbox, and the default on load. |

The two beam-reach scenarios moved from 7 to 9 m/s in v2 section 08. The
corrected righting curve made the boat stiffer, and 9 m/s sits in the measured
window where holding the sheet capsizes it and an early release recovers it.

Pick a scenario from the header, or load it directly: `?scenario=close_hauled`.
The URL keeps the scenario across a reload, so a link reproduces a run.

There is one other URL parameter, `?renderHz=20`, which throttles *drawing*
while leaving the physics on wall-clock time. It exists so the test suite can
measure that the two really are independent; it is also a fair way to see it
for yourself.

### Recording and replay

Record an episode at 5, 10, 20 or 50 Hz, then scrub it on a timeline, step it
frame by frame, or play it back at five speeds (0.25×–4×). Replay consumes the
**stored** trajectory rather than recomputing it.

Since v2 section 10, the **whole page** follows the recording during replay:
the world view, the HUD, the wind, the force overlays, the charts and the
diagnostics panel. Each frame carries 40 diagnostic scalars beside the state
(episode schema 2). Anything a recording does not carry reads
`Not recorded`, never zero, never the live value. Each episode also carries a
canonical **experiment identity**: model version and physics source tree, the
resolved parameters, initial state, wind, seed, `dt` and task. The page shows
it as a badge and uses it to decide whether two episodes are comparable.

Episodes save as JSON (readable) or as a binary `SGEP` file (compact), and both
re-import through the same control. A recording is capped at 13 443 frames
(8 MiB). The cap is shown before you start, but the page does not yet say when
a recording has reached it: the frame counter just stops rising. Schema-1 files from v1 still
open and scrub; they are honest about the fields they lack, and they are never
comparable with anything. `docs/v2/recording-format.md` is the normative
description.

### Try the primary demonstration

This is brief §46, and it is the thing the whole prototype exists to make
possible. It takes about a minute:

1. Load `beam_reach_capsize`.
2. Drag down on the boat to haul the mainsheet in, and hold it there.
3. Watch the heel build. Around ten seconds in, the boat goes over — and the
   capsize readout flips.
4. Press `R`, haul in again, and this time release the sheet with `Space` about
   four seconds in, while the boat is well heeled but not yet on its beam ends.
   At 9 m/s the last release that still recovers is at about seven seconds.
5. Watch the order of what happens: the tension drops, *then* the boom swings
   out, *then* the heeling moment collapses, *then* the sail force falls, and
   only then does the boat come back up.

Nothing in the codebase says that releasing the sheet causes a recovery — there
is a test, `no_shortcuts::no_release_recovery_rule`, whose job is to keep it
that way. The ordering above is asserted automatically in three browsers by
`web/tests/e2e/demonstrations.spec.ts`.

---

## Running headless

The physics crate has no browser dependency, which is what makes the native
simulator and the research stack possible (brief §45).

```sh
# Throughput and a per-subsystem breakdown, for one scenario or all six
cargo run --release -p sailgym-bench --bin bench -- --scenario close_hauled --seconds 600
cargo run --release -p sailgym-bench --bin bench -- --all

# The time-step convergence study; --write regenerates docs/v1/convergence.md
cargo run --release -p sailgym-bench --bin convergence

# Multi-core throughput: bare Simulations, then the real episode runner
cargo run --release -p sailgym-bench --bin vec_bench -- --write docs/v2/throughput.md
cargo run --release -p sailgym-bench --bin env_bench -- --write docs/v2/throughput.md

# Wind-field sampling and raw step-rate microbenchmarks
cargo run --release -p sailgym-bench --bin sailgym-bench
```

Always build `--release`: a debug build measures the optimiser rather than the
code, and the binaries say so on stderr if you forget. Run `env_bench` after
`vec_bench`: `vec_bench --write` rewrites the whole throughput document and drops
the section `env_bench` appends.

### From Python

The Gymnasium binding wraps `sailgym-env`: one `SailgymEnv`, or a
`SailgymVectorEnv` that steps N independent episodes in Rust with the GIL
released. Observation and action spaces are built from the layout Rust reports.
Episode boundaries are Rust's (termination and truncation are kept apart), and
no reward ships: you pass one as a Python callable.

```sh
uv sync --frozen
uv run maturin develop --uv --release --manifest-path crates/sailgym-py/Cargo.toml
```

```python
from sailgym import SailgymEnv

env = SailgymEnv("tack", max_steps=4000, cadence=10)
obs, info = env.reset(seed=0)
obs, reward, terminated, truncated, info = env.step(env.action_space.sample())
```

`sailgym_core.Spec` takes the rest of the episode contract: the sensor list,
bounds, autoreset convention, decision logging, and a course as `route_json`.
`docs/v2/progress/07-handoff.md` documents the binding in full.

---

## Testing

### The gate

One command runs everything, in order, failing fast:

```sh
scripts/check.sh               # Linux / CI
pwsh scripts/check.ps1         # Windows
```

Eleven steps, each of which proves something specific:

| # | Step | Proves |
|---|---|---|
| 1 | `cargo fmt --check` | Source is canonically formatted, so diffs are semantic. |
| 2 | `cargo clippy --all-targets -- -D warnings` | No lint regressions, tests and benches included. |
| 3 | `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent -p sailgym-env` | The physics core, the practice evaluator, the course layer, the agent interface and the episode runner are correct **and build on the host with no WASM toolchain**. |
| 4 | `cargo test -p sailgym-physics --test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance` | The physical invariants hold, no prohibited shortcut has crept in, the integrator converges, port/starboard symmetry holds, no coefficient has drifted from the catalogue, and the committed conformance bundle still describes the compiled physics. |
| 5 | `cargo test -p sailgym-physics --test regression` | Six recorded scenarios still reproduce bit-for-bit. |
| 6 | `wasm-pack build …` | The Rust core still compiles to WASM and the JS glue regenerates. |
| 7 | `pnpm --dir web typecheck` | The TypeScript side still matches the WASM surface. |
| 8 | `pnpm --dir web test:unit` | The pure TypeScript — projection, camera, clock, controls, touch input, replay selection, schemas — is correct without a browser. |
| 9 | `pnpm --dir web test:e2e` | The app runs in Chrome, Edge and Firefox (plus an emulated Pixel 5 for the touch suite) with no console or page errors. |
| 10 | `uv run ruff check python && uv run ruff format --check python` | The Python is canonically formatted and lint-clean. |
| 11 | `scripts/py-test.sh` | Builds the pyo3 extension, then runs pytest: the conformance-bundle loader, the JAX wind port, the Python constants audit, and the Gymnasium binding's spaces, seeding, returns identity and performance bounds. |

Most of the chain's wall time is step 9; the last recorded full run took
778 s, about 13 minutes (`docs/v2/progress/07-handoff.md` §6). Any single
step runs on its own:

```sh
scripts/check.sh 3
pwsh scripts/check.ps1 -Step 3
```

### The fast subset

While working, use:

```sh
scripts/check.sh --fast        # pwsh scripts/check.ps1 -Fast
```

Steps 1–9, with step 9 restricted to Chromium and to the specs not tagged
`@slow` — the browser performance measurement and the three brief §46
demonstrations, which are about half the suite's wall time and cannot be made
quick without making them mean less. **Steps 10 and 11 are not in `--fast`**,
which is a tracked debt. Run them explicitly when you touch Python or the
binding.

`--fast` is not the gate. The full chain is what has to be green before a
change lands.

### Not in the gate, but worth running

```sh
pnpm --dir web build           # the production bundle — the gate only builds dev
```

### If you change a coefficient

The project takes brief §43 seriously: physics coefficients must not be silently
tuned to make a scenario look better. Several things enforce it, and they will
stop you:

- `provenance::shipped_values_match_the_f7_table` parses the parameter tables
  out of `docs/v1/00-foundations.md` (and the v2 F18.1c override table) and
  compares every numeric row against what the code actually ships. A
  coefficient cannot move unless the normative document moves with it.
- `tests/regression.rs` compares six recorded 30-second trajectories. A **0.1 %**
  change to one hull coefficient fails all six within a fifth of a second of
  simulated time.
- `tests/conformance.rs` holds the committed bundle under `conformance/` to the
  compiled physics; a changed equation or coefficient needs the bundle
  regenerated with `gen_conformance`.
- The physics source tree id is part of every episode's identity, so an
  uncommitted edit under `crates/sailgym-physics/src` makes recordings
  comparable with nothing. Two practice browser tests fail on a dirty physics
  tree for that reason, by design.

If a change is genuine, record the reason, the source and the assumption — in
the `parameters.rs` doc comment and in `docs/v1/parameters.md` — and regenerate the
goldens afterwards:

```sh
cargo run -p sailgym-bench --bin gen_golden      # refuses on a dirty physics tree
SAILGYM_UPDATE_DOCS=1 cargo test -p sailgym-physics --test provenance docs_match_source
```

---

## Documentation

| File | What it is |
|---|---|
| `docs/v1/brief.md` | The original specification. **Authoritative on scope.** |
| `docs/v1/00-foundations.md` | Normative: frames, sign conventions, equations, the parameter catalogue, the WASM surface. Nothing may redefine it. |
| `docs/v1/acceptance.md` | The brief's fourteen success criteria, with evidence and a verdict on each, as of M9. |
| `docs/v1/parameters.md`, `invariants.md`, `convergence.md`, `performance.md` | Every parameter with its provenance; every invariant with its test and tolerance; the time-step study; the v1 performance figures. |
| `docs/v2/README.md` | The v2 milestone: delivery order, status of every section, open decisions. |
| `docs/v2/brief.md`, `docs/v2/00-foundations.md` | v2 scope rows and the **normative deltas** to v1 (F14–F19). A v2 delta amends v1 only where it says so. |
| `docs/v2/physics-validation.md` | The evidence behind section 08's corrected `GZ` curve and mainsheet. |
| `docs/v2/practice-validation.md` | The sweeps behind every practice-challenge threshold. |
| `docs/v2/recording-format.md` | Normative for the episode schema, identity and binary layout. |
| `docs/v2/conformance.md`, `docs/v2/throughput.md` | Generated: the conformance bundle's tolerances, and measured multi-core throughput. |
| `docs/v2/baseline-validation.md` | Generated by `course_bench`: the rule sailor's measured trim table, its two suites, and the rule the course time limits come from. A controller measurement on one machine, not a physical validation. |
| `docs/v2/prds/`, `docs/v2/progress/` | One PRD and one handoff note per v2 section. |
| `docs/v2/discussions/` | Design discussions behind v2, including `deferred-features.md`. |
| `docs/v3/discussions/` | Starting notes for what comes next. |
| `conformance/<key>/manifest.json` | The committed conformance bundle: what a second implementation is held to, and to what tolerance. Generated, never edited. |
| `CLAUDE.md` | Entry point for coding agents. |

---

## Layout

```
crates/sailgym-physics/   pure Rust core; all physics tests live here
crates/sailgym-task/      practice-task evaluation; depends on course and physics
crates/sailgym-course/    routes, marks, gates, guidance, passage, course catalogue
crates/sailgym-agent/     sensors, observation layout, actions, cadence, pilot/ (rule sailor)
crates/sailgym-env/       episode runner, Outcome, autoreset, decision log, VecEnv
crates/sailgym-py/        pyo3 binding over sailgym-env (never sailgym-wasm)
crates/sailgym-wasm/      thin wasm_bindgen wrapper
crates/sailgym-bench/     native benchmarks, golden/conformance/throughput generators
web/                      Vite + React + TypeScript app and Playwright/vitest specs
python/sailgym/           the Gymnasium environment (wrapper scope, no equations)
python/sailgym_conformance/  stack-neutral conformance-bundle loader
python/sailgym_jax/       the JAX verification port of the wind field
python/tests/             pytest
conformance/              the committed conformance bundle
scenarios/                the six scenario documents
courses/                  the three waypoint course documents (v2 section 12)
scripts/                  build-wasm, check, py-test (.ps1 and .sh)
docs/                     v1, v2 and v3 briefs, foundations, PRDs, progress notes
```

Three boundaries are load-bearing and are mechanically enforced:

- **No physical equation is implemented in TypeScript** (brief §23). Rust owns
  every derived quantity, and in v2 every task rule and identity verdict too;
  the browser gets them through coarse-grained calls.
  `provenance::no_physics_in_typescript` scans every hand-written source file for
  a density, a `g` or a `½ρV²`.
- **No numeric physical literal lives outside the parameter catalogue** — every
  coefficient is in one place, named, tagged and editable at runtime.
  `provenance::no_stray_constants` keeps it that way in Rust, and
  `python/tests/test_no_stray_constants.py` does the same for the Python.
- **Nothing depends back on the physics.** `cargo tree -p sailgym-physics`
  mentions no other sailgym crate, and the gate asserts it.

---

## Known limits

Recorded here rather than left to be discovered:

1. **Two v2 changes are awaiting a human ruling.** Section 08 moved
   `stability.gm` from 1.00 m to 0.55 m, because the v1 value was infeasible
   under the corrected righting-curve constraints. It also moved the two
   beam-reach scenarios from 7 to 9 m/s. Together these fixed the v1 limits
   "the boat cannot be inverted" and "a fully hauled sheet is pre-tensioned",
   but brief §43 puts coefficient changes with the human
   (`docs/v2/progress/08-handoff.md` §6).
2. **The hull model has no planing regime**, so above about 5 m/s it
   over-predicts resistance. The debug panel raises a warning whenever the boat
   is above that speed, because any number it reports up there is an
   extrapolation rather than a prediction.
3. **Touch has only been tested in emulation.** The touch suite runs on an
   emulated Pixel 5. No real digitiser, mobile GPU or retracting address bar
   has been tested.
4. **The research stack has no browser surface.** Courses, sensors, agents,
   episodes and evaluation are reachable from Rust and Python only. See
   `docs/v3/discussions/core-features-not-in-web.md`.
5. **Section 12 is part-landed.** Routes can now carry an explicit `start`, but
   `sailgym-env`'s missed-mark probe still rebuilds the route without it. An
   open route passed in through `route_json` can therefore report a false
   `MarkMissed` on its first leg. Task 12.2 is the fix.
