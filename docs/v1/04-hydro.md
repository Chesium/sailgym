# Section 04 — Hull, Centreboard and Rudder Hydrodynamics (M3)

**Prerequisite reading:** `docs/v1/00-foundations.md` (F4, F5, F6.5, F6.6 especially),
`docs/v1/progress/03-handoff.md`, `docs/v1/brief.md` §14, §15, §35.

## Goal

Replace the section 02 scaffold with real hydrodynamics: one shared foil model,
a centreboard, a rudder, hull resistance, and the true force-assembly pipeline.

## The propulsion gap — read before writing any test

There is **no sail until section 05**. After this section the boat has no way to
accelerate itself. Every test here must therefore be driven by one of:

- a non-zero initial velocity in the state, or
- an explicit external `Load` injected through the test-only
  `ForceModel` wrapper described in task 4.1, or
- a prescribed `delta_r` with an initial `u`.

Do **not** invent a temporary thrust term to make tests easier. Do **not** keep
the scaffold around "just for tests". A test helper that injects a named
external force is the sanctioned mechanism and is specified below.

---

## Tasks

### 4.1 — Shared foil model and test harness (contracts)
**P-group: S**
**Owns:** `crates/sailgym-physics/src/foil.rs`, `crates/sailgym-physics/src/forces/mod.rs`, `crates/sailgym-physics/src/testkit.rs`

`foil.rs` implements F5 **exactly** — `angle_of_attack`, `cl`, `cd`, `foil_force`,
`FoilParams`, `EPS_FLOW`, `smoothstep`. This is the only lift/drag implementation
in the repository; sail, board and rudder all call it.

`forces/mod.rs` gains the assembly type and the `Generalized::add` of F6.4, verbatim:

```rust
pub struct ForceBreakdown {
    pub sail: Load, pub board: Load, pub rudder: Load, pub hull: Load, pub sheet: Load,
    pub total: Generalized,
    pub alpha_sail: f64, pub alpha_board: f64, pub alpha_rudder: f64,
    pub cl_sail: f64, pub cd_sail: f64,
    pub sheet_tension: f64, pub m_beta: f64,
    pub k_restore: f64, pub gz: f64,
    pub aw_boat: Vec3, pub tw_boat: Vec2,
}
/// The real force model. Sail/sheet/roll terms stay zero until sections 05–07.
pub struct PhysicalForces;
impl ForceModel for PhysicalForces { /* … */ }
/// Computes the full breakdown once; `generalized` and diagnostics both use it.
pub fn evaluate(st: &BoatState, c: &Controls, p: &BoatParameters,
                w: &dyn WindField, t: f64) -> ForceBreakdown;
```

`testkit.rs` is `#[cfg(any(test, feature = "testkit"))]` and provides:

```rust
/// Wraps a ForceModel and adds a constant external Load. The sanctioned way to
/// drive tests before the sail exists. NOT compiled into the wasm build.
pub struct WithExternalLoad<F: ForceModel> { pub inner: F, pub extra: Load }
pub fn still_air() -> impl WindField;                 // zero wind everywhere
pub fn uniform_wind(speed: f64, bearing_deg: f64) -> impl WindField;
/// Mirror a state about the boat's centreline: y, v, r, p, beta, delta_r, psi all flip.
pub fn mirror_state(st: &BoatState) -> BoatState;
pub fn mirror_controls(c: &Controls) -> Controls;
```

`mirror_state` is the backbone of the port/starboard symmetry invariant
(brief §35). Its exact definition, given F2:

```
x → x        y → −y       psi → −psi     phi → −phi
u → u        v → −v       r  → −r        p   → −p
beta → −beta beta_dot → −beta_dot         delta_r → −delta_r
l_sheet → l_sheet         t → t
```

**Acceptance criteria**
- `cargo test -p sailgym-physics foil::`, by name:
  - `alpha_equals_rudder_angle`: for flow `(−1, 0)` and chord `rudder_chord(δ)`,
    `angle_of_attack` returns `δ` within 1e-12 for `δ ∈ [−0.6, 0.6]`.
  - `cl_is_odd`: `cl(−a) == −cl(a)` exactly (bit-identical) for 200 values of `a` spanning `[−π, π]`.
  - `cd_is_even`: `cd(−a) == cd(a)` exactly, same sweep.
  - `cl_zero_at_zero_and_pi`: `cl(0) == 0`, `|cl(±π/2)| < 1e-12`, `|cl(±π)| < 1e-12`.
  - `cd_endpoints`: `cd(0) == cd0`; `cd(±π/2) == cd0 + cn_max` within 1e-12; `cd(±π) == cd0` within 1e-12.
  - `cl_slope_at_origin`: `dcl/dα` at `α = 0` equals `2π/(1 + 2/AR)` within 1 %.
  - `continuity_across_stall`: `cl` and `cd` sampled at 1e-5 spacing across
    `[α_s − 2Δ, α_s + 2Δ]` show no step larger than 1e-3.
  - `zero_flow_zero_force`: `foil_force` with `|v| < EPS_FLOW` returns exactly `Vec2::ZERO`.
  - **`lift_direction_sign`**: flow `(−1, 0)`, chord `rudder_chord(+0.2)` ⇒ returned
    force has `y > 0` (to port). This is the F5.3 assertion and the primary R3 guard.
  - `finite_over_full_range`: no `NaN`/`Inf` for `α` on a 10 000-point sweep of
    `[−π, π]` across 5 parameter sets including `AR = 0.5` and `AR = 20`.
- `cargo test -p sailgym-physics testkit::mirror_involution`: `mirror_state` applied
  twice is the identity, bit-identical.
- `testkit` is absent from the wasm build: `wasm-pack build` output contains no
  `testkit` symbol, and `cargo tree` shows the feature off by default.

---

### 4.2 — Centreboard
**P-group: A**
**Owns:** `crates/sailgym-physics/src/hydro/centerboard.rs`
**Depends:** 4.1

Implements F6.5 with a fixed chord along the centreline.

```rust
pub struct FoilLoad { pub load: Load, pub alpha: f64, pub v_local: Vec2 }
/// Local water velocity relative to the surface, in B, spanwise component dropped.
pub fn local_flow(st: &BoatState, r_b: Vec3) -> Vec2;      // shared with rudder via hydro::mod
pub fn centerboard_load(st: &BoatState, p: &BoatParameters) -> FoilLoad;
```

**Acceptance criteria**
- `cargo test -p sailgym-physics hydro::centerboard::`:
  - `zero_flow_zero_lift`: `u = v = r = p = 0` ⇒ `load.f == Vec3::ZERO` exactly.
  - `side_force_opposes_leeway`: with `u = 3`, `v = +0.3` (drifting to port), the
    board force `y`-component is **negative** (to starboard) — it resists leeway.
  - `v_squared_scaling`: with leeway angle held fixed, `|load.f|` at speeds
    `{1, 2, 4, 6} m/s` fits `F ∝ V²` with each ratio within **2 %** of
    `(V₂/V₁)²` (brief §35 velocity scaling).
  - `alpha_equals_leeway_angle`: with `r = p = 0`, `alpha` equals
    `atan2(v, u)` within 1e-9. (Sign check: positive `v` — drift to port — gives
    positive `alpha`, hence lift to starboard, consistent with the test above.)
  - `yaw_rate_contributes`: with `u = 3, v = 0, r = 0.5`, `alpha != 0` — proving
    the `ω × r` term of F6.5 is present. Assert it matches
    `atan2(r·x_board, u)` within 1e-9.
  - `mirror_symmetry`: `centerboard_load(mirror_state(s))` equals the mirror of
    `centerboard_load(s)` — `f.y`, `r.y`, `alpha` negated, `f.x`, `r.x` equal —
    within 1e-14 for 50 random states.

---

### 4.3 — Rudder
**P-group: A**
**Owns:** `crates/sailgym-physics/src/hydro/rudder.rs`
**Depends:** 4.1

```rust
pub fn rudder_load(st: &BoatState, p: &BoatParameters) -> FoilLoad;
```

Same structure as the board, with chord `rudder_chord(st.delta_r)` (F2.2) and the
rudder mount position.

**Acceptance criteria**
- `cargo test -p sailgym-physics hydro::rudder::`:
  - `zero_speed_no_authority`: at `u = 0.01 m/s`, `|yaw moment| < 0.1 N·m`
    (brief §14 "weak rudder authority when nearly stationary").
  - **`positive_delta_turns_bow_to_starboard`**: `u = 3`, `delta_r = +0.2` ⇒ the
    yaw moment about `+z` is **negative**. The single most important sign test in
    the project (F2.2, R3).
  - `authority_grows_with_speed`: yaw moment magnitude at `u ∈ {1, 2, 4}` with
    fixed `delta_r` is monotonically increasing and matches `V²` within 3 %.
  - `rudder_stalls`: sweeping `delta_r` from 0 to 0.7 rad at `u = 3`, the yaw
    moment magnitude has an interior maximum — it rises, peaks, then falls
    (brief §14 "rudder stall at high angles"). Assert the peak lies within
    `[α_stall, α_stall + 3Δ_s]`.
  - `yaw_damping_sign`: with `delta_r = 0`, `u = 3`, `r = +0.4`, the rudder's yaw
    moment is negative — it damps the rotation (brief §14).
  - `mirror_symmetry`: as for the board.

---

### 4.4 — Hull resistance
**P-group: A**
**Owns:** `crates/sailgym-physics/src/hydro/hull.rs`
**Depends:** 4.1

Implements F6.6. Note `K_hull` is a moment added directly to `ΣK`, not a `Load`,
so the return type differs:

```rust
pub struct HullLoads { pub load: Load, pub k_roll: f64 }
pub fn hull_loads(st: &BoatState, p: &BoatParameters) -> HullLoads;
```

**Acceptance criteria**
- `cargo test -p sailgym-physics hydro::hull::`:
  - `drag_opposes_motion`: for 200 random `(u, v, r, p)`, each resistance
    component has the opposite sign to its velocity, or is exactly zero
    (brief §35 force sign sanity).
  - `resistance_anchor`: at `u = 2.06 m/s`, `v = r = p = 0`, total surge
    resistance is `48 ± 2 N` — the F7 sanity anchor. If this fails, the
    parameters changed; record why in the handoff note.
  - `quadratic_dominates_at_speed`: at `u = 6`, the quadratic term is > 90 % of
    the total, and the value is flagged in a doc comment as over-predicted (R6).
  - `zero_velocity_zero_force`: all-zero velocity ⇒ exactly `Vec3::ZERO` and `k_roll == 0.0`.
  - `mirror_symmetry`: as above.

---

### 4.5 — Force assembly, EOM wiring, and scaffold deletion (R4)
**P-group: S**
**Owns:** `crates/sailgym-physics/src/forces/mod.rs`, `crates/sailgym-physics/src/hydro/mod.rs`, `crates/sailgym-physics/src/simulation.rs`, and **deletes** `crates/sailgym-physics/src/forces/scaffold.rs`
**Depends:** 4.2, 4.3, 4.4

Wire `evaluate` to sum, in this fixed order (F9.4 — the order is part of the
determinism contract and must not be changed casually):

```
1. hull      2. centerboard      3. rudder
4. sail      (zero until §05)
5. mainsheet (zero until §06)
6. roll/hydrostatics (zero until §07)
```

Switch `Simulation` to `PhysicalForces`. Delete the scaffold file, its module
declaration, and the `scaffold_thrust` parameter.

**Acceptance criteria**
- `git ls-files | grep scaffold` returns **nothing**.
- `grep -rni "scaffold" crates/ web/ --include=*.rs --include=*.ts --include=*.tsx`
  returns **nothing**.
- `grep -rn "scaffold_thrust" .` returns nothing outside `docs/v1/`.
- `cargo test -p sailgym-physics forces::`:
  - `summation_order_documented`: a test reads `forces/mod.rs` and asserts the six
    calls appear in the order above. Crude, but it makes an accidental reorder
    visible in review.
  - `rest_equilibrium`: zero wind, zero velocity, neutral controls, 10 000 steps
    ⇒ every state field bit-identical to the start (brief §35). **This is the
    headline invariant of the section.**
  - `coast_down_decelerates`: from `u = 4`, no external load, `u` decreases
    monotonically and never crosses zero into negative.
  - `energy_not_created`: from 50 random initial velocity states with zero wind
    and no external load, total mechanical energy
    `½m_x u² + ½m_y v² + ½I_z r² + ½I_x p²` is non-increasing at every step
    (tolerance 1e-9 per step for integrator round-off) — brief §35 dissipative behaviour.
- The app still runs: a boat given an initial velocity coasts, decelerates, and
  can be turned with the rudder.

---

### 4.6 — Hydrodynamic invariants and the E2E update
**P-group: B**
**Owns:** `crates/sailgym-physics/tests/invariants.rs`, `web/tests/e2e/hydro.spec.ts`
**Depends:** 4.5

Create `tests/invariants.rs` — the file that grows through sections 05–07 and is
completed in section 10. At this milestone it must contain:

| Test | Brief ref | Tolerance |
|---|---|---|
| `rest_equilibrium` | §35 | bit-identical over 10 000 steps |
| `mirror_symmetry_trajectory` | §35 | mirrored 20 s trajectory matches within 1e-10 per field |
| `force_sign_sanity` | §35 | drag opposes motion, 200 random states |
| `velocity_scaling` | §35 | `F ∝ V²` within 2 % |
| `zero_flow_foil_behaviour` | §35 | exact zero |
| `dissipative_behaviour` | §35 | energy non-increasing, 1e-9/step |
| `coordinate_frame_consistency` | §35 | see below |
| `finite_number_invariant` | §35 | no NaN/Inf, 200 random 30 s episodes |

`coordinate_frame_consistency`: rotate the entire setup — initial `x, y, psi` and
the wind bearing — by `θ ∈ {30°, 90°, 217°}` in world coordinates, run 20 s, and
assert the resulting trajectory is the original rotated by `θ`, within 1e-9 in
position and 1e-11 in the body-frame quantities `u, v, r, p`. The body-frame
values must be **unchanged**, not rotated — that is the actual content of the test.

`web/tests/e2e/hydro.spec.ts`: with an initial speed set by a test scenario,
holding `D` produces a rightward-curving trajectory on screen, and releasing all
keys lets the rudder self-centre and the boat straighten.

**Acceptance criteria**
- `cargo test -p sailgym-physics --test invariants` exits 0 with all 8 tests present by name.
- `scripts/check.ps1` step 4 now runs a non-empty invariant suite.
- `pnpm --dir web test:e2e hydro` exits 0.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0.
2. **The scaffold is gone**, proven by the greps in 4.5. R4 closed.
3. `cargo test -p sailgym-physics --test invariants` green, 8 tests.
4. `cargo test -p sailgym-physics --test determinism` still green — the real force
   model did not break F9.
5. **Sign audit extended.** These now pass and must never be weakened:
   `foil::lift_direction_sign`, `hydro::rudder::positive_delta_turns_bow_to_starboard`,
   `hydro::centerboard::side_force_opposes_leeway`, `invariants::mirror_symmetry_trajectory`.
6. Exactly one lift/drag implementation exists:
   `grep -rn "fn cl\|fn cd\|C_L\|lift_coefficient" crates/sailgym-physics/src`
   matches only `foil.rs`.
7. The `F7` parameter values used are unchanged, or every change is recorded in
   `docs/v1/progress/04-handoff.md` with reason, source and assumption (brief §43).
8. `docs/v1/progress/04-handoff.md` written, including the measured `resistance_anchor`
   value and any R6 commentary.

## Risks touched

- **R3** — this section is where sign errors would first become visible. Section
  AC 5 is the gate.
- **R4** — closed here. Confirm in the handoff note.
- **R6** — the hull model's missing planing regime becomes concrete. Document the
  speed above which resistance is known to be over-predicted.
