# Section 10 — Invariants, Convergence, Performance, Demonstrations (M9)

**Prerequisite reading:** `docs/00-foundations.md` (all), all prior handoff notes,
`docs/brief.md` §35, §36, §37, §43, §46, §47.

## Goal

Close out the prototype: complete the brief §35 invariant suite with stated
tolerances, demonstrate numerical convergence, hit the brief §37 performance
targets, script the three brief §46 demonstrations as automated tests, and audit
every parameter for honest provenance.

This section adds no physics. If a physics change proves necessary, it is a
defect in an earlier section — fix it there conceptually, record it here, and
regenerate the affected goldens deliberately.

---

## Tasks

### 10.1 — Complete the invariant suite with tolerances
**P-group: S**
**Owns:** `crates/sailgym-physics/tests/invariants.rs`, `docs/invariants.md`

Every item in brief §35 must be present with an explicit, justified tolerance.
Audit the accumulated suite against the brief's list and fill every gap.

`docs/invariants.md` is a table: invariant, brief reference, test name, tolerance,
and **why that tolerance** — "1e-9 because RK2 round-off over 6000 steps at
`dt = 0.005` accumulates to roughly 1e-11" is acceptable; "1e-9 seemed fine" is
not. A tolerance chosen to make a failing test pass is a defect, and the
justification column is where that becomes visible.

Required final set (some already exist from sections 04–09):

| Invariant | Brief | Test |
|---|---|---|
| Rest equilibrium | §35 | `rest_equilibrium` |
| Port/starboard mirror symmetry | §35 | `mirror_symmetry_trajectory`, `roll_mirror_symmetry`, `sail_mirror_symmetry_trajectory` |
| Sheet unilateral constraint | §35 | `sheet_unilateral_constraint`, `tension_never_negative` |
| Force sign sanity | §35 | `force_sign_sanity` |
| Velocity scaling `F ∝ V²` | §35 | `velocity_scaling` |
| Zero-flow foil behaviour | §35 | `zero_flow_foil_behaviour` |
| Dissipative behaviour | §35 | `dissipative_behaviour`, `dissipative_with_roll`, `sheet_does_no_negative_work` |
| Coordinate-frame consistency | §35 | `coordinate_frame_consistency` |
| Time-step convergence | §35 | `timestep_convergence` (task 10.2) |
| Deterministic replay | §35 | `deterministic_replay` |
| Finite-number invariant | §35 | `finite_number_invariant`, `capsize_finite` |

**Acceptance criteria**
- `cargo test -p sailgym-physics --test invariants` green; every brief §35 item
  maps to at least one named test, asserted by a test that reads
  `docs/invariants.md` and checks each listed test name exists in the suite.
- No tolerance was loosened during this section. If one was, the handoff note
  states which, from what to what, and why — and that is a finding, not a fix.

---

### 10.2 — Time-step convergence study
**P-group: A**
**Owns:** `crates/sailgym-physics/tests/convergence.rs`, `crates/sailgym-bench/src/bin/convergence.rs`
**Depends:** 10.1

Brief §35 requires running at `dt`, `dt/2`, `dt/4` and demonstrating convergence
over a defined horizon.

Method: for each of three scenarios (`close_hauled`, `beam_reach_capsize`,
`gybe`), run a fixed 20 s control script at `dt ∈ {0.01, 0.005, 0.0025, 0.00125}`
and compare each against an `Rk4` reference at `dt = 0.0003125`. Report the error
norm in position, heading and heel.

```rust
pub struct ConvergenceResult { pub dt: f64, pub err_pos: f64, pub err_psi: f64, pub err_phi: f64 }
pub fn observed_order(results: &[ConvergenceResult]) -> f64;
```

**Acceptance criteria**
- `cargo test -p sailgym-physics --test convergence` exits 0.
- `timestep_convergence`: the observed order of accuracy for RK2 is in
  `[1.7, 2.3]` for all three scenarios in position and heading.
- `error_at_default_dt`: at the default `dt = 0.005`, position error over 20 s is
  below **0.05 m** and heel error below **0.5°** against the Rk4 reference.
- If `beam_reach_capsize` fails to show clean second order because capsize makes
  trajectories diverge chaotically, say so explicitly and evaluate convergence on
  the pre-capsize window instead — documenting the window. Do **not** widen the
  order tolerance to accommodate chaos.
- The full convergence table is written to `docs/convergence.md`.

---

### 10.3 — Symmetry and rotation harness
**P-group: A**
**Owns:** `crates/sailgym-physics/tests/symmetry.rs`
**Depends:** 10.1

Promote the ad-hoc symmetry checks into a systematic sweep: 6 scenarios × 8 world
rotations × mirrored/unmirrored, each run 20 s with a scripted control sequence.

**Acceptance criteria**
- `cargo test -p sailgym-physics --test symmetry` exits 0, 96 cases.
- Rotation: body-frame quantities `u, v, r, p, beta, delta_r` are **unchanged**
  within 1e-11; world position rotates by exactly the applied angle within 1e-9.
- Mirror: every quantity matches the F-tabulated mirror map within 1e-9.
- Any case that fails identifies itself by scenario, rotation and mirror flag in
  the failure message — a bare assertion failure here is not debuggable.

---

### 10.4 — Headless performance benchmark
**P-group: B**
**Owns:** `crates/sailgym-bench/src/bin/bench.rs`, `docs/performance.md`
**Depends:** 10.1

Brief §37: at least approximately **100× real time** for a single headless
environment, as an aspirational benchmark. Measure both native and in-browser
headless (rendering disabled).

```
cargo run --release -p sailgym-bench --bin bench -- --scenario close_hauled --seconds 600
```

Report: steps/s, real-time factor, and a per-subsystem breakdown (wind sampling,
foil evaluation, integration) so the next optimisation has a target.

**Acceptance criteria**
- Native release build achieves **≥ 100× real time** at `dt = 0.005` for
  `close_hauled`. If it does not, report the measured factor and the profile
  breakdown rather than optimising blindly — brief §37 says the exact number is
  secondary to correctness and architecture.
- In-browser headless (physics stepping with rendering disabled) real-time factor
  measured and recorded.
- `docs/performance.md` records all figures with the machine spec.
- The benchmark is deterministic: two runs report identical step counts.

---

### 10.5 — Render performance and 60 fps target
**P-group: B**
**Owns:** `web/tests/e2e/perf.spec.ts` (extends 8.6), `web/src/render/perfMarks.ts`
**Depends:** 10.4

Brief §37: 60 fps rendering on a modern desktop browser, physics independent of
render rate, no visible input lag from the WASM/UI architecture.

Instrument with `performance.mark`/`measure`: frame time, WASM call time, SVG
update time, deck.gl draw time.

Input-lag measurement: timestamp a synthetic keydown, and timestamp the first
frame in which the rendered rudder reflects it. This is the brief §37 "no visible
input lag" criterion made measurable.

**Acceptance criteria**
- 95th-percentile frame time in Sail Mode under 16.7 ms across a 30 s run on all
  three target browsers.
- Physics independence: at 4× speed with rendering throttled to 20 fps, `t`
  advances at the same rate as at 60 fps within 2 %.
- Measured input lag under **50 ms** at the 95th percentile.
- All figures recorded in `docs/performance.md`.

---

### 10.6 — The three brief §46 demonstrations as automated tests
**P-group: C**
**Owns:** `web/tests/e2e/demonstrations.spec.ts`
**Depends:** 10.1

Brief §46 defines three demonstrations. Each becomes a scripted E2E test
asserting the **physical chain**, not just the endpoint — a test that only checks
"did it capsize" would pass for a hard-coded capsize.

**Demonstration 1 — capsize and recovery** (`beam_reach_capsize` /
`sheet_release_recovery`). Assert in order: wind field animating; boat starts
near beam-to-wind; hauling raises `sheet_tension` above 0; `|beta|` stays small;
sail `|cl|` stays above 0.5 (still powered); heeling moment grows; `|phi|`
exceeds `phi_capsize`. Then reset and, from the same state, release: assert
`sheet_tension → 0`, then `|beta|` grows, then sail force magnitude falls, then
heeling moment falls, then `|phi|` decreases toward upright. The **ordering** of
those five is the assertion.

**Demonstration 2 — tack.** From `tack.json`, scripted rudder input: `psi` passes
through the wind; sail `|cl|` dips below 0.1 near head-to-wind (unloads);
`beta` changes sign; `|cl|` recovers above 0.5 on the new tack. Assert `beta`'s
sign change is continuous — no step larger than 0.3 rad between frames.

**Demonstration 3 — gybe.** From `gybe.json`, downwind, scripted turn: the sign
of the aerodynamic boom moment `m_beta` reverses; `|beta_dot|` peaks above
2 rad/s as the boom crosses; `sheet_tension` shows a transient spike above its
pre-gybe mean. Run twice — once with the sheet hauled through the gybe, once with
it eased — and assert the peak `|beta_dot|` and peak tension differ by more than
30 % between the two (brief §46: "result differs visibly between controlled and
uncontrolled sheet handling").

**Acceptance criteria**
- `pnpm --dir web test:e2e demonstrations` exits 0 on all three browsers.
- Each test asserts its full ordered chain; none asserts only an endpoint.
- Deliberately disabling the sheet's contribution to boom torque makes
  demonstrations 1 and 3 fail — verify once, then revert.

---

### 10.7 — Parameter provenance audit
**P-group: C**
**Owns:** `docs/parameters.md`, `crates/sailgym-physics/tests/provenance.rs`
**Depends:** 10.1

Brief §36 and §48: every important physical parameter must carry an honest
KNOWN / ASSUMED / TUNABLE / DEFERRED tag, and the prototype must **not claim
quantitative ILCA accuracy** (brief §36 last line).

`docs/parameters.md` is generated from `parameters.rs` doc comments: field, value,
unit, tag, source or rationale, and — for anything changed since F7 — the reason,
recorded per brief §43.

**Acceptance criteria**
- `cargo test -p sailgym-physics --test provenance`:
  - `every_field_tagged`: every leaf field's doc comment contains exactly one tag.
  - `no_stray_constants`: `grep` finds no numeric physical literal outside
    `parameters.rs` and `constants.rs`, excluding `0.0`, `1.0`, `2.0`, `0.5` and
    array indices. This is the F7 rule made enforceable.
  - `no_physics_in_typescript`: `grep` over `web/src` finds no occurrence of
    `1.225`, `1025`, `9.81`, `9.80665`, and no `0.5 *` adjacent to a density
    identifier.
  - `docs_match_source`: `docs/parameters.md` regenerated matches the committed
    copy — the doc cannot drift from the code.
- `docs/parameters.md` contains an explicit statement that the prototype does not
  claim validated ILCA performance (brief §36), and lists the deferred validation
  routes from brief §36.
- Every parameter that changed from its F7 default during sections 02–09 is
  listed with its reason, cross-referenced to the handoff note that recorded it.

---

### 10.8 — Final gate and success-criteria audit
**P-group: S**
**Owns:** `docs/acceptance.md`, `scripts/check.ps1`, `scripts/check.sh`
**Depends:** all

Walk brief §47's fourteen success criteria one by one. For each, name the test,
command or recorded demonstration that evidences it, and mark it met or not met.
An honest "not met" with an explanation is a valid outcome and far more useful
than a generous reading.

Update `scripts/check.*` to include `convergence`, `symmetry`, `provenance` and
`no_shortcuts`.

**Acceptance criteria**
- `docs/acceptance.md` has a row per brief §47 criterion with evidence.
- `pwsh scripts/check.ps1` runs the complete chain and exits 0.
- Total gate wall time recorded; if it exceeds 10 minutes, split it into a fast
  pre-commit subset and a full subset, and document both.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0 end to end, including every suite added here.
2. Every brief §35 invariant is implemented with a **justified** tolerance in
   `docs/invariants.md`.
3. RK2 convergence order in `[1.7, 2.3]` demonstrated for at least two of three
   scenarios, with any exclusion documented and justified.
4. All three brief §46 demonstrations pass as automated tests asserting ordered
   physical chains.
5. Performance: native headless real-time factor measured against the 100×
   target; browser 95th-percentile frame time and input lag measured and recorded.
6. `no_stray_constants` and `no_physics_in_typescript` pass — the F7 and brief §23
   boundaries are mechanically enforced.
7. `docs/acceptance.md` completed with an honest verdict on each of brief §47's
   fourteen criteria.
8. `docs/progress/10-handoff.md` written as a project-close note: what works,
   what is approximate, what an RL or validation effort should tackle first.

## Risks touched

- **R1** — final stability confirmation across the full `dt` sweep in 10.2.
- **R6** — the convergence and performance numbers make the hull model's limits
  concrete. Record the speed above which its resistance is untrustworthy.
- **R7** — golden regressions and convergence goldens both carry toolchain
  metadata; confirm the skip path works on a second machine if one is available.

## Explicitly out of scope

Everything in brief §44 remains deferred. In particular, this section does not
add: currents, waves, heave or pitch, sail cloth modelling, traveler/vang
controls, hiking, multiple boats, RL training, or any quantitative certification
against real ILCA performance. If the work here suggests one of them, record it
as a recommendation in the close-out note — do not implement it.
