# v2 Section 09 — Handoff (touch controls and a readable sailing view)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F18.2 and
`docs/v2/progress/08-handoff.md` before starting section 10.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. `crates/` is untouched — `git diff --name-only crates/`
is empty — so F3, F4, F5, F7, F8 and F9 are exactly as section 08 left them,
`STATE_LEN` is still 13, the snapshot layout is unchanged and no physics moved
to TypeScript.

Status: **complete**; `scripts/check.sh` is green end to end (§10). One
pre-existing failure was found at the section's starting revision and is
recorded separately (§8.1); one defect this section introduced was caught by
the gate and fixed, and §6 RV55 (2) states it in full. Seven things deviated
from the PRD as written and §7 states each one.

---

## 1. What the section is, in one paragraph

Every device — keyboard, mouse and two new touch pads — now becomes **one**
`Controls` value at **one** call site, and the page is laid out from a measured
viewport instead of a fixed 780 × 520 rectangle. The pads send normalised rate
commands and nothing else: no rate cap, no tension-dependent slip, no angle
controller, no hand model, no ratchet, no vibration. The metres per second and
radians per second those commands become are still `parameters.rs`'s, applied
in `dynamics.rs`, and a browser test measures that end to end — a full-scale
drag on the sheet pad runs the rope from stop to stop in **2.3064 s** against a
predicted `(l_sheet_max − l_sheet_min) / sheet_haul_rate` of **2.3064 s**.

---

## 2. What landed, file by file

### Task 9.1 — one input-composition path (P-group S, section agent)

- **`web/src/sim/controls.ts`** — `InputSources`, `TouchCommand`,
  `IDLE_SOURCES`, `IDLE_TOUCH`, `clearTransient()` and **`composeControls()`**,
  the one composition. `InputConfig` gained `touchRudderGain` and
  `touchSheetGain`, both defined as `1 / TOUCH_FULL_SCALE_PX` with their
  provenance at the constant. `controlsFromInput` is unchanged in behaviour and
  is now the keyboard half of the composition.
- **`web/src/sim/sheetInput.ts`** — `SheetEvent` gained `pointerType`, and a
  `touch` pointer on the world view now produces **no** sheet command (trimming
  by touch belongs to the pad). `ownsSheet(state)` distinguishes "the mouse is
  driving the sheet and asking for zero" from "the mouse is not driving it",
  which a bare `0` cannot. `normalisedDrag(pixels, gain, invert)` is the one
  pixels → command rule, now shared with the touch pad, and `rateFor` delegates
  to it.
- **`web/src/sim/useSimulation.ts`** — `applyControls()` is the **only**
  `sim.set_controls` call site in the application, and `clearInput()` the only
  clear path. The handle gained `setTouchCommand`, `clearInput`,
  `inputGeneration`, and `setSheetRate` now takes `number | null`.
  `RenderParams` gained the six actuator limits and rates (see task 9.2 below). The
  frame loop re-reads `parameters_json()` every `PARAMS_POLL_MS = 500`, and
  re-parses only when the text changed.
- **`web/tests/unit/controls.test.ts`** — 7 → **26 tests**, in five new
  `describe` blocks: source priority, clamps and dead-zone signs, release
  precedence, the clear path, and the single application boundary. The seven v1
  keyboard tests are **byte identical**.
- **`web/tests/unit/sheetInput.test.ts`** — 8 → **11 tests**: ownership, the
  touch-pointer exclusion and the shared drag rule. The eight v1 mouse tests
  are byte identical.

`web/src/sim/loadWasm.ts` is in the task's `Owns:` list and **needed no
change**: the `Sim` surface it re-exports is generated from the Rust
declarations and nothing was added to it (§2.2).

### Task 9.2 — read-only actuator metadata (P-group A, section agent)

**Nothing was missing, so no Rust changed.** `crates/sailgym-wasm/src/lib.rs`
is in the task's `Owns:` list and is byte identical. `Sim::parameters_json()`
serialises the whole `BoatParameters` catalogue in one coarse-grained call
(brief §24), so `rudder.delta_r_max`, `rudder.delta_r_rate_max`,
`rudder.delta_r_return_rate`, `sheet.sheet_haul_rate`, `sheet.sheet_ease_rate`
and `sheet.sheet_release_rate` already crossed the boundary — the same finding
section 01 made for the six geometry fields it needed
(`progress/01-handoff.md` §1, task 1.5).

The PRD anticipated this: "Task 9.1 owns RenderParams in useSimulation.ts and
adds the matching fields… never create a parallel type merely to fit this
list." That is what happened. **The corrected ownership path is
`web/src/sim/useSimulation.ts`**, task 9.1's, and the limits stay authoritative
in `BoatParameters`.

- **`web/tests/unit/parameterSchema.test.ts`** — 8 → **14 tests**: every gauge
  path is a leaf of a `parameters_json()` document by the *same* walk the
  parameter panel uses; the WASM surface still serialises the whole catalogue
  and grew no per-field accessor; the reference times are `(max − min) / rate`;
  a live edit moves every one of them; no catalogue value or superseded target
  time appears in the touch logic; and the panel still has no hand-written
  field list.

### Task 9.3 — two-pointer controls and release (P-group A, section agent)

- **`web/src/sim/touchInput.ts`** — new, and pure. `reduceTouchInput` and
  `touchCommand`; one pointer per pad, a second pointer on the same pad ignored,
  only the owning pointer able to move or end a grab, and `up` / `cancel` /
  `lostcapture` all ending one. The sheet pad reuses `sheetInput`'s
  `normalisedDrag`, so the pad and the mouse cannot disagree about which way is
  "in".
- **`web/src/ui/TouchControls.tsx`** — new. Two pads, a hold-to-release button
  and a Reset button; the boat's **actual** rudder angle and sheet length on
  their own rows in their own units, the **command** on separate rows labelled
  as rates, and the five full-travel reference times. Exports `fullTravel()`.
  `role="slider"` with `aria-valuenow`/`aria-valuetext` on each pad,
  `aria-label` and `aria-keyshortcuts` on all four controls, a `:focus-visible`
  outline, native `<button>`s for release and reset, and a 44 × 44 CSS-pixel
  floor with the pads at 132 px (92 px under 760 px of viewport height, 59 px
  under 460 px).
- **`web/src/render/BoatSvg.tsx`** — the world view now captures the pointer and
  suppresses the browser's gesture **only** when it owns one (a pan, or a
  mouse/pen trim), forwards `pointerType` to the sheet reducer, and handles
  `lostpointercapture` as a cancel.
- **`web/tests/unit/touchInput.test.ts`** — new, **30 tests** across five
  groups, including `it.each` over `up` / `cancel` / `lostcapture` on both pads
  and the source greps that keep a hand model, a ratchet, a cleat, a tension
  term or a vibration call out of both files.

### Task 9.4 — responsive presentation and sail-mode defaults (P-group B, section agent)

- **`web/src/ui/Layout.tsx`** — rewritten as a fixed-height grid
  (`100svh`, rows `auto auto minmax(0,1fr) auto`) with the **main** row as the
  only scroll container, safe-area padding on all four sides, capped header and
  readout strips in compact mode, and a two-column arrangement for a short wide
  viewport. Gained a `controls` slot, `compact`, `modeChosen`, `dragging`,
  `mainRef` and `worldRef`. `dragging` stops the main row scrolling while the
  world view owns a pointer — §6 RV55 (2) is why.
- **`web/src/App.tsx`** — `useWorldViewport()` (a `ResizeObserver` on the main
  row's height and the world cell's width — loop-free by construction, see §4)
  and `useCompactLayout()`. The fixed `VIEWPORT` constant is gone; the camera,
  the SVG and the deck.gl surface all take the measured one. Mounts
  `TouchControls`, clears input on entering replay, passes the mouse sheet
  channel as `null` when no drag owns it, and tracks whether the world view
  owns a drag so the layout can stop scrolling under it (§6 RV55 (2)).
- **`web/src/ui/store.ts`** — `modeChosen`, persisted, schema **2** with a
  migration that recovers it exactly from a schema-1 state.
- **`web/src/wind/particles.ts`** — `WindVisual`, `SAIL_MODE_WIND`,
  `DEBUG_MODE_WIND`, `visibleCount()`. Presentation only; the whole population
  is still advected through the same field at the same speed.
- **`web/src/wind/WindLayer.tsx`** — `particleLayers(input, visual)` draws a
  uniform random sample of the population at a faded alpha and a proportionally
  thinner trail, with a `MIN_CONTRAST` floor. The **layer** count is unchanged,
  which is what `wind.spec.ts` asserts.

### Task 9.5 — browser verification and handoff (P-group S, section agent)

- **`web/playwright.config.ts`** — a fourth project, `mobile-chromium`
  (`devices['Pixel 5']`), running **only** `mobile-controls.spec.ts`; the three
  desktop projects `testIgnore` it, so nothing runs twice and the desktop suite
  is not multiplied.
- **`web/tests/e2e/mobile-controls.spec.ts`** — new, **19 tests**. Multi-touch
  over CDP, so the page gets trusted `pointerType: 'touch'` events with real
  pointer capture.
- **`docs/v2/progress/09-handoff.md`** — this file.

---

## 3. The measured results

All from the `mobile-chromium` project on this host, Chromium via Playwright
1.56, `devices['Pixel 5']` (393 × 851 CSS px, DPR 2.75).

### 3.1 Full-rate travel — the section's headline number

The rate is measured by a least-squares fit over the frames where the actuator
is off both stops and the command is constant, where F4.3's integration is
exactly linear; the travel is then the fit extrapolated to both stops, rather
than the frame a crossing happened to land in (a frame is ~16 ms, two physics
ticks are 10 ms).

| Quantity | Measured | Catalogue / predicted | Bound |
|---|---|---|---|
| sheet haul rate | **1.500000 m/s** (117 frames) | `sheet_haul_rate` 1.5 | 0.1 % |
| full haul travel | **2.3064 s** | `(max − min)/rate` **2.3064 s** | 2·dt = 0.0100 s |
| sheet release rate | **6.000000 m/s** (19 frames) | `sheet_release_rate` 6.0 | 0.1 % |
| rudder rate | **2.090000 rad/s** (34 frames) | `delta_r_rate_max` 2.09 | 0.1 % |
| rudder stop to stop | **0.6679 s** | `2·δr_max/rate` **0.6679 s** | 2·dt = 0.0100 s |

The command is also shown to be **in force within two physics ticks** of being
applied: the fit's extrapolated departure time lies between the last frame that
saw no command and the first that saw one, each widened by `2·dt`.

The five reference times the gauges quote, at the shipped catalogue:

```
helm      0.6679 s stop to stop   (0.3340 s centre to stop)
sheet     2.3064 s in             (l_sheet_max − l_sheet_min) / sheet_haul_rate
          1.1532 s out            … / sheet_ease_rate
          0.5766 s released       … / sheet_release_rate
```

**The historical 2.4 s haul and 0.6 s release figures are not targets and were
not used.** They are close to the computed values by coincidence of the
catalogue; the numbers above are derived live from `parameters_json()` and move
when a parameter does — measured, halving `sheet_haul_rate` to 0.75 m/s takes
the haul time to 4.613 s on the page.

### 3.2 Input-to-render latency — measured separately, as the PRD requires

`render/perfMarks.ts`'s existing instrument, re-keyed in task 9.1 onto the
**composed** helm command rather than onto a key-down, so a touch pad is
measured through exactly the pipeline a key press is.

| | worst of the run |
|---|---|
| touch pad → rendered rudder | **7.8 ms** (budget 50 ms) |
| keyboard → rendered rudder (`perf.spec.ts`, unchanged) | p50 10.4 ms, p95 20.7 ms |

This is **wall** time and is deliberately not added to the `2·dt` physics
budget above: folding the two together is how a slow frame comes to look like a
physics error (RV54).

### 3.3 The viewport, at the four sizes the PRD names

Every one: no horizontal document overflow, **no vertical overflow either**
(the root is exactly one viewport tall and the main row scrolls), every control
and every gauge inside the viewport, every touch target ≥ 44 × 44.

| Viewport | World view | Document | Arrangement |
|---|---|---|---|
| 360 × 640 | 344 × 202 | 360 × 640 | compact, stacked |
| 390 × 844 | 374 × 354 | 390 × 844 | compact, stacked |
| 844 × 390 | 476 × 246 | 844 × 390 | compact, **two-column** |
| 1280 × 800 | 1264 × 363 | 1280 × 800 | full, stacked |

Rotation 390 × 844 → 844 × 390 → 390 × 844 keeps the SVG's own `width`/`height`
equal to its CSS box and the camera's emitted centre equal to half of them, so
one CSS pixel remains one camera coordinate and a tap lands where it is aimed.
A touch drag on the helm pad still moves the rudder after the rotation.

### 3.4 Visual settings, and their provenance

Every one is a count of screen pixels or a dimensionless fraction. **None
reaches the physics core**, and none was chosen to make a demonstration look
better (brief §43, and the PRD's extension of it to visual constants).

| Constant | Value | Where | Why |
|---|---|---|---|
| `TOUCH_FULL_SCALE_PX.rudder` | 64 px | `sim/controls.ts` | ≈ 1.5 × the 44 px target minimum: past finger tremor, inside a thumb's arc at 360 px |
| `TOUCH_FULL_SCALE_PX.sheet` | 96 px | `sim/controls.ts` | ≈ 2.2 × 44 px; trimming is sustained, and the pads are taller than they are wide |
| `TARGET_MIN_PX` | 44 px | `ui/TouchControls.tsx` | the PRD's rule; WCAG 2.2 SC 2.5.5 |
| `PAD_MIN_PX` | 132 px | `ui/TouchControls.tsx` | 2 × 64 px of travel about a centred grab, rounded up; 92 px under 760 px of height, 59 px under 460 px |
| `SAIL_MODE_WIND` | density 0.50, contrast 0.55 | `wind/particles.ts` | halves the trail count and the alpha so the hull's 0.8 px stroke and the boom read in front of the field (RV55); 0.50 keeps the drawn count at 2000, which is still the "thousands" `wind.spec.ts` asserts |
| `DEBUG_MODE_WIND` | density 1, contrast 1 | `wind/particles.ts` | Debug Mode exists to be read |
| `MIN_CONTRAST` | 1/3 | `wind/WindLayer.tsx` | a floor, so a future preset cannot turn the field off by accident |
| `COMPACT_MAX_WIDTH_PX` | 760 px | `App.tsx` | above the widest named phone landscape (844 is *wide*), below a two-row header |
| `COMPACT_MAX_HEIGHT_PX` | 560 px | `App.tsx` | a phone in landscape is wide but short; same remedy, same flag |
| `WORLD_PX.fallback` | 780 × 520 | `App.tsx` | v1's fixed size, kept as the pre-measurement value |
| `WORLD_PX.min` | 260 × 190 | `App.tsx` | an 85 px hull at the default zoom, plus about a hull-length of water |
| `WORLD_PX.maxAspect` | 1.15 | `App.tsx` | keeps a tall narrow phone from turning the world on its side |
| `CONTROL_COLUMN_MIN_PX` | 260 px | `ui/Layout.tsx` | two 44 px pads with their padding and the gauges' labels |
| `COMPACT_MAX_REM` | header 3.6, readouts 4.4 | `ui/Layout.tsx` | about two rows of controls and three lines of readouts, so the first line of each is always in view |
| `PARAMS_POLL_MS` | 500 ms | `sim/useSimulation.ts` | one boundary call every half second, re-parsed only when the text moves |

Wind, measured on the page: Sail Mode draws **2000 of 4000** at contrast 0.55;
Debug Mode draws **4000 of 4000** at contrast 1. The wind the *boat* feels is
identical across the switch, asserted to six decimals on `wind_at_boat()`.

### 3.5 Visual review — high heel and both tacks

Reviewed at 390 × 844, 844 × 390, 360 × 640 and 1280 × 800, by screenshot:

| Scene | State | Reads |
|---|---|---|
| `beam_reach_capsize` at t = 6.6 s | φ = **+70.5°** | hull on edge, topsides and sail both legible against the field; sheet gauge at 1.04 m of 4.50 m |
| `close_hauled` at t = 5.3 s | φ = +5.2°, β = **+0.61** | **port tack** — boom out to starboard, heeled to starboard, sheet 2.00 m |
| `gybe` at t = 2.6 s | φ = −0.6°, β = **−1.49** | **starboard tack** — boom out to port, sheet eased to 4.00 m |
| `free_sail`, Debug Mode, 1280 × 800 | β = +1.59 | full-density field, force overlay and legend beside the boat |

The tack is read off `β`'s sign, which F2.1 fixes: `β > 0` puts the boom to
starboard, so the wind is over the port side and the boat is on **port** tack.
Both tacks render correctly — the boom, the sail's shading and the sheet's path
all mirror — which is the thing the review is for, since section 01's depth sort
is the only thing that makes a heeled boat readable at all. The `gybe` scene is
nearly upright (−0.6°) because it is running downwind; heel is exercised by the
70.5° row above it, on the other tack's side.

---

## 4. The one measurement trap, stated for the next section

`useWorldViewport()` takes the world view's **width** from the world cell and
its **height** from the main row, and never the other way round. Both are boxes
whose size comes from the grid and not from their contents: the cell is
`width: 100%` of a flex line, and the main row is `minmax(0, 1fr)` of a
viewport-height grid with `overflow: auto`. A `ResizeObserver` on a box that its
own content sizes resizes forever, and the loop is silent — it presents as a
pegged CPU, not as an error.

---

## 5. Parameters changed, and the deferred list confirmed absent

**No parameter changed.** `crates/` is untouched, `parameters.rs` is untouched,
and no coefficient — physical or visual — was tuned to make a demonstration
look better. The two F18.1c overrides section 08 recorded are still the only
ones, and `tests/provenance.rs` compares 83 F7 rows on every gate run.

Every visual constant this section introduced is listed in §3.4 with its
provenance, and each is a count of screen pixels or a dimensionless fraction.
The one that most looks like tuning — Sail Mode's wind density — was chosen
against a legibility argument and a floor (`wind.spec.ts`'s "thousands of
particles"), not against how a scenario looked, and it moves nothing the boat
feels: `wind_at_boat()` is identical across the mode switch to six decimals.

The PRD's exclusion list is **absent, not merely unused**:

| Deferred | Status |
|---|---|
| position targets / an angle controller | absent; a zero rudder command still means self-centre (§9.4) |
| regripping, finite hand strokes, flick release | absent; a grab is a relative rate and lifting ends it |
| a ratchet, a cleat, a hand-force or tension-dependent slip law | absent, and `touchInput.test.ts` greps both new files for each word |
| haptics / `navigator.vibrate` | absent, and grepped for |
| tilt / device orientation | absent; nothing reads an accelerometer |

---

## 6. Risks that fired

**RV53 — stuck touch command. FIRED twice during development, both fixed, both
now guarded by a test.**

1. **The held-keys set was replaced, not emptied.** `clearInput()` built a
   fresh `InputSources` including a fresh `Set`, which orphaned the reference
   the key handlers had captured when their effect ran. Every key press after
   the first pause went into a set nothing read: all three brief §46
   demonstrations failed in all three browsers with "the boat never steered".
   `heldRef` is now constructed exactly once and cleared in place, and
   `controls.test.ts` asserts that the phrase `held: new Set` does not appear
   in `useSimulation.ts` at all — the set is built in `heldRef`'s declaration
   and nowhere else.
2. **The pads held gesture state the hook could not reach.** Clearing the
   *composed* command left `TouchControls` still showing a full-scale helm and
   re-sending it on the next `pointermove`. `clearInput()` now bumps
   `inputGeneration`, which the pads watch; a finger still down owns nothing
   until it lifts and lands again.

**RV54 — apparent control lag misread as physics. Did not fire, and the
separation is structural.** Actual state and command are different rows, in
different units, in different colours, and the two tolerances of §3.1 and §3.2
never meet.

**RV55 — mobile UI hides sailing. FIRED twice, and neither fix shrank a
bound.**

1. **Landscape.** Stacked at 844 × 390 the control deck and the two capped
   strips left the main row **0 px** tall and the world view was clipped behind
   the pads. A short, wide viewport now puts the controls beside the world;
   measured, the boat gets 476 × 246 of an 844 × 390 screen.
2. **The panel scrolled out from under a drag.** Caught by the gate, in
   Firefox, by `sheet.spec.ts`'s "releasing adds visible sag". Making the main
   row a scroll container put a scrollport directly under the boat, and a
   pointer captured by the world view and dragged past its bottom edge makes
   the browser autoscroll it: measured, a 250 px mainsheet haul from the middle
   of a 326 px-tall view scrolled the row by **224 px**, sliding the boat off
   the top so that the next press landed on the instruments. The player
   experiences it as the boat sliding out from under their hand mid-trim.
   While the world view owns a drag the main row now carries
   `overflow: hidden` — `App.tsx` derives it from the pointer events it already
   handles and the layout publishes it as `data-dragging`. Nothing else
   changed: the row keeps its scroll position and everything in it is reachable
   again the moment the drag ends. Measured after the fix, `scrollTop` stays 0
   through both hauls and both reach `l_sheet_min` exactly.

   **This is a defect this section introduced**, and it is the one place where
   a fixed-height layout is worse than v1's scrolling document: v1's world view
   was 520 px tall, so a 250 px drag never left it.

**RV56 — second physics implementation. Did not fire.** The only numbers
crossing the boundary are normalised commands in `[−1, 1]`; every rate and
limit is read from `parameters_json()`; `touchInput.test.ts` greps both new
files for a tension, a slip law, a ratchet, a cleat and a vibration call, and
`parameterSchema.test.ts` greps them for `0.9`, `4.5`, `2.4`, `0.6` and
`1.0404…` outside a comment.

**R5 (v1) — deck.gl bundle / WebGL context. Did not fire.** `DeckOverlay` is
unchanged; it already took a `viewport` prop, so the measured size needed no
edit to it and no second `Deck` instance exists.

---

## 7. What deviated from the PRD, and what needs a human

### 7.1 Task 9.2 wrote no Rust — **confirmation wanted**

Argued in §2, task 9.2. The task owns `crates/sailgym-wasm/src/lib.rs` and the honest
outcome was to leave it alone, because the fields it was told to add already
cross the boundary. The PRD's own instruction — "record a corrected ownership
path if the declaration lives elsewhere, never create a parallel type merely to
fit this list" — is what was followed, and the corrected path is
`web/src/sim/useSimulation.ts`. Flagging it because a task with an empty diff
on the file it owns deserves to be looked at rather than assumed.

### 7.2 Three files were written that appear in no task's `Owns:` list

Each is flagged rather than hidden, and each was forced.

| File | Why |
|---|---|
| `web/src/App.tsx` (task 9.1's share of it) | `RenderParams` gained six fields in 9.1, and `PENDING_PARAMS` is a `RenderParams` literal in `App.tsx`. Without the six placeholders the tree does not typecheck between group S and group A. The file *is* owned — by task 9.4, in group B — so this is a task-order overlap rather than an unowned file, and the same agent wrote both. |
| `web/tests/unit/ilca.ts` | The same six fields; it is section 01's shared F7 test fixture and is in no section-09 `Owns:` list. The added values are F7's, and `parameterSchema.test.ts` now checks them against the panel's own path walk. |
| `web/index.html` | `env(safe-area-inset-*)` is **zero** unless the viewport meta carries `viewport-fit=cover`, so task 9.4's safe-area requirement cannot be met without this one attribute. No task in this section owns the file. **Confirm or revert.** |

### 7.3 Task 9.4 owns no test file

Its acceptance criteria are browser-measurable and are all asserted in task
9.5's `mobile-controls.spec.ts`; the pure parts (`visibleCount`, the wind
presets) have no unit test because `tests/unit/particles.test.ts` belongs to
section 03 and no section-09 task owns it. The same `Owns:`-gap was flagged by
section 01 §2.3 and section 08 §6.3; it is now three sections old and worth
fixing in the next PRD's task template.

### 7.4 No task was delegated, and one gate run overlapped a group boundary

Groups A and B contain two tasks and one task respectively, and the two group-A
tasks turned out to be one finding (§2, task 9.2) and one component that reads
the type that finding is about. Splitting them would have had a subagent
rediscover the `parameters_json()` question from cold. Sections 01 and 08
recorded the same thing.

Group order was honoured. The gate runs were: the **full chain** after group S,
green; a **full chain** started during group A that completed green but whose
last steps therefore saw a tree partway into group A; focused steps 7 and 8
plus the whole `mobile-chromium` project between A and B; and the **full
chain** at the end (§10), which is the one this section's status rests on. The
middle one is the deviation from "run the full gate after each completed
group" — it was started early and not re-run at the group boundary. Stated
rather than counted as a clean group-A gate.

### 7.5 `--fast` does not run the touch suite

`scripts/check.sh --fast` restricts step 9 to `--project=chromium`, so the new
`mobile-chromium` project is skipped. That is consistent with `--fast` not
being the gate, and `check.sh` is owned by no task here, so it was left alone.
The touch suite costs **14 s**; adding `--project=mobile-chromium` to the
`--fast` line would be cheap and is recommended.

### 7.6 A finger on the world view does nothing

`sheetInput` drops a `touch` pointer, so touching the boat neither trims nor
pans. Trimming by touch belongs to the pad — a thumb put down to look at the
boat must not haul — but touch panning and pinch zoom are simply **not
implemented**, and the world view still carries `touch-action: none`, so the
page cannot be scrolled from over the boat either. Nothing was lost (v1 had no
touch behaviour at all) and nothing in the PRD asks for it. Worth a decision
before section 11 puts a course on the water that a player will want to look
around.

### 7.7 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` here, as in section 01 §5.5 and section 08 §6.7.
`check.ps1` was **not edited** by this section, and neither was `check.sh`, so
both should be unaffected — but `check.ps1` remains unverified on Windows.

---

## 8. Pre-existing failures, recorded separately

### 8.1 `identity::rebuilding_unchanged_source_keeps_it_stable` failed at `0bd007a`

Measured **before any section-09 edit**, with the working tree stashed:

```
the compiled source id is bc83d0dc6bc008de5e7c86452f67b05cbdf5742e
but git now says 99d1899974ad92919888f0487f1e540a90a5880e;
build.rs did not re-run when src changed (RV52)
```

**It is a stale build artefact, not a code defect in the section-08 sense, and
it is section 08's own RV52 guard doing its job.** The identity is a function
of **git HEAD** (`git rev-parse HEAD:crates/sailgym-physics/src`), but
`build.rs` only declares `cargo:rerun-if-changed=src`. Committing section 08's
work moved HEAD without touching a file mtime, so cargo did not re-run the
build script and the compiled constant stayed at the pre-commit tree id.
`touch crates/sailgym-physics/build.rs` re-ran it and the test passes; that is
what was done, and it changes no file content, so the tree is still clean.

**It will recur for any build that predates a commit**, which includes every CI
cache and every developer who pulls. The remedy is in `build.rs` — watch the
git refs as well as the sources, or fall back to hashing `src/` directly — and
`crates/sailgym-physics/build.rs` is owned by no section-09 task, so nothing
was changed there. **Section 10 keys on `ModelIdentity` and should fix this
first.**

---

## 9. What the next section must know

1. **There is exactly one `set_controls` call site and one `advance` call
   site.** `applyControls()` and the clock's sink, both in
   `sim/useSimulation.ts`, and `controls.test.ts` counts them. A new input —
   a gamepad, an agent, a replay scrubber that drives the boat — adds a field
   to `InputSources` and a case to `composeControls`, and touches nothing else.

2. **`null` is not `0` on an input channel.** `null` means *no device is
   driving this*, and it is what makes falling back to another device possible
   and a stale gesture impossible. A component that returns `0` when a finger
   lifts has silently taken the channel away from the keyboard.

3. **A component with gesture state of its own must watch `inputGeneration`.**
   Clearing the composed command is only half of clearing; §6 RV53 (2) is the
   defect this prevents. No unit test of the reducer can see it — the reducer
   is correct in both versions — so it is guarded from the outside, by
   `mobile-controls.spec.ts`'s "pausing clears a held command" and "entering
   replay clears a held command", both of which hold a finger down across the
   transition.

4. **A zero rudder command still means self-centre**, in Rust
   (`dynamics::rudder_rate`). Position-target control is still deferred and
   still needs an explicit engaged/released contract in a shared Rust adapter
   (v2 F18.2, `discussions/mobile-controls.md`). Nothing here holds a rudder
   angle, and nothing here should start.

5. **`params` is now live.** `useSimulation` re-reads `parameters_json()` twice
   a second and re-parses only on a change, so anything quoting the catalogue
   follows a brief §31 edit. It also means `sim.params` changes identity when a
   parameter moves — memoise renderer props on their **values**, which is
   section 01's RV4 lesson and still costs 14 ms of frame time if ignored.

6. **The layout is a fixed-height grid and the page never scrolls.** The
   `main` row is the scroll container. Anything added to the page belongs in a
   slot — `header`, `readouts`, `world`, `instruments`, `debug`, `footer`,
   `controls` — and anything that grows without a cap will squeeze the boat, as
   the landscape arrangement of §6 RV55 found out.

7. **`data-particles` now means what the frame *draws*.** It was the population
   size; the population is `data-particles-total` beside it. `wind.spec.ts`'s
   "there really are thousands of particles being drawn" now measures what it
   says, and still passes at 2000.

8. **A scroll container under the boat is a trap.** The world view captures
   the pointer for a trim or a pan, and a captured pointer dragged past the
   edge of an ancestor scrollport makes the browser scroll it. Anything that
   makes a new scrollable ancestor of `world-view` has to set `dragging` the
   same way, or the boat will slide out from under the player's hand. §6
   RV55 (2) has the measurement.

9. **The touch suite is one project and it is emulation.** `mobile-chromium`
   gives trusted multi-touch, real pointer capture and a real mobile viewport.
   It does **not** give a digitiser, a finger, palm rejection, a mobile GPU or
   a retractable browser chrome. **No real-device session was run: hardware
   coverage is outstanding**, and no claim in this document rests on one.

10. **`Input.dispatchTouchEvent` is asymmetric.** `touchStart` and `touchMove`
   take the whole set of active points; `touchEnd` takes the points being
   **released**. Sending the remaining point instead lifts the wrong finger and
   the test then reads a perfectly sensible page state that answers a different
   question. Measured, and documented at the helper.

11. **`docs/v2/README.md`'s open item V-D is discharged, and this section did
    not edit the index to say so.** V-D reads "Basic mobile rate controls
    promoted to M-next; hand model, position targets and tilt remain deferred";
    the controls have shipped and all three exclusions hold — there is no hand
    model, no position target and no tilt anywhere in this section. The row is
    left for the human to close, because section 01's handoff §5.2 flagged an
    edit to that file as an ownership deviation, section 08 §8 item 8 left V-G
    the same way, and no task here owns it either.

12. **Mobile-browser limitations observed, and the two that emulation cannot
    check.** `navigator.vibrate` specifies durations rather than force and is
    **not** used; `isMobile` is a Chromium-only Playwright option, which is why
    there is no mobile Firefox project. Two of the remedies are **unverified**
    and need the real-device session item 9 above calls for:

    - **safe areas.** The layout pads by `max(8px, env(safe-area-inset-*))` on
      all four sides and `index.html` now carries `viewport-fit=cover` (§7.2),
      but every inset is `0` in the emulator, so what is tested is that the
      padding is at least 8 px — not that a notch or a home indicator is
      actually cleared.
    - **retractable browser chrome.** The root is `100svh` with a `100vh`
      fallback, which is the *small* viewport height and is what keeps the
      control deck from sliding under a returning address bar. The emulator has
      no chrome to retract, so the `ResizeObserver` path is exercised by
      `setViewportSize` and not by a real toolbar.

    Neither was worked around in a way that changes what the boat does.

---

## 10. The gate

`scripts/check.sh`, from this section's working tree, exit 0:

| step | | time |
|---|---|---|
| 1 | `cargo fmt --check` | 0 s |
| 2 | `cargo clippy --all-targets -- -D warnings` | 0 s |
| 3 | `cargo test -p sailgym-physics` | 62 s |
| 4 | `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance` | 40 s |
| 5 | `cargo test -p sailgym-physics --test regression` | 0 s |
| 6 | `wasm-pack build` | 7 s |
| 7 | `pnpm --dir web typecheck` | 1 s |
| 8 | `pnpm --dir web test:unit` | 1 s |
| 9 | `pnpm --dir web test:e2e` | 505 s |
| | **all steps passed** | **617 s** |

Browser frame budgets, Chromium, from the same run (task 10.5's spans,
unchanged budgets): `svg` p50 **1.4 ms** / p95 **3.8 ms** against 12 ms,
`wasm` p50 0.2 ms / p95 **0.3 ms** against 8 ms, `frame` p95 0.7 ms, `deck`
p95 0.4 ms. The larger measured world view and the always-mounted control deck
cost nothing measurable; Sail Mode's halved particle count is the likely
reason `svg` and `deck` are, if anything, slightly cheaper than section 01
recorded.

Counts: **20 vitest files, 186 tests** (was 19 / 129); **286 Playwright tests
in 19 files** across Chromium, Firefox, Edge and mobile-Chromium (was 267 in
18), of which **19** are the new touch suite — the desktop three still run 89
each, unchanged. All three brief §46 demonstrations pass in all three desktop
browsers.

```
scripts/check.sh
pnpm --dir web test:e2e --project=mobile-chromium
pnpm --dir web test:unit
```

`pwsh scripts/check.ps1` was not run — §7.7.
