# Section 10 — Handoff (M9: invariants, convergence, performance, demonstrations)

Written per F13.6 on 2026-09-20. **M9 is complete, and this is the project
close-out note.** All eight tasks landed, the eight-step gate passes end to end
and exits 0, and brief §47's fourteen success criteria are walked one by one in
`docs/v1/acceptance.md`.

Nothing below redefines anything in `docs/v1/00-foundations.md`.

Read `docs/v1/acceptance.md` first if you want the verdict, `§9` below if you want
the list of what an RL or validation effort should tackle first, and `§3` if you
want the places where the PRD and the physics disagreed.

Six things want a human eye rather than just a read.

- **Two links of task 10.6's demonstration-1 chain are not what a correct model
  does**, and the tests assert what the model does instead. The sail's `|C_L|`
  is ≈ 0 with the boom sheeted flat on a beam reach (the sail is a stalled
  plate — all drag), and the heeling *moment* falls while the *heel* grows.
  Both are recorded with numbers in §3.1. **No threshold was relaxed into
  vagueness; the assertions were re-pointed at the quantities that carry the
  meaning, and the deviation is stated in the spec itself.**
- **Task 10.2's order bracket `[1.7, 2.3]` is exceeded by one scenario, from
  above.** `beam_reach_capsize` converges at order ≈ 2.85, *faster* than second
  order. The PRD's suggested remedy — the pre-capsize window — makes it worse,
  not better, and was measured before being set aside. §3.2. The bracket was
  **not** widened; the lower bound, which is the half that could catch a
  defect, is asserted for all three scenarios exactly as written.
- **`board.area` and `rudder.area` are now tagged ASSUMED, which required a
  change to `parameters.rs`.** Section 08 §5 recorded the defect and said the
  fix needed the F7 catalogue's shape to change. It did, minimally: the tag now
  comes from the owner rather than from the shared `FoilSection`. **No value
  changed.** §2.3.
- **The gate is now about ten minutes**, nine of them the browser suite, so task
  10.8's split applies: `scripts/check.sh --fast` is a two-and-a-half minute
  pre-commit subset. **It is not the gate.** §6.2.
- **`provenance::shipped_values_match_the_f7_table` is new and is the strongest
  brief §43 guard the project has had.** It parses the F7 tables out of
  `00-foundations.md` and compares 83 of their 85 rows against what
  `BoatParameters::ilca7()` actually ships. All 83 agree. Every handoff since
  section 01 has claimed no coefficient was tuned; this is the first thing that
  checks it. §2.6.
- **Three findings from earlier sections are still open and still need a human**
  — `stability.gm`, `l_sheet_min` and F6.7's internal contradiction. They are
  restated in §10 with what it would cost to act on them now.

---

## 1. What landed

### 10.1 — The brief §35 invariant suite, with justified tolerances (P-group S)

- **`crates/sailgym-physics/tests/invariants.rs`** grew from 21 to **23**.
  - `tension_never_negative` — the unilateral constraint as a property of the
    element rather than of a trajectory: 200 000 randomised cases, one in seven
    from deliberately impossible ranges (`|β| ≤ 8 rad`, `|β̇| ≤ 500 rad/s`,
    `L ∈ [−10, 100] m`). `T ≥ 0` **exactly**, and a slack rope produces exactly
    zero tension, zero torque and two zero loads, through the same expression
    that carries a loaded one. Both branches of the `max` are asserted to have
    been taken.
  - `documented_invariants_exist` — reads `docs/v1/invariants.md`, checks every
    test it names exists **in the file it names**, and checks every `#[test]` in
    the suite has a row. The document cannot drift from the suite in either
    direction.
- **`docs/v1/invariants.md`** is new: every brief §35 item, the test, the
  tolerance, and **why that tolerance**, with the measured worst residual behind
  each one. Every figure in the "why" column was measured, not asserted — a
  throwaway probe ran each comparison and reported its worst case before the
  document was written. The margins are in §4.1.

### 10.2 — Time-step convergence (P-group A)

- **`crates/sailgym-physics/tests/convergence/study.rs`** (new) holds the study:
  the scenarios, the fixed 20 s script, the runner, the error norms, and
  `observed_order` / `observed_order_of`. It is `#[path]`-included by both
  consumers, exactly as section 09 shares the golden control scripts, so the
  document and the assertions describe the same runs.
- **`crates/sailgym-physics/tests/convergence.rs`** — three tests:
  `timestep_convergence`, `error_at_default_dt` and `the_reference_is_a_reference`
  (added: the `Rk4` reference's own error is Richardson-estimated and must be
  under 1 % of the smallest RK2 error it is used to measure; it is 0.0004–0.06 %).
- **`crates/sailgym-bench/src/bin/convergence.rs`** (new) prints the table and
  writes `docs/v1/convergence.md` with `--write`.
- **`docs/v1/convergence.md`** — the full table, the observed orders, and an
  honest account of the one scenario that does not fit the bracket.

### 10.3 — The rotation and mirror sweep (P-group A)

- **`crates/sailgym-physics/tests/symmetry.rs`** (new) — **96 cases**: 6 shipped
  scenarios × 8 world rotations × {upright, mirrored}, each run 20 s under a
  scripted control sequence and compared at **every** step.
- The interesting part is `Transformed`, a `WindField` wrapper that rotates and
  reflects the field itself:
  `W_T(p, t) = R_θ · M · W_inner(M · R_(−θ) · p, t)`. A procedural field draws
  its wavenumbers in *world* coordinates (F6.1) and is not rotationally
  symmetric, so rotating the boat and leaving the field alone is not "rotating
  the complete physical setup" (brief §35) — it is putting the boat somewhere
  else in the same field. Wrapping the field means `gybe`, the one gusty
  scenario, is swept on its own wind rather than on a uniform stand-in.
- Every failure message names the scenario, the rotation and the mirror flag.

### 10.4 — The headless benchmark (P-group B)

- **`crates/sailgym-bench/src/bin/bench.rs`** (new) — `--scenario`/`--seconds`
  as task 10.4 specifies, plus `--all`. Reports steps/s, real-time factor and a
  per-subsystem attribution (wind sampling, foil evaluation, whole force model,
  integration remainder) over the real call counts.
- Determinism is checked harder than the criterion asks: the two runs must agree
  on the step count **and** on all thirteen fields of the final state, by
  `to_bits()`.
- **`docs/v1/performance.md`** — every figure, with the machine spec.

### 10.5 — Render performance and the 60 fps target (P-group B)

- **`web/src/render/perfMarks.ts`** (new) — four spans (`frame`, `wasm`, `svg`,
  `deck`) recorded through `performance.mark`/`performance.measure`, ring
  buffers, and the input-lag pairing. Exposed to the suite as
  `window.__sailgymPerf`, the same arrangement section 09 used for
  `window.__sailgym`.
- **`web/tests/e2e/perf.spec.ts`** — section 08's frame-time test extended to a
  30 s Sail-Mode run, plus four new tests: the span breakdown, physics
  independence at 20 fps against 60, input lag, and in-browser headless
  stepping.
- The app was instrumented at four points; see §2.4 for the `Owns:` note.

### 10.6 — The three brief §46 demonstrations (P-group C)

- **`web/tests/e2e/demonstrations.spec.ts`** (new) — three tests, 9 runs across
  Chromium, Firefox and Edge, driven through the **real controls**: a held mouse
  drag on the boat for the mainsheet, `A`/`D` for the tiller, `Space` for the
  release. Each asserts an ordered chain of physical events, and every failure
  message prints the whole chain with its times.
- **Verified negatively, once, then reverted** (§5.3).

### 10.7 — Parameter provenance (P-group C)

- **`crates/sailgym-physics/tests/provenance.rs`** (new) — five audits:
  `every_field_tagged`, `no_stray_constants`, `no_physics_in_typescript`,
  `docs_match_source`, and the added `shipped_values_match_the_f7_table`.
- **`docs/v1/parameters.md`** — the catalogue table generated from
  `parameters.rs`'s own doc comments, wrapped in hand-written prose: brief §36's
  disclaimer, the four tags, the deferred validation routes with what each one
  would replace, the three things most wrong with the parameters today, and the
  brief §43 record of what has changed since F7.
- `parameters.rs` gained a per-owner tag override so a shared, flattened field
  can carry a different tag for each surface (§2.3).

### 10.8 — The final gate and the acceptance audit (P-group S)

- **`docs/v1/acceptance.md`** — brief §47's fourteen criteria, one row each, with
  named evidence and an honest verdict. Eleven **Met**, two **Met with a stated
  limit**, one **Partly met**.
- **`scripts/check.sh`** and **`scripts/check.ps1`** — step 4 gained
  `convergence`, `symmetry` and `provenance`; both scripts now print each step's
  wall time and the total, and both gained the `--fast` / `-Fast` subset.

---

## 2. Deviations from the PRD, and why

### 2.1 The convergence study needed a script, and two of its properties are load-bearing

Task 10.2 says "a fixed 20 s control script" without saying what is in it. Two
properties of the one chosen are deliberate, and neither touches a coefficient
(brief §43):

1. **The rudder command is never zero.** With no steering command the tiller
   self-centres at a constant rate and therefore cannot land on zero; it
   limit-cycles within `±delta_r_return_rate·dt` ≈ 0.008 rad, which
   `dynamics::rudder_rate` documents at its source. That is an `O(dt)`
   oscillation fed straight into the yaw moment, and it caps the observed order
   at **one** for every scenario. A convergence study that measured it would be
   measuring a known first-order artefact rather than the integrator.
2. **The rudder sweep stays inside ±0.334 rad**, well short of
   `delta_r_max = 0.698`. The first draft used larger commands, saturated the
   tiller for most of the run and left the boat spinning in place at half a
   knot — a study of the actuator clamp rather than of the physics. The measured
   difference: `close_hauled`'s observed order in position went from **1.45** to
   **2.02** when the script stopped pinning the rudder on its stop, with the
   same integrator and the same parameters.

Both are stated in the module's own doc comment, so the next person to change
the script knows what it is protecting.

### 2.2 `no_stray_constants` is enforced in three tiers, not one

Task 10.7 asks for "no numeric physical literal outside `parameters.rs` and
`constants.rs`, excluding `0.0`, `1.0`, `2.0`, `0.5` and array indices". Taken
literally that rejects `smoothstep`'s `3 − 2t`, RK4's `1/6` and the compass
conversion's `90°`, none of which is a physical coefficient. What shipped:

1. **The F7 rule exactly**, and it is the strong one:
   `shipped_values_match_the_f7_table` compares every numeric F7 row against the
   shipped catalogue (§2.6).
2. **Every float literal in non-test physics source** outside the two files must
   be universal (`0.0`, `1.0`, `2.0`, `0.5`), or be the right-hand side of a
   **named `const` declaration** — which is how `EPS_FLOW`, `EPS_ROPE`,
   `EPS_DET`, `MODE_BAND`, `RMS_PER_VARIATION` and `HULL_MODEL_VALID_TO` already
   live — or appear in an `EXEMPT` table with a stated reason. There are 24
   exempt values across six files, and the whole scan covers 209 literals.
3. **Every named `f64` constant that introduces a number must be documented.**
   A constant whose right-hand side has no non-universal literal
   (`const TWO_PI: f64 = 2.0 * PI;`) introduces nothing and needs nothing; one
   doc comment may head a run of declarations, which is how the Cody–Waite split
   of `2π` is written.

Integer literals with no decimal point are not scanned: they are counts, indices
and array sizes. `2` is not a coefficient; `2.0` might be.

### 2.3 `parameters.rs` changed, to fix the tag defect section 08 recorded

Section 08 §5: `FoilSection::area`'s doc comment named **two** tags
("KNOWN for the sail, ASSUMED for board and rudder"), `catalogue()` took the
first, and the panel showed KNOWN for all three surfaces — wrong for two of
them. Task 10.7's `every_field_tagged` requires **exactly one** tag per leaf, so
this section could not both meet its criterion and leave the defect.

F5.3 pins `FoilParams`'s field list, so splitting `area` out of the shared
struct is not available (F13.1). What shipped instead:

- `FoilSection::area` carries **no tag of its own**. The explanation of why sits
  in a plain `//` comment so the parser cannot see a tag word in it.
- Each owner states the tag on its own `section` field, in a machine-read form:
  `Tag override: \`area\` KNOWN — the ILCA 7 sail is 7.06 m² by class rule
  (brief §3).` `catalogue()` reads those and applies them to the flattened
  struct's leaves, one level only.
- A surface that forgets to override leaves an untagged leaf, which
  `every_field_tagged` refuses. The escape hatch cannot be left open silently.
- `parameters::a_shared_field_takes_its_tag_from_its_owner` is the guard, and it
  also asserts the base declaration carries no tag.

**No value changed.** `sail.area` is 7.06 m² and KNOWN; `board.area` is 0.20 m²
and `rudder.area` is 0.105 m², both ASSUMED, which is what F7 says.

Section 02's `every_parameter_field_is_tagged` had to move with it: it scanned
*declarations*, and a tag may now legitimately live on the owner. It now
resolves the catalogue first and asserts on the leaves — which is strictly
stronger, because it is what the panel actually shows — and keeps its floor of
55 fields and its declaration walk. Net coverage increased.

### 2.4 `Owns:` lists were widened (as in sections 05–09)

| File | Task | Why |
|---|---|---|
| `crates/sailgym-physics/tests/convergence/study.rs` (new) | 10.2 | one definition of the study, shared by the test and the bench binary (§1); the same arrangement section 09 §2.5 used for the golden scripts |
| `crates/sailgym-physics/src/parameters.rs` | 10.7 | the tag override of §2.3, without which `every_field_tagged` cannot pass |
| `web/src/sim/useSimulation.ts` | 10.5 | the `frame` and `wasm` spans, the input-lag timestamp on a steering key-down, and `renderHz` — the throttle the physics-independence measurement turns |
| `web/src/render/BoatSvg.tsx` | 10.5 | the `svg` span (render **and** commit, via a layout effect) and the far end of the input-lag measurement |
| `web/src/wind/DeckOverlay.tsx` | 10.5 | the `deck` span, which can only be taken between deck.gl's own before/after render hooks |
| `web/src/App.tsx` | 10.5 | reads `?renderHz=` and passes it to `useSimulation` |
| `web/tests/e2e/debug.spec.ts` | — | not owned by any task; a latent timing flake in a section 08 spec fired in this section's first full gate run (§5.5) |

10.1 and 10.8 are `P-group: S`; 10.2–10.7 were executed by the section agent
rather than delegated, so no parallel write conflict was possible. Flagged
because F13.2 is a rule about *reporting*, and this is the report. The task
lists in `docs/v1/10-hardening.md` were **not** edited.

### 2.5 The 30 s Sail-Mode measurement, and the `@slow` tag

Task 10.5 asks for a 30 s run; section 08's was 10 s. Extending it, plus the
four new perf tests and the three demonstrations, took the browser suite from
5.9 min to about 9 min and the gate past ten minutes — which is the condition
task 10.8 names. The seven long `describe` blocks carry `@slow` in their titles
and `--fast` excludes them by `--grep-invert`. Tagging rather than listing file
names means the subset cannot go stale when a file is renamed.

### 2.6 `shipped_values_match_the_f7_table` is an addition, not a criterion

Task 10.7 does not ask for it. It exists because every handoff from section 01
onwards asserts "no coefficient was tuned" and nothing has ever checked it. The
test parses the F7 tables out of `docs/v1/00-foundations.md` — the normative
document, not a copy — and compares **83 of the 85 numeric rows** against
`BoatParameters::ilca7()`. The two it cannot parse are `delta_r_self_centre`
(`true`) and `integrator` (`Rk2Midpoint`); both are asserted by hand rather than
left in a skipped list as if nobody had looked at them.

The comparison is relative at 0.5 %, because the F7 table rounds (`155 kg·m²`,
`0.262 rad (15°)`). That is generous enough for the printed precision and far
too tight for a coefficient that has been tuned.

**A parameter cannot now move without `00-foundations.md` moving with it**,
which is an edit F13.1 puts with the human. That is exactly the control brief
§43 asks for.

---

## 3. Where the PRD and the physics disagree

Both are recorded here and in the tests themselves rather than resolved by
quietly moving a number.

### 3.1 Demonstration 1's chain has two links the model contradicts

Task 10.6 writes demonstration 1 as: "hauling raises `sheet_tension` above 0;
`|beta|` stays small; sail `|cl|` stays above 0.5 (still powered); heeling
moment grows; `|phi|` exceeds `phi_capsize`."

Measured on the shipped `beam_reach_capsize` (7.0 m/s northerly, boat heading
east, sheet at `l_sheet_min`):

| t (s) | heel | `\|F_sail\|` (N) | `C_L` | heeling moment (N·m) |
|---|---|---|---|---|
| 0.01 | 0.0° | 365 | −0.001 | 870 |
| 1.0 | 34.3° | 196 | 0.089 | 467 |
| 3.0 | 44.6° | 168 | 0.468 | 411 |
| 5.0 | 49.5° | 142 | 0.747 | 348 |
| 7.0 | 56.4° | 104 | 0.891 | 249 |
| 9.0 | 80.3° | 9 | 0.204 | −7 |
| 10.0 | 87.1° | 8 | 0.172 | 19 |

**`|cl|` starts at 0.000, not above 0.5.** With the boom sheeted flat on the
centreline and the apparent wind on the beam, the sail sits at ≈ 90° of attack.
F5.2 gives a stalled plate `C_L ≈ 0` and `C_D ≈ C_N,max`, so the 365 N that
knocks the boat down at `t = 0` is **all drag**. The boat is powered; "powered"
is not the lift coefficient here. The test asserts the sail **force magnitude**
stays above 20 N through the whole run to capsize.

**The heeling moment falls, monotonically, while the heel grows.** That is brief
§46 step 7's actual wording — "Aerodynamic side force creates increasing *heel*"
— and it is right: as the boat lies down the rig comes out of the wind and the
lever arm shortens, so the moment collapses even as the angle runs away. A model
in which the heeling moment *grew* with heel would be a model with no `R_x(φ)`
in it. The test asserts the ordered growth of the heel, to capsize.

**And the release chain's middle two links are the other way round.** Task 10.6
lists "sail force magnitude falls" before "heeling moment falls". Measured at
nine release times between 3 s and 12 s, the moment halves **before** the force
does, every time, by 0.08–0.46 s:

| release at | tension → 0 | boom out 0.5 rad | moment halves | force halves | heel falls |
|---|---|---|---|---|---|
| 3.0 s | 3.01 | 3.23 | **3.48** | **3.56** | 3.60 |
| 5.0 s | 5.01 | 5.29 | **5.43** | **5.60** | 5.69 |
| 7.0 s | 7.01 | 7.39 | **7.53** | **7.99** | 8.01 |

The mechanism is not subtle: the boom swings out first, and that rotates the
sail force and shortens its heeling lever (`Generalized::add`, F6.4) before the
force *magnitude* has decayed at all. The moment is `r × F`; it can fall while
`|F|` does not, and the moment is what heels the boat. The test asserts the
measured order, still asserts both links, and bounds the gap between them at 2 s
so "the sail depowers" stays part of the claim.

The browser test releases the sheet at `t ≈ 4–5 s`, and asserts it landed
between 3 s and 7.5 s. Past about eight seconds the boat is already on its beam
ends, the rig is out of the wind and the sail force has collapsed on its own — at
which point "releasing the sheet depowers the sail" is no longer the thing being
demonstrated.

### 3.2 `beam_reach_capsize` converges faster than second order

Task 10.2's bracket is `[1.7, 2.3]` in position and heading for all three
scenarios, with an escape hatch: "If `beam_reach_capsize` fails to show clean
second order because capsize makes trajectories diverge chaotically, say so
explicitly and evaluate convergence on the pre-capsize window instead."

Measured:

| Scenario | position | heading | heel |
|---|---|---|---|
| `close_hauled` | **2.015** | **1.984** | 1.989 |
| `gybe` | **1.958** | **1.976** | 1.993 |
| `beam_reach_capsize` | **2.849** | **3.036** | 2.636 |

The bracket is exceeded **from above**, not below. Three things were checked
before it was written off:

1. *Is it the reference?* No. Re-running the whole study against `Rk4` at
   `dt = 7.8125e-5` — a quarter of the reference step — moves every observed
   order by less than 0.01 and every error by less than 1 %.
   `the_reference_is_a_reference` keeps that honest.
2. *Is it the capsize, as the PRD guesses?* Not in the way the PRD expects. On
   the pre-capsize window `0–8 s`, where the boat is at 65° of heel and has not
   yet tripped `phi_capsize`, the order is **3.64** in position — *further* from
   2, not closer. Re-cutting the window does not address it, so the window was
   left at the full 20 s.
3. *Is it chaotic divergence?* No. Divergence would show as an error that stops
   falling; every column falls monotonically with `dt`, which
   `timestep_convergence` asserts for all three scenarios.

What it is: this scenario heels onto the strongly attracting quasi-equilibrium
at ≈ 87–91° that section 07 §4 documents, where roll damping is large and the
heeling and righting moments nearly balance. Truncation error injected into that
contracting direction is squeezed out rather than accumulated, so the global
error over a fixed horizon falls faster than the method's formal order.

**The bracket was not widened.** The lower bound — the half that could catch a
defect, an integrator less accurate than claimed — is asserted for all three
scenarios exactly as written. The upper bound is excluded for
`beam_reach_capsize` alone, with the reasoning in the test. Section acceptance
criterion 3 asks for the bracket in at least two of three with any exclusion
documented and justified; that is what this is.

### 3.3 Task 10.5's Sail-Mode frame-time criterion, for the third time

"95th-percentile frame time in Sail Mode under 16.7 ms." On a 60 Hz display the
refresh period is 16.667 ms and rAF timestamps are quantised to 0.1 ms, so a
page that never drops a frame reports a mixture of 16.7 and 16.8. No page comes
in strictly under the display's own period. Section 03 met the ceiling, section
08 met it and wrote it up, and this section met it again with the same numbers.
Asserted in section 08's reachable form — median ≤ 16.7 ms, 95th percentile
within one quantum of the median, fewer than 2 % long frames — with **no
threshold value changed**. `docs/v1/performance.md` has the figures.

### 3.4 No contradiction was found in the normative documents

Section 07's two F6.7 contradictions (its §3.1 and §3.2) are unchanged and
unresolved; this section neither depends on them nor touches them. Everything
above is a PRD criterion that the physics contradicts, which is a different
thing from two normative documents contradicting each other.

---

## 4. Measurements

### 4.1 The invariant margins (task 10.1)

Every tolerance in `docs/v1/invariants.md` carries the worst residual actually
observed. The ones that are not exact:

| Invariant | Tolerance | Measured worst | Margin |
|---|---|---|---|
| `mirror_symmetry_trajectory` | 1e-10 | **0.0** | exact |
| `apparent_wind_consistency` | 1e-12 | **0.0** | exact |
| `roll_mirror_symmetry` | 1e-9 | 1.41e-15 | 7×10⁵ |
| `sail_mirror_symmetry_trajectory` | 1e-9 | 1.60e-14 | 6×10⁴ |
| `slack_sheet_free_boom` | 1e-9 | 3.23e-14 | 3×10⁴ |
| `coordinate_frame_consistency`, world | 1e-9 | 7.82e-14 | 1×10⁴ |
| `coordinate_frame_consistency`, body | 1e-11 | 3.22e-14 | 3×10² |
| `velocity_scaling` | 2 % | 1.97e-16 relative | round-off |
| `dissipative_behaviour` | +1e-9 J/step | −2.48e-4 J (never rose) | one-sided |
| `dissipative_with_roll` | +1e-9 J/step | −2.55e-5 J (never rose) | one-sided |
| `sheet_does_no_negative_work` | +1e-9 J/step | −3.73e-9 J (never rose) | one-sided |
| `tack_through_wind` | 0.3 rad/step | 0.0177 rad | 17× |
| `sheet_geometry_continuous` | 1 N·m over 1e-5 rad | 0.623 N·m | 1.6× |
| `symmetry::rotation_and_mirror_sweep`, body | 1e-11 | **1.34e-12** | 7.5× |
| `symmetry::rotation_and_mirror_sweep`, world | 1e-9 | 2.24e-13 | 4×10³ |
| `symmetry::rotation_and_mirror_sweep`, mirror | 1e-9 | 1.24e-12 | 8×10² |

The tightest is the symmetry sweep's body-frame bound at 7.5×. It is the PRD's
number and it was kept: "unchanged" is the actual content of that invariant, and
a looser bound would let a genuine frame error through as round-off.

**No tolerance was loosened in this section.** The one fixture that is restricted
rather than the bound — `sheet_does_no_negative_work`, whose sheet lengths are
drawn above `ℓ(0)` — was restricted by section 06 and is restated in
`docs/v1/invariants.md` with the `dt³` measurements that identify it as integrator
truncation.

### 4.2 Convergence (task 10.2)

Full table in `docs/v1/convergence.md`. At the shipped default `dt = 0.005` over
20 s, against an `Rk4` reference at `dt = 3.125e-4`:

| Scenario | position error | heading error | heel error |
|---|---|---|---|
| `close_hauled` | 0.000092 m | 0.000096° | 0.000019° |
| `beam_reach_capsize` | 0.0079 m | 0.0111° | 0.0047° |
| `gybe` | 0.0000076 m | 0.000014° | 0.000018° |

Task 10.2's bounds are 0.05 m and 0.5°. The worst case is **6× inside** the
position bound and **45× inside** the heel bound.

### 4.3 Performance (task 10.4, 10.5)

Everything is in `docs/v1/performance.md`; the headlines:

- **Native headless: 3 144–3 532× real time**, against brief §37's ≈ 100×
  target — met with a factor of about 31 in hand. The slowest is `gybe`, the
  only gusty scenario, whose procedural field costs 64 ns a sample against
  1.8 ns for a uniform one.
- **Breakdown:** the foil model is 37.6 % of the run and is the next
  optimisation's target; the whole force model is 59.3 % and integration is the
  rest. Wind sampling is 0.3 % on a uniform field and 8.1 % on a gusty one.
- **In-browser headless: 2 255–2 633× real time**, i.e. the WebAssembly build
  runs at 64–75 % of native.
- **Frame time:** every median on every browser is the display period. Chromium
  and Edge hold 16.70 ms p50 / 16.70–16.80 ms p95 in Sail Mode over 30 s;
  Firefox's software path holds 17.06 ms p50.
- **Where a frame goes:** the WASM boundary is **0.1–0.3 ms** of a 16.7 ms
  frame, the SVG render and commit 1.1–1.3 ms, deck.gl's draw 0.4–0.5 ms. About
  88 % of the frame is the browser waiting for vsync.
- **Physics independence:** `t` advances within **0.31–0.56 %** of the same rate
  at 20 fps as at 60, against a 2 % bound.
- **Input lag:** p95 **23.2 ms** (Chromium), **28.9 ms** (Edge), **36.0 ms**
  (Firefox), against a 50 ms bound.

### 4.4 Demonstration figures (task 10.6)

From the full gate run, Chromium:

| Demonstration | Chain, with simulated times |
|---|---|
| 1, hold | heel > 17° @0.33 → heel > 52° @5.76 → **capsized @10.00** |
| 1, release | tension → 0 @4.29 → boom out @4.59 → heeling moment halves @4.73 → sail force halves @4.86 → heel falls @4.96 |
| 2, tack | sail unloads (\|cl\| < 0.1) @2.41 → psi passes head to wind @3.84 → beta changes sign @4.14 → sail fills (\|cl\| > 0.5) @4.14 |
| 3, gybe | boom torque reverses @8.86 → boom crosses the centreline @10.03 → boom reaches the other side @10.20 |

Largest boom step between animation frames, demonstration 2: **0.061 rad**
(Chromium), **0.124 rad** (Firefox), against the 0.3 rad bound.

Demonstration 3's controlled/uncontrolled contrast, the brief §46 "result
differs visibly" claim as a number:

| | uncontrolled (sheet where the scenario set it) | controlled (hauled and held through) | difference |
|---|---|---|---|
| peak `\|β̇\|` | 6.57 rad/s | 2.20 rad/s | **199 %** |
| peak sheet tension | 941 N | 3 395 N | **261 %** |
| peak `\|β\|` | 1.49 rad (out to the rope) | ≈ 0.01 rad (pinned on the centreline) | — |

Task 10.6's bound is 30 %.

---

## 5. Validation evidence

### 5.1 The gate

`bash scripts/check.sh`, full chain, terminating in `check: all steps passed`
and **exiting 0**.

| Step | Result | Wall |
|---|---|---|
| 1 `cargo fmt --check` | passed | < 1 s |
| 2 `cargo clippy --all-targets -- -D warnings` | passed | < 1 s warm, 8 s cold |
| 3 `cargo test -p sailgym-physics` | **199** lib + 1 boom + 3 convergence + 8 determinism + 23 invariants + 5 no_shortcuts + 5 provenance + 6 regression + 1 symmetry + 5 wind, 0 failed | 28 s |
| 4 `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance` | 23 + 5 + 3 + 1 + 5, 0 failed | 16 s |
| 5 `--test regression` | **6 passed** | < 1 s |
| 6 `wasm-pack build` | passed | 7 s |
| 7 `pnpm --dir web typecheck` | passed | 1 s |
| 8 `pnpm --dir web test:e2e` | **243 passed**, Chromium / Firefox / Edge | **516 s** |
| | **total** | **568 s — 9 min 28 s** |

`pwsh` is not installed on this host, so section acceptance criterion 1's
literal `pwsh scripts/check.ps1` could not be executed and the Linux equivalent
was run instead. `check.ps1` **was** edited this section — step 4's target list,
the per-step timing and the `-Fast` switch — and was written to mirror
`check.sh` line for line; it is **unproven by execution**, which is the same
position sections 04–09 recorded and is the one thing in this section that a
Windows host should re-check first.

The lib count rose from section 09's 198 to 199: `parameters` gained
`a_shared_field_takes_its_tag_from_its_owner`. The e2e count rose from 222 to
**243**: +9 demonstrations, +12 perf (four new tests × three browsers).

### 5.2 Task acceptance criteria

**10.1** — `cargo test -p sailgym-physics --test invariants` → **23 passed**.
Every brief §35 item in task 10.1's required table maps to at least one named
test, and `documented_invariants_exist` asserts that from the document's side.
`tension_never_negative` runs 200 000 cases and asserts both branches of the
`max` were taken. **No tolerance was loosened** (§4.1).

**10.2** — `cargo test -p sailgym-physics --test convergence` → **3 passed**.
`timestep_convergence` (order ≥ 1.7 for all three; `[1.7, 2.3]` for two of three,
with the third's exclusion measured and argued in §3.2, and monotone error
decrease asserted for all three); `error_at_default_dt` (§4.2);
`the_reference_is_a_reference` (added). `docs/v1/convergence.md` written by
`cargo run --release -p sailgym-bench --bin convergence -- --write docs/v1/convergence.md`.

**10.3** — `cargo test -p sailgym-physics --test symmetry` → **1 test, 96
cases**, 0 failed. Worst \|Δ\|: body **1.34e-12** (tol 1e-11), world **2.24e-13**
(tol 1e-9), mirror **1.24e-12** (tol 1e-9). Every case names itself in every
message; all six scenarios are asserted to have travelled more than a metre, so
no case passes by standing still.

**10.4** — `cargo run --release -p sailgym-bench --bin bench -- --all --seconds 600`.
Native ≥ 100× real time: **met, 3 144–3 532×** (§4.3). Deterministic: two runs
per scenario, identical step counts **and** bit-identical final states.
In-browser headless measured: **2 255–2 633×**. `docs/v1/performance.md` records
all figures with the machine spec.

**10.5** — `pnpm --dir web test:e2e perf` → **15 passed** (5 tests × 3 browsers).
Sail-Mode 95th percentile over 30 s: asserted in its reachable form (§3.3).
Physics independence: **0.31–0.56 %** against 2 %. Input lag: **23.2/28.9/36.0 ms**
p95 against 50 ms. All figures in `docs/v1/performance.md`.

**10.6** — `pnpm --dir web test:e2e demonstrations` → **9 passed** (3 × 3
browsers). Each test asserts its full ordered chain; none asserts only an
endpoint. The negative check is §5.3.

**10.7** — `cargo test -p sailgym-physics --test provenance` → **5 passed**:
`every_field_tagged` (89 leaves — 7 KNOWN, 48 ASSUMED, 28 TUNABLE, 6 DEFERRED,
exactly one tag each), `no_stray_constants` (209 literals scanned, 11 inside
named constants, no offender), `no_physics_in_typescript` (45 hand-written
TypeScript files, clean), `docs_match_source`, and
`shipped_values_match_the_f7_table` (83 rows, all equal).

**10.8** — `docs/v1/acceptance.md` has a row per brief §47 criterion with named
evidence; the gate runs the complete chain and exits 0; the total is recorded
and the subset documented (§6.2).

### 5.3 The negative check: disabling the sheet's boom torque

Task 10.6 requires it, once, then reverted. `BoomMoments::total` was changed
from `self.aero + self.sheet + …` to `self.aero + 0.0 * self.sheet + …`, the
WASM package rebuilt, and the demonstrations re-run on Chromium:

| Demonstration | Result | Message |
|---|---|---|
| 1 — capsize and recovery | **FAILED** | `the boom left the centreline at t = 0.165` |
| 2 — tack | passed | correct: the tack demonstration is about a boom the sheet is *not* restraining, so it does not depend on sheet torque |
| 3 — gybe | **FAILED** | `boom torque reverses never happened` — with no sheet torque there is no gybe to observe, because the boom never sits where one can start |

Reverted (`git diff` on `rigging/boom.rs` is empty) and the WASM package rebuilt
from the reverted source. Demonstration 2 passing is worth stating rather than
hiding: the criterion names demonstrations 1 and 3, and 2 is the one that should
survive.

### 5.4 Every new audit was proven able to fail, once, then reverted

Section 07 established the discipline: an audit that cannot fail is not an
audit. Each of the six new ones had its forbidden pattern introduced, the
failure observed, and the change reverted.

| Test | Pattern introduced | Result |
|---|---|---|
| `provenance::no_stray_constants` | `pub fn probe_drag(u: f64) -> f64 { 9.87 * u }` in `hydro/hull.rs` | **FAILED** — `hydro/hull.rs:182: 9.87` |
| `provenance::no_stray_constants` | `const PROBE_RHO: f64 = 1030.5;` with no doc comment | **FAILED** — `hydro/hull.rs:181: const PROBE_RHO: f64 = 1030.5;` |
| `provenance::no_physics_in_typescript` | `export const PROBE_RHO_AIR = 1.225` in `web/src/sim/units.ts` | **FAILED** — `sim/units.ts:61: 1.225 in …` |
| `provenance::shipped_values_match_the_f7_table` | `resistance.y_v` 40.0 → 40.4 (1 %) | **FAILED** — `resistance.y_v: shipped 40.4, F7 says 40 (40.0 N·s/m)` |
| `provenance::every_field_tagged` | a second tag word added to `sail.boom_length`'s doc comment | **FAILED** — `sail.boom_length: documentation names 2 tags ["KNOWN", "ASSUMED"] — exactly one is required` |
| `provenance::docs_match_source` | one value edited in the generated region of `docs/v1/parameters.md` | **FAILED**, with the regeneration command in the message |
| `invariants::documented_invariants_exist` | a new `#[test]` in `invariants.rs` with no row in the document | **FAILED**, naming the undocumented test |
| `invariants::documented_invariants_exist` | a document row renamed to a test that does not exist | **FAILED**, naming the missing test |

All eight reverted; `--test provenance` and `--test invariants` re-run green
afterwards (5 and 23 passed).

The two audits from earlier sections that this one extended —
`parameters::every_parameter_field_is_tagged`, rescoped to resolve the catalogue
first, and `symmetry` — are covered by the same reasoning: the first was proven
able to fail by the `FoilSection::area` defect itself, which it caught the
moment the tag was removed from the shared declaration, and the second's
tolerances have a measured 7.5× margin rather than an arbitrary one.

### 5.5 Two flakes found by the first full gate run, both fixed at the cause

Neither was in this section's physics.

| Failure | Cause | Fix |
|---|---|---|
| `perf.spec.ts` "the four instrumented spans are recorded", Firefox: `wasm span reported a zero median` | Firefox coarsens `performance.now()` to whole milliseconds by default (`privacy.reduceTimerPrecision`), so a span that genuinely costs 0.2 ms reports a median of exactly 0. That is a statement about the clock, not the page. | The non-zero check moved from the median to the **maximum**, which still lands on a non-zero quantum. The count check is unchanged and the p95 bounds on the GPU browsers are unchanged. |
| `debug.spec.ts` "R6 is surfaced as a warning", Edge: `expected > 5, received 4.9519` | Section 08's spec loads the `fast` fixture at `u = 6.5 m/s` and then switches modes before reading `u`. At 6.5 m/s the hull sheds ≈ 2.8 m/s², so the boat falls from 6.5 to 5.0 m/s in **half a second** — less time than a mode switch and its attribute wait. A latent flake since section 08, surfaced by the extra parallel load. | The clock is paused and reset **before** the mode switch, so the "above the limit" half is a statement about the scenario; the "back below it" half resumes and polls exactly as before. No assertion or threshold changed. |

### 5.6 Not in the gate, run anyway

- `pnpm --dir web test:unit` — 97 passed in 14 files, unchanged.
- `pnpm --dir web build` — exit 0; bundle unchanged at 1 734 kB / 356 kB gzipped.
- `bash scripts/check.sh --fast` — the subset: **74 passed, 149 s**, exit 0.

  It failed once before that, and the cause is worth recording because it is
  *not* the application: four `console.error: Failed to load resource:
  net::ERR_NETWORK_CHANGED` entries from Chromium, which is this host's network
  interface reconfiguring itself mid-run and the dev server becoming briefly
  unreachable. Section 01's console-error trap caught it, which is the trap
  doing its job on a real page error; the same spec had passed on all three
  browsers in the full gate run half an hour earlier and passed again on the
  re-run. Nothing was changed.

### 5.7 Exact tool versions

Measured on this host.

| Tool | Version |
|---|---|
| Host | `x86_64-unknown-linux-gnu`, Ubuntu 24.04.4 LTS, Linux 7.0.0-30-generic |
| CPU / GPU | AMD Ryzen AI 9 HX 370 (12C/24T) / NVIDIA RTX 4070 Max-Q + AMD Radeon 890M |
| rustc | `1.98.1 (48a229cea 2026-09-01)`, commit `48a229ceaefd4985c50990b14116b6d856af0985` |
| LLVM | `22.1.8` |
| cargo | `1.98.1 (797e8a9bc 2026-08-05)` |
| rustfmt | `1.9.0-stable (48a229ceae 2026-09-01)` |
| clippy | `0.1.98 (48a229ceae 2026-09-01)` |
| wasm-pack | `0.15.0` |
| wasm-bindgen (`Cargo.lock`) | `0.2.128` |
| serde_json (`Cargo.lock`) | `1.0.151`, `float_roundtrip` enabled |
| Node / pnpm | `v26.3.0` / `11.6.0` |
| TypeScript | `^7.0.2` |
| Vite / Vitest | `^7.3.6` / `^5.0.1` |
| React / React DOM | `^19.3.0` / `^19.3.0` |
| zustand | `^5.0.15` |
| @playwright/test | `^1.63.0` |
| @deck.gl/core, /layers, /react | `^9.4.0` |
| @swc/core | `1.15.47`, pinned (section 03 §5) |
| PowerShell | **not installed** |

**No new dependency was added this section**, in either Cargo or npm.
`Cargo.lock` and `web/pnpm-lock.yaml` are unchanged.

---

## 6. The gate

### 6.1 What step 4 now runs

```
cargo test -p sailgym-physics \
    --test invariants --test no_shortcuts \
    --test convergence --test symmetry --test provenance
```

Five targets in one invocation: the brief §35 invariants, section 07's
prohibited-shortcut audits, this section's convergence study and rotation/mirror
sweep, and the F7 provenance audit. Task 10.8 names `convergence`, `symmetry`,
`provenance` and `no_shortcuts`; `no_shortcuts` was already there from section
07.

### 6.2 Ten minutes, and the split

The full chain is **568 s — 9 min 28 s**, of which step 8 is 516 s. That does
not strictly *exceed* task 10.8's ten minutes, but it is close enough, and 91 %
of it is one step, that the split was built anyway:

```
scripts/check.sh --fast        # pwsh scripts/check.ps1 -Fast
```

runs the same eight steps with step 8 restricted to Chromium and to the specs
not tagged `@slow` — the browser performance measurement and the three brief §46
demonstrations. Measured: **74 tests in 149 s — 2 min 29 s**.

**`--fast` is not the gate**, and both scripts say so in their own header. F13.7
is unchanged: the full chain is what has to be green before a section is
finished, and it is what CI runs.

Both scripts now print each step's wall time and the total, so the next person
to ask "is this getting slower?" has the number without instrumenting anything.

---

## 7. Parameters changed

**No physical coefficient, no timestep and no F7 value was changed**, and
nothing was tuned to make a scenario look better (brief §43, F13.5). That is now
mechanically enforced by `provenance::shipped_values_match_the_f7_table` rather
than asserted (§2.6).

One **tag** changed, with no value behind it: `board.area` and `rudder.area`
KNOWN → ASSUMED, which is what F7 already said and what section 08 §5 recorded
as a defect. §2.3, and `docs/v1/parameters.md`'s change table.

No new constant was added to the physics crate. `parameters.rs` gained
`TagOverride` and `tag_overrides`, which are parsing machinery and hold no
number.

---

## 8. Risks

- **R1 — mainsheet stiffness vs. timestep: closed as far as this project goes.**
  Section 06's 3×3 table stands, and section 10 adds a full `dt` sweep: the
  convergence study runs every shipped scenario at `dt ∈ {0.01, 0.005, 0.0025,
  0.00125}` and the errors fall monotonically at every step, in every component,
  for all three scenarios. Nothing blows up anywhere in the sweep. Section 06's
  `dt = 0.01` caveat stands and is unchanged: the boom mode gains energy at that
  timestep in the one configuration where the sheet's own damping vanishes.
  **Keep `dt = 0.005`.**
- **R2 — the boat may be too tender: closed by section 07 and unchanged.** No
  `sailor_pos_b.y` exists. `docs/v1/README.md`'s open item requiring human sign-off
  can be closed.
- **R3 — sign-convention drift: the guard is now systematic.** The ad-hoc mirror
  tests are joined by a **96-case** sweep over every shipped scenario, eight
  world rotations and both mirror states, comparing at every step of 20 s, with
  the wind field itself rotated and reflected. This is the strongest form the
  project can give R3 and it holds at 1.3e-12 against a 1e-11 bound.
- **R4 — the M1 placeholder: closed since section 04.** Still gone.
- **R5 — deck.gl: does not fire on a GPU.** Chromium and Edge hold the vsync
  limit with all sixteen overlays and all eight charts. The `deck` span is now
  measured directly: 0.4–0.5 ms a frame on a GPU, 2 ms on Firefox's software
  path. Unchanged from section 08.
- **R6 — the hull model has no planing regime: quantified and surfaced.**
  `docs/v1/performance.md` states the speed above which its resistance is
  untrustworthy — **≈ 5 m/s**, where the quadratic term already carries 90 % of
  the total — and `diagnostics.hull_model_warning` publishes it to the panel.
  Task 10.4's risk item asks for exactly this.
- **R7 — golden files are build-sensitive: unchanged and working.** The six
  regressions pass with worst \|Δ\| **0** on this build. Task 10.8's "confirm the
  skip path works on a second machine if one is available" could **not** be
  done: this host is the only one available, and the goldens were recorded on
  it. The skip path itself was proven by section 09 by editing a recorded
  toolchain string, and that proof stands; what is unproven is the *end to end*
  case of a genuinely different machine. §10.
- **New this section: none.** No new risk was created.

---

## 9. What an RL or validation effort should tackle first

The close-out note task 10.8 asks for. In order.

1. **Decide `stability.gm`.** It is the single most consequential open number
   (section 07 §4): at 1.00 m the fitted `GZ` regains positive stability past
   ≈ 82° and peaks *larger* there than upright, so the boat lies down at ≈ 86°
   and cannot be sailed over or inverted at any wind speed. ≈ 0.55 m makes the
   F7 stability group self-consistent. It changes the roll period by `√0.55`
   and invalidates all six golden files, so it has to be decided **before**
   anything else is fitted, and `gen_golden` re-run after the change is
   committed.
2. **Fix `l_sheet_min`.** At 0.90 m it is shorter than the shortest geometric
   rope path (`ℓ(0) = 1.0404 m`), so a fully hauled sheet carries 2.8 kN of
   permanent pre-tension and the boom is undamped by the sheet at `β = 0`
   (section 06 §4). `l_sheet_min ≥ 1.05 m` removes both. This is also what the
   `sheet_does_no_negative_work` fixture restriction exists to work around.
3. **Fit the hull.** The eight `resistance.*` coefficients are the largest
   single source of quantitative error and the easiest to replace: they are one
   function behind a named-parameter interface, and R6 gives the speed above
   which the present ones are extrapolation. Towing-tank data or a published
   Laser VPP would move the model from "physically motivated" to "fitted"
   without touching anything else.
4. **Give the sail a camber.** `sail.alpha_camber` is DEFERRED at 0.0, so the
   sail is a symmetric plate. That is why `|C_L|` is ≈ 0 on a sheeted-in beam
   reach (§3.1) and why the boat is slower than an ILCA. The hook exists, is
   odd in `α` by construction (F5.2's `tanh`), and turning it on breaks no
   symmetry invariant.
5. **Batch the environment.** Nothing in `sailgym-physics` precludes it — no
   global state, no wall clock, an explicit seed, and `evaluate` is pure — but
   nothing has been done either. A single environment steps at 3 500× real time;
   the foil model is 38 % of that and is three independent calls to one
   function, which is where a SIMD or SoA rewrite would start.
6. **Give the episode a reward.** The hook is there and is asserted zero
   (`recording::reward_placeholder_zero`), the format is versioned and
   round-trips bit-for-bit, and brief §33 asks only for the hook. Nothing has
   been designed.
7. **Project a `Diagnostics` from a recorded frame.** Section 09 §8.1: the
   debug panel does not follow a replay because an `EpisodeFrame` carries 38
   scalars and the record has 50 fields. The clean fix is a `Simulation`-free
   diagnostics constructor, which is a design decision rather than a patch and
   is the natural next step toward brief §45's episode inspector.

---

## 10. Remaining issues

1. **`stability.gm = 1.00 m`** (section 07 §4). Needs a human decision. See §9.1.
2. **`sheet.l_sheet_min = 0.90 m`** (section 06 §4). Needs a human decision.
   See §9.2.
3. **F6.7's internal contradiction** (section 07 §3.1) and
   `gz_negative_beyond_vanishing` being asserted on a different parameter set
   than the F7 defaults (section 07 §3.2). Both are downstream of item 1.
4. **`check.ps1` is unproven by execution.** `pwsh` is not installed on this
   host and has not been on any host since section 04. This section edited it,
   so a Windows run is the first thing to do there. `check.sh` is proven.
5. **R7's second machine.** Task 10.8 asks to confirm the golden skip path on a
   second machine if one is available. None was. The skip path is proven by
   section 09's edited-fingerprint test; the end-to-end different-machine case
   is not.
6. **CI has never run.** The repository has no remote; `.github/workflows/ci.yml`
   is correct by inspection and calls `scripts/check.sh`, which is now proven,
   but the Linux CI path and `playwright install --with-deps msedge` on
   `ubuntu-latest` remain unexercised — unchanged since section 01.
7. **The production bundle is 1 734 kB / 356 kB gzipped and still one chunk**
   (section 08 §4). No budget exists. Section 08 suggested this section set one;
   it did not, because splitting the bundle is a build change with no acceptance
   criterion behind it and no measured problem — the page holds 60 fps.
8. **`gen_golden --allow-dirty` still exists** (section 09 §2.4). Used once,
   deliberately, recorded. Delete it if you would rather every regeneration
   followed a commit.
9. **The committed goldens are `debug`-profile only** (section 09 §10.3).
   `cargo test --release --test regression` skips all six, correctly and loudly.
10. **`@loaders.gl` moved 4.4.5 → 4.5.1 in the lockfile** (section 08 §6), as a
    side effect of adding `zustand`. Verified green many times since; revert
    with a pin if you would rather it had not.
11. **Task 10.6's demonstration-1 chain and task 10.2's order bracket** are
    stated in `docs/v1/10-hardening.md` in forms the physics contradicts (§3.1,
    §3.2). The PRD text should be corrected; **it has not been edited.**
12. **`CLAUDE.md`'s gate table under-describes step 4.** It still reads "The
    physical invariants still hold (brief section 35)", and step 4 now also runs
    the prohibited-shortcut audits (section 07), the convergence study, the
    96-case symmetry sweep and the F7 provenance audit. It should also mention
    `--fast`. No section-10 task owns `CLAUDE.md`, so per F13.2 this is reported
    rather than edited — one row and one line of prose, and neither is a
    convention, an equation or a parameter, so it does not conflict with that
    file's own rule about what it may contain.
