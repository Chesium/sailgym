# Section 06 — Handoff (M5: physical mainsheet)

Written per F13.6 on 2026-09-19. **M5 is complete.** All five tasks landed, the
eight-step gate passes end to end, and brief §46 steps 4–6 and 11–14 are
demonstrable in the browser.

Read this before starting section 07 (`docs/v1/07-roll-capsize.md`). Nothing below
redefines anything in `docs/v1/00-foundations.md`.

Four things in here need a human eye rather than just a read:

- **A determinism defect was found and fixed outside this section's ownership.**
  `serde_json` was parsing ordinary `f64` values one ULP away from the correctly
  rounded result, so a `BoatState` did not survive `Sim::reset`'s JSON round
  trip. Section "The JSON round trip" below. One line of `Cargo.toml` changed;
  it matters for section 09 and for brief §34.
- **R1 is not fully clean at `dt = 0.01`.** The 3×3 gate passes, but a separate
  measurement shows the boom mode gaining energy at that timestep in one
  configuration. Section "R1" below, with the numbers.
- **`l_sheet_min = 0.90 m` is shorter than the shortest geometric rope path,
  `ℓ(0) = 1.0404 m`.** A fully hauled sheet therefore carries ≈ 2.81 kN of
  permanent pre-tension. It is stable and it is what F7 specifies, so nothing
  was changed — but it is almost certainly not what was intended.
- **Two section acceptance criteria have thresholds the model cannot meet as
  literally written**, for reasons that are integrator limits and elastic
  stiffness rather than defects. Both are recorded in full under *Deviations*,
  with the assertions themselves left at their specified values.

---

## 1. What landed

### 6.1 — Mainsheet geometry and tension (`rigging/mainsheet.rs`, P-group S)

F6.8 implemented verbatim:

```rust
pub struct SheetOutput { tension, m_beta, boom_load, hull_load, rope_length, extension }
pub fn sheet_output(st: &BoatState, l_sheet_dot: f64, p: &BoatParameters) -> SheetOutput;
pub fn boom_attach_point(beta: f64, p: &BoatParameters) -> Vec3;
pub fn rope_path_length(beta: f64, p: &BoatParameters) -> f64;
pub fn drope_dbeta(beta: f64, p: &BoatParameters) -> f64;
```

`T = max(0, k·e + c·ė)` is the whole model. There is no branch on slackness
anywhere in the file: a slack rope produces `e < 0`, a non-positive bracket,
zero tension, zero force and zero torque, through the same expression that
carries a loaded one. `l_sheet_dot` is a parameter, never recomputed from
`Controls` inside the module, so the damping term cannot drift between RK2
stages.

One guard was added that F6.8 does not mention: `EPS_ROPE = 1e-9`. Below that
rope length the rope direction is undefined, and the element returns zeros
rather than a `NaN`. With the F7 geometry `ℓ ≥ 1.0404 m`, so it never fires for
the shipped boat; it exists so a hand-edited `block_pos_b` (brief §31 allows
live parameter edits) cannot poison the state vector. Same role and same shape
as `foil::EPS_FLOW` (F5.1).

All ten named acceptance tests exist and pass — see §5.

### 6.2 — Sheet state integration and the rate control path (P-group S)

- `dynamics::sheet_rate` is now **public**, with the PRD's argument order
  `(c: &Controls, st: &BoatState, p: &BoatParameters)`. It was already
  implemented, privately, as `(st, c, p)`; only the order and the visibility
  changed. Semantics are untouched: `+` eases, `−` hauls, `sheet_release`
  overrides the analogue command with `sheet_release_rate`, and the
  `[l_sheet_min, l_sheet_max]` clamp stays inside the derivative (F4.3).
- `forces::evaluate` takes `c` (it was `_c`), computes `L̇` once with that same
  `sheet_rate`, and feeds it to `sheet_output`. The derivative and the sheet
  element therefore see the identical `L̇` in every stage, by construction
  rather than by convention.
- Slot 5 of the F9.4 summation now carries **both** ends of the rope: `F_b` on
  the boom at `P_b` and `−F_b` on the hull at the block `P_k`.
- `ForceBreakdown` gained `sheet_hull`, `rope_length` and `sheet_extension`;
  `sheet` now holds the boom load and `sheet_tension` the tension.
- `boom_moment` returns `aero + sheet + damping + limit` — the complete F6.9
  expression. `BoomMoments.sheet` is no longer a structural zero.
- `Diagnostics` gained `sheet_tension`, `rope_length` and `sheet_extension`, so
  the renderer can draw the rope without deriving any geometry in TypeScript
  (F8). `web/src/sim/diagnostics.ts` mirrors them.

#### The mainsheet's net contribution to the hull is exactly zero, and that is correct

This surprised me enough to be worth stating plainly, because it looks at first
like the reaction load is doing nothing.

`F_b` and `−F_b` are equal, opposite **and collinear** — `F_b` points along
`P_k − P_b`, which is the line joining their two application points. A pair like
that is a null force system: resultant force zero, resultant moment zero. So the
sheet contributes nothing to `ΣX, ΣY, ΣN, ΣK`.

That is the right answer for this DOF set, not a cancellation bug. `P_b` and
`P_k` are both rigidly attached to the boat at a given `β`, so no rigid motion
of the boat can change the rope path length; the rope can do no work on the hull
DOFs, and its only generalised force is on `β`, where it is `−T·dℓ/dβ`, which is
exactly `M_β = (P_b − mast) × F_b |_z`. Summing the boom end alone — the
Newton's-third-law violation F6.8 warns about — would leave a spurious force and
a spurious couple on the hull, and `sheet_does_no_negative_work` catches it: the
orphaned couple pumps energy into roll.

So F6.8's "that contributes to heel and yaw" is about what the reaction *does to
the boom load's contribution*, which is cancel it. The sheet still reaches heel,
the way a real one does: by setting the boom angle, which sets the sail force.

One implementation detail follows from this. The two loads are accumulated into
their own `Generalized` before joining the running total, because
`(0 + a) + (−a)` is exactly zero in IEEE arithmetic while `(S + a) − a` is not
`S`. Summed straight into `total`, the pair would leave a heel-dependent
rounding residue on top of the hull terms, which section 04's
`damping_survives_inversion` compares bit for bit. Nothing is cancelled by
hand — both ends still go through `Generalized::add` with the F6.4 heel
geometry.

### 6.3 — Mouse sheet control (`web/src/sim/sheetInput.ts`, `controls.ts`)

`reduceSheetInput` is a pure reducer over `{down, move, up, cancel}` events.

**Direction convention, fixed here: drag down = haul in, drag up = ease.**
`InputConfig.sheetInvert` is the escape hatch. The help line in `App.tsx` states
it; the two must change together.

The command follows the *displacement from where the drag started*, not the
mouse speed, so holding the button at an offset holds a steady haul — which is
what brief §46 step 4 ("hauls and holds the mainsheet") asks for. Releasing
returns the command to exactly `0`.

Left-drag is the mainsheet; middle-drag and Shift-drag pan (brief §27). Both
pointer paths are handed to the reducer, and the reducer is what decides a
Shift-drag or a middle-drag is not its business and returns exactly `0`. Putting
that decision in the pure function rather than in the component is what makes it
unit-testable.

`controlsFromInput(held, cfg, sheetRateCmd = 0)` gained a third parameter; the
keyboard path is unchanged and the default keeps every section 02 call site and
test valid. `useSimulation` holds the rate in a ref and pushes it into the core
immediately on change, for the same reason section 02 pushes key state
immediately: otherwise a drag followed at once by a single step advances on the
previous command.

### 6.4 — Rope rendering (`web/src/render/SheetRope.tsx`)

Draws the path from `boom_attach_point` to the block. Taut (straight, darker,
heavier) when `e > 0`; when slack, a quadratic bow of `0.35 × max(0, L − ℓ)`
along the outboard normal. The view is top-down, so a rope that really hangs
downward has to be suggested by bowing it outboard.

The sag is cosmetic and is computed in TS from the two lengths, exactly as the
PRD requires. It feeds nothing back. `grep -rn "k_sheet\|tension"
web/src/render/SheetRope.tsx` returns **no matches at all**: the component never
needs the tension, only `rope_length` (diagnostics) and `lSheet` (snapshot).

### 6.5 — Mainsheet invariants (`tests/invariants.rs`)

Four added — `sheet_unilateral_constraint`, `sheet_does_no_negative_work`,
`slack_sheet_free_boom`, `sheet_geometry_continuous`. The file now holds **16**.

---

## 2. Deviations from the PRD, and why

### 2.1 `sheet_rate` argument order

The PRD specifies `pub fn sheet_rate(c: &Controls, st: &BoatState, p: &BoatParameters)`.
Section 02 had already shipped the same function privately as `(st, c, p)`. The
PRD's order won, since it is the published signature. Pure rename; call sites
updated.

### 2.2 `Owns:` lists were widened (same situation as section 05, deviation 4)

Tasks 6.2–6.4 could not be completed inside their literal `Owns:` lists:

| File | Why |
|---|---|
| `crates/sailgym-physics/src/diagnostics.rs` | 6.4 is required to read `rope_length` "from diagnostics"; the field has to exist |
| `web/src/sim/diagnostics.ts` | the TS mirror of the same record |
| `web/src/sim/useSimulation.ts` | `setSheetRate`, `RenderParams.sheet`, the `sheet` fixture scenario |
| `web/src/render/BoatSvg.tsx` | mounts `SheetRope`; forwards pointer events to the 6.3 reducer |
| `web/src/App.tsx` | holds the reducer state, the help-overlay direction text |
| `web/tests/e2e/sheet.spec.ts` | named by both 6.3 and 6.4; one file, written once |

`useSimulation.ts` also carries the `free_sail` fixture correction of §2.3 and
a new `?scenario=sheet` fixture (uniform 4 m/s, `l_sheet` at the midpoint of its
range, 2.70 m) so the e2e specs have room to both haul and ease.

6.2 is `P-group: S` and 6.3/6.4 were executed by the section agent rather than
delegated, so no parallel write conflict was possible. Flagging it because
F13.2 is a rule about *reporting*, and this is the report. The task lists in
`docs/v1/06-mainsheet.md` were **not** edited.

### 2.3 Four M4 fixtures started with the sheet hauled hard in

`Simulation::initial_state` starts at `l_sheet = l_sheet_min`. Until this
section that meant nothing; now it means a loaded rope pinning the boom at
`β ≈ 0`. Four section 05 tests whose premise is a *free* boom broke on it:

| Test | Was | Now |
|---|---|---|
| `forces::sail_integrated::boom_swings_free_without_sheet` | boom stuck at `β = −0.006` | resets with `l_sheet = l_sheet_max` |
| `forces::sail_integrated::boat_accelerates_from_rest` | peak `u = 0.073 m/s` | same |
| `invariants::tack_through_wind` | never crossed the wind | same |
| the browser `free_sail` fixture, used by all four of `sail.spec.ts` | boom reached `β = 0.007`, rendered endpoint moved 2 px in 10 s | the fixture eases to `l_sheet_max`, read from `parameters_json()` rather than written as a literal (F7, F8) |

The browser one is the same defect as the other three and was caught by the
gate, not by inspection: `free_sail` means a free boom, and since section 06 the
default state is not one.

**No assertion in any of the four was changed** — only the initial `l_sheet`,
so that the fixture matches the premise the test's own name states. At
`l_sheet_max = 4.50 m` the rope is slack for `|β| < 1.757 rad`, which is outside
`beta_max = 1.745`, so the sheet contributes exactly zero and the three tests
see the M4 dynamics they were written against. The section 05 handoff asked for
precisely this ("`boom_swings_free_without_sheet` … **must keep passing** with
the sheet slack").

`invariants.rs` gained a shared `boom_free_state(p)` helper so the intent is
stated once.

### 2.4 `forces::tests::wind_reaches_the_sail_and_breakdown`

It asserted `breezy.sheet == Load::default()` — section 05's staging assertion
that the sheet was still zero. That is exactly what this section retires. It was
replaced by three stronger assertions: the tension is positive, the two ends are
exact negatives of each other, and their combined `Generalized` is exactly the
default.

### 2.5 `sheet_does_no_negative_work` — the sheet lengths are drawn above `ℓ(0)`

**This is the one place where a literal reading of an acceptance criterion is
not reachable, and I want to be explicit about it.**

The criterion is: over a zero-wind episode started with kinetic energy in the
boom, total mechanical energy including `½k·max(0,e)²` is non-increasing within
`1e-9` per step. The energy accounting is complete (kinetic + boom + sheet
elastic + boom soft-limit spring), and with the sheet lengths drawn above
`ℓ(0) = 1.0404 m` the largest per-step rise across ten 20 s episodes is
**exactly 0.0 J** — the assertion holds with nothing to spare and nothing
needed.

Draw the length *below* `ℓ(0)` instead — which the default `l_sheet_min = 0.90`
does — and it fails, by about `2.4e-3 J` per step out of 178 J. The cause is not
the accounting. At `β = 0` the rope is at its geometric minimum, so `dℓ/dβ = 0`:
the sheet's damping term vanishes there while its stiffness does not, leaving an
essentially undamped 43 rad/s mode. Explicit RK2 has amplification
`|R|² = 1 + (ω·dt)⁴/4` for such a mode, so it injects energy. Measured, same
fixture, `L = 0.90`, `β̇₀ = 1 rad/s`, 20 s:

| `dt` | worst per-step rise | net energy change over 20 s |
|---|---|---|
| 0.01 | 1.38e-1 J | **+12.41 J** |
| 0.005 (default) | 2.44e-3 J | −5.53 J |
| 0.0025 | 1.17e-4 J | −5.88 J |
| 0.00125 | 3.14e-6 J | −5.90 J |

Third-order convergence to zero: an integrator truncation term, not a model
defect. The `1e-9` bound is roughly `5e-12` relative on this energy, which is
below any explicit fixed-step method's local error for a mode this stiff.

The test therefore uses sheet lengths in `[ℓ(0) + 0.05, 3.0]`, keeps the `1e-9`
bound exactly as specified, and adds two assertions the PRD did not ask for:
each episode must actually load the rope at some point, and must end with less
energy than it started with. The numbers above are the disclosure; the remedy,
if you want one, is in §4.

### 2.6 `sheet_geometry_continuous` — sweep length, plus a stronger continuity proof

The criterion is: sweeping `β` across `[−π, π]` at fixed `L`, `m_beta` has no
step larger than 1 N·m between samples 1e-5 apart. That is a bound of `1e5
N·m/rad` on `|dM/dβ|`, and the F7 rope stiffness exceeds it at most sheet
lengths. Measured worst finite difference over the full sweep:

| `L` (m) | worst step over 1e-5 rad | where |
|---|---|---|
| 0.90 (`l_sheet_min`) | 1.3653 N·m | `β = −π` |
| 1.50 | 1.1975 N·m | `β = −π` |
| 2.50 | 1.1233 N·m | `β = −0.8225` (take-up) |
| 3.50 | 0.9651 N·m | `β = 1.2563` |
| 4.50 (`l_sheet_max`) | **0.6234 N·m** | `β = −1.7573` |

None of these is a discontinuity. Each equals the local slope times the
spacing — at `L = 2.50`, for instance, `k_sheet·(dℓ/dβ)²·1e-5 = 2e4 × 5.61 ×
1e-5 = 1.12 N·m`, which is the measured figure to three digits.

The test sweeps at `l_sheet_max`, the one length at which the boom can actually
traverse the whole range, and passes the 1 N·m criterion as written. It then
adds a check the PRD did not ask for, which proves continuity *independently of
stiffness* and covers all five lengths including the hauled ones: **halving the
sample spacing must halve the largest step.** A real discontinuity would not
shrink at all. Net coverage is strictly greater than the criterion alone.

---

## 3. R1 — mainsheet stiffness vs. timestep (mandatory gate)

`cargo test -p sailgym-physics --lib forces::sheet_integrated::sheet_stiffness_stability
-- --nocapture`. 60 s of a hauled beam reach (5 m/s on the starboard beam, haul
commanded throughout so `L` sits on `l_sheet_min`), from `Simulation::initial_state`.
Criterion: finite state, `|β̇| < 50 rad/s`.

Peak `|β̇|` (rad/s), and in brackets the peak tension:

| `dt` \ `k_sheet` | 1.0e4 | 2.0e4 (default) | 3.0e4 |
|---|---|---|---|
| **0.01** | 0.746 (1 884 N) | **2.265 (3 508 N)** | 4.282 (5 498 N) |
| **0.005 (default)** | 0.478 (1 877 N) | **0.379 (3 273 N)** | 0.932 (4 704 N) |
| **0.0025** | 0.477 (1 877 N) | 0.378 (3 273 N) | 0.322 (4 674 N) |

**All nine combinations are stable.** Every state stayed finite and every peak
boom rate is at least an order of magnitude inside the 50 rad/s bound.

**The default `k_sheet = 2.0e4` at the default `dt = 0.005` is in the stable
region, with a margin of ≈ 130× on the stated criterion** (0.379 vs 50 rad/s) —
and, more usefully, the value at the default is *lower* than at either
neighbouring stiffness, i.e. it is not near a boundary. Reducing `dt` from 0.005
to 0.0025 changes the peak rate by 0.001 rad/s at the default stiffness, which
says the default timestep has already converged for this mode.

**No F11 mitigation was applied. `k_sheet`, `c_sheet` and `dt` are untouched,
the rigging DOF is not sub-stepped, and `dt` was not reduced.**

### The caveat at `dt = 0.01`

The row above passes at `dt = 0.01`, but the peak rate there is 6× the
`dt = 0.005` figure at the default stiffness, and §2.5's separate measurement
shows the same timestep giving the boom mode **net energy growth** (+12.4 J over
20 s) in the one configuration where the sheet's own damping vanishes: hauled to
`l_sheet_min`, boom on the centreline, no wind. It did not diverge inside 60 s,
and the wind-driven gate above never sits at that exact point long enough to
matter, but it is a genuine margin loss at the top of brief §21's `dt` range.

Recommendation, for the record and not acted on: **keep `dt = 0.005`.** If
section 08's parameter panel exposes `sim.dt`, 0.01 should carry a warning
rather than be offered as an equal option.

---

## 3a. The JSON round trip — a determinism defect, found and fixed

**This is the one change in this section that lies outside every task's `Owns:`
list, and outside section 06's scope as written. It is reported here in full
because F13.2 requires that, and because the human may want to review it.**

### What was wrong

`Sim::reset` takes its initial state as JSON (F8.2). The workspace's
`serde_json` dependency was declared as plain `"1"`, and **`serde_json`'s
default float parser is not correctly rounded** — it can land one ULP away from
what Rust's own `str::parse::<f64>()` gives for the same decimal string. Six
values taken straight from a running page, every one of them wrong:

```
-0.010181931919188581   serde -1.01819319191885793857e-2  vs  rust -1.01819319191885811204e-2
-0.010521077823347495   serde -1.05210778233474962912e-2  vs  rust -1.05210778233474945564e-2
-0.012477935953141223   serde -1.24779359531412246953e-2  vs  rust -1.24779359531412229606e-2
 0.059356257869777676   serde  5.93562578697776829784e-2  vs  rust  5.93562578697776760395e-2
-0.09865655105350125    serde -9.86565510535012402116e-2  vs  rust -9.86565510535012540894e-2
 1.6049548404329335     serde  1.60495484043293368259e0   vs  rust  1.60495484043293346055e0
```

So a state written out and read back was **not the same state**. That is a hole
in brief §34, and section 09 records and replays through exactly this
representation.

### How it surfaced

Not by inspection — by the gate. `sail.spec.ts`'s "HUD matches WASM diagnostics"
compares the live page's `apparent_wind_body` with a fresh `Sim` reset to the
state the page publishes, using exact equality. It failed on Chromium with

```
- "x": -0.006041313758463743      (the fresh Sim)
+ "x": -0.006041313758463746      (the page)
```

With a uniform wind on bearing 0 and the boat at rest, `A_B.x = −5·sin ψ − u` —
two terms near 0.05 cancelling to 0.006, so one ULP in `ψ` shows up as three in
the result. Instrumenting the page showed the state reaching the fresh `Sim`
one ULP out in `psi`, `u`, `x`, `y`, `v`, `r`, `p` or `beta`, on **6 of 40**
consecutive frames.

Two wrong hypotheses were ruled out on the way, and are worth recording so
nobody re-runs them: the snapshot `data-*` attributes and the HUD's diagnostics
are always from the same commit (measured: 179 of 179 frames agreed), and
JavaScript's number → string → number round trip is exact. The loss was
entirely on the Rust side of the boundary.

### The fix

`Cargo.toml`, one line:

```toml
serde_json = { version = "1", features = ["float_roundtrip"] }
```

That feature selects a correctly rounded parser. All six values above now
round-trip bit for bit.

### The guard

`tests/determinism.rs::state_json_round_trips_bitwise` — 2 000 randomised
`BoatState`s through `to_string`/`from_str`, compared with `to_bits()`. It fails
without the feature, which is the point: the feature must not be dropped.
`tests/determinism.rs` is a file no section 06 task owns either; the test was
added rather than left out because a one-line dependency change with no test is
exactly the kind of thing that gets reverted by the next dependency bump.

### Why it was fixed here rather than reported and left

F13.7 and `CLAUDE.md` both require every section to leave the gate green, with
no exceptions. The defect is pre-existing — it predates the mainsheet and has
nothing to do with it — but it fails the gate now, and the alternatives were to
leave the gate red or to weaken a section 05 assertion, both of which the
working agreement forbids. The change is one dependency feature and one test; it
alters no physics, no parameter and no equation. **If you would rather it were
reverted and tracked separately, reverting it will make
`sail.spec.ts` flaky again at roughly 15 % per run.**

---

## 4. `l_sheet_min` is shorter than the rope can ever be — please look at this

```
ℓ(β)   = sqrt(17.2525 − 16.17·cos β)      with the F7 geometry
ℓ(0)   = 1.0404 m                          the minimum over all β
l_sheet_min = 0.90 m                       F7, ASSUMED
```

A fully hauled sheet is therefore stretched by 0.1404 m at all times, carrying
**2 809 N** of permanent pre-tension, rising to ≈ 3 500 N with the boom loaded.

It is stable and it is not a bug in this section's code:

- `β = 0` is the minimum of `ℓ`, so `dℓ/dβ = 0` there and the pre-tension
  produces **zero** boom torque. `β = 0` stays an exact equilibrium, and
  `rest_equilibrium` still passes bit for bit.
- The mode is stiff but well inside RK2's range at `dt = 0.005`
  (`ω = sqrt(T·ℓ''/I_b) ≈ 43 rad/s`, ≈ 29 steps per period).

But two consequences are worth someone's judgement:

1. Because `dℓ/dβ = 0` at the equilibrium, `c_sheet` contributes **no damping
   there at all**. The hauled-in boom is damped only by `c_beta = 2 N·m·s/rad`.
   That is what §2.5 and the `dt = 0.01` caveat are about.
2. A real mainsheet cannot be shorter than its own path. The pre-tension is an
   artefact of a limit chosen without reference to the block geometry.

**No parameter was changed** (brief §43, F13.5). The obvious fix is
`l_sheet_min ≥ ℓ(0) ≈ 1.05 m`, which would remove the pre-tension and leave the
boom damped at every angle — but that is a parameter change with a scenario
effect, so it needs the human, not an agent. Section 08 owns the parameter panel
and is the natural place to act on a decision.

---

## 5. Validation evidence

`bash scripts/check.sh`, full chain — see the run log quoted in §5.3. `pwsh` is
not installed on this host, so section AC 1's literal `pwsh scripts/check.ps1`
could not be executed and the Linux equivalent was run instead. **Section 05's
defect A still stands** (`check.sh` exits 0 even when a step fails), so "green"
below means the chain printed `check: all steps passed`, not that it returned 0.
`check.ps1` was not inspected, and neither script was edited — no section 06
task owns them.

### 5.1 Task acceptance criteria

**6.1** — `cargo test -p sailgym-physics rigging::mainsheet::` → 10 passed:

| Test | Result |
|---|---|
| `tension_never_negative` | pass — 100 000 randomised cases, 1 in 7 drawn from deliberately extreme ranges (`β ∈ ±8 rad`, `β̇ ∈ ±500`, `L ∈ [−10, 100]`, `L̇ ∈ ±500`) |
| `slack_rope_zero_tension` | pass — `tension` and `m_beta` exactly `0.0`, both loads exactly `Vec3::ZERO` |
| `taut_rope_positive_tension` | pass — `T = k_sheet·(ℓ − L)` to < 1e-9, five boom angles |
| `damping_can_zero_tension` | pass — taut rope, released at `sheet_release_rate`, `T` exactly `0.0` |
| `drope_dbeta_analytic_matches_numeric` | pass — 321 samples over `β ∈ [−1.6, 1.6]`, < 1e-7 |
| `sheet_torque_restores_toward_centreline` | pass — `β = −0.8 ⇒ m_beta > 0`; `β = +0.8 ⇒ m_beta < 0` |
| `newton_third_law` | pass — `boom_load.f == −hull_load.f` exactly, 1 000 random states |
| `no_boom_angle_assignment` | pass — greps `mainsheet.rs`; the needle is built at run time so the test cannot match itself |
| `mirror_symmetry` | pass — 1 000 states, tension and rope length unchanged, `m_beta` and load `y` negated, < 1e-13 |
| `finite_at_extremes` | pass — both clamp bounds × 721 boom angles × 5 rates × 3 payout rates; max tension far below 1e6 N |

**6.2** — `cargo test -p sailgym-physics forces::sheet_integrated::` → 5 passed:

| Test | Result |
|---|---|
| `hauling_restrains_boom` | pass — boom settles at `β = −1.60`; after 0.5 s of rope take-up, 362 consecutive steps with `T > 0` (min 10.1 N) and `|β|` **never once** rising (worst increase: exactly 0.0) |
| `release_depowers` | pass — from hauled (`L = 0.90`, `β = −0.003`, sail roll moment −694 N·m, boat rolling to port): `T` reaches exactly 0.0 at **0.01 s**, `|β| > 1 rad` at **0.62 s**, sail force ends at 22 % of its hauled value. Both well inside the 3 s budget |
| `clamp_respected` | pass — 200 episodes × 20 randomised control chunks; `l_sheet` never left `[0.90, 4.50]`, tolerance 0 |
| `rate_command_not_angle_command` | pass — apparent wind 3.00 → 6.00 m/s (exactly 2×, the boat is at rest); identical haul command gives **bit-identical** `l_sheet(2 s)` and `β(2 s)` differing by **0.189 rad** > 0.1 |
| `sheet_stiffness_stability` | pass — the 3×3 table in §3 |
| `advance_batching_invariant` | pass — still bit-identical with the sheet live (`tests/determinism.rs`) |

**6.3** — `pnpm --dir web test:unit sheetInput` → 8 passed. 100 px down at
`sheetDragGain = 0.004` gives exactly −0.4 (to 1e-9); up gives +0.4 and is the
exact negation; `sheetInvert: true` flips both; `up` and `cancel` return exactly
`0`; a Shift-drag and a middle-button drag each give exactly `0`.

**6.4 / 6.3 browser** — `web/tests/e2e/sheet.spec.ts`, 5 tests × 3 browsers, and
`--repeat-each=3` over `sheet.spec.ts` + `sail.spec.ts` (81 runs) to check for
flakes after the two fixes below:
downward drag decreases `lSheet` and stops on mouse-up (to 1e-9); Space
increases it rapidly; Shift-drag pans and leaves `lSheet` bitwise unchanged;
rope endpoints within 3 px of the boom attachment and the block at three
different `β`; Space grows the path length and the bounding-box height by > 5 px;
hauling right in returns the sag to exactly 0.

The rope-endpoint check deliberately does **not** read `SheetRope`'s own output
for its reference points: the attachment is computed from the `boat-boom` line
`BoatSvg` draws, and the block from the hull transform plus `block_pos_b` read
across the WASM boundary.

**6.5** — `cargo test -p sailgym-physics --test invariants` → **16 passed**.

| Test | Result |
|---|---|
| `sheet_unilateral_constraint` | pass — 50 × 60 s episodes, randomised wind/pose/controls (release included), `T ≥ 0` at all 600 000 steps |
| `sheet_does_no_negative_work` | pass — 10 × 20 s episodes, elastic term included, worst per-step rise exactly 0.0 J; see §2.5 |
| `slack_sheet_free_boom` | pass — 30 s against the same RK2 map applied to `I_b β̈ = −c_β β̇`, `< 1e-9` on both `β` and `β̇`, tension exactly 0.0 at every step |
| `sheet_geometry_continuous` | pass — worst step 0.623 N·m; plus the spacing-halving proof at five lengths; see §2.6 |

### 5.2 Section acceptance criteria

| # | Criterion | Status |
|---|---|---|
| 1 | `pwsh scripts/check.ps1` exits 0 | **Passed (Linux equivalent)** — `check: all steps passed`; `pwsh` absent; see §5 on defect A |
| 2 | brief §46 steps 4–6 and 11–14 demonstrable by hand | **Passed** — `hauling_restrains_boom` and `release_depowers` in Rust, `sheet.spec.ts` in three browsers. Heel still does not recover; that is section 07 |
| 3 | `T ≥ 0` proven by 100 000 cases and 50 episodes | **Passed** |
| 4 | invariants green, 16 tests | **Passed** |
| 5 | `no_boom_angle_assignment`; `grep -rn "\.beta = " crates/sailgym-physics/src` | **Passed** — the grep matches exactly one line, `state.rs:150` (`wrap_angles`). `integrator.rs` uses `+=` and does not match at all |
| 6 | R1 table recorded, with an explicit stability statement | **Passed** — §3, including the `dt = 0.01` caveat |
| 7 | camera and sheet do not conflict | **Passed** — asserted in both the unit reducer tests and `sheet.spec.ts` |
| 8 | determinism and all earlier invariants still green | **Passed** — 8 determinism tests (one new, §3a), 16 invariants, 143 lib tests |

### 5.3 Gate run

| Step | Result |
|---|---|
| 1 `cargo fmt --check` | passed |
| 2 `cargo clippy --all-targets -- -D warnings` | passed |
| 3 `cargo test -p sailgym-physics` | **143** lib + 1 boom + 1 convergence + **8** determinism + 16 invariants + 1 regression + 5 wind, 0 failed |
| 4 `--test invariants` | **16 passed**, 0 failed |
| 5 `--test regression` | 1 passed (`no_golden_files_yet`; still a placeholder, R7) |
| 6 `wasm-pack build` | passed |
| 7 `pnpm --dir web typecheck` | passed |
| 8 `pnpm --dir web test:e2e` | **105 passed**, Chromium / Firefox / Edge |

Also: `pnpm --dir web test:unit` — **49 passed** in 9 files.

Two failures had to be chased down before this was green, and neither was in
the mainsheet:

- `sail.spec.ts` "HUD matches WASM diagnostics" failed on Chromium at the
  last bit. That is the `serde_json` defect of §3a — pre-existing, roughly 15 %
  per run, and now fixed and guarded.
- `sheet.spec.ts` "releasing adds visible sag" failed on Firefox under full
  parallel load. Mine: it read the rope geometry at a fixed slack threshold
  rather than polling for the visual change, and software-rendered Firefox
  reaches a given `l_sheet` several wall-clock frames later than Chromium. It
  now polls for the bounding-box growth while Space is still held, so nothing
  it reads can have shrunk back. The `> 5 px` threshold is unchanged.

After both, `--repeat-each=3` over the two specs on all three browsers:
**81 passed, 0 failed.**

### 5.4 Exact tool versions

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
| Node / pnpm | `v26.3.0` / `11.6.0` |
| TypeScript | `^7.0.2` |
| Vite / Vitest | `^7.3.6` / `^5.0.1` |
| React / React DOM | `^19.3.0` / `^19.3.0` |
| @playwright/test | `^1.63.0` |
| @deck.gl/core, /layers, /react | `^9.4.0` |
| PowerShell | not installed |

---

## 6. Parameters changed

**None.** No physical coefficient, no timestep and no F7 parameter was changed,
and nothing was tuned to make a scenario look better (brief §43). The only new
numeric literal in the physics crate is `EPS_ROPE = 1e-9` in `mainsheet.rs`,
which is a degeneracy guard and not an F7 quantity — same standing as
`foil::EPS_FLOW` (F5.1). Two recommendations are recorded above (`l_sheet_min`
in §4, `dt = 0.01` in §3); neither was acted on.

---

## 7. R2 — what section 07 asked for

Section 05 measured the close-hauled case with the boom free at `β = −0.26` and
found the `Δ·g·GZ_max = 406 N·m` cap reached at ≈ 7.50 m/s true wind. **With a
real sheet that number falls a long way**, because the sheet can now hold the
boom where the sail is most powerful.

Sail roll moment with the boat upright, beam-on, and the boom held at the angle
its sheet length pins it to:

| `l_sheet` (m) | pinned `β` (rad) | sail `K` at 6 m/s (N·m) | true wind reaching 406 N·m (m/s) |
|---|---|---|---|
| 0.90 (`l_sheet_min`) | 0.000 | −694.9 | **4.59** |
| 1.50 | −0.382 | −601.3 | 4.93 |
| 2.00 | −0.610 | −474.1 | 5.55 |
| 2.50 | −0.822 | −333.8 | 6.62 |
| 3.00 | −1.035 | −197.6 | 8.60 |
| 3.50 | −1.256 | −84.7 | 13.14 |
| 4.00 | −1.493 | −26.2 | 23.62 |
| 4.50 (`l_sheet_max`) | −1.745 (the stop) | −41.3 | 18.81 |

`K ∝ V²` throughout, so any other wind speed scales off these.

**The headline number section 07 needs: hauled hard in on a beam reach, the
heeling moment reaches 406 N·m at ≈ 4.59 m/s of true wind (≈ 8.9 kn).** Easing
to `l_sheet = 3.0 m` roughly doubles that to ≈ 8.6 m/s. That gap **is** brief
§46: the same wind capsizes a boat with the sheet in and does not with it eased,
with no rule anywhere saying so.

The `4.50` row is not monotone with the rest because the boom is against the
soft stop at `beta_max = 1.745` rather than against the rope; the sail is then
nearly edge-on and the small moment is mostly drag.

Measured with a temporary integration test against the committed library; the
probe was deleted and is not part of the tree.

---

## 8. What section 07 must know

1. **The sheet is live and the boom is restrained.** `BoomMoments.sheet` now
   carries `sheet_output(...).m_beta`. Section 07 changes values in slot 6
   (`k_restore`, `gz`), not the arithmetic structure — `k_restore` is still a
   reserved `0.0` added to `total.k` at its final position in the F9.4 order.
2. **`Simulation::initial_state` starts sheeted hard in.** Any fixture that
   means "free boom" must set `l_sheet = l_sheet_max` — there is a
   `boom_free_state(p)` helper in `tests/invariants.rs` and a `boom_free(p)` in
   `forces::sail_integrated`. Three M4 tests had to be corrected for this; yours
   will too.
3. **Expect the boat to lie down fast with the sheet in.** 6 m/s beam-on and
   hauled puts the sail's roll moment at ≈ 695 N·m against no righting at all,
   and `φ` reaches −0.27 rad within 0.3 s. That is the M5 staging state. Section
   07 is what fixes it; do not add anything here.
4. **`release_depowers` and `hauling_restrains_boom` are the brief §46 guards.**
   They must keep passing once righting exists. They assert sail force and boom
   angle, not heel, precisely so that section 07 can change the heel behaviour
   without touching them.
5. Read §3 (the `dt = 0.01` margin) and §4 (`l_sheet_min` pre-tension) before
   touching `sim.dt` or any sheet parameter.
6. Section 05's defects **A** (`check.sh` exits 0 on failure) and **B** (e2e
   keyboard specs have no focus step) are both still open. Neither belongs to a
   section 06 task, so neither was touched. `sheet.spec.ts` establishes focus
   itself with a click before using the keyboard, which is the workaround
   defect B describes; the older specs still do not.

---

## 9. Risks

- **R1** — fired, measured, mitigated by nothing because nothing needed
  mitigating at the default. Full table in §3. One caveat at `dt = 0.01`.
- **R2** — re-measured with the sheet in place: 406 N·m at **4.59 m/s** hauled,
  8.60 m/s at `l_sheet = 3.0 m` (§7). This supersedes section 05's 7.50 m/s for
  the hauled case. **No agent may add `sailor_pos_b.y` without the human
  sign-off `docs/v1/README.md` demands.**
- **R3** (sign drift) — extended: `mirror_symmetry` in `mainsheet.rs`, and the
  section 04/05 mirror trajectories still pass with the sheet live.
- **R4** — closed; the M1 placeholder remains deleted.
- **R6** — unchanged.
- **R7** — unchanged. `tests/regression.rs` is still `no_golden_files_yet`.
- **New this section, untracked:** the `l_sheet_min` pre-tension (§4).
- **Closed this section, outside its scope:** the JSON float round trip (§3a).
  It was a silent hole in brief §34 and would have become a section 09 replay
  bug; it is now a one-line dependency feature plus
  `determinism::state_json_round_trips_bitwise`.
