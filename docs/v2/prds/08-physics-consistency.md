# v2 Section 08 — Physics consistency and an identified baseline

**Milestone:** M-next.1. **Status:** proposed; S8 and F18 model deltas open.
**Read first:** v1 foundations F6/F7/F9/F11/F13, v1 §43, v2 brief and foundations, section-01 handoff, [physics discussion](../discussions/physics-tightening.md).
**Dependencies:** shipped 01 only. This section precedes 02's fixture generation.

## Goal

Make the reduced model consistent with its declared stability and rope contracts before adding learning scores or freezing a second implementation. Preserve the 13-state model, deterministic RK2 stepping and force-based motion. Correctness evidence comes before new default values.

## Observed baseline and decisions required

| Finding in current source | Required outcome |
|---|---|
| Three-harmonic GZ fit constrains slope, value at a named peak and a zero, but not zero derivative at that peak; default curve can regain positive stability beyond the intended vanishing angle | Declare the supported heel domain and make `phi_peak` an actual maximum; no unintended positive stability between vanishing angle and inversion |
| `l_sheet_min = 0.9 m` is below the approximately `1.0404 m` shortest configured rope path | Eliminate unintended initial preload, or explicitly model and document intentional preload; do not hide roughly 2.8 kN initial tension by tuning damping |
| `max(0, k*e + c*edot)` can be positive for `e < 0` | Slack has zero tension even under large positive extension rate; specify the exact `e == 0` boundary |
| Goldens preserve current trajectories | Keep before/after evidence; new goldens follow reviewed corrections rather than certifying the old model |

Do not preselect arbitrary replacement GM, stiffness, damping or sheet limits in this PRD. Prefer correcting constraint implementation over retuning coefficients. Choose the smallest smooth GZ representation satisfying the declared constraints; retain one shared `gz`, derivative and integral rather than independent force/energy approximations. A changed representation must expose serializable data to future conformance generation without assuming three coefficients.

## Normative deltas

- D1: F6.7 stability constraints and supported domain, plus any revised F7 defaults: **open**, resolved by 8.1 evidence and recorded rationale.
- D2: F6.9 slack law and geometry-consistent minimum: **open**; explicitly define boundary and parameter validation.
- D3: Baseline/model identity and controlled golden migration: **open**, F18. No blanket exemption from §43 or provenance tests.
- F3 state/controls and RK2 integrator are unchanged. No planing, crew, current, waves or extra actuator DOF.

## Tasks

### 8.1 — Model contracts and reproducible evidence
**P-group: S**
**Owns:** `docs/v2/00-foundations.md`, `docs/v2/physics-validation.md`

Record baseline plots/tables from the actual parameter set: GZ and derivative across `[-pi, pi]`, stationary points and roots; rope path range versus beta; initial tension for every shipped scenario. Include source revision, resolved parameters, dt and commands. Add the exact proposed equation/domain and rationale before its implementation. The new report remains evidence, not a claim of real-ILCA validation.

**Acceptance:** report lists all six scenarios, numerical extrema/root residuals and units; the proposed curve specifies `GZ(0)=0`, `GZ'(0)=GM`, odd symmetry, peak value and zero derivative at the peak, vanishing angle and sign thereafter. If these cannot coexist for a configuration, reject that configuration with a useful error. No approval/date is fabricated.

### 8.2 — Shared physics correction and validation entry points
**P-group: A** (after 8.1's required model decision)
**Owns:** `crates/sailgym-physics/src/stability/hydrostatics.rs`, `crates/sailgym-physics/src/rigging/mainsheet.rs`, `crates/sailgym-physics/src/parameters.rs`, `crates/sailgym-physics/src/scenario.rs`, `crates/sailgym-physics/src/simulation.rs`

Implement the approved curve and unilateral rope law once in their existing modules. Validate construction, scenario loading, parameter editing and reset through the shared validation path. Derive the relevant minimum rope path from configured geometry; validate explicit prestretch only if the chosen contract permits it. Update defaults/scenario initialization only to match the approved contract. A rejected edit must leave the previous simulation intact.

**Acceptance:** targeted module tests cover positive/negative heel, the peak and vanishing boundary, every root in the supported domain, negative/zero/positive extension with both rate signs, finite values at zero flow and invalid geometry. GZ derivative and integral agree with numerical differentiation away from joins with explicit step size and error bound. Slack tension is exactly zero; taut tension never compresses. Failed set/reset/load calls preserve the pre-call state and parameters.

### 8.3 — Regression, convergence and provenance migration
**P-group: B**
**Owns:** `crates/sailgym-physics/tests/invariants.rs`, `crates/sailgym-physics/tests/convergence.rs`, `crates/sailgym-physics/tests/symmetry.rs`, `crates/sailgym-physics/tests/provenance.rs`, `crates/sailgym-physics/tests/regression.rs`, `crates/sailgym-physics/tests/golden/`, `crates/sailgym-bench/src/bin/gen_golden.rs`

Add regressions for the three observed defects. Run sheet transients and stability-boundary trajectories at dt, dt/2 and dt/4; discontinuity crossings get event-aware checks, not a false smooth-order assertion. Quantify energy behavior around slack/taut transitions and preserve force/moment symmetry. Update provenance lookup to recognize explicit v2 overrides without making the original catalogue check vacuous.

Preserve the old outputs and compare their differences before regenerating goldens. Generation normally requires a clean identified physics tree. If generation must precede a commit, narrowly extend the existing dirty override to record the exact source content identity and declared changes; never stash/delete user work or make an unrequested commit. No golden update solely to make the gate pass.

**Acceptance:** focused invariant/convergence/symmetry/provenance/regression commands pass; each original defect fails under the old implementation and passes under the correction. Record tolerances and convergence behavior, including any nonsmooth exception. Before/after plots explain trajectory changes rather than simply replacing expected values.

### 8.4 — Baseline metadata and handoff
**P-group: S**
**Owns:** `crates/sailgym-physics/build.rs`, `crates/sailgym-physics/src/lib.rs`, `docs/v2/progress/08-handoff.md`

Expose a small model-version/source-identity record for 10 and 02 to reuse. Parameters are separate data, not a substitute for implementation identity. Build metadata must refresh when relevant source changes; an unknown/dirty identity is explicit and cannot authorize comparison with a known baseline. Do not write a new cryptographic implementation.

**Acceptance:** changing a physical equation with identical parameters changes the implementation identity (or marks it unknown/dirty); rebuilding unchanged source keeps it stable. Full nine-step gate passes. Handoff names approved deltas, parameter changes/rationale, commands and measured errors, preserved old evidence and the new baseline identifier.

## Section acceptance and risks

Ship only with all task criteria and the existing full gate satisfied; no changed state layout or TypeScript physics. Section 02 must regenerate from this baseline, not v1's known defects.

| Risk | Trigger | Response |
|---|---|---|
| RV49: curve constraints conflict | Fit residual or sign test fails | Reject parameters; revise documented representation, never clamp silently |
| RV50: preload “fix” hides unstable integration | Sheet transient error grows under refinement | Diagnose law/integrator boundary; no damping fudge |
| RV51: golden laundering | Goldens change without before/after report | Stop migration until evidence exists |
| RV52: identity misses source edits | Equation change keeps compatibility identity | Fix invalidation before 10/02 depend on it |

Rollback is a whole identified baseline, including defaults and fixtures; never combine old equations with new goldens. Real-world calibration, planing and post-capsize recovery remain deferred.
