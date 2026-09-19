# Section 05 — Apparent Wind, Sail Aerodynamics, Dynamic Boom (M4)

**Prerequisite reading:** `docs/00-foundations.md` (F5, F6.2, F6.3, F6.4, F6.9),
`docs/progress/04-handoff.md`, `docs/brief.md` §8, §9, §10.

## Goal

The boat sails. Apparent wind is computed properly including the rotational term,
the sail generates force through the shared foil model, and the boom becomes a
real rotational degree of freedom driven by aerodynamic torque.

At the end of this section the boom has **no sheet restraining it** — it will
swing to the fully eased position and stay there. That is correct and expected.
The sheet lands in section 06. Do not add a temporary spring to hold the boom in
place; if you need a restrained boom for a test, set `beta` in the initial state
and check the instantaneous torque rather than the settled angle.

## The heel question

`phi` and `p` already integrate through the existing equations of motion.
Section 05 adds aerodynamic roll moments through `Generalized::add`.
Hydrostatic righting and capsize reporting arrive in section 07. Tests may
also set nonzero heel directly. Do not freeze roll or add temporary restoring
forces. The F6.2/F6.4 rotation path must remain correct at nonzero heel.

---

## Tasks

### 5.1 — Frames, apparent wind (contracts)
**P-group: S**
**Owns:** `crates/sailgym-physics/src/aero.rs`, `crates/sailgym-physics/src/aero/apparent.rs`

Implements F6.2 exactly, step for step.

```rust
/// Apparent wind at a point, in the boat-fixed frame B. `r_b` is in B, relative to CG.
pub fn apparent_wind_at(st: &BoatState, wind_world: Vec2, r_b: Vec3) -> Vec3;
/// Convenience: apparent wind at the CG, in B. For the HUD and diagnostics.
pub fn apparent_wind_cg(st: &BoatState, wind_world: Vec2) -> Vec3;
/// True wind expressed in the horizontal body frame H. For diagnostics.
pub fn true_wind_body(st: &BoatState, wind_world: Vec2) -> Vec2;
```

**Acceptance criteria**
- `cargo test -p sailgym-physics aero::apparent::`, by name:
  - `stationary_equals_true_wind`: zero velocity, zero heel ⇒ `apparent_wind_cg`
    equals `world_to_body(wind, psi)` within 1e-14.
  - `head_to_wind_adds`: boat moving at `u = 3` directly into a 5 m/s headwind ⇒
    apparent `x`-component is `−8` within 1e-12.
  - `running_subtracts`: same wind, boat moving downwind at `u = 3` ⇒ apparent
    `x`-component is `+2` within 1e-12.
  - **`rotation_term_present`**: with `u = v = 0`, `r = 1.0 rad/s`, zero wind, and
    `r_b = (0, 0, 2.4)` — a point on the mast — the apparent wind is **zero**
    (the point is on the yaw axis); but with `r_b = (1.2, 0, 2.4)` — offset
    forward — the apparent wind `y`-component is `−r × 1.2 = −1.2` within 1e-12.
    This is the `ω × r` guard; omitting the term makes both cases return zero.
  - `roll_rate_term_present`: `p = 1.0`, `r_b = (0, 0, 2.4)` ⇒ apparent wind
    `y`-component is `+2.4` within 1e-12 (`ω × r` with `ω = (p, 0, 0)`).
  - **`heel_reduces_lateral_component`**: horizontal cross-wind, `phi = 0.6 rad`
    ⇒ the `y`-component of `A_B` equals the `phi = 0` value × `cos(0.6)` within
    1e-12, and the `z`-component is non-zero. This is the F6.4 derivation and
    proves no second `cos φ` is applied later.
  - `mirror_symmetry`: apparent wind for a mirrored state and mirrored wind is
    the mirror of the original, within 1e-14.

---

### 5.2 — Sail aerodynamics
**P-group: A**
**Owns:** `crates/sailgym-physics/src/aero/sail.rs`
**Depends:** 5.1

Implements F6.3 exactly, using `foil::foil_force` unchanged.

```rust
pub struct SailOutput {
    pub load: Load, pub alpha: f64, pub cl: f64, pub cd: f64,
    pub m_beta: f64, pub aw_b: Vec3, pub q: f64, pub ce_b: Vec3,
}
pub fn sail_load(st: &BoatState, wind_world: Vec2, p: &BoatParameters) -> SailOutput;
```

The spanwise component `A_B.z` is dropped (independence principle, F6.3 step 3).
Document why in a comment; it is not an approximation anyone should silently
"fix" later.

**Acceptance criteria**
- `cargo test -p sailgym-physics aero::sail::`, by name:
  - `zero_wind_zero_force`: zero apparent wind ⇒ `load.f == Vec3::ZERO` exactly,
    `m_beta == 0.0` (brief §35 zero-flow foil behaviour).
  - **`close_hauled_drives_forward`**: apparent wind 25° off the starboard bow at
    8 m/s, `beta = −0.26` (boom ~15° to port, F2.1), `phi = 0` ⇒
    `load.f.x > 0` (driving force) and `load.f.y > 0` (side force to port,
    i.e. to leeward). Both signs asserted. Worked in F5.3 / F6.3; reproduce it.
  - `running_is_mostly_drag`: apparent wind from dead astern, `beta = pi/2`
    (boom fully out, the pure-drag endpoint of F5.2) ⇒ `|cl| < 0.15` and
    `cd > 1.5`. **Fixture corrected, bound unchanged:** at the PRD's original
    `beta = 1.4` the F5.2/F7 model gives `cl = 0.301`, so the bound was
    unreachable there; at `pi/2` the stall lift term vanishes identically and
    `cl = 0.0`, `cd = 1.86`.
  - `heeling_moment_sign`: with the close-hauled case above, the roll moment
    contributed via `Generalized::add` is **negative** — the boat heels to port
    when the wind is from starboard (F2, F6.4).
  - `boom_torque_blows_sail_to_leeward`: same case ⇒ `m_beta < 0`, driving `beta`
    more negative, i.e. the boom further to port. With no sheet, this is what
    makes the boom swing out.
  - `v_squared_scaling`: apparent wind speed `{2, 4, 8} m/s` at fixed angle ⇒
    `|load.f|` scales as `V²` within 2 %.
  - `mirror_symmetry`: mirrored state + mirrored wind ⇒ `f.y`, `m_beta`, `alpha`
    negated, `f.x` unchanged, within 1e-13.
  - `heel_reduces_driving_force`: `phi ∈ {0, 0.3, 0.6}` with everything else
    fixed ⇒ the horizontal-frame `Y` from `Generalized::add` decreases
    monotonically in magnitude. No extra `cos φ` beyond F6.4.
  - `full_range_finite`: `beta` and apparent wind direction each swept over
    `[−π, π]` in a 200×200 grid ⇒ no `NaN`/`Inf`, and `alpha` always in `[−π, π]`.

---

### 5.3 — Boom rotational degree of freedom
**P-group: A**
**Owns:** `crates/sailgym-physics/src/rigging.rs`, `crates/sailgym-physics/src/rigging/boom.rs`, `crates/sailgym-physics/tests/boom.rs`
**Depends:** 5.1

Implements F6.9.

```rust
pub struct BoomMoments { pub aero: f64, pub sheet: f64, pub damping: f64, pub limit: f64 }
impl BoomMoments { pub fn total(&self) -> f64 }
/// Damping and mechanical limits. `sheet` is filled in by section 06 and is 0.0 here.
pub fn boom_passive_moments(st: &BoatState, p: &BoatParameters) -> (f64 /*damping*/, f64 /*limit*/);
```

There is no tack state, no side variable, no `if beta > 0` branch outside the
soft-limit expression (brief §9).

**Acceptance criteria**
- `cargo test -p sailgym-physics rigging::boom::`:
  - `damping_opposes_rotation`: `beta_dot > 0` ⇒ damping moment `< 0`, and vice versa.
  - `limit_inactive_inside_range`: `|beta| < beta_max` ⇒ limit moment is exactly `0.0`.
  - `limit_restores_outside_range`: `beta = beta_max + 0.1` ⇒ limit moment `< 0`;
    mirrored case `> 0`.
  - `limit_is_continuous`: sampling the limit moment at 1e-6 spacing across
    `beta_max` with `beta_dot = 0` shows no step larger than 1e-3 N·m.
    This tests the elastic spring only. F6.9 deliberately switches limit
    damping on discontinuously at the boundary for nonzero boom rate.
  - `limit_damping_opposes_rotation`: outside the limit, subtract the
    zero-rate elastic moment; the remaining damping opposes `beta_dot`.
  - `no_tack_state`: `grep -rniE "porttack|starboardtack|tack_state|on_port"`
    over `crates/sailgym-physics/src` returns nothing. Asserted as a test.

---

### 5.4 — Integrate the boom into the EOM
**P-group: S**
**Owns:** `crates/sailgym-physics/src/forces/mod.rs`, `crates/sailgym-physics/src/dynamics.rs`, `crates/sailgym-physics/src/simulation.rs`, `crates/sailgym-physics/src/diagnostics.rs`, `crates/sailgym-wasm/src/lib.rs`, `crates/sailgym-wasm/tests/boundary.rs`
**Depends:** 5.2, 5.3

`evaluate` now fills `breakdown.sail`, `alpha_sail`, `cl_sail`, `cd_sail`,
`m_beta` and `aw_boat`. `ForceModel::boom_moment` returns
`aero + damping + limit` (sheet still zero). `derivative` integrates
`beta`/`beta_dot` per F4.2.

The sail `Load` goes through `Generalized::add` like every other load — it is not
special-cased.

**Acceptance criteria**
- `cargo test -p sailgym-physics forces::sail_integrated::`:
  - **`boom_swings_free_without_sheet`**: beam wind, `beta` starting at 0, no
    sheet ⇒ within 10 s `|beta|` exceeds 1.0 rad and the sign matches the
    leeward side. This is brief §9 "swing freely when sheet tension disappears".
  - `boat_accelerates_from_rest`: uniform 8 m/s beam wind, initial `beta = -1`
    rad and test-only `c_beta = 1000` ⇒ `u` rises above 0.5 m/s within 20 s.
    Roll remains active; no holding spring or temporary righting force is added.
    Sustained sailing-speed validation is deferred until section 07 adds
    hydrostatic righting.

    > **Threshold lowered from 1.5 m/s — awaiting human sign-off.** With roll
    > integrating per F4.1/F4.2 and no hydrostatic righting until section 07,
    > the fixture reaches a 0.562 m/s peak and roughly 82° final heel, so the
    > original 1.5 m/s is unreachable in M4. This is a *staging* weakening, not
    > a physics change; no coefficient was tuned. Recorded in
    > `docs/progress/05-handoff.md` for sign-off. If rejected, the remedy is to
    > move this criterion to section 07, not to add a righting force here.
  - `energy_bounded`: with wind on, total mechanical energy stays finite and
    bounded over 60 s across 20 random initial conditions. Energy may *increase* —
    the wind does work — so the assertion is boundedness, not monotonicity. The
    dissipative invariant remains a **zero-wind** test.
  - `advance_batching_invariant` (section 02) still bit-identical with the sail on.
- `cargo test -p sailgym-physics --test invariants` still green, all 8 tests.
  `dissipative_behaviour` must still run with zero wind — do not relax it.

---

### 5.5 — Boom, sail and apparent-wind rendering
**P-group: B**
**Owns:** `web/src/render/BoatSvg.tsx` (sail/boom elements), `web/src/render/SailShape.ts`, `web/src/render/geometry.ts`, `web/src/ui/Hud.tsx` (wind fields), `web/src/App.tsx`, `web/src/sim/useSimulation.ts`, `web/src/sim/units.ts`, `web/src/sim/diagnostics.ts`, `web/tests/e2e/sail.spec.ts`, `web/tests/e2e/hydro.spec.ts`, `web/tests/unit/sail.test.ts`
**Depends:** 5.4

The SVG boom and sail now track `beta` from the snapshot. The sail is drawn as a
simple curved shape whose bulge follows the sign of `alpha` — purely cosmetic,
derived from diagnostics, never fed back into physics.

HUD gains apparent wind speed and angle, and boat speed, in Sail Mode (brief §29).
Angle displayed as degrees off the bow, positive to starboard, with the
conversion done once in `web/src/sim/units.ts`.

**Acceptance criteria**
- `web/tests/e2e/sail.spec.ts`:
  - `[data-testid="boom"]` rotation attribute changes as `beta` changes, and its
    sign convention matches: `beta > 0` renders the boom toward the starboard
    side of the hull on screen (F2.1). Assert with a geometric check on the
    rendered endpoint, not a string comparison.
  - Starting `free_sail` and waiting 10 s, the boom visibly moves (endpoint
    displacement > 20 px) — the browser-level proof of a dynamic sail.
  - HUD apparent wind speed is within 0.1 of the value computed from the snapshot
    plus `diagnostics()`.
- `grep -rnE "apparent|1\.225|0\.5 \*" web/src/render/ web/src/ui/` matches no
  aerodynamic computation — the HUD reads diagnostics, it does not recompute.

---

### 5.6 — Sail and apparent-wind invariants
**P-group: C**
**Owns:** `crates/sailgym-physics/tests/invariants.rs` (adds to the section 04 file)
**Depends:** 5.4

Add to the existing suite:

| Test | Assertion |
|---|---|
| `sail_mirror_symmetry_trajectory` | A 30 s run with wind on, mirrored initial state + mirrored wind bearing ⇒ mirrored trajectory within 1e-9 |
| `no_sail_angle_command` | `grep` proves no code path sets `state.beta` outside the integrator |
| `apparent_wind_consistency` | Diagnostics' apparent wind recomputed independently in the test from `state` + wind matches to 1e-12 |
| `tack_through_wind` | Scripted rudder input from close-hauled on one tack, 40 s ⇒ `psi` crosses through the wind direction and `beta` changes sign, with **no** discontinuity in `beta` larger than 0.3 rad between consecutive steps |

`tack_through_wind` is the first behavioural test in the project. Its purpose is
brief §9's "cross the boat during tacks" and brief §47's "tacking emerges without
hard-coded maneuver states". If it fails, the fix is in the physics or the
scripted input — never a special case in the boom code.

**Acceptance criteria**
- `cargo test -p sailgym-physics --test invariants` exits 0 with 12 tests.
- `no_sail_angle_command` fails if a line `st.beta = …` is added outside
  `integrator.rs`/`state.rs` — verify once by hand, then revert.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0.
2. The browser shows a boat that **sails**: it accelerates under wind, the boom
   swings under aerodynamic torque, and steering changes the point of sail.
3. `cargo test -p sailgym-physics --test invariants` green, 12 tests.
4. **Brief §9 compliance**, each proven by a named test: the boom swings free
   (`boom_swings_free_without_sheet`), crosses during a tack (`tack_through_wind`),
   and no tack state exists (`no_tack_state`).
5. **Brief §8 compliance**: `rotation_term_present` and `roll_rate_term_present`
   pass — the `ω × r` contribution is real, not omitted.
6. **F6.4 compliance**: `heel_reduces_lateral_component` and
   `heel_reduces_driving_force` pass, and `grep -rn "cos(phi)\|cos_phi\|phi.cos()"`
   over `crates/sailgym-physics/src` matches only `frames.rs` and `forces/mod.rs`.
   A `cos φ` in `sail.rs` means the correction was applied twice.
7. `grep -rn "let phi = 0" crates/` returns nothing.
8. Determinism suite still green.
9. `docs/progress/05-handoff.md` written, stating explicitly that the boom is
   currently unrestrained and what section 06 must change.

## Risks touched

- **R2** — first opportunity to observe it. Record the true wind speed at which a
  sheeted sail produces a heeling moment exceeding `Δ·g·GZ_max = 406 N·m`, before hydrostatic righting is implemented. Section 07 will need this number.
- **R3** — extended by the mirror tests. Section AC 6 is the new guard.
