# Section 08 — Debug Instrumentation and Live Parameter Editing (M7)

**Prerequisite reading:** `docs/v1/00-foundations.md` (F7, F8.2),
`docs/v1/progress/07-handoff.md`, `docs/v1/brief.md` §29, §30, §31.

## Goal

Make every force, moment and coefficient in the model inspectable, numerically
and visually, and let the engineer change parameters live without a rebuild.

Brief §30 is explicit that this is **a core prototype feature, not optional
polish**. Treat a missing diagnostic as a missing requirement, not a nice-to-have.

---

## Tasks

### 8.1 — Diagnostics structure and WASM surface
**P-group: S**
**Owns:** `crates/sailgym-physics/src/diagnostics.rs`, `crates/sailgym-wasm/src/lib.rs` (diagnostics methods), `web/src/sim/diagnostics.ts`

Every item in brief §30's list must be present. The struct is the checklist.

```rust
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Diagnostics {
    pub t: f64, pub steps: u64,
    // environment
    pub true_wind_world: Vec2, pub true_wind_body: Vec2,
    pub apparent_wind_body: Vec3, pub apparent_wind_speed: f64, pub apparent_wind_angle: f64,
    // motion
    pub velocity_body: Vec2, pub speed_over_ground: f64, pub course_over_ground: f64,
    pub acceleration_body: Vec2, pub yaw_rate: f64, pub roll_rate: f64,
    pub leeway_angle: f64,
    // forces, each with application point, all in B
    pub sail: Load, pub board: Load, pub rudder: Load, pub hull: Load, pub sheet_hull: Load,
    pub total_force_h: Vec2,
    // moments
    pub yaw_moment: f64, pub heeling_moment: f64, pub righting_moment: f64,
    pub boom_moment: BoomMoments,
    // geometry
    pub sail_ce_b: Vec3, pub board_centre_b: Vec3, pub rudder_centre_b: Vec3,
    pub sheet_attach_b: Vec3, pub sheet_block_b: Vec3,
    // coefficients and angles
    pub alpha_sail: f64, pub cl_sail: f64, pub cd_sail: f64,
    pub alpha_board: f64, pub cl_board: f64, pub cd_board: f64,
    pub alpha_rudder: f64, pub cl_rudder: f64, pub cd_rudder: f64,
    // rig
    pub beta: f64, pub beta_dot: f64, pub sheet_tension: f64,
    pub sheet_rope_length: f64, pub sheet_extension: f64,
    // stability
    pub gz: f64, pub heel_deg: f64, pub capsize: CapsizeState,
    // health
    pub energy_kinetic: f64, pub energy_roll_potential: f64, pub energy_sheet_elastic: f64,
}
pub fn diagnostics(sim: &Simulation) -> Diagnostics;
```

`diagnostics` reuses the `ForceBreakdown` from the last step — it must **not**
re-run `evaluate` with a different argument set, or the displayed numbers will
subtly disagree with the ones that moved the boat.

`web/src/sim/diagnostics.ts` declares the mirror TypeScript type. A test asserts
field-name parity between the Rust struct and the TS type by parsing both.

**Acceptance criteria**
- `cargo test -p sailgym-physics diagnostics::`:
  - `covers_brief_30`: a test containing the literal list of brief §30 items
    asserts each maps to a named field. Any future removal breaks it.
  - `matches_step_forces`: after a step, `diagnostics.sail.f` equals the `Load`
    used by that step's `evaluate`, bit-identical.
  - `energy_terms_consistent`: the three energy terms sum to a value matching an
    independent computation in the test within 1e-9.
- `pnpm --dir web test:unit diagnostics` asserts Rust/TS field parity (no missing
  or extra field, both directions).
- A `wasm-bindgen-test` asserts `diagnostics()` serialises without error and
  round-trips through `JSON.parse`.

---

### 8.2 — Sail Mode / Debug Mode and layout
**P-group: A**
**Owns:** `web/src/ui/ModeSwitch.tsx`, `web/src/ui/Layout.tsx`, `web/src/ui/store.ts`
**Depends:** 8.1

Two modes per brief §29.

**Sail Mode** shows only: wind direction and speed, boat speed, heading, heel,
rudder indication, sheet indication, capsize state. Nothing else. The value of
this mode is what it omits.

**Debug Mode** exposes the full diagnostic set and the visualization toggles.

Mode lives in the `zustand` UI store, persisted to `localStorage`, defaulting to
Sail Mode. Toggle bound to a key and a visible control.

**Acceptance criteria**
- `web/tests/e2e/modes.spec.ts`:
  - Sail Mode renders exactly the 7 readouts listed in brief §29 and no
    `[data-testid^="diag-"]` element;
  - Debug Mode renders `[data-testid^="diag-"]` elements for every field group;
  - the mode survives a page reload;
  - switching modes does not pause the simulation or change `t`'s rate.

---

### 8.3 — Force and moment vector overlays
**P-group: A**
**Owns:** `web/src/render/ForceOverlay.tsx`, `web/src/render/vectorScale.ts`
**Depends:** 8.1

Independently toggleable SVG overlays for each item in brief §30's visual list:
true wind, apparent wind, boat velocity, acceleration, sail force, centreboard
force, rudder force, hull force, total force, yaw moment, heeling moment,
righting moment, sail CE, board and rudder centres, boom angular velocity, sheet
tension.

Vectors are drawn at their application point with a shared, adjustable
newtons-per-pixel scale, plus a per-category colour. Moments are drawn as arcs at
the CG. A legend lists each active overlay with its current numeric value so the
visual and the number are never inconsistent (brief §30 "inspectable numerically
where practical").

Overlay count is bounded — these are SVG, and brief §39 only forbids SVG for the
*dense wind field*, which this is not.

**Acceptance criteria**
- `pnpm --dir web test:unit vectorScale`: a 200 N force at scale
  `2 N/px` renders 100 px; the auto-scale mode keeps the largest active vector
  between 40 and 200 px across a 10× force range.
- `web/tests/e2e/debug.spec.ts`:
  - toggling each of the 16 overlays changes the DOM element count as expected,
    in both directions;
  - the sail force vector's rendered origin coincides with the rendered sail CE
    within 3 px;
  - with all overlays on, total SVG element count stays under 300;
  - the legend's numeric value for sail force matches `diagnostics.sail.f`
    magnitude within 0.5 N.

---

### 8.4 — Numeric readouts and time-series charts
**P-group: A**
**Owns:** `web/src/ui/DebugPanel.tsx`, `web/src/ui/Charts.tsx`, `web/src/ui/ringBuffer.ts`
**Depends:** 8.1

Grouped numeric readouts for the whole `Diagnostics` struct, plus scrolling
time-series charts for a user-selected subset (heel, sheet tension, boat speed,
angle of attack, yaw rate are the sensible defaults).

Chart data comes from a fixed-capacity ring buffer sampled at a configurable rate
(default 20 Hz) — **not** every physics step, and **not** every frame. Sampling
must not perturb physics timing.

```ts
export class RingBuffer<T> {
  constructor(capacity: number)
  push(v: T): void
  toArray(): T[]          // oldest → newest
  get length(): number
}
```

**Acceptance criteria**
- `pnpm --dir web test:unit ringBuffer`: capacity respected, overwrite order
  correct, `toArray` ordering correct, no allocation after construction
  (asserted by reusing the same backing array).
- `web/tests/e2e/debug.spec.ts`:
  - every `Diagnostics` field appears in the panel — the test enumerates the TS
    type's keys and asserts a matching `[data-testid="diag-<key>"]` exists;
  - charts accumulate points over 5 s and the newest point matches the current
    diagnostic value within 1 %;
  - enabling all charts keeps frame time under 16.7 ms (measured, recorded).

---

### 8.5 — Live parameter editor
**P-group: B**
**Owns:** `web/src/ui/ParameterPanel.tsx`, `web/src/ui/parameterSchema.ts`
**Depends:** 8.1

A collapsible engineering panel (brief §31) driven by `parameters_json()`, with
edits applied through `set_parameter(path, value)` (F8.2). The panel is
**generated from the parameter tree**, not hand-written — a new parameter in
`parameters.rs` appears automatically.

Each control shows the parameter's tag (KNOWN/ASSUMED/TUNABLE/DEFERRED) from the
Rust doc comment, surfaced through `parameters_json()`. KNOWN parameters are
editable but visually marked, because changing one means you are no longer
simulating an ILCA.

At minimum the brief §31 list must be reachable: wind magnitude and direction,
wind variation amplitude, boat mass, sailor mass, sail area, drag and damping
coefficients, sheet stiffness, sheet damping, maximum rudder angle,
righting-moment parameters.

`set_parameter` returns whether a reset is required (F8.2); the panel shows a
"reset required" badge and a reset button rather than silently invalidating the
run (brief §31).

A **Reset to ILCA defaults** button restores `BoatParameters::ilca7()`.

**Acceptance criteria**
- `pnpm --dir web test:unit parameterSchema`: the schema derived from a sample
  `parameters_json()` produces one control per leaf field, with correct type and
  tag, and no hand-written field list exists in the source.
- `web/tests/e2e/params.spec.ts`:
  - changing `sail.area` from 7.06 to 3.0 measurably reduces `diagnostics.sail.f`
    magnitude within 2 s of simulation;
  - changing `stability.gm` to an invalid combination surfaces the `fit` error as
    a visible message and does **not** put the sim into a `NaN` state;
  - "Reset to ILCA defaults" restores every edited value, verified by comparing
    `parameters_json()` before and after;
  - a parameter marked reset-required shows the badge and the reset button works.
- Every brief §31 example parameter is reachable in the panel — asserted by a
  test enumerating that literal list against the rendered controls.

---

### 8.6 — Debug-mode E2E and performance guard
**P-group: C**
**Owns:** `web/tests/e2e/debug.spec.ts`, `web/tests/e2e/perf.spec.ts`
**Depends:** 8.3, 8.4, 8.5

`perf.spec.ts` establishes the frame-time baseline that section 10 will harden:
measure `requestAnimationFrame` intervals over 10 s in Sail Mode, in Debug Mode
with all overlays, and in Debug Mode with all charts. Assert the 95th percentile
stays under 16.7 ms in Sail Mode and under 25 ms in full Debug Mode, and record
all three figures.

**Acceptance criteria**
- `pnpm --dir web test:e2e debug` and `… perf` exit 0 on all three browsers.
- Measured figures recorded in the handoff note.
- No console errors with every overlay and chart enabled simultaneously.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0.
2. **Every item in brief §30's list is both visualizable and numerically
   readable**, proven by `covers_brief_30` (Rust) and the field-enumeration test
   in 8.4 (browser). This is the section's headline requirement.
3. **Every item in brief §31's list is live-editable**, proven by the
   enumeration test in 8.5.
4. Rust and TypeScript diagnostic field sets are in exact parity.
5. Displayed forces are the forces that moved the boat (`matches_step_forces`).
6. Sail Mode shows exactly the brief §29 set and nothing more.
7. Frame-time figures recorded for all three configurations.
8. All earlier suites — determinism, invariants, no_shortcuts — still green.
9. `docs/v1/progress/08-handoff.md` written.

## Risks touched

- **R5** — the heaviest rendering load so far. If deck.gl and the SVG overlays
  contend, record it; section 10 owns the optimisation.
- **R6** — surface the hull model's over-prediction above ≈ 5 m/s as a visible
  warning in the debug panel when `u` exceeds it, so no one mistakes the number
  for validated data.
