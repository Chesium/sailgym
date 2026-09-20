# Section 09 — Handoff (M8: scenarios, recording and replay)

Written per F13.6 on 2026-09-20. **M8 is complete.** All seven tasks landed,
the eight-step gate passes end to end and exits 0, the six brief §32 scenarios
ship and load, episodes record and replay from stored data, and the six golden
regressions are in the tree with R7 handled.

Read this before starting section 10 (`docs/10-hardening.md`). Nothing below
redefines anything in `docs/00-foundations.md`.

Six things want a human eye rather than just a read.

- **The application's default scenario is now `free_sail`, not
  `WindConfig::default()`.** `free_sail.json` was authored to reproduce the old
  `?scenario=free_sail` browser fixture exactly, so sections 05–08's specs are
  testing the same boat — but the *bare* URL changed from a gusty westerly to a
  uniform northerly, and two section 02/03 specs had to say which wind they
  meant. §2.1.
- **`Sim::reset` and a new `Sim::restart` now mean different things.** A reset
  (the `R` key, the Reset button) replays the scenario's initial condition and
  **keeps the live parameter catalogue**; choosing a scenario in the picker
  applies its `parameter_overrides`. Without the split, every reset threw away
  the brief §31 edit it exists to make good, and two section 08 specs proved it.
  §2.2.
- **`crates/sailgym-physics/build.rs` is new, and it is what makes R7 honest.**
  It asks the compiler for its version and bakes `rustc`/`target`/`profile`
  into the crate, so a golden file records the build that produced it rather
  than a guess. §2.3.
- **`gen_golden --allow-dirty` exists and was used exactly once**, to bootstrap
  the golden files in the same change that introduces them. The refusal on a
  dirty physics tree is the default and was verified. §2.4.
- **The committed goldens are `rustc 1.98.1 / x86_64-unknown-linux-gnu /
  debug`.** On any other build the six regressions **skip with a message
  naming both toolchains**; that is the deliverable, not a workaround. §5.
- **In replay, the world view follows the episode but the Debug-Mode
  diagnostics panel keeps describing the (paused) live simulation.** An
  `EpisodeFrame` carries the F3 state, four forces, four moments, the sheet
  tension and the wind — not the whole fifty-field brief §30 record. The
  timeline says so on screen. §8.1.

---

## 1. What landed

### 9.1 — Scenario schema (P-group S)

`crates/sailgym-physics/src/scenario.rs`, to the PRD's struct:

```rust
pub struct Scenario { schema_version, name, description, seed,
                      parameter_overrides: BTreeMap<String, f64>,
                      initial_state, wind, camera, initial_controls }
pub struct InitialState { x, y, heading_deg, heel_deg, speed,
                          boom_deg_to_port, sheet_length }
impl Scenario {
    pub fn load(json: &str)   -> Result<Self, ScenarioError>;
    pub fn to_boat_state(&self) -> BoatState;
    pub fn to_parameters(&self) -> Result<BoatParameters, ScenarioError>;
    pub fn to_controls(&self)  -> Controls;
    pub fn validate(&self)     -> Result<(), ScenarioError>;
}
```

`BTreeMap`, not a hash container (F9.3): overrides are applied in key order, so
two maps built by different insertion orders produce bit-identical
`BoatParameters`. `override_determinism` asserts that on the bits, not on
`PartialEq`.

**Degrees are converted in exactly one place.** `to_boat_state` holds the two
conversions the PRD names, written out inline:

```
psi  = wrap_pi((90° − heading_deg))     compass CW-from-north → ψ CCW-from-east
beta = wrap_pi(−boom_deg_to_port)       F2.1's sign flip, applied once
```

`InitialState::from_boat_state` is the documented inverse (it is what lets an
ad-hoc browser reset still produce an honest recording header), and
`angle_round_trip` is what keeps the two in step.

`ScenarioError` has seven variants, because "the scenario is bad" is not a
message anyone can act on: `Parse`, `MissingField`, `UnsupportedSchemaVersion`,
`OutOfRange`, `UnknownOverride`, `Parameter`, `Wind`. `load` checks the
required keys and the schema version **before** deserialising the rest, so a
document from a future schema reports its version rather than complaining about
whichever field happened to move.

`to_parameters` validates like `Simulation::set_parameter` does (section 08
handoff §2.1): the catalogue must validate *and* `GzCurve::fit` must succeed,
because a `stability` group `fit` rejects yields a boat with no righting arm at
all, silently.

The six documents are embedded with `include_str!` from `scenarios/`, so the
browser and the native headless simulator read the same six files and there is
one copy in the repository.

`web/src/sim/scenarioTypes.ts` mirrors `Scenario`, `InitialState`,
`EpisodeHeader`, `EpisodeFrame`, `ToolchainInfo` and `Episode`;
`web/tests/unit/scenarioTypes.test.ts` parses both sides and compares the field
sets **in both directions and in order**, exactly as `diagnostics.test.ts` does
for the debug record.

### 9.2 — The six shipped scenarios (P-group A)

`scenarios/*.json`, exactly six, no more:

| File | Wind | Initial condition | Camera |
|---|---|---|---|
| `beam_reach_capsize` | uniform 7.0 m/s from 000° | heading 090°, at rest, sheet 0.90 m | follow |
| `sheet_release_recovery` | **identical** | **identical** | follow |
| `close_hauled` | uniform 3.5 m/s from 270° | heading 315°, at rest, sheet 2.00 m | northUp |
| `tack` | uniform 4.0 m/s from 000° | heading 045°, 1.8 m/s, boom 35° to stbd, sheet 2.00 m | follow |
| `gybe` | **gust** 5.0 m/s from 000°, variation 0.12 | heading 170°, 2.5 m/s, boom 75° to port, sheet 4.00 m | follow |
| `free_sail` | uniform 5.0 m/s from 000° | heading 090°, at rest, sheet 4.50 m | northUp |

Where the numbers come from, so nobody has to guess:

- **7.0 m/s for the beam reach** is the figure section 07's handoff §6
  recorded: hauled hard in on a beam reach the boat first capsizes between
  **6.90 and 6.95 m/s**, and 7.0 is just above it. The capsize is therefore
  reachable in about ten seconds and is *driven into*, not placed.
- **3.5 m/s for `close_hauled`** is F11 R2's figure, confirmed by section 07's
  §7 (peak heel 6.8°, steady 1.78 m/s, no capsize at any sheet setting). R2
  does **not** fire.
- **2.00 m of sheet for `close_hauled`** is the setting section 07 §7 measured
  as best.
- **`gybe` is the one gusty scenario**, so the shipped set exercises the F6.1
  temporal mode rather than only the uniform one.

`sheet_release_recovery` is byte-identical to `beam_reach_capsize` in `seed`,
`parameter_overrides`, `initial_state`, `wind`, `camera` and
`initial_controls` — asserted field by field, and by comparing the resolved
catalogues as strings, in Rust (`recovery_matches_capsize_setup`) and again over
the files on disk in TypeScript. Only `name` and `description` differ. That is
what makes the brief §46 demonstration honest: the golden trajectories show the
two scripts diverging from the same start to **86.9°** of heel (hold) against
**53.9°** (release, then haul back in) at t = 30 s.

No scenario scripts an outcome. `no_scripted_outcomes` walks every key of every
document and checks it against an **allowlist** of the twenty-four keys the
schema has, which is strictly stronger than the PRD's `script|sequence|events|
timeline|forced` scan, and then runs that scan too. (`description` is exempt
from the substring scan for the obvious reason: de-**script**-ion.)

`web/src/ui/ScenarioPicker.tsx` is fed the rows from `Sim.scenarios_json()` and
authors nothing.

### 9.3 — Recorder (P-group A)

`crates/sailgym-physics/src/recording.rs`, to the PRD's structs.
`FRAME_LEN = 38` scalars per frame, in the declared field order, which is what
makes the binary form a header plus one contiguous `Float64Array`.

```rust
pub struct Recorder { /* … */ }
impl Recorder {
    pub fn start(hz: f64, header: EpisodeHeader) -> Self;
    pub fn due(&self, t: f64) -> bool;          // added; see below
    pub fn observe(&mut self, sim: &Simulation, diag: &Diagnostics);
    pub fn finish(self) -> Episode;
}
```

`due` is an addition to the PRD's signature list, not a change to it: it lets a
caller skip building a fifty-field `Diagnostics` on the nine steps in ten that
will not be kept. `observe` applies the same test itself, so skipping `due` is
only slower, never wrong.

**Two serialisations, both the core's:**

```text
Episode::to_json / from_json      inspection; exact f64 round trip
Episode::to_binary / from_binary  "SGEP" + u32 header + JSON header
                                  + 8-byte-aligned little-endian f64 block
```

Both check `schema_version` first, so an episode from another build gets a
message rather than a shape complaint.

**No wall clock in the physics.** `determinism::no_wall_clock` greps every file
under `crates/sailgym-physics/src`, test code included, so the needles in
`recording::tests::no_wall_clock_in_physics` are assembled from fragments at run
time rather than spelled out. `EpisodeHeader::created_utc` is supplied by the
caller and frozen at `start`; `recording::iso8601_utc(epoch_millis)` is a pure
function of its argument that reads nothing. The browser's `Date.now()` is bound
in `crates/sailgym-wasm/src/lib.rs` and is the whole of the wall clock in the
browser build.

**R7.** `ToolchainInfo::current()` reads three `env!` values emitted by the new
`build.rs`, which asks `$RUSTC --version` and reads `TARGET` and `PROFILE`.

WASM surface added: `start_recording`, `stop_recording`, `is_recording`,
`recorded_frames`, `episode_to_binary`, `episode_from_binary`,
`episode_from_json`, `scenarios_json`, `scenario_json`, `restart`. While
recording, `Sim::advance(n)` issues the steps one at a time so the recorder sees
every completed state; F9.7 makes that bit-identical to the batched call.

### 9.4 — Replay player and timeline (P-group B)

`web/src/sim/replay.ts` to the PRD's interface, plus `startTime`, `endTime`,
`indexAt` and `snapshotFromFrame`. `frameAt` returns the **stored object**, not
a copy — that is what makes the frame-edit proof possible.

Interpolation is display only (F8): `sampleAt` at a frame's own time returns
that frame unchanged; between frames it lerps; `psi` and `beta` take the
**shorter arc** across the `±π` wrap while `phi`, which F3 leaves unwrapped, is
lerped plainly. `controls` and `capsized` are **held**, not averaged: a rudder
command of −1 averaged with +1 would display a helm position that never existed.

`web/src/ui/Timeline.tsx` — play, pause, continuous scrub (`step="any"`), frame
step both ways, five playback speeds and reset to episode start. It owns a
`requestAnimationFrame` loop that exists only while a replay is playing and
advances a number; the physics loop in `useSimulation.ts` remains the only one
that can reach the core.

### 9.5 — Export and import (P-group B)

`web/src/sim/episodeIo.ts` — `jsonBlob`, `binaryBlob`, `download`,
`episodeFilename`, `importEpisode`. The binary codec and the version check are
the core's; this file decides the form by the `SGEP` magic so a mistyped
extension is not an error.

`web/src/ui/RecordControls.tsx` — record/stop with a live frame counter, the
four logging rates (5/10/20/50 Hz), Replay, Save JSON, Save binary and
Load episode… through a hidden `<input type="file">`, which is what the E2E
suite drives with `setInputFiles` instead of a dialog.

### 9.6 — Golden scenario regression tests (P-group C)

- `crates/sailgym-physics/tests/golden/script.rs` — the six fixed 30 s control
  scripts and the runner, `#[path]`-included by **both** consumers so there is
  one definition. The scripts are in the test tree, not in the scenarios
  (brief §32), and cues are indexed by **step**, never by a float compared with
  an accumulated clock.
- `crates/sailgym-bench/src/bin/gen_golden.rs` — writes the files; refuses on a
  dirty `crates/sailgym-physics/src`.
- `crates/sailgym-physics/tests/golden/*.json` — six files, 151 samples each
  (30 s at 5 Hz plus `t = 0`), 305 kB in total.
- `crates/sailgym-physics/tests/regression.rs` — six tests, one per scenario.
  Tolerance `1e-9` on positions and angles, `1e-10` on `u`, `v`, `r`, `p` and
  `β̇`; measured worst |Δ| on this build is **0** at every sample.

### 9.7 — Determinism and replay E2E (P-group C)

- `invariants::deterministic_replay` — two `beam_reach_capsize` and two `gybe`
  episodes, 30 s each at 20 Hz under an identical step-indexed control script,
  compared over all 38 scalars of all ~600 frames **bit for bit**, and asserted
  to have actually loaded the rig and exercised the release command. The
  invariant suite is now **21 tests**.
- `web/tests/e2e/determinism.spec.ts` — the existing spec plus *"two recordings
  of the same scripted episode are frame-identical"*: `beam_reach_capsize`,
  paused, 60 single-steps driven by real key presses across three steering
  phases, recorded at 50 Hz, twice. Compared at 0 ULP with `Object.is`, and the
  two documents are asserted identical apart from `created_utc`.

---

## 2. Deviations from the PRD, and why

### 2.1 The page default changed, and two older specs had to name their wind

brief §32 makes `free_sail` "the default on load" and task 9.2's
`free_sail_is_default` asserts it, so a bare URL now loads the `free_sail`
document instead of `Sim::new("{}")`'s `WindConfig::default()`.

`free_sail.json` was authored to reproduce the **old `?scenario=free_sail`
fixture exactly** — uniform 5 m/s from 000°, at rest at the origin heading east,
sheet at 4.50 m — so every section 05–08 spec that drives `free_sail`
(`modes`, `debug`, `params`, `perf`, `sail`) is testing the same boat and none
of them changed. `sail.spec.ts` in particular builds its own comparison `Sim`
with a uniform 5 m/s northerly and compares `apparent_wind_body` exactly; that
pins `free_sail`'s wind and is why the default could not simply be made gusty.

Two specs used the *bare* page and depended on what the bare page used to be:

| Spec | Was | Now | Why |
|---|---|---|---|
| `wind.spec.ts` "a westerly reads 270 degrees" | `gotoApp(page)` — the default bearing was 270 because `WindConfig::default()` said so | `gotoApp(page, { scenario: 'close_hauled' })` | `free_sail` is a northerly; `close_hauled` is the shipped westerly |
| `controls.spec.ts` "Space eases the mainsheet" | `gotoApp(page)` — default sheet was `l_sheet_min` | `gotoApp(page, { scenario: 'close_hauled' })` | `free_sail` starts at `l_sheet_max`, where easing is a no-op *by construction* |

Neither assertion was weakened; both tests now say which wind and which sheet
they mean, which they arguably should have all along. Both files are section
02/03 property and the edits are recorded here.

The five remaining browser fixtures — `coast`, `sheet`, `capsize`, `knockdown`,
`fast` — are **kept**, renamed in the source to `LEGACY_FIXTURES` and documented
as test initial conditions that brief §32 does not ask the product to ship.
Section 08's handoff §10.4 asked for the `fast` R6 coverage not to be lost; it
is not. `free_sail` was removed from that table because it is now a real
scenario.

### 2.2 `Sim::reset` keeps the parameters; `Sim::restart` is new

Making the clock's Reset go through the scenario document broke two section 08
specs, and the breakage was real rather than incidental:

- `params.spec.ts` "a reset-required edit shows the badge, and the reset works"
  edits `sim.dt` to 0.0075, which `set_parameter` reports as reset-required
  (F8.2) — and the reset then restored `dt = 0.005` from the scenario. The edit
  had become unreachable.

So the two operations are now distinct:

```rust
Simulation::load_scenario(&sc)     // parameters + state + seed + wind + controls
Simulation::restart_scenario(&sc)  // everything except the parameters
Sim::reset(json)                   // load; remembers the document
Sim::restart()                     // restart from the remembered document
```

`useSimulation`'s clock sink calls `restart()`; the scenario picker calls
`reset(document)`. `Sim` keeps the reset document as **text**, not as a parsed
`Scenario`, because the browser's ad-hoc fixtures carry a full thirteen-field F3
state and `InitialState` — being the human-facing form (F1) — cannot express
`v`, `r`, `p`, `β̇` or `δr`. `knockdown` starts with `p = 8 rad/s`, and it has to
still do so after `R`.

### 2.3 `crates/sailgym-physics/build.rs` is new

R7 asks golden files to record "the toolchain that produced them", and the only
honest source for the compiler version is the compiler. The build script emits
three `cargo:rustc-env` values and adds no dependency; `ToolchainInfo::current()`
reads them with `env!`. `build.rs` lives at the package root and is not scanned
by `determinism::physics_sources` or `no_shortcuts::sources`, both of which walk
`src/` only.

Section 01's placeholder `toolchain_fingerprint()` — which returned
`option_env!("CARGO_PKG_RUST_VERSION")`, i.e. the string `"stable"` — is gone
with the placeholder test that used it.

### 2.4 `gen_golden --allow-dirty`, used exactly once

The PRD requires `gen_golden` to refuse to run with uncommitted changes under
`crates/sailgym-physics/src`, and it does — verified, exit code 2, the offending
files named:

```
gen_golden: refusing to run — crates/sailgym-physics/src has uncommitted changes:
 M crates/sailgym-physics/src/recording.rs
 M crates/sailgym-physics/src/scenario.rs
 M crates/sailgym-physics/src/simulation.rs
```

That refusal is unconditional for the one change that cannot satisfy it: the
change that *introduces* the golden files necessarily also introduces the
`scenario.rs` and `recording.rs` the runner needs. `--allow-dirty` was added,
spelled the way `cargo publish` spells it, prints a warning naming every dirty
file, and was used once to write the six committed goldens.

**This is recorded rather than hidden because it is exactly the affordance the
criterion exists to prevent being abused.** Two things make the result
trustworthy anyway. The files were written partway through the section and
**not** regenerated afterwards, including across the §2.2 split of
`load_scenario`/`restart_scenario`; `cargo test --test regression` nevertheless
passes with a worst |Δ| of **0** at every sample, which is both the evidence
that they describe the tree as committed and independent evidence that that
refactor changed no trajectory. The sensitivity check of §4 shows a 0.1 %
coefficient change breaks all six. If you would rather the flag did not exist,
deleting it costs four lines and the next regeneration has to follow a commit.

### 2.5 The golden control scripts live in a file both consumers include

The PRD says the scripts are "defined **in the test**, not in the scenario". A
second copy in `gen_golden.rs` would be the drift the whole exercise is against,
so they live in `crates/sailgym-physics/tests/golden/script.rs` — inside the
test tree, beside the files they produce, and `#[path]`-included by
`tests/regression.rs` and by `gen_golden.rs`. Files in a subdirectory of
`tests/` are not auto-discovered as test targets, so this adds no test binary.

`crates/sailgym-bench/Cargo.toml` gained `serde` and `serde_json` for it.

### 2.6 Scenario `name` is the id; there is no separate title

The PRD's struct has `name` and `description` and no third label, so `name`
carries the id (`beam_reach_capsize`) and is asserted equal to the file stem,
and `description` carries the prose. The picker shows the id and puts the
description in the control's tooltip.

### 2.7 The `Diagnostics` record does not follow a replay

See §8.1. The world view, the F8.3 snapshot readouts and the heel angle follow
the episode; the debug panel does not. The timeline says so on screen.

### 2.8 `data-playback` is on `record-controls`, not on the snapshot element

`[data-testid="snapshot"]` carries F8.3 values that specs read with
`Number(...)`, and `params.spec.ts` asserts every one of them is finite. A
string-valued attribute there fails that assertion, as it duly did. The
playback mode is published on `[data-testid="record-controls"]` instead and the
snapshot element stays numeric.

### 2.9 `Owns:` lists were widened (as in sections 05–08)

| File | Task | Why |
|---|---|---|
| `crates/sailgym-physics/build.rs` (new) | 9.3 | R7 needs the compiler's own version (§2.3) |
| `crates/sailgym-physics/src/simulation.rs` | 9.1, 9.3 | `load_scenario`, `restart_scenario`, `set_parameters` — the scenario has to reach the owner of state, parameters and wind |
| `crates/sailgym-physics/tests/golden/script.rs` (new) | 9.6 | the one definition of the control scripts (§2.5) |
| `crates/sailgym-bench/Cargo.toml` | 9.6 | `serde`, `serde_json` for `gen_golden` |
| `web/src/sim/useSimulation.ts` | 9.2, 9.4 | scenario resolution, `loadScenario`, and the reset→`restart` change |
| `web/src/App.tsx` | 9.2, 9.4, 9.5 | composition: mounting the picker, the record controls and the timeline, and switching the rendered snapshot between live and replay |
| `web/tests/e2e/wind.spec.ts`, `controls.spec.ts` | 9.2 | §2.1 — one line each, naming the wind they need |
| `web/tests/unit/replay.test.ts`, `scenarioTypes.test.ts` (new) | 9.1, 9.4 | named by the tasks' acceptance criteria, absent from the `Owns:` lists |
| `web/tests/e2e/scenarios.spec.ts`, `replay.spec.ts` (new) | 9.2, 9.4 | likewise |

9.1 is `P-group: S`; 9.2–9.7 were executed by the section agent rather than
delegated, so no parallel write conflict was possible. Flagged because F13.2 is
a rule about *reporting*, and this is the report. The task lists in
`docs/09-scenarios-replay.md` were **not** edited.

### 2.10 The E2E probe on `window.__sailgym`

Task 9.4 requires the frame-edit proof and task 9.5 requires export/import
"avoiding a real file dialog by exercising the underlying blob path directly".
`App.tsx` installs a documented `ReplayProbe` (declared in `sim/replay.ts`) with
seven methods. Nothing in the application reads it and it is removed when the
component unmounts. Import in the specs still goes through the page's own file
input via `setInputFiles`, so the real path is exercised; the probe supplies the
bytes and reads the frames back.

### 2.11 The scrub is continuous, and the frame buttons are how you land on a sample

The scrub was first given a step of a thousandth of the episode. Playwright
refuses `fill()` on a range input whose value is not a multiple of the step
("Malformed value"), and — more to the point — a quantised scrub makes the
interpolation unreachable. `step="any"` makes the playhead continuous, which is
what `sampleAt` is for; `◀ Frame` / `Frame ▶` snap exactly onto a recorded
sample, which is how `replay.spec.ts` gets an exact comparison.

---

## 3. Contradictions found in the normative documents

**None.** Section 07's two F6.7 contradictions (its §3.1 and §3.2) are unchanged
and unresolved; this section neither depends on them nor touches them.

Two places where the PRD needed a reading rather than a choice, both recorded
above: §2.2 (a "reset" to a scenario has to mean something precise once
parameters are live-editable) and §2.4 (`gen_golden`'s refusal cannot be
satisfied by the change that introduces the files it guards).

---

## 4. The regression suite is sensitive, and it was proven once

`resistance.y_v` was changed from `40.0` to `40.04` — **0.1 %** — and
`cargo test -p sailgym-physics --test regression` run:

```
failures:
    beam_reach_capsize
    close_hauled
    free_sail
    gybe
    sheet_release_recovery
    tack
test result: FAILED. 0 passed; 6 failed
```

**Six of six**, against the criterion's four. The first failure is at sample 1
(`t = 0.2 s`), field `x`, |Δ| = 1.2e-8 against a tolerance of 1e-9 — so the
suite catches a 0.1 % coefficient change within a fifth of a second of
simulated time.

The change was reverted (`y_v: 40.0`) and the suite re-run green before
anything else was done. **No coefficient was tuned** (brief §43, F13.5).

The toolchain-mismatch path was proven the same way: `tack.json`'s recorded
`rustc` was edited to `1.42.0`, and the test printed

```
skip: tack golden was recorded on a different toolchain.
  expected: rustc 1.42.0 (deadbeef 2019-01-01) / x86_64-unknown-linux-gnu / debug
  actual:   rustc 1.98.1 (48a229cea 2026-09-01) / x86_64-unknown-linux-gnu / debug
  Golden trajectories are only valid for the build that produced them (F9, F11 R7).
  Regenerate with `cargo run -p sailgym-bench --bin gen_golden` if this toolchain
  is the one you mean to hold.
```

and passed. The file was restored and the suite re-run green.

---

## 5. The golden files, and the control scripts that produced them

Committed on **`rustc 1.98.1 (48a229cea 2026-09-01)` /
`x86_64-unknown-linux-gnu` / `debug`**. Every file records that triple, and the
six tests skip with the message above on anything else.

30 s per scenario, sampled every 40 steps (5 Hz at `dt = 0.005`), 151 samples
each including `t = 0`.

The control scripts, verbatim — `(t, rudder_rate, sheet_rate, release)`, each
applied at the step `round(t/dt)`:

| Scenario | Script |
|---|---|
| `beam_reach_capsize` | (0, 0, −1, false) (5, 0, 0, false) (12, +0.4, 0, false) (18, 0, 0, false) |
| `sheet_release_recovery` | (0, 0, −1, false) (5, 0, 0, false) **(8, 0, 0, true)** (12, 0, 0, false) (20, 0, −0.5, false) |
| `close_hauled` | (0, 0, 0, false) (6, −0.3, 0, false) (10, 0, 0, false) (16, 0, −0.4, false) (20, 0, 0, false) |
| `tack` | (0, 0, 0, false) (4, −1.0, 0, false) (8, 0, 0, false) (14, +0.6, 0, false) (18, 0, 0, false) |
| `gybe` | (0, 0, 0, false) (5, +0.8, 0, false) (10, 0, 0, false) (15, 0, −1.0, false) (20, 0, +1.0, false) |
| `free_sail` | (0, 0, −1, false) (6, 0, 0, false) (10, +0.5, 0, false) (15, 0, 0, false) (20, 0, 0, true) (24, 0, 0, false) |

The two beam-reach scripts are identical for the first eight seconds and then
differ in one thing: whether the human releases the sheet. Their final states:

| Scenario | x | y | ψ | heel | u | β |
|---|---|---|---|---|---|---|
| `beam_reach_capsize` | 6.12 m | −7.31 m | −0.533 | **86.9°** | 0.294 m/s | 0.002 |
| `sheet_release_recovery` | 31.42 m | −17.65 m | −0.565 | **53.9°** | 1.243 m/s | 0.013 |

Same seed, same parameters, same initial state, same wind — different human.
That is brief §46 in two committed files.

| File | Samples | Bytes |
|---|---|---|
| `beam_reach_capsize.json` | 151 | 52 030 |
| `close_hauled.json` | 151 | 52 419 |
| `free_sail.json` | 151 | 52 188 |
| `gybe.json` | 151 | 52 832 |
| `sheet_release_recovery.json` | 151 | 50 828 |
| `tack.json` | 151 | 51 881 |
| **total** | **906** | **312 178** |

---

## 6. Validation evidence

`bash scripts/check.sh`, full chain, terminating in `check: all steps passed`
and **exiting 0**.

`pwsh` is not installed on this host, so section acceptance criterion 1's
literal `pwsh scripts/check.ps1` could not be executed and the Linux equivalent
was run instead. `check.ps1` was not edited this section.

### 6.1 Gate run

| Step | Result |
|---|---|
| 1 `cargo fmt --check` | passed |
| 2 `cargo clippy --all-targets -- -D warnings` | passed |
| 3 `cargo test -p sailgym-physics` | **198** lib + 1 boom + 1 convergence + 8 determinism + 21 invariants + 5 no_shortcuts + **6 regression** + 5 wind, 0 failed |
| 4 `--test invariants --test no_shortcuts` | **21 passed**, **5 passed**, 0 failed |
| 5 `--test regression` | **6 passed** — the placeholder is gone |
| 6 `wasm-pack build` | passed |
| 7 `pnpm --dir web typecheck` | passed |
| 8 `pnpm --dir web test:e2e` | **222 passed**, Chromium / Firefox / Edge, 5.9 min |

Also, outside the gate:

- `pnpm --dir web test:unit` — **97 passed** in 14 files (was 82 in 12).
- `wasm-pack test --headless --chrome crates/sailgym-wasm` — **7 passed**, in a
  real browser. As in section 08 the runner was pointed at Playwright's
  Chromium with a throwaway `crates/sailgym-wasm/webdriver.json`, **not**
  committed because it holds an absolute host path.
- `cargo run -p sailgym-bench --bin gen_golden` (no flag) — **exit 2**, refusal
  message as quoted in §2.4.

The lib count rose from section 08's 177 to 198: `scenario` contributes 12 and
`recording` 9.

Three gate-adjacent failures were found and fixed at the cause during
development, all in this section's own work:

| Failure | Cause | Fix |
|---|---|---|
| `params.spec.ts` ×2 (`dt` reverting, NaN in the snapshot) | the scenario reset re-applied parameters; and `data-playback` made a snapshot attribute non-numeric | §2.2 and §2.8 |
| `replay.spec.ts` ×3, `locator.fill: Malformed value` | the scrub's `step` quantised the playhead | §2.11 |
| `scenarios.spec.ts` ×2 (`testid is NaN`, `psi ≈ −0.003`) | the finite-check did not skip the two non-numeric attributes; the initial-condition assertion read a page that had been sailing since load | the spec skips `testid`/`capsized`, and pauses and resets before reading |

**Nothing in sections 01–08's specs failed once §2.1's two one-line edits were
made.**

### 6.2 Task acceptance criteria

**9.1** — `cargo test -p sailgym-physics scenario::` → **12 passed**:

| Test | Result |
|---|---|
| `round_trip` | pass — serialise → deserialise → serialise **byte-identical** for all six, and equal by value |
| `boom_conversion` | pass — `boom_deg_to_port: 30` gives `β = −π/6 = −0.5236` rad, and the mirror case |
| `heading_conversion` | pass — six hand-computed headings: 90°→0, 0°→π/2, 180°→−π/2, 270°→π, 45°→π/4, 315°→3π/4 |
| `angle_round_trip` | pass — **added**; `from_boat_state` is the inverse of `to_boat_state` over four cases |
| `override_determinism` | pass — five overrides inserted in two orders; equal by value, **by bits**, and by serialised string |
| `unknown_override_rejected` | pass — `Err(UnknownOverride)` from `to_parameters`, from `validate` and from `load` |
| `validate_rejects_bad` | pass — negative `sheet_length` → `OutOfRange`, absent `seed` → `MissingField`, `schema_version: 2` → `UnsupportedSchemaVersion`, and all three asserted pairwise distinct |
| `shipped::exactly_six_shipped` | pass — **added**; the list is exactly brief §32's, and each id is the document's own `name` |
| `shipped::all_load_validate_and_run_finite` | pass — 60 s × 6 under neutral controls, every state finite at all 12 000 steps |
| `shipped::recovery_matches_capsize_setup` | pass — field by field, plus the resolved catalogues as strings |
| `shipped::free_sail_is_default` | pass — and it reads `web/src/sim/scenarioTypes.ts` and asserts the browser's constant agrees |
| `shipped::no_scripted_outcomes` | pass — key allowlist plus the five-word scan |

`pnpm --dir web test:unit scenarioTypes` → **8 passed**: six struct pairs
compared in both directions and in order, the schema versions and the default id
mirrored, and the six documents checked on disk for the §32 properties.

**9.2** — `web/tests/e2e/scenarios.spec.ts`, 6 tests × 3 browsers = **18
passed**:

| Test | Result |
|---|---|
| the picker offers exactly the six, `free_sail` default | pass — `data-count=6`, the option values equal brief §32's list |
| each of the six loads and reaches a finite state | pass — switched, restarted, run past `t = 4 s` at 4×, every F8.3 value finite |
| switching takes under 500 ms | pass — measured **in-page** from the `change` event to the DOM: **2.4–7.2 ms** over nine switches, on Chromium and Edge alike |
| a scenario carries its own wind, initial condition and camera | pass — `beam_reach_capsize` ψ = 0, `l_sheet` = 0.90, wind 7 m/s from 000°, camera follow; `close_hauled` `l_sheet` = 2.00, 3.5 m/s from 270°, camera northUp |
| reset replays the chosen scenario | pass — `tack` comes back at `u = 1.8`, `β > 0.5`, ψ = π/4 |
| the URL keeps the scenario across a reload | pass |

**9.3** — `cargo test -p sailgym-physics recording::` → **9 passed**:

| Test | Result |
|---|---|
| `log_rate_respected` | pass — 60 s at 20 Hz gives **1201** frames (1200 ± 1), and consecutive samples are one interval apart |
| `recording_does_not_perturb` | pass — 60 s with recording and 60 s without end on **bit-identical** states across all thirteen fields, with > 1000 frames logged and the boat demonstrably moved |
| `binary_round_trip` | pass — every one of 38 scalars of every frame bit-identical; a tampered version byte is refused with `UnsupportedSchemaVersion` |
| `json_round_trip` | pass — same, bit for bit, and re-encoding gives the identical string |
| `header_captures_toolchain` | pass — `rustc` non-empty and containing `rustc`, `target` non-empty, `profile` one of `debug`/`release` |
| `reward_placeholder_zero` | pass — every frame's `reward` is exactly `0.0` by bits |
| `no_wall_clock_in_physics` | pass — `observe`'s body mentions no time call and not `created_utc`; `start` takes the header rather than producing one; the file uses no clock type. The crate-wide guard `determinism::no_wall_clock` also passes |
| `frame_layout_is_the_declared_order` | pass — **added**; each field lands at its documented offset and `from_array` inverts `to_array` |
| `iso8601_is_a_pure_function_of_its_argument` | pass — **added**; six known instants including a leap day, and the same input twice |

**9.4** — `pnpm --dir web test:unit replay` → **7 passed**:

| Test | Result |
|---|---|
| exact at a frame time | pass — all three frames returned unchanged; out-of-range requests clamp rather than extrapolate |
| linear between frames | pass — hand-computed midpoint and quarter point for `x`, `phi`, the twelve forces, the wind and the tension |
| the `±π` wrap | pass — 100 samples across a `+3.0916 → −3.0916` step: **no interpolated step exceeds the true 0.1 rad delta**, the total path is 0.1 rad (not 2π − 0.1), every sample stays inside `±π`, and the midpoint is the branch cut itself |
| index and clamp | pass, including `frameAt` returning the stored object by identity |
| empty episode | pass — throws rather than producing an unusable player |
| `snapshotFromFrame` | pass — the F8.3 index map |
| `wrapPi` | pass |

`web/tests/e2e/replay.spec.ts`, 6 tests × 3 browsers = **18 passed**:

| Test | Result |
|---|---|
| record 10.5 s, replay, `t = 5 s` matches the recorded frame | pass — > 200 frames; `x`, `y`, `ψ` and `φ` all within **1e-6** of the stored frame, on an episode asserted to have moved > 1 m and heeled > 0.05 rad |
| shows stored states, not recomputed ones | pass — the frame at ≈ 8 s is edited in memory by +1234 m; scrubbing to 2 s (which itself matches its stored frame to 1e-9) and back reads the **edited** value to 1e-9 |
| reset to episode start | pass — index 0, time = `startTime`, and the render equals frame 0 to 1e-12 |
| replay with the physics clock paused | pass — the playhead advances > 0.5 s while `clock-time`'s `data-sim-time` does not move at all and the clock stays paused |
| export → re-import, both formats | pass — the JSON validates field for field against `EpisodeHeader`/`EpisodeFrame`, `reward` is 0 in every frame, and re-importing the **binary** and then the **JSON** through the page's file input reproduces the byte-identical document; the re-imported episode replays and four sampled frames match |
| another `schema_version` is rejected | pass — `record-error` reads *"schema_version 7 is not supported; this build reads 1"*, the held episode is untouched and the page still replays |

**9.5** — covered by the last two rows above. Export is exercised through
`episode_to_binary` and `JSON.stringify`; `download()` is separated from the
blob builders precisely so the encode path can be tested without a dialog.

**9.6** — `cargo test -p sailgym-physics --test regression` → **6 passed**,
worst |Δ| **0** at every sample of every scenario. Sensitivity and the R7 skip:
§4. `gen_golden`'s refusal: §2.4.

**9.7** — `cargo test -p sailgym-physics --test invariants` → **21 passed**.
`pnpm --dir web test:e2e determinism` → **6 passed** (2 × 3 browsers).

### 6.3 Section acceptance criteria

| # | Criterion | Status |
|---|---|---|
| 1 | `pwsh scripts/check.ps1` exits 0, including the regression step | **Passed (Linux equivalent)** — `check: all steps passed`, exit 0; step 5 is now six real tests. `pwsh` absent on this host |
| 2 | All six brief §32 scenarios ship, load, and script no outcomes | **Passed** — `exactly_six_shipped`, `all_load_validate_and_run_finite`, `no_scripted_outcomes`, and the browser half in `scenarios.spec.ts` and `scenarioTypes.test.ts` |
| 3 | `sheet_release_recovery` is physically identical to `beam_reach_capsize` | **Passed** — field by field in Rust and on disk in TypeScript; §5 shows the two golden trajectories diverging from the same start |
| 4 | Recording does not perturb the simulation | **Passed** — `recording_does_not_perturb`, bit-identical over all thirteen fields after 60 s |
| 5 | Replay consumes stored data, proven by the frame-edit test | **Passed** — `replay.spec.ts`, on all three browsers |
| 6 | Six golden regressions pass, proven sensitive to a 0.1 % change | **Passed** — six pass; a 0.1 % `y_v` change fails **all six**, §4 |
| 7 | R7 handled: toolchain mismatch skips with a clear message | **Passed** — §4's quoted skip, and §5 states the platform the goldens came from |
| 8 | `reward` placeholder present and zero; schema versioned at 1 | **Passed** — `reward_placeholder_zero`, and `schema_version` checked before decoding in both formats and in both directions |
| 9 | `docs/progress/09-handoff.md` written | **Passed** — this file |

### 6.4 Exact tool versions

Measured on this host.

| Tool | Version |
|---|---|
| Host | `x86_64-unknown-linux-gnu`, Linux 7.0.0-30-generic |
| rustc | `1.98.1 (48a229cea 2026-09-01)`, commit `48a229ceaefd4985c50990b14116b6d856af0985` |
| LLVM | `22.1.8` |
| cargo | `1.98.1 (797e8a9bc 2026-08-05)` |
| rustfmt | `1.9.0-stable (48a229ceae 2026-09-01)` |
| clippy | `0.1.98 (48a229ceae 2026-09-01)` |
| wasm-pack | `0.15.0` |
| wasm-bindgen (`Cargo.lock`) | `0.2.128` |
| js-sys (wasm dev-dependency) | `0.3.105` |
| serde / serde_json | `1` (workspace), `float_roundtrip` enabled |
| Node / pnpm | `v26.3.0` / `11.6.0` |
| TypeScript | `^7.0.2` |
| Vite / Vitest | `^7.3.6` / `^5.0.1` |
| React / React DOM | `^19.3.0` / `^19.3.0` |
| zustand | `5.0.15` |
| @playwright/test | `^1.63.0` |
| @deck.gl/core, /layers, /react | `^9.4.0` |
| @loaders.gl/* (transitive) | `4.5.1` |
| @swc/core | `1.15.47`, pinned |
| Browsers under Playwright | chromium (bundled, GPU flags), msedge (system channel, GPU flags), firefox (bundled, software WebGL) |
| PowerShell | not installed |

**No new npm dependency was added this section.** `Cargo.lock` moved only
because `sailgym-bench` gained `serde` and `serde_json`, both already in the
workspace.

---

## 7. Parameters changed

**None.** No physical coefficient, no timestep and no F7 value was changed, and
nothing was tuned to make a scenario look better (brief §43, F13.5). The 0.1 %
`resistance.y_v` perturbation of §4 was a deliberate, reverted sensitivity
check.

The scenario documents contain numbers — `sheet_length`, wind speeds, headings —
but these are **initial conditions and environment**, not F7 coefficients. Two
of them coincide with F7 values on purpose and it is worth stating why:

- `free_sail.sheet_length = 4.5` is `l_sheet_max`, meaning "fully eased"; the
  rope is slack over the whole boom range (`ℓ(β) ≤ 4.48 m`) so the boom is free.
- `beam_reach_capsize.sheet_length = 0.9` is `l_sheet_min`, meaning "hauled hard
  in", which is the state section 07 measured the 6.93 m/s capsize threshold at.

Section 10's F7 grep ("no numeric literal from the table outside
`parameters.rs`") must be scoped to **source files**; scenario JSON is data and
has to be able to say where the boat starts.

The only new constants in the physics crate are the two schema versions
(`SCENARIO_SCHEMA_VERSION`, `EPISODE_SCHEMA_VERSION`, both 1), `FRAME_LEN = 38`
and the golden harness's `GOLDEN_DURATION_S = 30`, `GOLDEN_SAMPLE_HZ = 5`,
`TOL_POSITION = 1e-9`, `TOL_VELOCITY = 1e-10` — the last four live in the test
tree and are the PRD's own figures.

Recommendations from earlier sections remain recorded and **not acted on**:
`stability.gm` (section 07 §4), `l_sheet_min` (section 06 §4), and the
`FoilSection::area` tag (section 08 §5).

---

## 8. Risks

- **R7 — resolved here, as the PRD asks.** The committed goldens carry
  `rustc 1.98.1 (48a229cea 2026-09-01) / x86_64-unknown-linux-gnu / debug`; a
  mismatch skips with a message naming both toolchains, verified by editing a
  file and running the suite (§4). The determinism evidence in this repository
  is now same-build, same-platform **and says so in the files themselves**.
- **R1** — untouched. `k_sheet`, `c_sheet` and `dt` are unchanged. The two
  beam-reach goldens run the sheet up to full load and back to slack for 30 s
  without a blow-up, which is new standing evidence that the stiffness and the
  timestep are compatible.
- **R2 — closed by section 07 and unchanged.** `close_hauled` ships at 3.5 m/s
  as F11 anticipated. No `sailor_pos_b.y` exists and none was added.
  `docs/README.md`'s open item requiring human sign-off can be closed.
- **R3 (sign drift)** — the two human-facing angle conventions
  (`heading_deg`, `boom_deg_to_port`) are converted in one function with a
  hand-computed test for each and a round-trip test against the inverse. No new
  sign convention was introduced.
- **R4** — closed; the M1 placeholder remains deleted.
- **R5** — unchanged. The timeline and the record controls are HTML, not SVG;
  the debug SVG count is still 45 in the overlay and 135 on the page, and the
  `perf.spec.ts` figures are unchanged (Chromium Sail Mode p50 16.70 / p95
  16.80 ms).
- **R6** — unchanged and still surfaced; the `fast` fixture and its test
  survive (§2.1).

### 8.1 Not a risk, but a known limitation: the debug panel does not replay

An `EpisodeFrame` carries the F3 state, the four component forces, the four
moments, the sheet tension, the wind at the boat and the capsize flag — 38
scalars — and not the fifty-field brief §30 `Diagnostics` record. During replay:

- **follows the episode**: the boat drawing, the camera, the trajectory, the
  `[data-testid="snapshot"]` values, the heel angle, the Sail-Mode heading,
  rudder and sheet readouts;
- **does not**: the Debug-Mode diagnostics rows, the force overlay, the charts,
  and the Sail-Mode apparent-wind, boat-speed and capsize readouts, which keep
  describing the live simulation. It is paused while replaying, so they are
  frozen rather than moving independently.

The timeline says this on screen rather than leaving it to be discovered. The
clean fix — and the natural next step toward brief §45's episode inspector — is
a `Diagnostics`-shaped projection of a frame, published by the core so that no
derived quantity is recomputed in TypeScript (F8). That needs either a wider
frame or a `Simulation`-free diagnostics constructor, which is a design decision
rather than a patch, so it is recorded here.

---

## 9. What section 10 must know

1. **`?scenario=` resolves in three steps** (`sim/useSimulation.ts`): a shipped
   scenario id, then a legacy browser fixture (`coast`, `sheet`, `capsize`,
   `knockdown`, `fast`), then the default `free_sail`. The bare URL is
   `free_sail`.
2. **`Sim::reset` applies a scenario's parameters; `Sim::restart` does not**
   (§2.2). Anything that adds a reset path must choose deliberately between
   them.
3. **`Sim` remembers the reset document as text.** If you add a mutator that
   should survive `R`, it must not be part of that document.
4. **Recording makes `advance` step one at a time** and builds a `Diagnostics`
   on sampled steps only. If section 10 measures step throughput in the
   browser, measure it with recording off, or say which.
5. **`Recorder::due(t)` exists** so a caller can avoid building a diagnostics
   record it will throw away. Use it in any new recording loop.
6. **The golden files are build-sensitive by construction** (§5). If section 10
   changes any coefficient, integrator or summation order, `gen_golden` must be
   re-run **after** the change is committed, and the handoff must say so. If it
   changes nothing physical, the six regressions must stay green — they are the
   cheapest proof of that available.
7. **`build.rs` is new in `sailgym-physics`** and emits `SAILGYM_RUSTC`,
   `SAILGYM_TARGET`, `SAILGYM_PROFILE`. It runs on the host under `wasm-pack`
   too, so a wasm build records the host's rustc and the `wasm32` target.
8. **F7's "no numeric literal outside `parameters.rs`" grep must skip
   `scenarios/*.json`** (§7).
9. **222 e2e tests now run, ~5.9 min across three browsers**, and 97 unit tests.
   Budget for it.
10. **`window.__sailgym` is a test probe installed by `App.tsx`** (§2.10). It is
    not an API; if section 10 hardens the page, it may want to gate it on the
    dev build, but three specs depend on it.
11. **The episode schema is versioned at 1 and is the forward RL interface**
    (brief §45). Changing `EpisodeFrame`'s field order changes the binary
    layout; bump `EPISODE_SCHEMA_VERSION` if you do.

---

## 10. Remaining issues

1. **The debug panel does not follow a replay** (§8.1). The limitation is stated
   on screen and the fix is a design decision, not a patch.
2. **`gen_golden --allow-dirty` exists** (§2.4). Used once, deliberately,
   recorded; delete it if you would rather every regeneration followed a commit.
3. **The committed goldens are `debug`-profile only.** Running
   `cargo test --release -p sailgym-physics --test regression` skips all six,
   correctly and loudly. If a release-profile regression is wanted, it needs a
   second set of files and a way to choose between them.
4. **`scenarios/*.json` are not schema-validated outside Rust.** The browser
   never parses them directly — it reads `Sim.scenarios_json()` — but an
   external tool writing one has only `Scenario::load` to check it against.
5. **Section 08's open items stand**: the `FoilSection::area` tag (its §5), the
   `@loaders.gl` lockfile movement (its §6), the 1 734 kB single-chunk bundle
   (its §4), and the Sail-Mode frame-time percentile (its §2.6).
6. **Section 07's open items stand**: `stability.gm = 1.00 m` (its §4) and
   F6.7's internal contradiction (its §3.1). Note that changing `gm` invalidates
   all six golden files, as section 07 predicted.
7. **Section 06's open items stand**: `l_sheet_min = 0.90 m` pre-tension, and
   `dt = 0.01`'s lost margin.
