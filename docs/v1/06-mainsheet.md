# Section 06 — Physical Mainsheet (M5)

**Prerequisite reading:** `docs/00-foundations.md` (F6.8, F6.9, F11/R1),
`docs/progress/05-handoff.md`, `docs/brief.md` §11, §12.

## Goal

A geometric, unilateral, elastic mainsheet that restrains the boom through rope
tension and geometry, controlled by a payout/haul **rate** command from the
mouse. After this section the primary demonstration of brief §46 is half
possible: haul in and the sail stays powered; release and the boom swings out and
the sail depowers.

## The unilateral constraint is structural

`T ≥ 0` (brief §11) is not a post-hoc clamp on a computed value. The model is
`T = max(0, k·e + c·ė)` and the `max` *is* the model. There is no branch anywhere
that says "if the sheet is slack, do something different" — slack simply produces
zero tension and therefore zero torque, through the same expression.

---

## Tasks

### 6.1 — Mainsheet geometry and tension (contracts)
**P-group: S**
**Owns:** `crates/sailgym-physics/src/rigging/mainsheet.rs`

Implements F6.8 exactly.

```rust
pub struct SheetOutput {
    pub tension: f64,        // N, ≥ 0 always
    pub m_beta: f64,         // N·m, boom moment about +z_B at the mast
    pub boom_load: Load,     // force on the boom, applied at P_b, in B
    pub hull_load: Load,     // reaction −F_b on the hull, applied at P_k, in B
    pub rope_length: f64,    // ℓ(β), the geometric path length
    pub extension: f64,      // e = ℓ − L (may be negative when slack)
}
/// `l_sheet_dot` is the commanded payout rate, needed for the damping term ė.
pub fn sheet_output(st: &BoatState, l_sheet_dot: f64, p: &BoatParameters) -> SheetOutput;

/// Geometry helpers, public for diagnostics and rendering.
pub fn boom_attach_point(beta: f64, p: &BoatParameters) -> Vec3;   // P_b(β)
pub fn rope_path_length(beta: f64, p: &BoatParameters) -> f64;     // ℓ(β)
pub fn drope_dbeta(beta: f64, p: &BoatParameters) -> f64;          // dℓ/dβ, analytic
```

The hull reaction is **not dropped** (F6.8). A rope pulling down and inboard on
the boom pushes up and outboard on the transom block; that contributes to heel
and yaw, and omitting it silently violates Newton's third law.

`l_sheet_dot` must be the *same* value the integrator uses for `L̇` in this
derivative evaluation, otherwise the damping term is inconsistent across RK2
stages. Pass it explicitly; do not recompute it from controls inside this module.

**Acceptance criteria**
- `cargo test -p sailgym-physics rigging::mainsheet::`, by name:
  - **`tension_never_negative`**: 100 000 randomised `(beta, beta_dot, l_sheet,
    l_sheet_dot)` combinations, including extreme values ⇒ `tension >= 0.0` in
    every case. Brief §11's fundamental requirement (brief §35 sheet unilateral
    constraint).
  - `slack_rope_zero_tension`: `L > ℓ(β) + 0.01` ⇒ `tension == 0.0` exactly and
    `m_beta == 0.0` exactly, and both `Load`s are exactly `Vec3::ZERO`.
  - `taut_rope_positive_tension`: `L < ℓ(β) − 0.01` with `beta_dot = 0` ⇒
    `tension == k_sheet × (ℓ − L)` within 1e-9.
  - `damping_can_zero_tension`: taut rope but a fast-easing `l_sheet_dot` large
    enough that `k·e + c·ė < 0` ⇒ `tension == 0.0` exactly. The `max` doing its job.
  - `drope_dbeta_analytic_matches_numeric`: central difference of
    `rope_path_length` matches `drope_dbeta` within 1e-7 across `beta ∈ [−1.6, 1.6]`.
  - **`sheet_torque_restores_toward_centreline`**: `beta = −0.8` (boom to port),
    rope taut ⇒ `m_beta > 0`, pulling the boom back toward `beta = 0`. Mirror
    case `beta = +0.8` ⇒ `m_beta < 0`. The core geometric behaviour.
  - `newton_third_law`: `boom_load.f == −hull_load.f` exactly.
  - `no_boom_angle_assignment`: `grep` proves `mainsheet.rs` never writes to
    `beta` — tension produces torque, it does not set an angle (brief §11).
  - `mirror_symmetry`: mirrored state ⇒ `tension` unchanged, `m_beta` negated,
    load `y`-components negated, within 1e-13.
  - `finite_at_extremes`: `L` at both clamp bounds, `beta` swept over `[−π, π]`,
    `beta_dot` up to ±20 rad/s ⇒ no `NaN`/`Inf`, `tension < 1e6 N`.

---

### 6.2 — Sheet state integration and the sheet-rate control path
**P-group: S**
**Owns:** `crates/sailgym-physics/src/dynamics.rs`, `crates/sailgym-physics/src/forces/mod.rs`
**Depends:** 6.1

`derivative` computes `L̇` from `Controls` per F4.3 and passes it to both
`sheet_output` and the `l_sheet` derivative, so the two agree within every RK2
stage.

```rust
/// Commanded payout rate, m/s. Positive = easing. Clamped so L stays in range.
pub fn sheet_rate(c: &Controls, st: &BoatState, p: &BoatParameters) -> f64;
```

Semantics (brief §12): the control commands `L̇`, never the sail angle.
`sheet_release` (Space) overrides `sheet_rate_cmd` and eases at
`sheet_release_rate`. Clamping at `l_sheet_min`/`l_sheet_max` happens inside the
derivative (F4.3).

`evaluate` fills `breakdown.sheet` and `sheet_tension`; `boom_moment` now returns
`aero + sheet + damping + limit` — the complete F6.9 expression.

**Acceptance criteria**
- `cargo test -p sailgym-physics forces::sheet_integrated::`:
  - **`hauling_restrains_boom`**: beam wind, start with the sheet eased and the
    boom out; command a haul; `|beta|` decreases monotonically while the rope is
    taut, and `sheet_tension > 0` throughout.
  - **`release_depowers`**: from a hauled, powered, heeled-torque state, command
    `sheet_release`; within 3 s `sheet_tension` reaches `0.0` and `|beta|`
    increases past 1.0 rad. Brief §46 steps 11–14.
  - `clamp_respected`: `l_sheet` never leaves `[l_sheet_min, l_sheet_max]` across
    200 random control sequences, tolerance 0 (exact).
  - `rate_command_not_angle_command`: a test asserts that with a constant haul
    command, `beta` follows a trajectory determined by the wind, not a fixed
    ramp — specifically, that doubling the apparent wind speed changes the
    resulting `beta(t)` by more than 0.1 rad at `t = 2 s`. If the sheet were
    commanding an angle, the wind would not matter.
  - `advance_batching_invariant` still bit-identical with the sheet on.
- **R1 stability gate**: `sheet_stiffness_stability` — run 60 s of a hauled beam
  reach at `dt ∈ {0.01, 0.005, 0.0025}` and `k_sheet ∈ {1e4, 2e4, 3e4}`. All nine
  combinations must remain finite with `|beta_dot| < 50 rad/s`. Record the results
  as a table in the handoff note. If `k_sheet = 2e4` at `dt = 0.01` is unstable,
  say so — do not silently change the default.

---

### 6.3 — Mouse sheet control
**P-group: A**
**Owns:** `web/src/sim/sheetInput.ts`, `web/src/sim/controls.ts` (sheet path only)
**Depends:** 6.2

Brief §12: vertical mouse drag hauls or eases; Space releases. Initial UX does
**not** require grabbing a specific SVG rope segment (brief §12) — that is
deferred.

```ts
export interface SheetInputState {
  dragging: boolean; lastY: number | null; accumulated: number
}
/** Pure reducer. Left-drag anywhere on the world view maps vertical motion to a rate. */
export function reduceSheetInput(s: SheetInputState, ev: SheetEvent,
                                 cfg: InputConfig): [SheetInputState, number /*rateCmd*/]
```

Left-drag is the mainsheet (reserved for it in section 02); camera pan stays on
middle-drag/Shift-drag. They must not conflict (brief §27).

Direction convention, fixed here and displayed in the help overlay: **drag down =
haul in, drag up = ease**. Pick it, state it, make `sheetInvert` in `InputConfig`
the escape hatch, and do not change it later without updating the help text.

**Acceptance criteria**
- `pnpm --dir web test:unit sheetInput`:
  - dragging down 100 px with `sheetDragGain = 0.004` yields a haul command of
    `−0.4` within 1e-9;
  - dragging up yields the positive mirror;
  - `sheetInvert: true` flips both;
  - releasing the mouse returns the rate command to exactly `0`;
  - a Shift-drag produces a sheet rate of exactly `0` (it belongs to the camera).
- `web/tests/e2e/sheet.spec.ts`: a synthetic downward drag decreases `lSheet` in
  the snapshot; Space increases it rapidly; camera pan via Shift-drag leaves
  `lSheet` unchanged.

---

### 6.4 — Rope rendering
**P-group: A**
**Owns:** `web/src/render/SheetRope.tsx`
**Depends:** 6.2

The displayed rope must visibly correspond to sheet state (brief §12). Draw the
path from the boom attachment to the block using `rope_path_length` and
`l_sheet` from diagnostics: taut when `e > 0`, with a catenary-ish sag
proportional to `max(0, L − ℓ)` when slack.

The sag is cosmetic and computed in TS from the two lengths; it feeds nothing
back into physics.

**Acceptance criteria**
- `web/tests/e2e/sheet.spec.ts`:
  - `[data-testid="sheet-rope"]` path endpoints coincide with the rendered boom
    attachment and the block within 3 px at three different `beta` values;
  - after pressing Space, the rendered path length grows and visible sag appears
    (path bounding-box height increases by > 5 px);
  - hauling fully in removes the sag.
- `grep -rn "k_sheet\|tension" web/src/render/SheetRope.tsx` matches only reads
  from the diagnostics object.

---

### 6.5 — Mainsheet invariants
**P-group: B**
**Owns:** `crates/sailgym-physics/tests/invariants.rs` (adds to the existing file)
**Depends:** 6.2

| Test | Assertion |
|---|---|
| `sheet_unilateral_constraint` | Across 50 full 60 s episodes with randomised control sequences, `sheet_tension >= 0` at **every** step. Brief §35. |
| `sheet_does_no_negative_work` | Over a zero-wind episode started with kinetic energy in the boom, total mechanical energy including the sheet's elastic term `½k·max(0,e)²` is non-increasing within 1e-9/step |
| `slack_sheet_free_boom` | With `L = l_sheet_max` and zero wind, the boom's decay from an initial `beta_dot` matches pure damping `I_b β̈ = −c_β β̇` within 1e-9 — proving the sheet contributes nothing when slack |
| `sheet_geometry_continuous` | Sweeping `beta` across `[−π, π]` at fixed `L`, `m_beta` has no step larger than 1 N·m between samples 1e-5 apart |

**Acceptance criteria**
- `cargo test -p sailgym-physics --test invariants` exits 0 with 16 tests.
- `sheet_does_no_negative_work` uses the **elastic energy term**; a version that
  omits it will fail spuriously, which is the point — it forces the energy
  accounting to be correct.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0.
2. **Brief §46 steps 4–6 and 11–14 are demonstrable by hand** in the browser:
   hauling restrains the boom and keeps the sail powered; releasing drops tension
   and lets the boom swing out, depowering the sail. Heel does not yet respond —
   that is section 07.
3. `T ≥ 0` proven by `tension_never_negative` (100 000 cases) and
   `sheet_unilateral_constraint` (50 episodes). Brief §11's hard requirement.
4. `cargo test -p sailgym-physics --test invariants` green, 16 tests.
5. No angle assignment: `no_boom_angle_assignment` passes, and
   `grep -rn "\.beta = " crates/sailgym-physics/src` matches only `state.rs` and
   `integrator.rs`.
6. **R1 recorded**: the 3×3 stability table from 6.2 is in
   `docs/progress/06-handoff.md`, with an explicit statement of whether the
   default `k_sheet` and `dt` are in the stable region and by what margin.
7. Camera and sheet controls do not conflict (E2E asserted in 6.3).
8. Determinism and all earlier invariants still green.

## Risks touched

- **R1** — the central risk of this section. The 3×3 gate in 6.2 is mandatory.
  If instability appears, apply the F11 mitigations **in the stated order**
  (lower `k_sheet`, then raise `c_sheet`, then sub-step the rigging DOF) and
  record which was used. Reducing the global `dt` is the last resort and must be
  called out explicitly in the handoff.
- **R2** — with the sheet in place the heeling moment is now controllable.
  Record the sheet setting and wind speed at which heeling moment reaches
  406 N·m; section 07 will capsize at exactly that point.
