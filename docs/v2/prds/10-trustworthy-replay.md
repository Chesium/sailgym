# v2 Section 10 — Truthful replay and experiment identity

**Milestone:** M-next.3. **Status:** proposed, S9/F18 recording deltas open.
**Read first:** v1 section 09, F8/F9/F13, v2 foundations; 08 handoff.
**Dependencies:** 08 baseline identity; integrate after 09 to avoid shared UI ownership conflicts.

## Goal

When replaying an episode, every visible quantity belongs to that episode and time. Today App selects a recorded pose while HUD, wind, force overlay, charts and debug diagnostics can still use the live simulator. Fix this at the shared display selection boundary, not by patching one panel.

This is **sampled inspection**, not deterministic action resimulation. Interpolated poses are useful presentation; thresholds, discrete events and scores use recorded samples/events. Future decision logs are a separate artifact and need not block this milestone.

## Normative deltas

D1: versioned recording schema and additive inspection metadata, F8.4: proposed. D2: compatibility identity F18: proposed. D3: recorded diagnostics / replay source selection: proposed. Preserve v1 import with unavailable fields represented explicitly; never silently fill missing measurements with zero or recompute them using current physics.

## Tasks

### 10.1 — Recording schema and migration contract
**P-group: S**
**Owns:** `crates/sailgym-physics/src/recording.rs`, `web/src/sim/scenarioTypes.ts`, `docs/v2/recording-format.md`

Extend the existing Episode/Header/Frame instead of creating a competing recorder. Define a schema version after inspecting the actual current version. Document JSON and binary layouts, header offsets, frame widths, optional fields and migration behavior. Record the diagnostic subset needed for replay HUD, force vectors/arms/moments, charts and capsize status. Fields not retained must be unavailable in replay, not supplied live. Reserve a typed, optional, versioned practice envelope for task config and ordered events (IDs, physics-step indices, values), populated by 11 through the existing recorder API. This is a concrete dependent feature, not a general extension registry. Keep schema-1 decoding explicitly supported; refuse unknown future versions with an actionable message.

Identity is a canonical structured record: implementation/model version and source identity; resolved parameters; integrator/dt; full initial state/controls; scenario, wind configuration and seed; task ID/version/thresholds; action adapter/version/cadence; observation layout/units/normalization/noise/privilege where relevant. Browser manual episodes may mark research-only fields not applicable. Unknown is distinct from not applicable. Toolchain and recording cadence remain metadata. Stable equality of canonical records is enough; optional hashes must cover these records, not just parameter values.

**Acceptance:** JSON/binary round trips cover new and checked-in legacy fixtures; all fields retain units/meaning. Bad magic, truncation, invalid lengths/nonfinite values and unsupported version are rejected without altering the loaded episode. A changed equation, seed, dt, initial condition or task threshold makes strict comparison incompatible. An unknown legacy identity can be viewed but cannot be labeled a same-conditions experiment.

### 10.2 — Capture at sample time through the existing boundary
**P-group: A**
**Owns:** `crates/sailgym-wasm/src/lib.rs`, `web/src/sim/episodeIo.ts`, `web/src/sim/loadWasm.ts`, `web/src/sim/diagnostics.ts`

Retain Rust as codec owner. Capture diagnostics for the same state/time as each recorded frame; reuse the existing due-sample cadence rather than evaluating all diagnostics at every render. Record exactly resolved defaults/overrides, not merely a scenario name. Document memory growth and normal stop/export behavior; impose a visible bounded recording duration/size before allocation can exhaust the browser.

**Acceptance:** selected captured fields equal live diagnostics at their recorded sample times on the same build. Recording on/off leaves the physical trajectory bit-identical. TS/Rust schema parity holds and invalid imports fail visibly. A maximum-duration recording stays within the documented size budget, established from bytes/frame × samples rather than an unexplained threshold.

### 10.3 — One live-or-replay display selection
**P-group: B**
**Owns:** `web/src/sim/replay.ts`, `web/src/App.tsx`, `web/src/sim/useSimulation.ts`, `web/src/ui/Hud.tsx`, `web/src/ui/DebugPanel.tsx`, `web/src/render/ForceOverlay.tsx`, `web/src/wind/WindLayer.tsx`, `web/src/wind/useWindField.ts`, `web/src/wind/ArrowOverlay.tsx`, `web/src/ui/WindReadout.tsx`

Select an inspection frame once and pass it to all consumers. On replay entry, neutralize input and pause live advancement; on exit remain paused until the user explicitly resumes. Recorded pose, controls, wind-at-boat, forces and diagnostics must never mix with live data. Spatial wind visualization is hidden unless the recording contains enough version-matched data to reconstruct that field; the recorded local vector alone does not identify the entire field.

Interpolate position and wrapped heading/boom with existing conventions; heel remains unwrapped. Discrete controls/status use the preceding sample. Diagnostics use the preceding recorded sample and expose its timestamp; do not present interpolation as an exact force calculation. Scrubbing before/after endpoints clamps predictably. Missing fields render “Not recorded”.

**Acceptance:** changing the paused live simulation's parameters/scenario cannot alter a replay view. Tests scrub forward/backward, across angular wrap and capsize events, and into a legacy episode. Every consumer uses recorded or explicitly unavailable data. No force evaluation using current model parameters is triggered to fill a legacy gap.

### 10.4 — Charts follow recorded time
**P-group: B**
**Owns:** `web/src/ui/Charts.tsx`, `web/src/ui/Timeline.tsx`, `web/tests/unit/replay.test.ts`

Build replay plots from saved samples bounded by the selected time. Repeated render or backward scrub must not append wall-clock samples, duplicate history or carry live chart buffers into playback. Display sampling resolution, and preserve the existing live chart path. Empty and one-frame episodes have defined disabled/constant timelines.

**Acceptance:** playing twice or scrubbing arbitrarily produces identical chart points for the same playhead. Tests assert timestamp ordering, point count, wrap semantics, held discrete values and legacy availability. No score is calculated from interpolated frames.

### 10.5 — End-to-end truth test and handoff
**P-group: S**
**Owns:** `web/tests/e2e/replay-truth.spec.ts`, `docs/v2/progress/10-handoff.md`

Create an episode, alter the live run conspicuously, then replay and inspect pose, HUD, vector overlays, plots and debug values against the saved episode. Exercise export/import in both codecs, reset/replay transitions and malformed import.

**Acceptance:** full gate passes; all display consumers are listed with recorded source or unavailable policy. Handoff records schema migration, fixture versions, exact identity fields, recording size and any intentionally omitted diagnostic. Existing schema-1 files remain viewable.

## Risks

| Risk | Trigger | Mitigation |
|---|---|---|
| RV57: mixed timelines | Replay changes when live parameters change | Single selection boundary and adversarial E2E |
| RV58: false comparability | Parameter-only digest accepts different model/task | Full structured identity, unknown blocks strict comparison |
| RV59: replay becomes re-simulation | Legacy diagnostics derived from current core | Explicit unavailable fields and retained original samples |
| RV60: migration loses recordings | Schema bump rejects all old files | Legacy fixture decoding and untouched source files on error |

Future section 06 must extend this schema deliberately, not blindly bump 1→2 or create a second Episode type in physics. Complete action re-simulation and general session storage are deferred.
