# Section 02 — Handoff (M1: state, integrator, clock, controls, SVG, determinism)

**Written per F13.6.** Read this before starting section 03
(`docs/v1/03-wind-field.md`). Conventions remain normative in
`docs/v1/00-foundations.md`; nothing below redefines them.

Status: **complete.** `pwsh scripts/check.ps1` exits 0 end to end (8/8).

---

## 1. What landed

### Task 2.1 — State, parameters, frames (P-group S, section agent)

- **`crates/sailgym-physics/src/state.rs`** — `BoatState` (13 fields, verbatim
  F3), `StateDot`, `Controls`, `STATE_LEN`, plus `to_array`/`from_array`
  (F8.3 order), `axpy`, `wrap_angles` (`phi` untouched), `is_finite`.
  Adds `STATE_FIELDS`, the single shared field-name list F8.3 asks for;
  `web/src/sim/snapshot.ts` mirrors it and `simulation::snapshot_layout`
  checks the two agree.
- **`crates/sailgym-physics/src/parameters.rs`** — the **entire** F7 catalogue,
  including the subsystems that do not exist yet. 63 tagged scalar fields,
  `Default` impls carrying the F7 values, serde on everything, `ilca7()`,
  `total_mass()`, `validate()`, `set_path`/`get_path`.
- **`crates/sailgym-physics/src/frames.rs`** — the only file in the repository
  containing a rotation matrix: `rot_z`, `body_to_world`, `world_to_body`,
  `rot_x`, `rot_x_inv`, `boom_dir`, `boom_dir_dbeta`, `rudder_chord`,
  `wrap_pi`, plus a module-local `Mat2`.

### Task 2.2 — RK2 integrator and the fixed-step driver (P-group A)

- **`dynamics.rs`** — `Load`, `Generalized` (with `add` verbatim from F6.4),
  the `ForceModel` trait, and `derivative`: F4.1 kinematics, F4.2 rigid-body
  dynamics with added mass, F4.3 actuator integration with the rate limits and
  self-centring. No `&mut` anywhere in `derivative` or `ForceModel`.
- **`integrator.rs`** — `Integrator { SemiImplicitEuler, Rk2Midpoint, Rk4 }`
  (`Rk2Midpoint` is `#[default]`) and `step`. Angle wrapping is applied once,
  after the step; actuator saturation is applied after **every** stage.

### Task 2.3 — Scaffold force model (P-group A, R4 debt)

- **`forces/mod.rs`**, **`forces/scaffold.rs`** — `ScaffoldForces`, header
  comment `//! SCAFFOLD — DELETED BY TASK 4.5. NOT PHYSICS.` verbatim.
  Constant thrust from `sim.scaffold_thrust`, linear drag on `u`, `v`, `r`, a
  yaw moment linear in `delta_r · u`, roll and boom at zero.

### Task 2.4 — Simulation object and the real WASM boundary (P-group B)

- **`simulation.rs`** — `Simulation` exactly as the PRD specifies, plus
  `initial_state`, `controls()`, `seed()`, `steps()` and `set_parameter`
  (needed by the wrapper; `seed` and `steps` are read, so no dead code).
- **`crates/sailgym-wasm/src/lib.rs`** — `new`, `reset`, `set_controls`,
  `advance`, `snapshot` (F8.3 layout), `set_parameter`, `parameters_json`,
  plus `dt()` and the pre-existing `version()`.
- **`web/src/sim/snapshot.ts`** — `SNAPSHOT_FIELDS`, `SnapshotField`,
  `Snapshot`, `readSnapshot`, `SNAPSHOT_LENGTH`.

### Task 2.5 — Simulation clock and React driver (P-group C)

- **`web/src/sim/clock.ts`** — `createClock(dt, sink)`, `SpeedMultiplier`,
  `SPEEDS`, `ClockState`, `MAX_STEPS_PER_TICK = 240`. Contains no
  animation-frame call of any kind.
- **`web/src/sim/useSimulation.ts`** — the one animation-frame loop in the app,
  key listeners, trajectory recording, and the imperative clock API.
- **`web/src/ui/ClockControls.tsx`** — pause/resume, reset, single-step and the
  four speeds, each with a stable `data-testid`.

### Task 2.6 — Keyboard controls and input mapping (P-group C)

- **`web/src/sim/keymap.ts`** — `Action`, `KEYMAP`, `normaliseKey`,
  `actionFor`, `EDGE_ACTIONS`.
- **`web/src/sim/controls.ts`** — `InputConfig`, `DEFAULT_INPUT`, `Controls`
  (TS mirror), `controlsFromInput` — pure, no DOM. Rudder self-centring is
  **not** implemented in TypeScript.

### Task 2.7 — SVG boat rendering and camera (P-group C)

- **`web/src/render/geometry.ts`** — normalised hull outline plus boom, sail,
  rudder and board geometry, all scaled by values read from
  `parameters_json()`.
- **`web/src/render/Camera.ts`** — `createCamera`, `worldToScreen`,
  `screenToWorld`, `worldTransform`, `boatTransform`, `trackedCentre`,
  `PIXELS_PER_METRE = 20`, zoom clamped to `[0.1, 20]`.
- **`web/src/render/BoatSvg.tsx`**, **`Trajectory.tsx`** — the world view, grid,
  boat, rig, trajectory, wheel zoom and middle-/Shift-drag pan. **Left-drag is
  deliberately unbound**, reserved for the mainsheet in section 06.

### Task 2.8 — Determinism, finiteness and clamping (P-group D)

- **`crates/sailgym-physics/tests/determinism.rs`** — all seven named tests.
- **`web/tests/e2e/determinism.spec.ts`** — the browser counterpart.

### Section-agent integration work (see §2.1 for the ownership note)

`web/src/App.tsx`, `web/package.json` (+`test:unit`, +`vitest`),
`web/vitest.config.ts`, `web/src/sim/loadWasm.ts`, `web/playwright.config.ts`,
`web/tests/e2e/fixtures.ts`, `crates/sailgym-wasm/tests/boundary.rs`, and the
four new E2E specs.

---

## 2. Deviations from the PRD, and why

### 2.1 Files written that no section-02 task owns

The PRD's `Owns:` lists cover the new modules but not the integration surface
they have to plug into. As section agent I took these on myself rather than
widening any subagent's scope (F13.2); every one is listed so section 03 knows
what moved:

| File | Why |
|---|---|
| `web/src/App.tsx` | Section AC 2 and 3 require a steerable boat, both camera modes and the clock controls in the browser. Nothing else can wire them together. |
| `web/package.json`, `web/vitest.config.ts` | Tasks 2.4–2.7 all specify `pnpm --dir web test:unit <name>` as an acceptance command. No unit runner existed; vitest 5 + a `test:unit` script is the minimum that makes those criteria executable. |
| `web/src/sim/loadWasm.ts` | Its hand-written `SimHandle` listed only the four section-01 methods. Rather than extend the duplicate, `SimHandle` is now `type SimHandle = Sim`, taken from the `wasm-pack`-generated declarations — one source of truth for the WASM surface (F8). |
| `web/playwright.config.ts` | Worker cap (`workers: CI ? 2 : 4`); see §2.6. |
| `web/tests/e2e/fixtures.ts` | Added `readSnapshot(page)`, the shared `data-*` snapshot reader every new spec uses. The section-01 harness is the right home for it. |
| `crates/sailgym-wasm/tests/boundary.rs` | Task 2.4's own acceptance criterion ("a `wasm-bindgen-test` asserts `snapshot().length === 13`") cannot be met without editing it; the section-01 assertions there described the placeholder `[counter]` snapshot and would otherwise fail. |
| `crates/sailgym-physics/src/forces.rs` | Deleted. Task 2.3 owns `forces/mod.rs`, and a crate cannot have both `forces.rs` and `forces/mod.rs`. The file was a one-line stub. |
| `web/tests/e2e/{clock,controls,render}.spec.ts` | Named in the acceptance criteria of tasks 2.5, 2.6 and 2.7 but missing from their `Owns:` lists. Treated as owned by the task whose criteria name them. |

### 2.2 F4.3's "clamp inside the derivative", reconciled with the `dt` vs `dt/2` test

F4.3 says both the rate clamp and the value clamp live inside `derivative`.
Task 2.2 then asserts that a *saturated* rudder command reaches the same
`delta_r` at `dt` and `dt/2` to 1e-9. **Those two cannot both hold as literally
written**, and I verified it rather than assuming:

A hard rate cut-off at the limit (`rate = 0` when `|δr| ≥ δr_max`) makes RK2
stall. Once the midpoint stage lands at or beyond the stop, `k2 = 0`, so the
full step makes no progress and the rudder freezes at `δr_max − h·Ṙ/2` — a
value that *depends on `dt`*, which is exactly what the test forbids.

What is implemented, and what makes both requirements true:

1. `dynamics::limit_rate` zeroes the rate only when the value is **strictly**
   outside the box and the rate pushes further out. That is the F4.3 clamp,
   inside the derivative, and it is what keeps a state that starts out of range
   (e.g. after `delta_r_max` is lowered live) from running away.
2. `integrator::stage` saturates `delta_r` and `l_sheet` into their boxes after
   **every** stage, not only after the finished step — which is what F4.3's
   sentence "so RK2 stages stay consistent … clamping only the integrated
   result breaks time-step convergence" is actually protecting.

Together the actuator lands on *exactly* its limit for any `dt`.
`integrator::clamp_in_derivative` passes at `1e-12`, and
`integrator::sheet_length_saturates_at_both_ends` asserts exact equality with
`l_sheet_min`/`l_sheet_max` at two timesteps.

**This is not a redefinition of F4.3** — it is the only implementation of F4.3
that also satisfies task 2.2. Recorded here for the human, per the escalation
rule; nothing in `00-foundations.md` was changed.

### 2.3 `wrap_pi` returns an in-range angle bit-identically

`(a + π) − π` is not the identity in `f64`. Without a fast path, a boat sitting
at rest on a heading of 0.6 rad has its heading perturbed by 1 ULP *every
step* — `integrator::zero_force_zero_motion` caught it on the first run.
`wrap_pi` now returns `a` unchanged when `a ∈ (−π, π]`. This changes no
convention; it removes a silent determinism leak.

### 2.4 `zero_force_zero_motion` excludes `t`

The criterion says "10 000 steps leave every field bit-identical". `t` is the
simulation clock and `ṫ = 1` by F3, so it cannot be bit-identical. The test
asserts bit-identity for indices 0–11 (named in the failure message) and
asserts separately that `t` advanced. This is a statement of what the
criterion can mean, not a weakening — every field the boat actually has is
checked bit-for-bit.

### 2.5 The browser determinism replay runs in step space

`determinism.spec.ts` replays through `page.keyboard` as the PRD requires, but
the clock is **paused** and advanced with `.` (single-step), so the number of
physics steps between key events is fixed by the script rather than by machine
speed. A wall-clock replay could not be a 0-ULP comparison on any real machine.
Key-state changes are given one frame to settle before the phase's first step.

### 2.6 Playwright worker cap (`workers: CI ? 2 : 4`)

Every page is now a live simulator — a WASM instance plus an animation-frame
loop — and at 2× or 4× it does two or four times the physics work per second.
57 specs across three browsers at Playwright's default worker count starved
this host, and it showed up as two different failures:

1. A Firefox page could not reach `data-ready` inside `gotoApp`'s 5 s window.
   **Verified:** reproducible only in the combined run; `--project=firefox`
   alone passed 18/18 in 16.6 s.
2. At 6 workers, `clock.spec.ts` "2× advances simulated time about twice as
   fast as 1×" failed once in five runs on Firefox, with the ratio below 1.7.
   The mechanism is `MAX_STEPS_PER_TICK`: a frame stall longer than ~600 ms
   clips the 2× page's step burst and the discarded wall time never comes
   back. That is the cap doing its job — a starved tab really does lose
   simulated time — so the measurement was reporting machine health, not a
   clock defect.

The cap addresses the oversubscription. **Neither the 5 s readiness timeout nor
the `[1.7, 2.3]` ratio bounds were touched**, and `MAX_STEPS_PER_TICK` stays at
the PRD's 240. **Verified:** seven consecutive full-suite runs at 4 workers,
57/57 each, 26–30 s.

### 2.7 `camber_blend` is not in the F7 table

F5.3's `FoilParams` requires `camber_blend` (F5.2's `α_b`); the F7 table does
not list it. It is declared DEFERRED with a default of **0.2 rad** and a doc
comment saying why: with `alpha_camber = 0` the camber hook is inert, so the
value has no physical effect in v1; it is non-zero only so `tanh(α / α_b)` is
defined at `α = 0`. **This is not a tuned coefficient** — no scenario, test or
visual depends on it. Section 04 owns `foil.rs` and should keep it inert.

### 2.8 Dotted-path scheme for `set_path` / `get_path`

F7 names the fields but not the paths. The scheme chosen — recorded here so
section 08's parameter panel and section 09's scenarios agree with it:

- `"<group>.<field>"`, matching F7's own prefixes: `sail.area`, `board.cd0`,
  `rudder.delta_r_max`, `stability.gm`, `sim.dt`.
- The foil coefficient block is a `FoilSection`, `#[serde(flatten)]`ed into its
  owner, so the JSON keys and the dotted paths are the same shape.
- Vector parameters take a component suffix: `hull.sailor_pos_b.z`.
  F7's `board_pos_b`, `rudder_pos_b`, `mast_pos_b` and `block_pos_b` live
  inside their group, so they are `board.pos_b.*`, `rudder.pos_b.*`,
  `sail.mast_pos_b.*`, `sheet.block_pos_b.*`.
- `rudder.delta_r_self_centre` is a flag; `set_path` treats non-zero as `true`,
  `get_path` returns `1.0`/`0.0`.
- `sim.integrator` is **not** addressable by `set_path` (it is not a scalar).
- `set_path` returns `Ok(true)` — reset required — for `sim.*` only. Changing
  `dt` or the integrator changes the meaning of every recorded trajectory
  (F9, brief §33); everything else is safe to edit while running.

### 2.9 `parameters_json()` returns a JSON **string**

F8.2 types it `Result<JsValue, JsValue>`, which a string satisfies. Returning a
structured object would need `js-sys` or `serde-wasm-bindgen` in
`crates/sailgym-wasm/Cargo.toml`, a file no section-02 task owns. The caller
does one `JSON.parse`. Section 08 may want to revisit this.

### 2.10 `Vec3` serde lives in `parameters.rs`

`vec.rs` is section 01's and derives no serde traits. `parameters.rs` carries a
local `vec3_serde` module used via `#[serde(with = ...)]` rather than reaching
into a file this section does not own. If section 09 needs `Vec3` serialised
elsewhere, deriving serde on `vec.rs` and deleting the helper is the clean fix.

### 2.11 `diagnostics.rs`'s stub comment is wrong

`crates/sailgym-physics/src/diagnostics.rs` says "Implemented in section 02".
No section-02 task owns it and `diagnostics()` is not in task 2.4's method
list; F8.2 diagnostics are section 08's work. **I did not edit the file** — no
task owns it. Whoever owns it next should correct the comment to section 08.

**No physical coefficient was tuned.** Every F7 value is the tabulated one. The
only new numbers in the physics crate are (a) `sail.camber_blend` (§2.7),
(b) `sim.scaffold_thrust = 120 N`, DEFERRED and deleted by task 4.5, and
(c) four non-physical constants inside `forces/scaffold.rs` itself, chosen only
so the M1 app is steerable and bounded, and deleted with it.

**No contradiction between the brief and `00-foundations.md` was found.** §2.2
is a PRD-vs-PRD tension inside section 02, resolved without changing
foundations.

---

## 3. Validation evidence

Every command below was run on this host and its exit status observed.

### Section acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | `pwsh scripts/check.ps1` exits 0 | **pass** — 8/8, `check: all steps passed`. Step 3: 53 lib + 7 determinism tests. Step 8: **57 passed** across chromium, firefox, msedge, 26 s |
| 2 | Steerable boat, trajectory, both camera modes, all four speeds | **pass** — `controls.spec.ts` (D/A/arrows change `deltaR` and `psi`), `render.spec.ts` (hull transform changes; trajectory has > 1 point; northUp/follow), `clock.spec.ts` "all four speeds are selectable and keep the simulator running" |
| 3 | Pause, resume, reset, single-step per brief §22 | **pass** — `clock.spec.ts` (4 tests) and `controls.spec.ts` "P pauses and R resets" |
| 4 | `cargo test -p sailgym-physics --test determinism` green, all 7 | **pass** — 7 passed, 0.21 s |
| 5 | `phi` confirmed unwrapped at 4.0 rad | **pass** — `state::tests::wrap_angles_leaves_phi_untouched` sets `phi = 4.0` and asserts `wrap_angles()` leaves it at 4.0 |
| 6 | Scaffold confined, grep count holds | **pass** — `grep -rn "scaffold" crates/sailgym-physics/src \| wc -l` prints **11** (limit 12) |
| 7 | Sign audit — four named assertions exist and pass | **pass**, all four: `frames::tests::boom_dir_sign`, `frames::tests::rot_x_horizontal_vector`, `controls.spec.ts` "D steers the bow to starboard" (`deltaR > 0` **and** `psi < 0`), `integrator::tests::zero_force_zero_motion` |
| 8 | Handoff listing every F7 parameter left unused | **pass** — §4 below |

### Task acceptance criteria

| Task | Criterion | Result |
|---|---|---|
| 2.1 | `state::` round-trip identity, 100 randomised cases, exact `==` | **pass** |
| 2.1 | `frames::` — `rot_round_trip`, `boom_dir_sign`, `boom_dir_derivative`, `rot_x_horizontal_vector`, `wrap_pi_boundaries` | **pass**, all five present by name (+ 3 more) |
| 2.1 | `parameters::` — `ilca7().validate()` is `Ok`; ≥ 10 path round-trips; unknown path is `Err` | **pass** — 14 paths, 6 rejected paths |
| 2.1 | Every parameter field tagged, enforced by a source-reading test | **pass** — `every_parameter_field_is_tagged`: 63 leaf fields, 63 tagged |
| 2.2 | `rk2_matches_analytic_decay`, ratio in [3.6, 4.4] | **pass** |
| 2.2 | `clamp_in_derivative`, 1e-9 | **pass** — agreement is exact; both land on `delta_r_max` within 1e-12 |
| 2.2 | `zero_force_zero_motion`, 10 000 steps | **pass** — see §2.4 |
| 2.2 | `advance_n_equals_n_advance_1` | **pass** (integrator level; `Simulation` level in `determinism.rs`) |
| 2.2 | `derivative`/`ForceModel` take `&BoatState`, no `&mut` | **pass** by inspection |
| 2.3 | File header comment verbatim | **pass** — line 1 of `forces/scaffold.rs` |
| 2.3 | `scaffold_finite` — 20 episodes × 60 s finite | **pass** |
| 2.3 | `scaffold_bounded` — `\|u\| < 20`, `\|r\| < 5` | **pass** |
| 2.3 | grep count ≤ 12 | **pass** — 11 |
| 2.4 | `simulation::snapshot_layout` | **pass** — asserts the layout by name *and* reads `web/src/sim/snapshot.ts` and compares the two lists |
| 2.4 | `pnpm --dir web test:unit snapshot` | **pass** — 3 tests: length 13, index 3 → `phi`, index 10 → `deltaR` |
| 2.4 | `wasm-bindgen-test`: `snapshot().length === 13` | **pass — in a real browser**: `wasm-pack test --headless --chrome crates/sailgym-wasm` → 6 passed |
| 2.4 | `set_parameter("sail.area", 8.0)` reflected in `parameters_json()` | **pass** — asserted in the same browser run |
| 2.5 | `test:unit clock` — 200 ± 1 at 1×, 800 ± 1 at 4×, stall ≤ cap, singleStep semantics | **pass**, 10 tests |
| 2.5 | `clock.spec.ts` — pause freezes `t`; single-step = exactly `dt` (1e-9); 2× ratio in [1.7, 2.3] | **pass** |
| 2.5 | rAF only in `useSimulation.ts`; none in `clock.ts` | **pass** — `grep -rln "requestAnimationFrame" web/src/sim/` prints only `useSimulation.ts`; `clock.ts` matches zero times |
| 2.6 | `test:unit controls` — D > 0, A < 0, both → 0, Space → release | **pass**, 8 tests |
| 2.6 | `controls.spec.ts` — D drives `deltaR` positive and `psi` negative; A mirrors | **pass** on all three browsers |
| 2.6 | `grep -rn "0\.698\|delta_r_max\|self_centre" web/src/` | **pass** for source — zero matches in any `.ts`/`.tsx`. See §5 for the generated-WASM caveat |
| 2.7 | `test:unit camera` — round-trip 1e-9, 100 points, both modes, zoom [0.1, 20] | **pass** |
| 2.7 | `render.spec.ts` — hull exists, transform changes, northUp grid unrotated | **pass** |
| 2.7 | Hull length = `loa × pixelsPerMetre` within 1 px at zoom 1 | **pass** — `camera.test.ts` |
| 2.7 | `grep -rn "4\.23\|1\.37" web/src/` | **pass** — zero matches |
| 2.8 | `--test determinism` exits 0, 7 tests by name | **pass** |
| 2.8 | `pnpm --dir web test:e2e determinism` | **pass** — 3/3 (one per browser) |
| 2.8 | The grep tests fail if a violation is introduced | **pass — proven live**, see below |

### The grep guards, proven live (task 2.8)

Required to be verified by hand, then reverted.

1. Appended to `crates/sailgym-physics/src/diagnostics.rs`:
   `fn probe() -> std::time::Instant { std::time::Instant::now() }` and
   `type Probe = std::collections::HashMap<u8, u8>;`.
2. `cargo test -p sailgym-physics --test determinism` → **`no_wall_clock`
   FAILED**, **`no_hash_iteration` FAILED**, 5 passed, 2 failed. The failure
   messages named `diagnostics.rs:5` (`Instant`) and `diagnostics.rs:9`
   (`HashMap`).
3. Reverted the file (it is back to its one-line stub; `cat` confirmed).
4. Re-ran: **7 passed**, exit 0.

### Unit tests

`pnpm --dir web test:unit` → **4 files, 24 tests, all passing**, 0.2 s.
Not part of the F12 gate — see §5.

---

## 4. F7 parameters section 02 left **unused** (section AC 8)

Section 04 can assume none of the following is read by any code path today, in
Rust or TypeScript. Nothing is missing; nothing has been pre-consumed.

**Wholly unused groups**

- `resistance.*` — all eight (`x_u`, `x_uu`, `y_v`, `y_vv`, `n_r`, `n_rr`,
  `k_p`, `k_pp`). The scaffold has its own non-physical drag constants.
- `stability.*` — all six (`gm`, `phi_peak`, `gz_max`, `phi_vanish`,
  `phi_capsize`, `t_capsize`).
- `board.*` — all nine section coefficients; `board.pos_b.y`/`.z` unused
  (`.x` is read by the renderer only).

**Partially used groups**

| Group | Used at M1 | Unused at M1 |
|---|---|---|
| `hull` | `m_hull`, `m_sailor` (via `total_mass`); `loa`, `beam` by the renderer | `lwl`, `sailor_pos_b` (all three components) |
| `inertia` | `i_zz`, `i_xx`, `a_x`, `a_y`, `a_psi`, `a_phi` — all six | — |
| `sail` | `i_boom`; `boom_length` and `mast_pos_b.x` by the renderer | `area`, `ar`, `alpha_stall`, `stall_blend`, `cn_max`, `cd0`, `oswald`, `alpha_camber`, `camber_blend`, `d_ce`, `z_ce`, `c_beta`, `beta_max`, `k_lim`, `c_lim`, `mast_pos_b.y`/`.z` |
| `rudder` | `delta_r_max`, `delta_r_rate_max`, `delta_r_return_rate`, `delta_r_self_centre`; `pos_b.x` by the renderer | `area`, `ar`, `alpha_stall`, `stall_blend`, `cn_max`, `cd0`, `oswald`, `alpha_camber`, `camber_blend`, `pos_b.y`/`.z` |
| `sheet` | `l_sheet_min`, `l_sheet_max`, `sheet_haul_rate`, `sheet_ease_rate`, `sheet_release_rate` | `k_sheet`, `c_sheet`, `d_sheet`, `z_boom`, `block_pos_b` |
| `sim` | `dt`, `integrator`, `scaffold_thrust` | — |

**Constants (F1):** `G`, `RHO_AIR` and `RHO_WATER` are **all three unused**.
Nothing in section 02 computes a hydrodynamic or aerodynamic force.

**`foil.rs`** still contains only `EPS_FLOW` (section 01, §2.1 of its handoff).
The F5 coefficient model does not exist yet.

---

## 5. Toolchain versions (R7)

Same host as section 01; `rustc` unchanged at 1.98.1, so golden files recorded
against section 01's fingerprint remain valid.

| Tool | Version |
|---|---|
| host triple | `x86_64-pc-windows-msvc` (Windows 11 Pro 26200) |
| rustc | **1.98.1** (48a229cea 2026-09-01) |
| cargo | 1.98.1 (797e8a9bc 2026-08-05) |
| rustfmt | 1.9.0-stable (48a229ceae 2026-09-01) |
| clippy | 0.1.98 (48a229ceae 2026-09-01) |
| wasm-pack | 0.15.0 |
| wasm-bindgen | 0.2.128 |
| serde / serde_json | 1.0.229 / 1.0.151 |
| node | v26.6.0 |
| pnpm | 12.4.2 |
| PowerShell | 7.6.5 |
| vite | 7.3.6 |
| **vitest** | **5.0.1** (new in section 02) |
| react / react-dom | 19.3.0 |
| typescript | 7.0.2 |
| @playwright/test | 1.63.0 |
| @vitejs/plugin-react | 5.2.0 |
| vite-plugin-wasm / vite-plugin-top-level-await | 3.6.0 / 1.6.0 |
| Chrome (wasm-pack test) | headless chrome via chromedriver |
| browsers under Playwright | chromium, firefox, msedge (system Edge channel) |

---

## 6. Risks and open items

**R4 — THE M1 SCAFFOLD NOW EXISTS AND IS SECTION 04's TO DELETE.**
`crates/sailgym-physics/src/forces/scaffold.rs` is **not physics**. Its four
drag/authority constants are arbitrary. **Task 4.5 deletes the file in its
entirety, together with `sim.scaffold_thrust` in `parameters.rs`**, and nothing
else deletes it. The whole blast radius is 11 grep hits:

```
forces/mod.rs        2   (pub mod scaffold; pub use scaffold::ScaffoldForces;)
forces/scaffold.rs   4   (header pointer, the thrust read, two test names)
parameters.rs        5   (doc comment ×2, field, Default value, path arm)
simulation.rs        0   (imports the re-export, so no lowercase match)
dynamics.rs          0
```

`Simulation::new` hard-wires `Box::new(ScaffoldForces)`; task 4.5's swap is
that one line plus the deletion.

**R3 — sign-convention drift: mitigated, and the guards are now live.** The
four section-AC-7 assertions all pass. **If any later section weakens or
deletes one of them, that is a defect.** `controls.spec.ts` "D steers the bow
to starboard" is the browser-level guard and asserts both `deltaR > 0` and
`psi < 0`.

**R1 — mainsheet stiffness vs. timestep: not yet exercised.** `k_sheet` is
catalogued at its F7 value and read by nothing. `l_sheet` integrates as a pure
actuator with rate limits, which is stiff-free. R1 fires in section 06.

**R2 — the boat may be too tender: untouched.** No sail force, no righting
moment, no roll (`K = 0` in the scaffold, so `phi` and `p` are constant). No
`sailor_pos_b.y` parameter was added; it still requires human sign-off.

**R7 — golden regression files: unchanged.** `regression.rs` still skips with
its message; `rustc` did not move.

**Open item — `pnpm --dir web test:unit` is not in the F12 gate.** F12 fixes
the gate at eight steps and I did not alter it. The unit tests are therefore
run by hand (and by any agent following the task acceptance criteria) but not
by `scripts/check.ps1` or by CI. Section 10 should decide whether F12 gains a
ninth step; that is a foundations change and needs the human.

**Open item — the acceptance grep for rudder literals hits a generated
binary.** `grep -rn "0\.698\|delta_r_max\|self_centre" web/src/` matches
`web/src/wasm/sailgym_wasm_bg.wasm`, because F12 step 6 puts the generated
package inside `web/src/` and the parameter path strings are baked into it. No
hand-written TypeScript matches. Section 10's greps should exclude
`web/src/wasm/` (which is gitignored) or restrict to `--include=*.ts
--include=*.tsx`.

**Not executed:** CI has still never run; the repository has no remote.

---

## 7. What section 03 must know

1. **The gate is still the contract**, and it is green. `pwsh
   scripts/check.ps1`. Add to `invariants.rs` and `regression.rs`; do not
   recreate them. Run `pnpm --dir web test:unit` too — it is not in the gate.
2. **`Simulation` owns `seed: u64` and exposes `seed()`.** Nothing consumes it
   yet. The wind field is the first consumer; `rng.rs` (PCG32, F9.2) is still
   an empty stub and is yours.
3. **`Sim::new(config_json)` already reads `"seed"` and an optional full
   `"parameters"` object; `Sim::reset(scenario_json)` reads `"seed"` and an
   optional full 13-field `"state"`.** Both ignore unknown keys, so adding
   `"wind"` is additive. A partial parameter override is **not** supported —
   `serde` wants the whole catalogue. Section 09 will want to fix that.
4. **`Sim` now exposes `new`, `reset`, `set_controls`, `advance`, `snapshot`,
   `set_parameter`, `parameters_json`, `dt`, `version`.** `diagnostics`,
   `sample_wind_grid`, `start_recording` and `stop_recording` are still
   unstubbed; add them when their section arrives. `sample_wind_grid` is yours.
5. **`web/src/sim/loadWasm.ts` types `SimHandle` as the generated `Sim`
   class.** New WASM methods appear in TypeScript automatically after gate step
   6; do not hand-mirror them.
6. **The animation-frame loop is in `web/src/sim/useSimulation.ts` and only
   there.** It calls `clock.tick(deltaMs)` once per frame and reads the
   snapshot. deck.gl (R5) must share that loop, not open a second one. The
   Vite-7 pin from section 01 §2.5 is still in force and is the constraint to
   re-check first.
7. **`data-testid` contracts now in use by specs:** `wasm-status`/`data-ready`
   (frozen since 1.3), `snapshot` (all thirteen F8.3 values plus `dt`, as
   `data-*`), `clock-pause`/`clock-reset`/`clock-step`/`clock-speed-{0.25,1,2,4}x`/
   `clock-time`, `camera-mode`/`camera-reset`, `world-view`/`world-grid`,
   `boat-hull`/`boat-boom`/`boat-sail`/`boat-rudder`/`boat-board`/`boat-mast`,
   `trajectory`. Renaming any of them breaks a spec.
8. **`readSnapshot(page)` in `web/tests/e2e/fixtures.ts`** is the shared way to
   read numeric state in a spec. Use it; do not re-implement it.
9. **Left mouse drag is unbound on the SVG**, reserved for the mainsheet in
   section 06 (brief §27). Pan is middle-button or Shift+drag. Do not bind
   left-drag for a wind-layer interaction.
10. **The camera defaults to `northUp`** and tracks the boat with a dead zone;
    `follow` pins the boat at the centre, bow up. `Camera.ts` is the only place
    the world→screen mapping exists; the wind layer must use `worldToScreen`
    /`worldTransform` rather than inventing a second projection.
11. **Playwright runs with `workers: CI ? 2 : 4`** (§2.6), because each page
    now runs a real simulation. If section 03 adds a WebGL context per page,
    expect to lower it again — and check host starvation before blaming the
    app or loosening an assertion.
12. **No numeric physical literal outside `constants.rs` and `parameters.rs`**
    still holds in the physics crate, and no ILCA dimension appears anywhere in
    `web/src/*.ts(x)`. The wind parameters belong in `parameters.rs` (or in the
    scenario schema), not in `wind.rs`.
