# Section 07 — Roll Dynamics, Righting Moment, Capsize (M6)

**Prerequisite reading:** `docs/00-foundations.md` (F6.4, F6.7, F6.10, F11/R2),
`docs/progress/06-handoff.md`, `docs/brief.md` §16, §17, §46.

## Goal

Roll becomes a live degree of freedom driven by a nonlinear righting moment, and
capsize emerges from it. At the end of this section the **entire primary
demonstration of brief §46 works**: haul in on a beam reach and the boat heels
and capsizes; reset, ease the sheet, and the boat recovers — with no rule
anywhere that says "release causes recovery".

## The prohibited shortcut

Brief §7, §17 and §46 all forbid the same thing from three angles. Concretely,
none of the following may exist in the codebase:

- a branch on `capsized` that changes any force or moment;
- a term that reduces heel when the sheet is eased, other than through the sail
  force actually falling;
- a heel angle clamp, a "max heel" limit, or a termination at a heel threshold;
- a righting moment that is a globally linear spring (brief §16 explicitly).

`capsized` is an output. It is written by the diagnostics path and read by the
UI. Nothing in `forces/` or `dynamics.rs` may read it. Task 7.6 asserts this by
grep.

---

## Tasks

### 7.1 — GZ curve
**P-group: S**
**Owns:** `crates/sailgym-physics/src/stability/mod.rs`, `crates/sailgym-physics/src/stability/hydrostatics.rs`

Implements F6.7: the three-term odd harmonic fit.

```rust
#[derive(Clone, Copy, Debug)]
pub struct GzCurve { c1: f64, c2: f64, c3: f64 }
impl GzCurve {
    /// Solves the 3×3 system of F6.7. Errors if the result is unphysical.
    pub fn fit(gm: f64, phi_p: f64, gz_max: f64, phi_v: f64) -> Result<Self, ParamError>;
    pub fn gz(&self, phi: f64) -> f64;
    pub fn dgz(&self, phi: f64) -> f64;
    /// Sampled curve for the parameter panel and diagnostics plot.
    pub fn sample(&self, n: usize) -> Vec<(f64, f64)>;
}
/// K_restore = −Δ·g·GZ(φ)
pub fn righting_moment(phi: f64, curve: &GzCurve, total_mass: f64) -> f64;
```

`fit` must reject parameter sets that produce a non-monotonic `GZ` on `[0, φ_p]`
or a sign change before `φ_v` (F6.7). This matters because the parameter panel in
section 08 lets a user drag `GM` and `φ_v` into nonsense; the error must be
clean, not a silently inverted boat.

**Acceptance criteria**
- `cargo test -p sailgym-physics stability::hydrostatics::`, by name:
  - `fit_reproduces_constraints`: with the F7 defaults, `dgz(0) == gm` within
    1e-9, `gz(phi_v)` within 1e-9 of 0, `gz(phi_p) == gz_max` within 1e-9.
  - `gz_is_odd`: `gz(−φ) == −gz(φ)` bit-identically for 200 values across `[−π, π]`.
  - `gz_rises_then_falls`: on `[0, φ_v]` the curve is positive, has exactly one
    interior maximum, and the maximum lies within `±15°` of `phi_peak`. (The fit
    pins the value at `phi_peak`, not the location — F6.7. If the peak is further
    off than 15°, the parameter set is poorly posed; record it.)
  - `gz_negative_beyond_vanishing`: `gz(phi_v + 0.2) < 0` — negative restoring
    moment after sufficient capsize (brief §16).
  - `restoring_moment_sign`: `righting_moment(+0.3, …) < 0` and
    `righting_moment(−0.3, …) > 0` — positive heel (starboard down) produces a
    moment rolling back to port. The F2/F6.7 sign contract.
  - `not_linear_spring`: `gz(0.8) / 0.8` differs from `gz(0.1) / 0.1` by more than
    20 % — proof the model is not a global linear spring (brief §16).
  - `fit_rejects_bad_params`: `fit(gm: 0.1, phi_p: 1.4, gz_max: 2.0, phi_v: 0.5)`
    returns `Err`.
  - `anchor_value`: `Δ·g·gz_max` with F7 defaults is `406 ± 5 N·m` — the F7 anchor.

---

### 7.2 — Roll dynamics
**P-group: A**
**Owns:** `crates/sailgym-physics/src/stability/roll.rs`
**Depends:** 7.1

```rust
pub struct RollMoments { pub restore: f64, pub damping: f64 }
/// Hydrostatic restoring + hull roll damping. Sail/foil/sheet heeling moments
/// arrive separately through Generalized::add — they are NOT recomputed here.
pub fn roll_moments(st: &BoatState, p: &BoatParameters, curve: &GzCurve) -> RollMoments;
```

Roll damping is the `K_p`, `K_pp` pair from F6.6 — it lives in `hull.rs` and is
*used* here, not duplicated. If you find yourself writing a second damping
coefficient, stop.

**Acceptance criteria**
- `cargo test -p sailgym-physics stability::roll::`:
  - `damping_opposes_roll_rate`: `p > 0` ⇒ damping `< 0`, and mirror.
  - `no_duplicate_damping`: a test asserts `roll.rs` contains no numeric
    coefficient and reads `k_p`/`k_pp` from parameters.
  - `upright_equilibrium`: `phi = 0`, `p = 0` ⇒ both moments exactly `0.0`.
  - `free_decay_period`: with no external moment, small-amplitude roll from
    `phi = 0.1` oscillates with period `2π√(I_x / (Δ·g·GM))` within 5 %. This is
    the physical sanity check on `i_xx`, `a_phi` and `gm` together; record the
    measured period in the handoff note.
  - `free_decay_decays`: successive peak amplitudes are strictly decreasing.

---

### 7.3 — Wire roll into the EOM
**P-group: S**
**Owns:** `crates/sailgym-physics/src/forces/mod.rs`, `crates/sailgym-physics/src/dynamics.rs`, `crates/sailgym-physics/src/simulation.rs`
**Depends:** 7.2

`evaluate` fills `breakdown.k_restore` and `gz`; `derivative` integrates `p` and
`phi` per F4.2. Every existing `Load` already contributes to `ΣK` through
`Generalized::add` (F6.4) — **no force module changes in this task.** If a force
module needs editing to make roll work, something is wrong in section 05's
implementation of F6.4; report it rather than patching here.

`phi` remains unwrapped (F3).

**Acceptance criteria**
- `git diff --stat` for this task touches no file under `aero/`, `hydro/` or
  `rigging/`. If it does, the F6.4 contract was violated earlier — record it.
- `cargo test -p sailgym-physics forces::roll_integrated::`:
  - **`sail_force_heels_boat`**: beam wind from starboard, sheet hauled ⇒ `phi`
    becomes negative (heel to port). The full-chain sign test.
  - **`easing_reduces_heel`**: from a steady heeled state, command
    `sheet_release`; `|phi|` decreases and the boat returns toward upright. Assert
    that it does so **because** `sheet_tension → 0` then `|beta|` grows then the
    sail heeling moment falls — check all three in order in the same test, so a
    regression that produces the right answer for the wrong reason still fails.
    Brief §46 steps 11–16 and §47.
  - `couple_from_sail_and_board`: with steady sailing, the sail's roll moment and
    the centreboard's roll moment have the **same sign** (they form a heeling
    couple, force above the CG and force below it) — a geometry check that
    catches a sign error in `board_pos_b.z`.
  - `rest_equilibrium` (from section 04) still bit-identical with roll live.

---

### 7.4 — Capsize state
**P-group: A**
**Owns:** `crates/sailgym-physics/src/stability/capsize.rs`, `crates/sailgym-physics/src/diagnostics.rs` (capsize fields only)
**Depends:** 7.2

Implements F6.10. Informational only.

```rust
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct CapsizeState { pub capsized: bool, pub since: f64, pub max_heel: f64 }
impl CapsizeState {
    /// Called once per completed step, from Simulation::advance — never from
    /// within `derivative`, which must stay pure.
    pub fn update(&mut self, phi: f64, t: f64, p: &BoatParameters);
}
```

**Acceptance criteria**
- `cargo test -p sailgym-physics stability::capsize::`:
  - `not_capsized_below_threshold`: `|phi|` held just under `phi_capsize` for
    10 s ⇒ `capsized == false`.
  - `requires_duration`: `|phi|` exceeding the threshold for `t_capsize / 2` then
    returning ⇒ `capsized == false`.
  - `sets_after_duration`: exceeding for `t_capsize + ε` ⇒ `capsized == true`,
    `since` equal to the crossing time within `dt`.
  - **`simulation_continues_past_90_degrees`**: a scripted high-wind hauled beam
    reach reaches `|phi| > π/2`, keeps integrating for a further 30 s, and
    produces no `NaN`/`Inf` and no panic. Brief §17's explicit requirement.
  - `passes_through_inversion`: a test drives `phi` past `π` (by initial
    condition plus roll rate) and asserts `phi` is not wrapped and `gz` remains
    finite and correctly signed.
  - `capsize_flag_not_read_by_physics`: grep asserts `capsized` appears in no file
    under `forces/`, `aero/`, `hydro/`, `rigging/`, and not in `dynamics.rs`.
- `update` is called from `Simulation::advance`, never from `derivative` — a test
  asserts `derivative` takes `&BoatState` and `CapsizeState` is not among its
  arguments.

---

### 7.5 — Heel indicator and capsize UI
**P-group: B**
**Owns:** `web/src/render/HeelIndicator.tsx`, `web/src/ui/Hud.tsx` (heel and capsize fields)
**Depends:** 7.3, 7.4

Brief §26: a compact stern/transverse-section view plus a numeric heel angle. No
3D scene.

The indicator must be legible at four regimes: upright, moderate heel, severe
heel, inversion. Beyond `|phi| > π/2` the section view should show the hull past
horizontal; beyond `π`, inverted. Display heel as degrees with sign labelled
port/starboard, derived once in `web/src/sim/units.ts`.

Capsize state appears in **both** Sail Mode and Debug Mode (brief §29 lists
capsize state under Sail Mode's minimal instrumentation).

**Acceptance criteria**
- `web/tests/e2e/heel.spec.ts`:
  - `[data-testid="heel-indicator"]` exists and its numeric readout matches
    `phi` from the snapshot within 0.5° across a 20 s run;
  - the section-view hull `transform` rotation equals `phi` in degrees within 0.5;
  - driving the sim to `|phi| > 100°` renders without error and the readout shows
    a value beyond 90;
  - `[data-testid="capsize-state"]` flips to capsized after the threshold and
    duration, and the simulation keeps advancing (`t` still increasing).
- The indicator renders correctly at `phi = 0, ±45°, ±95°, ±185°` — asserted by
  four screenshot-free geometric checks on the rendered transform.

---

### 7.6 — Roll and capsize invariants, and the prohibited-shortcut audit
**P-group: C**
**Owns:** `crates/sailgym-physics/tests/invariants.rs` (adds), `crates/sailgym-physics/tests/no_shortcuts.rs`
**Depends:** 7.3, 7.4

Add to `invariants.rs`:

| Test | Assertion |
|---|---|
| `roll_mirror_symmetry` | Mirrored setup ⇒ `phi` trajectory exactly negated, within 1e-9 over 30 s |
| `dissipative_with_roll` | Zero wind, initial roll and yaw energy ⇒ total mechanical energy including roll potential `∫Δ·g·GZ dφ` is non-increasing, 1e-9/step |
| `capsize_finite` | 100 randomised high-wind episodes driven to capsize ⇒ no `NaN`/`Inf` over 60 s |
| `heel_reduces_drive` | Steady state at `phi = 0.5` has lower forward force than the same setup at `phi = 0`, via F6.4 only |

New file `tests/no_shortcuts.rs` — the mechanical audit of the prohibited list at
the top of this document. Each is a source grep asserted as a test:

| Test | Grep |
|---|---|
| `no_capsize_branch_in_physics` | `capsized` absent from `forces/`, `aero/`, `hydro/`, `rigging/`, `dynamics.rs` |
| `no_heel_clamp` | no `phi.clamp`, `phi.min(`, `phi.max(`, `MAX_HEEL` in `src/` |
| `no_release_recovery_rule` | no line matching `released.*heel|heel.*released|depower` in `src/` |
| `no_linear_righting_spring` | `stability/` contains no expression of the form `-k * phi` |
| `no_maneuver_state` | no `tacking`, `gybing`, `maneuver`, `port_tack`, `starboard_tack` identifiers |

**Acceptance criteria**
- `cargo test -p sailgym-physics --test invariants` exits 0 with 20 tests.
- `cargo test -p sailgym-physics --test no_shortcuts` exits 0 with 5 tests.
- `no_shortcuts.rs` is added to `scripts/check.*` step 4.
- Each grep test is verified to fail when the forbidden pattern is deliberately
  introduced. Do this once per test, then revert. An audit that cannot fail is
  not an audit.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0.
2. **The primary demonstration of brief §46 works end to end, by hand, in the
   browser** — all 16 steps. Record a written walkthrough in the handoff note
   confirming each step was observed.
3. `easing_reduces_heel` passes, and it verifies the *causal chain*
   (tension → boom angle → heeling moment → heel), not just the outcome.
4. `cargo test -p sailgym-physics --test no_shortcuts` green, 5 tests, each
   proven capable of failing.
5. `cargo test -p sailgym-physics --test invariants` green, 20 tests.
6. The boat passes dynamically through `|phi| > 90°` without termination,
   clamping or `NaN` (brief §17).
7. Task 7.3 touched no file under `aero/`, `hydro/` or `rigging/` — confirming
   the F6.4 contract held.
8. `docs/progress/07-handoff.md` records: the measured free-decay roll period,
   the wind speed and sheet setting at which capsize first occurs, and the
   outcome of R2.

## Risks touched

- **R2 — this is the section where it resolves.** With roll live, determine
  empirically whether the boat can sail close-hauled at `close_hauled`'s 3.5 m/s
  without capsizing. Report the answer plainly in the handoff note. If it cannot,
  **do not** add the lateral sailor offset — escalate to the human for sign-off
  as F11 requires. Section 09 depends on knowing this before authoring scenarios.
- **R3** — `roll_mirror_symmetry` and `restoring_moment_sign` extend the audit.
