# Section 04 — Handoff (M3: hull, centreboard and rudder hydrodynamics)

**Written per F13.6.** Read this before starting section 05
(`docs/05-sail-boom.md`). Conventions remain normative in
`docs/00-foundations.md`; nothing below redefines them.

Status: **complete, with two items escalated to the human** (§2.4 and §2.5).
`scripts/check.sh` exits 0 end to end (8/8), twice in succession. R4 is closed.

> **Host change, read first (R7).** Sections 01–03 ran on Windows
> (`x86_64-pc-windows-msvc`). **This section ran on Linux
> (`x86_64-unknown-linux-gnu`)**, and `pwsh` is not installed here, so the gate
> was run as `scripts/check.sh` — the documented Linux/CI equivalent, which
> `scripts/check.ps1` mirrors step for step and which neither this section nor
> any earlier one modified. Section acceptance criterion 1 names
> `pwsh scripts/check.ps1`; that exact command was **not** executed on this
> host. Per F9 the bit-identity guarantee is per-platform, so any golden file
> recorded against section 01's Windows fingerprint is now on a different
> platform. See §5.

---

## 1. What landed

### Task 4.1 — Shared foil model and test harness (P-group S, section agent)

- **`crates/sailgym-physics/src/foil.rs`** — F5 in full: `smoothstep`,
  `angle_of_attack`, `cl`, `cd`, `foil_force`, `FoilParams`, `EPS_FLOW`, plus
  the private `lift_curve_slope`, `alpha_effective`, `stall_fraction` and
  `cl_attached` that the F5.2 formulae name. There is **no branch on the sign
  of `α`** anywhere, which is what makes `cl` exactly odd and `cd` exactly even
  bit-for-bit. 10 named tests.
- **`crates/sailgym-physics/src/forces/mod.rs`** — `ForceBreakdown`,
  `PhysicalForces`, `evaluate`, and the private `NoWind` field (§2.11).
- **`crates/sailgym-physics/src/testkit.rs`** (new) — `WithExternalLoad`,
  `still_air`, `uniform_wind`, `mirror_state`, `mirror_controls`, and the
  file-scope test `testkit::mirror_involution`. Gated
  `#[cfg(any(test, feature = "testkit"))]`; the feature is off by default.

### Task 4.2 — Centreboard (P-group A)

- **`crates/sailgym-physics/src/hydro/centerboard.rs`** — `FoilLoad`,
  `local_flow`, `foil_hydro_load` (the F6.5 workhorse both underwater foils
  call) and `centerboard_load`. 6 named tests.

### Task 4.3 — Rudder (P-group A)

- **`crates/sailgym-physics/src/hydro/rudder.rs`** — `rudder_load`, which is
  `foil_hydro_load` with `rudder_chord(δr)` and the rudder mount. Everything
  brief §14 asks for falls out of that one substitution. 6 named tests.

### Task 4.4 — Hull resistance (P-group A)

- **`crates/sailgym-physics/src/hydro/hull.rs`** — `HullLoads` (§2.3),
  `hull_loads`, and the shared `resist` term. 5 named tests, including the F7
  sanity anchor and the R6 commentary.

### Task 4.5 — Force assembly, EOM wiring, scaffold deletion (P-group S, section agent)

- **`crates/sailgym-physics/src/hydro/mod.rs`** — replaces `hydro.rs` (§2.8);
  module declarations plus the `FoilLoad` / `local_flow` / `foil_hydro_load`
  re-export the PRD's "shared with rudder via hydro::mod" asks for.
- **`crates/sailgym-physics/src/forces/mod.rs`** — `evaluate` sums in the fixed
  F9.4 order: hull, centreboard, rudder, sail (zero), mainsheet (zero),
  roll/hydrostatics (zero). 6 named tests.
- **`crates/sailgym-physics/src/simulation.rs`** — `Box::new(PhysicalForces)`.
- **`crates/sailgym-physics/src/forces/scaffold.rs` — deleted**, together with
  its module declaration, its re-export, and `sim.scaffold_thrust` in
  `parameters.rs`. **R4 closed.**

### Task 4.6 — Invariants and the E2E update (P-group B)

- **`crates/sailgym-physics/tests/invariants.rs`** — all eight brief §35 tests,
  by the names the PRD gives them.
- **`web/tests/e2e/hydro.spec.ts`** — two browser tests: the coasting boat
  slows under hull and foil drag alone, and holding `D` bends the track to
  starboard with the tiller re-centring in Rust on release.

### Section-agent integration work (see §2.9 for the ownership note)

`crates/sailgym-physics/src/lib.rs`, `crates/sailgym-physics/Cargo.toml`,
`crates/sailgym-physics/src/parameters.rs`, `web/src/App.tsx`,
`web/src/sim/useSimulation.ts`, and the three section-02 E2E specs that
assumed the boat could accelerate itself.

---

## 2. Deviations from the PRD, and why

### 2.1 `forces/mod.rs` re-exports `Load`, `Generalized` and `ForceModel` rather than declaring them

Task 4.1 says `forces/mod.rs` "gains the assembly type and the
`Generalized::add` of F6.4, verbatim". Section 02 had already landed `Load`,
`Generalized`, `Generalized::add` (verbatim) and the `ForceModel` trait in
`dynamics.rs`, and `dynamics.rs` is owned by no section-04 task. Declaring a
second `Generalized::add` would have been two copies of the one heel
correction — exactly the drift F6.4 and R3 exist to prevent.

`forces/mod.rs` therefore uses the `dynamics.rs` types and adds only what
section 04 introduces: `ForceBreakdown`, `PhysicalForces` and `evaluate`.
Nothing in F6.4 changed.

### 2.2 `foil::FoilParams` is `parameters::FoilSection` under another name

F5.3 writes `pub struct FoilParams { … }` in `foil.rs`; section 02 catalogued
the identical nine fields as `FoilSection` in `parameters.rs` (its §2.7), where
the F7 tagging test counts them. Declaring the struct twice would put nine
parameters in two places and require a conversion that can silently go stale.

`foil.rs` therefore does `pub use crate::parameters::FoilSection as FoilParams;`.
`foil::FoilParams` exists with exactly the F5.3 field list, `p.board.section`
and `p.rudder.section` are already values of it, and there is one declaration.

### 2.3 `HullLoads` carries `n_yaw` as well as `k_roll`

Task 4.4 specifies `pub struct HullLoads { pub load: Load, pub k_roll: f64 }`
and explains that `K_hull` cannot be a `Load`. **The same is true of `N_hull`**,
which F6.6 also requires and which a `Load` at `r = 0` cannot carry either: the
struct as written has no slot for it, and the task's own
`drag_opposes_motion` criterion asks for the sign of all four DOF including
yaw. `HullLoads` gains `pub n_yaw: f64`. This is an addition the PRD omitted,
not a change to F6.6.

### 2.4 ESCALATION — hull resistance is summed in `H`, not rotated as a boat-fixed load

**This is a foundations ambiguity and it caused a real divergence. Please
confirm the resolution.**

The two passages:

- **F4.4:** "Every force-producing module returns the same struct … `Load` … a
  force and its application point, expressed in the boat-fixed frame `B`", and
  `Generalized::add` applies the F6.4 `cos φ` on the way into `H`.
- **F6.6:** `X_hull = −(X_u·u + X_uu·u·|u|)` … "applied at the CG (`r = 0`)
  except `K_hull`, which is added directly to `ΣK`." Here `u, v, r, p` are
  **F3 state fields defined in `H`**, and F4.2 consumes the result as
  `ΣX, ΣY, ΣN, ΣK` in `H`.

Read as a boat-fixed `Load`, `Y_hull` picks up a factor `cos φ` on its way into
`H`. Past 90° of heel `cos φ` changes sign, so the sway damping starts
**driving** the motion it is meant to resist. That is not theoretical: it
reached `Inf` in `tests/determinism.rs::finite_under_random_controls`,
episode 18, within about one second of the boat rolling past `φ = −90°`
(`v` went −2.8 → −6.5 m/s in 33 steps and then exploded). At `φ = π` the same
reading has hull drag pushing the boat forwards.

Resolution implemented: `evaluate` adds the four F6.6 terms straight into
`total.x`, `total.y`, `total.n`, `total.k`. The foils, the sail, the sheet and
the righting moment all still go through `Generalized::add` exactly as F6.4
specifies. `forces::tests::damping_survives_inversion` is the guard: with the
foils switched off it asserts, for 200 random velocity states × 25 heel angles
spanning `[−π, π]`, that the hull contribution is bit-identical at every `φ`
and that each term opposes its own velocity.

brief §17 requires the boat to pass dynamically through `|φ| > 90°`, so the
boat-fixed reading is not survivable. **No text in `00-foundations.md` was
changed.** If the human prefers a different resolution — e.g. wording F6.6 to
say explicitly that hull resistance is an `H`-frame generalised force — that is
a foundations edit and is yours to make.

### 2.5 ESCALATION — two board acceptance criteria in `04-hydro.md` have the wrong sign

`docs/04-hydro.md`, task 4.2, writes:

> `alpha_equals_leeway_angle`: with `r = p = 0`, `alpha` equals `atan2(v, u)`
> within 1e-9. (Sign check: positive `v` — drift to port — gives positive
> `alpha`, hence lift to starboard, consistent with the test above.)

and

> `yaw_rate_contributes`: … Assert it matches `atan2(r·x_board, u)` within 1e-9.

Both are negated relative to what F2 + F5 + F6.5 produce. The derivation, which
uses nothing but foundations:

- F6.5: `v_local_H = −[(u, v, 0) + ω×r_b]`. With `r = p = φ = 0` that is
  `(−u, −v)`, in `B`.
- F6.5 fixes the board chord at `ĉ = (−1, 0)`.
- F5.1: `α = cross(f̂, ĉ).atan2(dot(f̂, ĉ))` with `f̂ ∝ (−u, −v)` gives
  `cross = −v/V`, `dot = u/V`, hence **`α = atan2(−v, u)`**.

The parenthetical in the PRD is also self-contradictory: with `α > 0`, F5.3's
lift direction `(f̂.y, −f̂.x)` puts the board force to **port**, which
*amplifies* leeway and breaks the PRD's own `side_force_opposes_leeway` — a
section-AC-5 test that may never be weakened. The same `−1` applies to
`yaw_rate_contributes`.

Resolution implemented: the physics follows foundations; the two tests assert
`atan2(−v, u)` and `atan2(−r·x_board, u)`, keep their PRD names, and each
carries the reasoning in a comment pointing here. `alpha_equals_leeway_angle`
additionally asserts that `|α|` equals the leeway angle, which is true on
either sign convention. Both `side_force_opposes_leeway` and
`positive_delta_turns_bow_to_starboard` pass with the signs the PRD states for
*them*, so the two criteria above are a slip in the PRD text and not a defect
in the model. **The PRD text should be corrected; I have not edited it.**

### 2.6 `rudder_stalls` asserts the **first** interior maximum

The criterion asks that the yaw-moment magnitude "rises, peaks, then falls",
with the peak inside `[α_stall, α_stall + 3Δ_s]`. It does, at
**δr = 0.220 rad (12.6°)**, inside the band `[0.209, 0.470]`, falling to 60 % of
the peak by `δr ≈ 0.30`. That is the stall, and it is what brief §14 asks for.

But the *global* maximum over a 0 → 0.7 rad sweep is at the far end, because
past the stall F5.2 hands over to the flat-plate branch `C_N,max·sin α·cos α`,
which climbs again to `C_N,max/2 = 0.95` at 45° — slightly above the attached
peak of `C_Lα·sin α_s ≈ 0.88`. That follows from `C_N,max = 1.9`, an F7
ASSUMED value, and **it was not tuned** (brief §43): at those angles the blade
is a brake rather than a rudder, and `delta_r_max` is 0.698 rad in any case.
The test therefore walks up to the first turn-over instead of taking the global
`max`, and additionally asserts that the trough afterwards is below 80 % of the
peak so that "falls" means a stall and not round-off. The reasoning is in the
test's own comment.

### 2.7 `rest_equilibrium` compares twelve fields, not thirteen

`t` is the simulation clock and `ṫ = 1` by F3, so it cannot be bit-identical
after 10 000 steps. Both copies of the test (`forces::tests::rest_equilibrium`
and `invariants::rest_equilibrium`) compare indices 0–11 by `to_bits()`, name
the offending field in the failure message, and assert separately that `t`
advanced. This is the treatment section 02 established for
`integrator::zero_force_zero_motion` (its §2.4), not a new weakening.

### 2.8 `hydro.rs` replaced by `hydro/mod.rs`

Task 4.5 owns `hydro/mod.rs`; section 01 had created `hydro.rs` beside the
`hydro/` directory, and a crate cannot have both. Same resolution section 02
applied to `forces.rs` and section 03 to `environment.rs`. Moved with `git mv`,
so the history is preserved.

### 2.9 Files written that no section-04 task owns

As in sections 02 and 03, the `Owns:` lists cover the new modules but not the
surface they plug into. Every one is listed:

| File | Why |
|---|---|
| `crates/sailgym-physics/src/lib.rs` | `testkit.rs` is a new module and has to be declared. Four lines, `#[cfg]`-gated. |
| `crates/sailgym-physics/Cargo.toml` | The `testkit` feature and the dev-dependency that enables it for the `tests/` targets (§2.10). |
| `crates/sailgym-physics/src/parameters.rs` | Task 4.5's own text says to delete `sim.scaffold_thrust`, and section AC 2's grep requires it, but `parameters.rs` is not in its `Owns:` list. Four lines removed: the field, its doc comment, its `Default` value and its `field_mut` arm. **Nothing else changed; no F7 value was touched.** |
| `web/src/sim/useSimulation.ts`, `web/src/App.tsx` | The `?scenario=` test hook (§2.12). Task 4.6's `hydro.spec.ts` needs "an initial speed set by a test scenario" and no such mechanism existed. |
| `web/tests/e2e/{controls,determinism,render}.spec.ts` | Four tests written against the M1 placeholder's constant thrust assumed a boat at rest would start moving. With R4 closed and no sail until section 05, it does not. **Only the `gotoApp` call changed, to `{ scenario: 'coast' }`; every assertion is byte-for-byte what section 02 wrote,** including the two R3 sign assertions. See §2.13. |

### 2.10 The `testkit` feature is enabled for the `tests/` targets by a self dev-dependency

`#[cfg(test)]` covers the library's own unit tests but **not** the integration
targets under `tests/`, which are separate crates — and
`tests/invariants.rs` needs `mirror_state` and `WithExternalLoad`. Gate step 4
is `cargo test -p sailgym-physics --test invariants` with no `--features`, so
the feature has to arrive some other way.

`crates/sailgym-physics/Cargo.toml` now carries:

```toml
[features]
testkit = []

[dev-dependencies]
sailgym-physics = { path = ".", features = ["testkit"] }
```

Feature unification turns `testkit` on when a test target is built and leaves
it off for `cargo build` and `wasm-pack build`. **Verified both ways:**
`cargo tree -p sailgym-wasm -e features` shows `sailgym-physics feature
"default"` only, and `strings web/src/wasm/sailgym_wasm_bg.wasm | grep -i
"testkit\|mirror_state\|WithExternalLoad"` returns nothing. The only trace in
`Cargo.lock` is one added line naming `sailgym-physics` in its own dependency
list.

### 2.11 `PhysicalForces` sees no wind, and section 05 has to fix that

`evaluate` takes `w: &dyn WindField` exactly as the PRD specifies, but
`ForceModel::generalized` (F4.4, `dynamics.rs`, section 02) **has no wind
parameter**, and `PhysicalForces` is specified as a unit struct. At M3 that is
harmless: the water is still (F6.5), there is no sail, and the wind reaches
only the diagnostic `ForceBreakdown::tw_boat`. So `PhysicalForces::generalized`
calls `evaluate` with a private zero field, `NoWind`.

**This is the first thing section 05 must deal with** — see §7 item 2. It is
recorded here rather than solved because changing the `ForceModel` signature
means editing `dynamics.rs`, which no section-04 task owns.

### 2.12 A test-only `?scenario=` hook, replaced by section 09

`gotoApp(page, { scenario })` has appended `?scenario=` since task 1.4, and the
app ignored it. `App.tsx` now maps a name through a two-entry table to an
**initial surge speed** (`coast` → 4 m/s) and hands it to `useSimulation`,
which builds a `Sim::reset` document from the core's own initial snapshot with
`u` replaced. Everything else — notably `l_sheet`, which starts at
`l_sheet_min` — keeps the value Rust chose, so **no parameter and no physics is
duplicated in TypeScript** (F8). The `4` is an initial condition, not an F7
coefficient. Section 09 replaces the table with the real scenario system
(brief §32).

### 2.13 Four section-02 browser tests now start from `coast`

The propulsion gap reaches the browser: with the M1 placeholder deleted and the
sail still two sections away, a boat starting at rest stays at rest, and
`controls.spec.ts` "D steers the bow to starboard" / "A mirrors both signs",
`determinism.spec.ts`, and `render.spec.ts` "the boat is drawn and moves" all
depended on it moving. The fix is one argument per test. **No assertion,
tolerance or timeout was touched**, and the two R3 sign assertions
(`deltaR > 0 && psi < 0`, and its mirror) are exactly as section 02 wrote them —
they now exercise the real rudder rather than the placeholder's
`−YAW_AUTHORITY · δr · u`, which is strictly stronger.

**No physical coefficient was tuned and no F7 value changed** (section AC 7).
The only parameter edit is the *deletion* of `sim.scaffold_thrust`, which task
4.5 mandates and which was never physics. The F7 tagging test now counts 62
leaf fields instead of 63, still above its floor of 55.

**No contradiction between the brief and `00-foundations.md` was found.** §2.4
is an ambiguity *within* foundations (F4.4 vs F6.6); §2.5 is a PRD-vs-
foundations slip. Both are escalated above and neither was resolved silently.

---

## 3. Validation evidence

Every command below was run on this host and its exit status observed.

### Section acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | `pwsh scripts/check.ps1` exits 0 | **pass, via `scripts/check.sh`** — 8/8, `check: all steps passed`, twice. Step 3: 103 lib + 7 determinism + 5 wind + 2 harness. Step 4: 8. Step 8: **78 passed** across chromium, firefox, msedge, 1.3 min. `pwsh` is not installed on this host, so the literal PowerShell command was **not** run; see the note at the top. |
| 2 | The scaffold is gone, proven by the 4.5 greps. R4 closed | **pass** — all three greps return nothing; see below |
| 3 | `--test invariants` green, 8 tests | **pass** — 8 passed, 2.54 s, all present by the PRD's names |
| 4 | `--test determinism` still green | **pass** — 7 passed. It was **not** green on the first run with the real model; §2.4 is what it caught and how it was fixed |
| 5 | Sign audit extended, four named tests | **pass**, all four (below) |
| 6 | Exactly one lift/drag implementation | **pass in substance**, with a grep caveat (below) |
| 7 | F7 values unchanged, or every change recorded | **pass** — no value changed; `sim.scaffold_thrust` deleted per task 4.5 |
| 8 | Handoff written with the measured anchor and R6 commentary | **pass** — this file, §4 |

### Section AC 2 — the scaffold greps, run verbatim

```
$ git ls-files | grep scaffold                                   -> nothing (exit 1)
$ grep -rni "scaffold" crates/ web/ --include=*.rs --include=*.ts --include=*.tsx
                                                                 -> nothing (exit 1)
$ git grep -n "scaffold_thrust" -- .                             -> 7 hits, all under docs/
```

The third grep's only matches are `docs/02-kinematics-svg.md`,
`docs/04-hydro.md` (the criterion itself) and `docs/progress/02-handoff.md`,
i.e. nothing outside `docs/` as required. An early draft of this section left
the word "scaffold" in four explanatory comments; they were reworded to "M1
placeholder force model" so the grep stays a real check rather than matching
prose about the thing it is looking for.

### Section AC 5 — the sign audit

| Test | Result |
|---|---|
| `foil::tests::lift_direction_sign` | **pass** — flow `(−1, 0)`, chord `rudder_chord(+0.2)` ⇒ `f.y > 0` (to port), and the mirrored case is its exact negation |
| `hydro::rudder::tests::positive_delta_turns_bow_to_starboard` | **pass** — `u = 3`, `δr = +0.2` ⇒ `M_z < 0` |
| `hydro::centerboard::tests::side_force_opposes_leeway` | **pass** — `u = 3`, `v = +0.3` ⇒ `f.y < 0`, and the mirror image the other way |
| `invariants::mirror_symmetry_trajectory` | **pass** — 20 s (4 000 steps), all 13 fields within 1e-10 at every step, with the trajectory asserted to have actually left the centreline |

Section 02's four sign assertions are also still green, including the browser
one (§2.13).

### Section AC 6 — one lift/drag implementation

```
$ grep -rn "fn cl\|fn cd\|C_L\|lift_coefficient" crates/sailgym-physics/src
```

matches `foil.rs` **and two lines in `integrator.rs`**:
`fn clamp_actuators` and `fn clamp_in_derivative`. Both are section-02 actuator
clamping and neither is a coefficient; `fn cl` is simply a prefix of
`fn clamp`. With word boundaries —

```
$ grep -rnE "\bfn cl\b|\bfn cd\b|C_L|lift_coefficient" crates/sailgym-physics/src
```

— the only file that matches is `foil.rs`. The criterion holds; the grep as
written needs `\b`, which section 10 should adopt.

### Task acceptance criteria

| Task | Criterion | Result |
|---|---|---|
| 4.1 | `foil::alpha_equals_rudder_angle`, 1e-12, `δ ∈ [−0.6, 0.6]` | **pass** — 121 samples |
| 4.1 | `foil::cl_is_odd`, bit-identical, 200 values over `[−π, π]` | **pass** — by `to_bits()`, for the rudder, board **and** sail sections. Verified first that `sin`, `cos`, `tanh` and `atan2` are exactly symmetric on this host (0 mismatches in 20 001 samples), which is what makes bit-identity achievable without a `sign(α)` branch F5.2 forbids |
| 4.1 | `foil::cd_is_even`, bit-identical, same sweep | **pass** |
| 4.1 | `foil::cl_zero_at_zero_and_pi` | **pass** — `cl(0) == 0` exactly; `cl(±π/2)` and `cl(±π)` below 1e-12 |
| 4.1 | `foil::cd_endpoints` | **pass** — `cd(0) == cd0` exactly; the other two within 1e-12 |
| 4.1 | `foil::cl_slope_at_origin`, within 1 % | **pass** for all three sections |
| 4.1 | `foil::continuity_across_stall`, 1e-5 spacing, step < 1e-3 | **pass** — ~35 000 samples per section |
| 4.1 | `foil::zero_flow_zero_force`, exactly `Vec2::ZERO` | **pass** — three sub-`EPS_FLOW` velocities |
| 4.1 | `foil::lift_direction_sign` | **pass** — see the sign audit |
| 4.1 | `foil::finite_over_full_range`, 10 000 points × 5 sets incl. AR 0.5 and 20 | **pass** |
| 4.1 | `testkit::mirror_involution`, bit-identical | **pass** — 200 random 13-field states by `to_bits()`, plus the controls |
| 4.1 | `testkit` absent from the wasm build; feature off by default | **pass** — see §2.10 |
| 4.2 | `zero_flow_zero_lift`, exact `Vec3::ZERO` | **pass** |
| 4.2 | `side_force_opposes_leeway` | **pass** |
| 4.2 | `v_squared_scaling`, {1,2,4,6} m/s within 2 % | **pass** |
| 4.2 | `alpha_equals_leeway_angle`, 1e-9 | **pass, with the sign of §2.5** |
| 4.2 | `yaw_rate_contributes`, 1e-9 | **pass, with the sign of §2.5** |
| 4.2 | `mirror_symmetry`, 1e-14, 50 random states | **pass** |
| 4.3 | `zero_speed_no_authority`, `< 0.1 N·m` at `u = 0.01` | **pass** — measured **0.0101 N·m** at full rudder |
| 4.3 | `positive_delta_turns_bow_to_starboard` | **pass** |
| 4.3 | `authority_grows_with_speed`, monotone and `V²` within 3 % | **pass** — the ratios are exact to round-off, because `α = δr` is independent of `u` when `v = r = 0` |
| 4.3 | `rudder_stalls`, interior peak within `[α_s, α_s + 3Δ_s]` | **pass, as the first interior maximum** — peak at δr = **0.2200 rad (12.6°)**, band `[0.209, 0.470]`, trough afterwards 60 % of the peak. See §2.6 |
| 4.3 | `yaw_damping_sign` | **pass** |
| 4.3 | `mirror_symmetry` | **pass** |
| 4.4 | `drag_opposes_motion`, 200 random `(u, v, r, p)` | **pass**, all four DOF |
| 4.4 | `resistance_anchor`, `48 ± 2 N` at `u = 2.06` | **pass** — measured **48.49 N** |
| 4.4 | `quadratic_dominates_at_speed`, > 90 % at `u = 6`, R6 flagged | **pass** — **91.5 %**; R6 is in the module doc comment |
| 4.4 | `zero_velocity_zero_force`, exact | **pass** |
| 4.4 | `mirror_symmetry` | **pass** |
| 4.5 | The three scaffold greps | **pass** — see above |
| 4.5 | `forces::summation_order_documented` | **pass** — reads `mod.rs` via `include_str!` and walks the six markers in order |
| 4.5 | `forces::rest_equilibrium`, 10 000 steps bit-identical | **pass** — see §2.7 |
| 4.5 | `forces::coast_down_decelerates` | **pass** — from `u = 4`, 12 000 steps (60 s), strictly decreasing, never negative, down to below 1 m/s |
| 4.5 | `forces::energy_not_created`, 50 states, 1e-9/step | **pass** — 5 s each |
| 4.5 | The app still runs; a boat given an initial velocity coasts, decelerates and can be turned | **pass** — `hydro.spec.ts`, both tests, on all three browsers |
| 4.6 | `--test invariants` exits 0 with all 8 present by name | **pass** |
| 4.6 | `check` step 4 runs a non-empty invariant suite | **pass** — it ran 1 harness test before this section and runs 8 real invariants now |
| 4.6 | `pnpm --dir web test:e2e hydro` exits 0 | **pass** — 6/6 (2 tests × 3 browsers) |

### Not in the gate, run anyway (section 03 handoff item 1)

- `pnpm --dir web test:unit` → **6 files, 38 tests**, 0.33 s.
- `pnpm --dir web build` → exit 0, 4.13 s, 1 646 kB / 340 kB gzipped. The
  `@swc/core` pin from section 03 §5 is still doing its job.

---

## 4. Measurements

### Hull resistance (section AC 8, R6)

| `u` | linear `X_u·u` | quadratic `X_uu·u²` | total | quadratic share |
|---|---|---|---|---|
| 2.06 m/s (4 kn) | 10.30 N | 38.19 N | **48.49 N** | 78.8 % |
| 5.0 m/s | 25.00 N | 225.00 N | 250.00 N | 90.0 % |
| 6.0 m/s | 30.00 N | 324.00 N | 354.00 N | 91.5 % |

The F7 anchor is met at **48.49 N**, inside the `48 ± 2 N` the PRD asks for,
with the tabulated coefficients unchanged.

**R6, made concrete.** The model is a single quadratic with no planing regime
and no wave-making hump. It is credible up to roughly hull speed
(`1.34·√L_wl` ≈ 2.6 m/s for `L_wl = 3.81 m`, and the quadratic branch is still
mixed with the linear one there). **Above ≈ 5 m/s it over-predicts**: the
quadratic term already carries 90 % of the total at 5 m/s and 91.5 % at 6 m/s,
and a real ILCA planing at that speed sees resistance flatten rather than keep
climbing as `u²`. 354 N of drag at 6 m/s is a hard speed cap the boat will run
into once the sail arrives. Accepted for v1 per brief §15; `hull.rs`'s module
doc comment carries the same numbers, and section 08 should surface it in the
diagnostics panel.

### Rudder (brief §14)

| Quantity | Value |
|---|---|
| Stall peak, at `u = 3 m/s` | `δr = 0.2200 rad` (12.6°), `\|N\| = 857 N·m` |
| Trough after the stall | 511 N·m, 60 % of the peak |
| Band the criterion allows | `[0.209, 0.470]` rad |
| Authority at `u = 0.01 m/s`, full rudder | **0.0101 N·m** (criterion: < 0.1) |
| Authority scaling | exactly `V²` at fixed `δr` with `v = r = 0` |

---

## 5. Toolchain versions (R7)

**The host changed.** Sections 01–03 recorded `x86_64-pc-windows-msvc`;
everything below was measured on `x86_64-unknown-linux-gnu`. `rustc` is the
same version. Per F9, bit-identity is claimed only for the same build on the
same platform, so a golden trajectory recorded under either fingerprint is not
valid under the other — section 09 must record the platform, not just the
compiler version.

| Tool | Version |
|---|---|
| host triple | **`x86_64-unknown-linux-gnu`** (Linux 7.0.0-30-generic) |
| rustc | 1.98.1 (48a229cea 2026-09-01) |
| cargo | 1.98.1 (797e8a9bc 2026-08-05) |
| rustfmt | 1.9.0-stable (48a229ceae 2026-09-01) |
| clippy | 0.1.98 (48a229ceae 2026-09-01) |
| wasm-pack | 0.15.0 |
| wasm-bindgen | 0.2.128 |
| console_error_panic_hook | 0.1.7 |
| serde / serde_json | 1.0.229 / 1.0.151 |
| node | **v26.3.0** (was v26.6.0 on Windows) |
| pnpm | **11.6.0** (was 12.4.2) |
| PowerShell | **not installed** |
| vite | 7.3.6 |
| vitest | 5.0.1 |
| react / react-dom | 19.3.0 |
| typescript | 7.0.2 |
| @playwright/test | 1.63.0 |
| @deck.gl/core, /layers, /react | 9.4.0 |
| @swc/core | 1.15.47, pinned (section 03 §5) |
| vite-plugin-wasm / vite-plugin-top-level-await | 3.6.0 / 1.6.0 |
| browsers under Playwright | chromium, firefox, msedge |

---

## 6. Risks

### R3 — sign-convention drift: **exercised, and it held**

This is the section where a sign error would first show. Every named sign test
passes, and the two that matter most — `foil::lift_direction_sign` and
`hydro::rudder::positive_delta_turns_bow_to_starboard` — are asserted directly
rather than inferred from a trajectory. Mirror symmetry is **structural**, not
tuned: `cl` is exactly odd and `cd` exactly even bit-for-bit, so
`invariants::mirror_symmetry_trajectory` holds at 1e-10 over 4 000 steps
because the arithmetic is symmetric, not because the tolerance is loose.

The one sign question that did arise is §2.5, and foundations answered it
unambiguously.

### R4 — the M1 placeholder force model: **CLOSED**

`crates/sailgym-physics/src/forces/scaffold.rs` is deleted, its module
declaration and re-export are gone, `sim.scaffold_thrust` is out of the F7
catalogue, and `Simulation::new` builds `PhysicalForces`. All three greps in
task 4.5 return nothing. Nothing in the repository now depends on a fake force.

### R6 — no planing regime: **concrete, and quantified** in §4

### R2 — the boat may be too tender: still untouched

There is no sail force and no righting moment, so nothing here bears on it. But
note that the foils now generate real heeling moments (the board at
`z = −0.45 m`, the rudder at `z = −0.28 m`), and with `K_restore` still zero
until section 07 there is **nothing to stop `φ` from rotating**. That is
correct for M3 and expected; it is also why §2.4 mattered. No
`sailor_pos_b.y` parameter was added; it still requires human sign-off.

### R1 — mainsheet stiffness: not exercised

`k_sheet` is still read by nothing. `l_sheet` integrates as a rate-limited
actuator. R1 fires in section 06.

### R5 — deck.gl: unchanged

No new layer, no new deck.gl code. Zero context-loss events over the ten-second
soak, in all three browsers, as before.

### R7 — golden regression files: **the platform moved**, see §5

`regression.rs` still skips with its message. `rng::pcg32_known_vector` still
pins the algorithm against the published PCG demo output.

### Not executed

CI has still never run; the repository has no remote. The Windows gate path
(`scripts/check.ps1`) was not exercised this section.

### A note on an existing grep

Section 01's `grep -rnE "rho|9\.8|1\.225|1025" crates/sailgym-physics/src` now
matches two lines in `foil.rs` — the parameter **name** `rho: f64` in
`foil_force`, which F5.3 specifies verbatim, and its use `0.5 * rho * vsq`.
**No numeric physical literal was added anywhere outside `constants.rs` and
`parameters.rs`.** Section 10's version of this grep should look for numeric
literals rather than the identifier.

---

## 7. What section 05 must know

1. **The gate is still the contract**, and it is green. Also run
   `pnpm --dir web test:unit` (38 tests) and `pnpm --dir web build` — neither is
   in the gate.

2. **`ForceModel` has no wind, and you need one (§2.11).** `evaluate(st, c, p,
   w, t)` takes a `&dyn WindField` and is ready for you, but
   `PhysicalForces::generalized` currently feeds it a private `NoWind`, because
   `ForceModel::generalized(&self, st, c, p, t)` — F4.4, `dynamics.rs`,
   section 02 — carries no field. **This is your first task and it means
   editing `dynamics.rs` and `simulation.rs`.** The two obvious routes are
   widening the trait signature, or giving `PhysicalForces` a borrowed field
   and making `Simulation` build it per step. Whichever you choose, keep
   `evaluate`'s signature — the PRD pins it — and keep the summation order.

3. **`foil.rs` is finished and is yours to call, not to change.** `sail_load`
   (F6.3) must use `foil_force` unchanged. `foil::FoilParams` is
   `parameters::FoilSection`, so `&p.sail.section` is what you pass. Section AC
   6 of this section greps for a second lift/drag implementation.

4. **The summation order in `forces::evaluate` is fixed (F9.4)** and
   `forces::summation_order_documented` reads the source to prove it. Slot 4 is
   the sail, and it is already there returning `Load::default()`. Replace the
   value; do not move the call. The same goes for
   `ForceBreakdown::{sail, alpha_sail, cl_sail, cd_sail, m_beta, aw_boat}`,
   which exist and are zero.

5. **Hull resistance is summed directly into `H` and everything else goes
   through `Generalized::add` (§2.4).** The sail load is boat-fixed, at
   `r_CE` in `B`, so it goes through `add` — that is where the heel correction
   comes from, and F6.4 says do not add a second `cos φ`.

6. **`testkit` is how you drive a test before a force exists**, and after
   section 05 you mostly will not need it. It is `#[cfg]`-gated; if you add a
   helper there, remember that integration tests see it only through the self
   dev-dependency in `Cargo.toml` (§2.10).

7. **`hydro::local_flow` is the F6.5 flow model and it is shared.** The sail's
   equivalent is `aero::apparent::apparent_wind_at` (F6.2), which is yours and
   which additionally keeps the `z` component before you drop it. Do not
   reuse `local_flow` for air — the water is still, the air is not.

8. **The `?scenario=` test hook (§2.12) is two entries in `App.tsx` and one
   helper in `useSimulation.ts`.** Once the sail exists most browser tests will
   not need it, but `hydro.spec.ts` and the four section-02 tests in §2.13 do.
   Section 09 replaces it wholesale; until then, adding an entry is adding an
   initial surge speed, nothing more.

9. **There is still no righting moment.** `φ` integrates freely under the foil
   heeling moments and will roll past 90° given a side force. That is correct
   until section 07, and `damping_survives_inversion` and
   `finite_number_invariant` are what keep it from diverging. When you add sail
   side force, expect the boat to heel over and stay there — do not "fix" it in
   section 05.

10. **`tests/invariants.rs` has 8 tests and is yours to grow, not replace.**
    Sections 05–07 add to it; section 10 completes it. `mirror_symmetry_trajectory`
    and `coordinate_frame_consistency` already rotate the wind bearing, so they
    will keep meaning what they say once the sail reads the field.

11. **No numeric physical literal outside `constants.rs` and `parameters.rs`**
    still holds in the physics crate, and no ILCA dimension appears in any
    hand-written `.ts`/`.tsx`.
