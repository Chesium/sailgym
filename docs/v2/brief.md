# sailgym v2 — scope and milestone brief

**Status: proposed implementation plan, updated 2026-09-20.** The user requested this roadmap and PRD revision, including integration of commit `6358c92`. This records that planning direction; it does not sign off unmeasured physical coefficients, claim any PRD completed, or authorize all later research as one milestone.

## 1. Product direction

An explainable sailing playground: learn the controls, try a short task, inspect what happened, retry under the same conditions. Retain the reduced physical model and SVG boat renderer. Borrow Vibesail's clear onboarding, goals and replay affordances, without adopting photorealism or a full 3-D world.

The next milestone is sections **08 → 09 → 10 → 11**. Existing sections 02–07 remain detailed follow-on proposals with stable IDs. Conformance fixtures should describe the corrected model, so 02 follows 08 and reuses 10's identity. The first JAX scope is wind sampling only.

## 2. Separable scope proposals

| ID | Scope | Placement and limit |
|---|---|---|
| S1 | Conformance bundle and measured native throughput | Follow-on 02; generated from identified corrected source, not physical certification |
| S2 | JAX wind verification pilot | Follow-on 03; no full dynamics/integrator port or training claim |
| S3 | Courses and controller interface | Later 04–05; one rule sailor before polar racing or planning; web integration separately scoped |
| S4 | RL environment and Python binding | Later 06–07; infrastructure only, no training programme |
| S5 | Multiple non-interacting boats | Later optional product/research slice; independent VecEnv episodes do not require live fleets; a recorded ghost requires neither |
| S6 | Obstacles, raycasting, collision-as-termination | Deferred. 4.5–4.6 are optional designs, absent from default delivery |
| S7 | Basic mobile controls and readable layout | M-next 09; rate input, gauges, explicit release; no hand-force or hiking model |
| S8 | Reduced-model physics consistency | M-next 08; GZ constraints, sheet geometry/preload and slack law; no extra DOF or cosmetic tuning |
| S9 | Truthful replay and guided practice | M-next 10–11; sampled inspection, three short challenges, two same-conditions attempts |

## 3. Model and architecture boundaries

Rust remains authoritative for physical dynamics, controls, task evaluation and recording codecs. React handles input and presentation. No new game engine, service, account system or generic plugin system is needed. Reuse `Simulation`, the existing scenarios, recorder, timeline, input reducers and SVG camera.

The v1 baseline is historical evidence, not a requirement to preserve a discovered defect. Changes to F6/F7 must be explicit v2 deltas with before/after plots, mechanical tests and a parameter rationale under v1 §43. Do not tune parameters until a tutorial passes. `STATE_LEN = 13` remains unchanged. Defaults and model identity must travel with episodes; old recordings remain inspectable without being silently upgraded into comparable experiments.

The Python binding cannot implement physics or duplicate Rust layouts. A separately identified JAX verification implementation necessarily contains equations; that narrow exception is specified in F17.1 and is not permission for Python wrappers to invent physics.

## 4. Costs and exclusions

M-next requires targeted core corrections, recording migration, touch UI and a small task evaluator. It does not require the four proposed research crates or a Python toolchain. Sections 02–07 add those costs only when selected. Native single-core performance is measured in v1; multi-core scaling and end-to-end environment throughput remain to be measured. Independent implementation agreement detects port discrepancies; it cannot establish real-boat accuracy or rule out a shared modeling mistake.

[Deferred features](discussions/deferred-features.md) is the complete disposition of v1 §44 and the later ideas raised by the discussions. Natural heel recovery after easing the sheet is distinct from physically righting a capsized dinghy.

## 5. Implementation decision record

Before implementing a changed normative contract, record the selected S rows, exact F deltas, decision source/date and required validation in the section handoff or this table. Drafting this document does not fill the table. Existing section-01 approvals remain as recorded in its handoff.

| Slice | Decision | Evidence |
|---|---|---|
| 08–11 model/API/task deltas | Proposed; review against the concrete PRDs | No runtime validation performed by this planning revision |
| 02–03 conformance/Python gate deltas | Proposed after M-next | No bundle or JAX result claimed |
| 04–07 course/agent/env/Python scope | Later proposal | S6 obstacles and live fleet scope excluded by default |
