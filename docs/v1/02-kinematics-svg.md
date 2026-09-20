# Section 02 — State, Integrator, Clock, Controls, SVG Boat, Determinism (M1)

**Prerequisite reading:** `docs/v1/00-foundations.md` (all — F2, F3, F4, F9 especially),
`docs/v1/progress/01-handoff.md`, `docs/v1/brief.md` §5, §13, §21, §22, §25, §27, §28, §34.

## Goal

A boat that moves on screen, steered by the keyboard, driven by a **deliberately
fake** force model, integrated at a fixed timestep by RK2, with the full clock
control set, an SVG render, a camera, and — critically — a passing determinism
test. Everything except the force model is permanent.

## The scaffold rule (R4 — read this twice)

Task 2.3 implements `forces::scaffold`, a placeholder that is **not physics**. It
exists so the integrator, the WASM boundary, rendering, controls and the
determinism test can all be built and verified before any real force model
lands. It is deleted in its entirety by **task 4.5** of section 04.

The scaffold must therefore:
- live in exactly one file, `crates/sailgym-physics/src/forces/scaffold.rs`;
- be gated behind a `ScaffoldForces` implementation of the same `ForceModel`
  trait the real model will implement, so 4.5 is a swap, not a rewrite;
- carry a file-header comment `//! SCAFFOLD — DELETED BY TASK 4.5. NOT PHYSICS.`;
- never be referenced from `web/` except through the normal snapshot path.

Do not improve it. Do not tune it. Do not write tests that assert its behaviour
beyond finiteness and determinism.

---

## Tasks

### 2.1 — State, parameters, frames (contracts)
**P-group: S**
**Owns:** `crates/sailgym-physics/src/state.rs`, `parameters.rs`, `frames.rs`

`state.rs` — `BoatState`, `StateDot`, `Controls`, `STATE_LEN` **verbatim from F3**,
plus:

```rust
impl BoatState {
    pub fn to_array(&self) -> [f64; STATE_LEN];     // F8.3 order
    pub fn from_array(a: &[f64; STATE_LEN]) -> Self;
    pub fn axpy(&self, h: f64, d: &StateDot) -> Self;  // self + h*d, field-wise
    pub fn wrap_angles(&mut self);                   // psi, beta to (−π, π]; phi untouched
    pub fn is_finite(&self) -> bool;
}
```

`parameters.rs` — the **entire** F7 catalogue as nested structs with
`Default` impls carrying the F7 default values, serde `Serialize`/`Deserialize`,
and a doc comment on every field giving unit, tag (KNOWN/ASSUMED/TUNABLE/DEFERRED)
and the F7 note. Include the fields for subsystems not yet implemented — sail,
board, rudder, sheet, stability. Later sections consume them; none of them
re-declares a parameter.

```rust
pub struct BoatParameters {
    pub hull: HullParams, pub inertia: InertiaParams, pub resistance: ResistanceParams,
    pub sail: SailParams, pub board: FoilMountParams, pub rudder: RudderParams,
    pub sheet: SheetParams, pub stability: StabilityParams, pub sim: SimParams,
}
impl BoatParameters {
    pub fn ilca7() -> Self;                         // the F7 defaults
    pub fn total_mass(&self) -> f64;                // m_hull + m_sailor
    pub fn validate(&self) -> Result<(), ParamError>;
    /// Dotted path setter for live editing (F8.2 set_parameter).
    /// Returns Ok(true) if the change requires a simulation reset.
    pub fn set_path(&mut self, path: &str, value: f64) -> Result<bool, ParamError>;
    pub fn get_path(&self, path: &str) -> Result<f64, ParamError>;
}
```

`frames.rs` — **the only file in the repository containing a rotation matrix.**

```rust
pub fn rot_z(psi: f64) -> Mat2;              // body H → world W
pub fn body_to_world(v: Vec2, psi: f64) -> Vec2;
pub fn world_to_body(v: Vec2, psi: f64) -> Vec2;
pub fn rot_x(phi: f64, v: Vec3) -> Vec3;     // B → H
pub fn rot_x_inv(phi: f64, v: Vec3) -> Vec3; // H → B
pub fn boom_dir(beta: f64) -> Vec3;          // b̂(β) = (−cos β, −sin β, 0)   [F2.1]
pub fn boom_dir_dbeta(beta: f64) -> Vec3;    // (sin β, −cos β, 0)
pub fn rudder_chord(delta_r: f64) -> Vec3;   // ĉ_r = (−cos δ, −sin δ, 0)     [F2.2]
pub fn wrap_pi(a: f64) -> f64;               // to (−π, π]
```

**Acceptance criteria**
- `cargo test -p sailgym-physics state::` — `to_array`/`from_array` round-trip is
  the identity for a randomised state (100 cases, exact `==`).
- `cargo test -p sailgym-physics frames::` includes, by name:
  - `rot_round_trip`: `world_to_body(body_to_world(v, ψ), ψ) == v` within 1e-12 for 100 random `(v, ψ)`.
  - `boom_dir_sign`: `boom_dir(0.3).y < 0.0` — **positive β puts the boom to starboard** (F2.1).
  - `boom_dir_derivative`: numerical `d/dβ` of `boom_dir` matches `boom_dir_dbeta` within 1e-7.
  - `rot_x_horizontal_vector`: `rot_x_inv(φ, (0, a, 0)).y == a·cos φ` within 1e-12 (the F6.4 heel correction).
  - `wrap_pi_boundaries`: `wrap_pi(π) == π`, `wrap_pi(−π) == π`, `wrap_pi(3π) == π`.
- `cargo test -p sailgym-physics parameters::` — `ilca7().validate()` is `Ok`;
  `set_path`/`get_path` round-trip for ≥ 10 distinct paths; an unknown path is `Err`.
- Every field in `BoatParameters` has a doc comment containing one of
  `KNOWN`/`ASSUMED`/`TUNABLE`/`DEFERRED`. Enforced by a test that reads the
  source file and asserts the count matches the field count.

---

### 2.2 — RK2 integrator and the fixed-step driver
**P-group: A**
**Owns:** `crates/sailgym-physics/src/integrator.rs`, `crates/sailgym-physics/src/dynamics.rs`
**Depends:** 2.1

```rust
pub trait ForceModel {
    /// Pure: must not mutate anything. Returns generalised forces in H (F4.4).
    fn generalized(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> Generalized;
    /// Boom moment about +z_B at the mast. Zero until section 05.
    fn boom_moment(&self, st: &BoatState, c: &Controls, p: &BoatParameters, t: f64) -> f64;
}

/// F4.1–F4.3 assembled. The ONLY place the equations of motion appear.
pub fn derivative(st: &BoatState, c: &Controls, p: &BoatParameters,
                  fm: &dyn ForceModel, t: f64) -> StateDot;

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum Integrator { SemiImplicitEuler, Rk2Midpoint, Rk4 }

/// One fixed step. Angle wrapping is applied once, after the step completes.
pub fn step(st: &BoatState, c: &Controls, p: &BoatParameters,
            fm: &dyn ForceModel, dt: f64, method: Integrator) -> BoatState;
```

RK2 midpoint is the default (brief §21). `Rk4` exists only as the reference
integrator for the section 10 convergence study (brief §21 last line) — it is
never the default and is not exposed in the UI.

Actuator rate clamping happens **inside `derivative`** (F4.3). A test proves it:
integrating with `dt` and `dt/2` from a saturated rudder command must reach the
same `delta_r` within 1e-9, which is only true if the clamp is in the derivative.

**Acceptance criteria**
- `cargo test -p sailgym-physics integrator::`:
  - `rk2_matches_analytic_decay`: for `ẏ = −λy`, RK2 error over 1 s scales as
    `O(dt²)` — halving `dt` reduces max error by a factor in `[3.6, 4.4]`.
  - `clamp_in_derivative`: the `dt` vs `dt/2` test above, tolerance 1e-9.
  - `zero_force_zero_motion`: with a `ForceModel` returning all zeros and a
    zero-velocity state, 10 000 steps leave every field bit-identical.
  - `advance_n_equals_n_advance_1`: F9.7, bit-identical over 1000 steps.
- `derivative` takes `&BoatState` and returns a value — no `&mut` anywhere in
  its signature or that of `ForceModel`.

---

### 2.3 — Scaffold force model (throwaway, R4)
**P-group: A**
**Owns:** `crates/sailgym-physics/src/forces/mod.rs`, `crates/sailgym-physics/src/forces/scaffold.rs`
**Depends:** 2.1

```rust
//! SCAFFOLD — DELETED BY TASK 4.5. NOT PHYSICS.
pub struct ScaffoldForces;
impl ForceModel for ScaffoldForces { /* … */ }
```

Behaviour, chosen only to make the app steerable and bounded:
- constant forward thrust proportional to `sim.scaffold_thrust` (a `DEFERRED`-tagged
  parameter that 4.5 also deletes);
- linear drag on `u`, `v`, `r`;
- yaw moment linear in `delta_r` and `u`;
- roll and boom left at zero.

**Acceptance criteria**
- The file header comment is present verbatim.
- `cargo test -p sailgym-physics forces::scaffold::`:
  - `scaffold_finite`: 60 s of simulation from 20 randomised states and control
    sequences produces `is_finite()` at every step.
  - `scaffold_bounded`: `|u| < 20`, `|r| < 5` over the same runs.
- No test asserts any quantitative scaffold behaviour beyond these two.
- `grep -rn "scaffold" crates/sailgym-physics/src | wc -l` ≤ 12 — the scaffold's
  blast radius stays small so 4.5 is a clean deletion.

---

### 2.4 — Simulation object and the real WASM boundary
**P-group: B**
**Owns:** `crates/sailgym-physics/src/simulation.rs`, `crates/sailgym-wasm/src/lib.rs`, `web/src/sim/snapshot.ts`
**Depends:** 2.2, 2.3

```rust
pub struct Simulation {
    state: BoatState, params: BoatParameters, controls: Controls,
    force_model: Box<dyn ForceModel>, seed: u64, steps: u64,
}
impl Simulation {
    pub fn new(params: BoatParameters, seed: u64) -> Self;
    pub fn reset(&mut self, state: BoatState, seed: u64);
    pub fn set_controls(&mut self, c: Controls);
    pub fn advance(&mut self, n: u32) -> u32;
    pub fn state(&self) -> &BoatState;
    pub fn params(&self) -> &BoatParameters;
}
```

`sailgym-wasm` now implements the F8.2 methods that exist at this milestone:
`new`, `reset`, `set_controls`, `advance`, `snapshot`, `set_parameter`,
`parameters_json`. `snapshot` returns the F8.3 layout.

`web/src/sim/snapshot.ts` mirrors F8.3:

```ts
export const SNAPSHOT_FIELDS = ['x','y','psi','phi','u','v','r','p',
                                'beta','betaDot','deltaR','lSheet','t'] as const
export type SnapshotField = typeof SNAPSHOT_FIELDS[number]
export type Snapshot = Record<SnapshotField, number>
export function readSnapshot(buf: Float64Array): Snapshot
```

**Acceptance criteria**
- `cargo test -p sailgym-physics simulation::snapshot_layout` asserts the
  `to_array` index of each field matches the documented F8.3 order by name.
- `pnpm --dir web test:unit snapshot` asserts `SNAPSHOT_FIELDS.length === 13`
  and that `readSnapshot` maps index 3 to `phi` and index 10 to `deltaR`.
- A `wasm-bindgen-test` asserts `snapshot().length === 13`.
- `set_parameter("sail.area", 8.0)` then `parameters_json()` reflects 8.0.

---

### 2.5 — Simulation clock and React driver
**P-group: C**
**Owns:** `web/src/sim/clock.ts`, `web/src/sim/useSimulation.ts`, `web/src/ui/ClockControls.tsx`
**Depends:** 2.4

Physics is **decoupled from `requestAnimationFrame`** (brief §21). The clock
accumulates wall time and issues `advance(n)` in whole steps; rendering reads the
most recent snapshot.

```ts
export type SpeedMultiplier = 0.25 | 1 | 2 | 4
export interface ClockState {
  running: boolean; speed: SpeedMultiplier; simTime: number; stepsTaken: number
}
export interface Clock {
  start(): void; pause(): void; reset(): void
  singleStep(): void                         // exactly one dt, only while paused
  setSpeed(s: SpeedMultiplier): void
  /** Called once per rAF. Returns the number of physics steps issued. */
  tick(wallDeltaMs: number): number
}
/** Max steps issued in one tick, so a stalled tab cannot produce a spiral of death. */
export const MAX_STEPS_PER_TICK = 240
```

`ClockControls.tsx` renders pause/resume, reset, single-step and the four speeds,
each with a stable `data-testid` (`clock-pause`, `clock-step`, `clock-speed-2x`, …).

**Acceptance criteria**
- `pnpm --dir web test:unit clock`:
  - at 1×, 1000 ms of wall time at `dt = 0.005` issues 200 steps ± 1;
  - at 4×, the same issues 800 ± 1;
  - a 10 s wall stall issues at most `MAX_STEPS_PER_TICK`;
  - `singleStep()` while running is a no-op;
  - `singleStep()` while paused issues exactly 1.
- `web/tests/e2e/clock.spec.ts`: pause freezes `t` in the snapshot readout;
  single-step advances it by exactly `dt` (within 1e-9); 2× advances `t` about
  twice as fast as 1× over a 2 s window (ratio in `[1.7, 2.3]`).
- `grep -rn "requestAnimationFrame" web/src/sim/` matches nothing outside
  `useSimulation.ts`, and `clock.ts` contains no rAF at all.

---

### 2.6 — Keyboard controls and input mapping
**P-group: C**
**Owns:** `web/src/sim/controls.ts`, `web/src/sim/keymap.ts`
**Depends:** 2.4

Brief §28's mapping, with every rate, gain and dead zone in a config object, not
scattered through the code (brief §28 last line).

```ts
export interface InputConfig {
  rudderKeyRate: number      // normalised command magnitude while a key is held, default 1.0
  rudderDeadZone: number     // default 0.02
  sheetDragGain: number      // normalised command per pixel of vertical drag, default 0.004
  sheetInvert: boolean       // default false
}
export const DEFAULT_INPUT: InputConfig
export type Action = 'steerPort'|'steerStarboard'|'sheetRelease'|'reset'|'pause'|'singleStep'
export const KEYMAP: Record<string, Action>   // 'a'|'ArrowLeft' → steerPort, etc.
/** Pure: current key set + config → Controls. No DOM access. */
export function controlsFromInput(held: ReadonlySet<string>, cfg: InputConfig): Controls
```

`D`/`ArrowRight` must steer the bow to **starboard**, which per F2.2 means a
**positive** `rudder_rate_cmd`. Assert it.

Rudder self-centring when no key is held is implemented in Rust
(`delta_r_self_centre`, F7), not in TypeScript — TS sends `rudder_rate_cmd = 0`
and Rust applies the return rate. Do not implement centring in the browser.

**Acceptance criteria**
- `pnpm --dir web test:unit controls`:
  - `controlsFromInput(new Set(['d']), DEFAULT_INPUT).rudderRateCmd > 0`;
  - `controlsFromInput(new Set(['a']), …).rudderRateCmd < 0`;
  - both keys held → `0`;
  - `' '` (Space) → `sheetRelease === true`.
- `web/tests/e2e/controls.spec.ts`: holding `D` for 1 s drives `deltaR` positive
  in the snapshot and `psi` negative (bow to starboard, F2.2); holding `A`
  mirrors both signs. This test is the browser-level guard against R3.
- `grep -rn "0\.698\|delta_r_max\|self_centre" web/src/` matches nothing.

---

### 2.7 — SVG boat rendering and camera
**P-group: C**
**Owns:** `web/src/render/BoatSvg.tsx`, `web/src/render/Camera.ts`, `web/src/render/Trajectory.tsx`, `web/src/render/geometry.ts`
**Depends:** 2.4

Top-down SVG at approximately correct ILCA scale (brief §25). Hull outline, mast,
boom, sail, rudder, centreboard indication, trajectory polyline. Boom and sail are
drawn from `beta` — they will be static until section 05, which is expected.

`geometry.ts` holds the hull outline as a normalised path scaled by
`loa`/`beam` read from `parameters_json()`. No hard-coded metre values in the
renderer.

```ts
export type CameraMode = 'follow' | 'northUp'
export interface Camera {
  mode: CameraMode; zoom: number; centre: { x: number; y: number }
  worldToScreen(p: Vec2): Vec2
  screenToWorld(p: Vec2): Vec2
}
```

Both modes per brief §27, plus pan (drag with the **middle button or Shift+drag**)
and zoom (wheel) and a reset-camera control. Left-drag is reserved for the
mainsheet in section 06 — do not bind it here (brief §27 last line).

**Acceptance criteria**
- `pnpm --dir web test:unit camera`: `screenToWorld(worldToScreen(p)) ≈ p` within
  1e-9 for 100 random points across both modes and zoom in `[0.1, 20]`.
- `web/tests/e2e/render.spec.ts`: `[data-testid="boat-hull"]` exists; its
  `transform` attribute changes between two snapshots 1 s apart while running;
  in `northUp` mode the world grid `transform` has no rotation component.
- Hull length on screen at zoom 1 equals `loa × pixelsPerMetre` within 1 px.
- `grep -rn "4\.23\|1\.37" web/src/` matches nothing.

---

### 2.8 — Determinism, finiteness and clamping tests
**P-group: D**
**Owns:** `crates/sailgym-physics/tests/determinism.rs`, `web/tests/e2e/determinism.spec.ts`
**Depends:** 2.4, 2.5, 2.6

Determinism lands **now**, not at M9 (F9, brief §34). Retrofitting it later is
far more expensive than maintaining it from here.

Required tests, by name:

| Test | Assertion |
|---|---|
| `identical_seed_identical_trajectory` | Two `Simulation`s, same params/seed/state, same 5000-step scripted control sequence → **bit-identical** final state and bit-identical arrays at 10 checkpoints |
| `advance_batching_invariant` | `advance(1000)` vs 1000 × `advance(1)` → bit-identical (F9.7) |
| `no_wall_clock` | `grep` over `crates/sailgym-physics/src` finds no `Instant`, `SystemTime`, `now(`, `rand::thread_rng` |
| `no_hash_iteration` | `grep` finds no `HashMap`/`HashSet` in `src/` |
| `finite_under_random_controls` | 200 randomised 30 s episodes → `is_finite()` at every logged step, no `NaN`, no `Inf` (brief §35) |
| `rudder_clamped` | `delta_r` never exceeds `delta_r_max` by more than 1e-12 across those episodes |
| `sheet_length_clamped` | `l_sheet` stays within `[l_sheet_min, l_sheet_max]` |

`determinism.spec.ts` is the browser-side counterpart: reset with a fixed seed,
replay a scripted key sequence twice via `page.keyboard`, and assert the two
snapshot sequences are equal to within 0 ULP.

**Acceptance criteria**
- `cargo test -p sailgym-physics --test determinism` exits 0, all 7 tests present by name.
- `pnpm --dir web test:e2e determinism` exits 0.
- The grep-based tests fail if a violation is introduced — verify once by
  temporarily adding `Instant::now()`, then revert.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0.
2. The browser shows a boat that can be steered with `A`/`D`, with the trajectory
   trailing behind it, in both camera modes, at all four speeds.
3. Pause, resume, reset and single-step behave per brief §22, verified by E2E.
4. `cargo test -p sailgym-physics --test determinism` green, all 7 tests.
5. `phi` is confirmed unwrapped: a test sets `phi = 4.0 rad` and asserts
   `wrap_angles()` leaves it at 4.0.
6. The scaffold is confined to `forces/scaffold.rs` and the grep count in 2.3 holds.
7. **Sign audit passed.** These four assertions exist and pass, each in the file
   named, and together they pin every sign in F2:
   - `frames::boom_dir_sign` (β > 0 ⇒ boom to starboard)
   - `frames::rot_x_horizontal_vector` (heel gives `cos φ`)
   - `controls.spec.ts` "D steers starboard" (`δr > 0` ⇒ `ψ̇ < 0`)
   - `integrator::zero_force_zero_motion` (rest equilibrium, brief §35)
8. `docs/v1/progress/02-handoff.md` written, explicitly listing every parameter in
   F7 that section 02 left unused, so section 04 knows what is untouched.

## Risks touched

- **R3** — mitigated by section AC 7. If any of those four tests is weakened or
  deleted in a later section, that is a defect.
- **R4** — the scaffold now exists. Section 04 must delete it. Restate this
  prominently in the handoff note.
