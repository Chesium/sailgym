# v2 Section 11 — Guided practice: try, inspect, retry

**Milestone:** M-next.4 and release gate. **Status:** proposed, S9/task delta open.
**Read first:** v2 brief and F18, 09 and 10 handoffs, [learning-loop discussion](../discussions/learning-loop.md).
**Dependencies:** 08 corrected baseline, 09 usable controls, 10 trustworthy recordings.

## Goal and experience

Give the simulation a useful one-minute loop: choose a goal, sail, see the result, inspect the relevant moment, retry under the same conditions. Ship exactly three practice challenges from the existing scenarios, plus free sail. No account, leaderboard, course editor, agent framework or Python is required.

| Challenge | Teaching goal | Measured success / feedback |
|---|---|---|
| Get moving | Steer out of irons and trim for drive | Sustain task-defined forward speed for a fixed duration; show time and speed trace |
| Complete a tack | Cross head-to-wind and establish the opposite tack | Ordered approach/crossing/opposite-tack-settled events, with forward-speed recovery; reject a gybe or repeated angle jitter |
| Recover from excessive heel | Release/ease the sheet early and regain control | Reduce absolute heel below recovery threshold for a duration before capsize is declared; show peak heel and release timing |

Threshold values are **task configuration**, versioned and visible, not physical constants. Determine the first values by scripted runs of 08's baseline; record the expected traces and rationale. Start heel recovery below the capsize condition; do not imply physical righting of an overturned boat. The existing capsize/recovery scenarios share setup, so distinguish these tasks through explicit instructions and evaluation rather than claiming different dynamics.

## Normative delta

D1: a deterministic task evaluator and additive WASM/episode task metadata: proposed, F18. Keep scoring outside the physics crate. Use a small `sailgym-task` crate only for shared pure evaluation; `physics` has no dependency on it. Future `course`/`env` consume the same task outcomes instead of defining conflicting completion semantics. This is a deliberate bounded crate, not a generic task/plugin registry.

## Tasks

### 11.1 — Three task specifications and evaluator
**P-group: S**
**Owns:** `crates/sailgym-task/Cargo.toml`, `crates/sailgym-task/src/lib.rs`, `crates/sailgym-task/tests/practice.rs`, `Cargo.toml`, `Cargo.lock`, `docs/v2/practice-validation.md`

Define task ID/version/config, deterministic runtime state and `Running`, `Succeeded`, `Failed(reason)`, `TimedOut` outcomes. Use physics-step indices/time, not render frames. Validate finite thresholds, positive durations and valid ordering; freeze them for the attempt. Consume state/controls/wind/capsize observation without changing the physical state. The tack detector uses relative wind direction and hysteresis with an ordered transition, not a raw heading sign flip.

**Acceptance:** table-driven traces demonstrate each success and each isolated failure: insufficient hold duration, backward drift, wrong-way gybe, repeated boundary jitter, late release, capsize and time expiry. Identical state/control sequences produce identical outcome/event steps under six batching sizes. `cargo tree -p sailgym-physics` does not include task/UI crates. Report chosen thresholds with successful and unsuccessful scripted baseline runs; no arbitrary pass claim from UI screenshots.

### 11.2 — Runtime and recording integration
**P-group: A**
**Owns:** `crates/sailgym-wasm/Cargo.toml`, `crates/sailgym-physics/src/recording.rs`, `crates/sailgym-wasm/src/lib.rs`, `web/src/sim/useSimulation.ts`, `web/src/sim/loadWasm.ts`, `web/src/sim/scenarioTypes.ts`

Evaluate tasks after every physical step in the existing WASM advance path, including while recording, without adding a second physics clock. Add a versioned optional task metadata/events envelope to the recording schema via the existing typed envelope/API from 10; adapt recording serialization only if necessary, without adding a task-crate dependency to physics; if 10 did not provide an envelope, record a new explicit schema delta before modifying it. Rust owns events and results. Free sail remains available with no task.

Retry restores the exact resolved initial state, controls, parameters, wind/seed and task configuration; it clears all transient input, timers, recorder state and success flags. It must not use a reset path that silently retains edited parameters. Parameter/scenario changes during an attempt end it as “conditions changed” and cannot produce a comparable result.

**Acceptance:** recorded event times agree with task runtime outcomes; task-on/task-off physics is bit-identical for the same controls. Retry yields the same initial snapshot and identity. Pause consumes no task time; replay cannot emit success events. Attempt cancellation/reset cannot retain a pressed release button.

### 11.3 — Prompt, result and two-attempt comparison
**P-group: B**
**Owns:** `web/src/ui/PracticePanel.tsx`, `web/src/App.tsx`, `web/src/ui/Timeline.tsx`, `web/src/ui/Charts.tsx`, `web/src/ui/store.ts`

Show one short instruction and measurable goal before starting; minimal progress during sailing; result with one useful metric and Inspect/Retry actions. Inspect jumps to the relevant recorded event (tack crossing, peak heel or release). Explain failure with the observed event, not an unsupported causal diagnosis.

Retain at most the two most recent attempts in session memory. Compare only when 10's structured identities match apart from attempt ID/time/actions/outcome. Show elapsed task time on a common axis, the same metric units and measured deltas. Unknown or differing identities remain separately inspectable with a plain explanation. Export uses the existing recorder; no new persistence service.

**Acceptance:** no lesson text claims real-boat certification; all displayed metrics derive from recorded samples or evaluator events. Comparison rejects differing seed, parameter, model, task version and dt. Result text and controls are accessible by keyboard and at 360×640. Free sail and debugging remain reachable.

### 11.4 — User-flow verification and gate integration
**P-group: S**
**Owns:** `web/tests/e2e/practice.spec.ts`, `scripts/check.sh`, `scripts/check.ps1`, `CLAUDE.md`, `docs/v2/00-foundations.md`, `docs/v2/progress/11-handoff.md`

Extend gate step 3 to include the task crate; keep nine steps before Python lands. Update all current gate descriptions consistently with the recorded delta. Verify goal → attempt → result → inspect → retry → compare, with both keyboard and touch input. Use scripted outcomes to check scoring and a brief real play session to assess discoverability; neither substitutes for the other.

**Acceptance:** full gate passes. At least one successful and one unsuccessful run for each challenge is retained as a test/evidence fixture with seed and task config. A user can start, inspect and retry without opening Debug or reading a separate document. Report the observed usability session rather than claiming an unmeasured completion-rate target. M-next release handoff lists 08–11 criteria, schema compatibility and hardware test coverage.

## Risks and follow-on

| Risk | Trigger | Mitigation |
|---|---|---|
| RV61: tutorial tunes physics | Coefficients changed to make success easier | Tune/version task thresholds only, preserve 08 baseline |
| RV62: score depends on browser FPS | Same commands yield different task time | Physics-step evaluator and chunking tests |
| RV63: retry changes conditions | Live edits survive retry | Restore exact recorded initial contract |
| RV64: misleading recovery lesson | Success after true capsize without righting model | Fail on capsize, describe excessive-heel recovery accurately |

After this release, build a recorded ghost and one short course with a rule sailor. Sections 04–07 are research infrastructure, not a substitute for that product integration. Full training, live fleets, polar planning and curricula stay deferred until this loop is useful.
