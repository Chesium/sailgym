# v2 Section 09 — Touch controls and a readable sailing view

**Milestone:** M-next.2. **Status:** proposed, S7/F18 UI boundary deltas open.
**Read first:** v1 F3/F4/F8/F13, v2 brief, [mobile discussion](../discussions/mobile-controls.md), 08 handoff.
**Dependency:** 08 for final parameter limits. UI work may prototype earlier but acceptance uses its baseline.

## Goal and user flow

A new user can steer, trim and release the sheet on a phone without opening the debug panel. They see what the boat is doing and can operate sheet and rudder simultaneously. Keep desktop keyboard/mouse behavior and the shipped SVG renderer.

Default to **rate control**: relative drag from a neutral grab produces a bounded rudder/sheet rate. Lifting stops sheet input and returns rudder behavior to the core's existing self-centering law. Show actual rudder angle and sheet length separately from rate-command feedback; different units are not a position error. Use a large, labeled hold-to-release button, not a flick gesture.

## Normative deltas and scope

D1: mobile browser input and targeted touch E2E coverage, S7: proposed. D2: additive read-only render parameter fields if necessary, F8.2: proposed; return core limits/rates instead of copying constants into JS. No physical rate caps, tension/slip law or angle controller in React. Position tracking is deferred until a Rust adapter defines engaged versus released semantics; zero rudder command currently means self-centre, not hold.

## Tasks

### 9.1 — One input-composition path
**P-group: S**
**Owns:** `web/src/sim/controls.ts`, `web/src/sim/sheetInput.ts`, `web/src/sim/useSimulation.ts`, `web/src/sim/loadWasm.ts`, `web/tests/unit/controls.test.ts`, `web/tests/unit/sheetInput.test.ts`

Reuse the pure reducers. Compose keyboard, mouse and touch into one Controls value at the simulation application boundary instead of independent writers. Release overrides sheet rate. For steering/trim, active touch owns its channel, otherwise mouse drag owns sheet, otherwise keyboard applies. Dropping ownership cannot resurrect a stale held gesture. Blur, hidden page, pause, reset, scenario switch and replay entry clear transient input and release latches.

**Acceptance:** reducer tests prove source priority, clamp/deadzone signs, release precedence and every clear path. Equal normalized commands yield equal Rust trajectories regardless of input device. Neither key handlers nor pointer handlers directly advance physics. Existing keyboard/mouse tests remain green.

### 9.2 — Read-only actuator metadata
**P-group: A**
**Owns:** `crates/sailgym-wasm/src/lib.rs`, `web/tests/unit/parameterSchema.test.ts`

Inspect existing exported parameter types before adding fields. Task 9.1 owns RenderParams in useSimulation.ts and adds the matching fields. Expose only missing limits/rates needed by gauges; reuse sheet fields already available. Keep limits authoritative in BoatParameters. Match existing TS/WASM parity checks and keep all conversions in the existing boundary.

**Acceptance:** live parameter edits update both gauges and computed full-travel reference times; no `0.9`, `4.5`, `2.4 s` or `0.6 s` assumptions in touch logic. Check exact existing type paths before execution; record a corrected ownership path if the declaration lives elsewhere, never create a parallel type merely to fit this list.

### 9.3 — Two-pointer controls and release
**P-group: A**
**Owns:** `web/src/ui/TouchControls.tsx`, `web/src/render/BoatSvg.tsx`, `web/tests/unit/touchInput.test.ts`, `web/src/sim/touchInput.ts`

Use a small pure touch reducer, reusing sheetInput behavior; component interactions are covered by 9.5 browser tests without adding a component-test framework. Each pad captures and tracks its own pointerId. A second pointer on the same pad is ignored, while the other pad remains independently usable. Handle pointerup, pointercancel, lostpointercapture and unmount. Relative grab begins neutral with no jump. Prevent camera/boat gestures only while a control owns that pointer. Do not make lift equivalent to sheet dump.

Provide names, focus indication and keyboard access; native buttons for release/reset where practical. Touch targets are at least 44 by 44 CSS pixels. Release needs continuous deliberate contact; cancellation clears it within the next input application.

**Acceptance:** synthetic pointer tests cover simultaneous steering/trim, unrelated pointerup, outside-pad lift, cancel and lost capture; each leaves the expected other channel unchanged. Gauge values are actual state, commands are explicitly labeled rate inputs. No hand/ratchet simulation or required vibration API.

### 9.4 — Responsive presentation and sail-mode defaults
**P-group: B**
**Owns:** `web/src/App.tsx`, `web/src/ui/Layout.tsx`, `web/src/ui/store.ts`, `web/src/wind/particles.ts`, `web/src/wind/WindLayer.tsx`

Replace the fixed 780×520 view assumption with measured available space and existing camera transforms. Respect safe areas, portrait/landscape changes and browser chrome resize. Preserve a deliberately selected/persisted debug mode; new users start in sail mode. Reduce visual wind density/contrast in sail mode so hull, boom, heading and wind direction remain legible. These are presentation controls, never physical wind edits. Do not add a rendering engine.

**Acceptance:** at 360×640, 390×844, 844×390 and 1280×800 CSS pixels, controls and primary feedback remain inside the viewport with no horizontal document overflow. Resize keeps the boat visible and pointer coordinates consistent. Scene visual review includes high heel and both tacks. Debug access remains available without occupying the default sailing area.

### 9.5 — Browser verification and handoff
**P-group: S**
**Owns:** `web/playwright.config.ts`, `web/tests/e2e/mobile-controls.spec.ts`, `docs/v2/progress/09-handoff.md`

Add a targeted touch-emulation project/testMatch using the installed browser setup; do not multiply all desktop suites unnecessarily. Test full-rate travel against `(max-min)/rate` from the current parameters, allowing at most two physics ticks after a directly applied command; separately measure input-to-render latency rather than folding it into the physics tolerance. P-controller settling and hand-stroke times are not rate-travel times.

**Acceptance:** desktop and targeted touch tests plus full gate pass. Touch suite covers dual input, cancellation, release, reset, rotation and viewport bounds. Record at least one real-device session if available (device/browser/version); emulation alone must be labeled as such, with hardware coverage outstanding. Handoff records visual settings, measured latency and any mobile browser limitation.

## Risks and release limits

| Risk | Trigger | Mitigation |
|---|---|---|
| RV53: stuck touch command | Navigation/cancel leaves nonzero source | Central clear path and dual-pointer tests |
| RV54: apparent control lag misread as physics | Command and actual position share an unlabeled scale | Separate command-rate feedback and actual-state gauges |
| RV55: mobile UI hides sailing | Pads/debug/wind obscure boat | Viewport assertions and real visual review |
| RV56: second physics implementation | UI scales rates by tension or changes limits | Only normalized commands and core metadata at the boundary |

Advanced position targets, regripping, flicks, haptics and tilt remain in the deferred note. No new tuning is justified merely by making a touch demo look better.
