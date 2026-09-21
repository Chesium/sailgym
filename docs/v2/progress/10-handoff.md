# v2 Section 10 — Handoff (truthful replay and experiment identity)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F18.3,
`docs/v2/recording-format.md` and `docs/v2/progress/08-handoff.md` before
starting section 11.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. F1–F7 are untouched: no equation, no coefficient and no
frame convention moved, `STATE_LEN` is still 13, the F8.3 snapshot layout is
unchanged, `parameters.rs` is byte identical and `provenance.rs` compares the
same 83 F7 rows. F8.2's method set **grew** by five read-only accessors (§2,
task 10.2) and F9 is unchanged.

Status: **complete**; `scripts/check.sh` is green end to end (§11). Two defects
this section introduced were caught by the gate and fixed, and §7 states both
in full. One item needs a human ruling (§6.1) and six things deviated from the
PRD as written (§6).

---

## 1. What the section is, in one paragraph

Before this, a replay showed a recorded **pose** inside the live run's
**numbers**: `App.tsx` selected a frame for the boat while the HUD, the wind,
the force overlay, the charts and the diagnostics panel went on reading the
live simulator. Now there is one selection — `selectInspection` — and every
consumer takes its numbers from it. The episode carries what those consumers
need (schema 2's `FrameDiagnostics`, 40 scalars beside each sample) and a
canonical `ExperimentIdentity` that says whether two recordings describe the
same conditions. What an episode does **not** carry reads `Not recorded`: not a
zero, not the live value, and never a fresh evaluation of the model currently
loaded. A schema-1 file still opens, still scrubs, and is honest about its
gaps.

---

## 2. What landed, file by file

### Task 10.1 — recording schema and migration contract (P-group S, section agent)

- **`crates/sailgym-physics/src/recording.rs`** — rewritten around three
  additions. `EPISODE_SCHEMA_VERSION` 1 → **2**; `SUPPORTED_SCHEMA_VERSIONS`
  `[1, 2]`; an episode is **re-encoded in the schema its own header declares**,
  in both codecs, so a legacy file that is read and written again is still a
  legacy file.
  - `FrameDiagnostics` — 23 fields, `DIAG_LEN` = 40 scalars, appended to the
    38-scalar schema-1 frame as a **strict extension** (`FRAME_LEN` = 78). The
    first 38 scalars of a schema-2 frame are exactly a schema-1 frame, which is
    what lets one decoder read both widths.
  - `Recorded<T>` = `Value | Unknown | NotApplicable`, with `Unknown` equal to
    nothing including itself, and seven `#[serde(default)]` header fields
    (`identity_version`, `model`, `initial_state`, `initial_controls`,
    `practice`, `action`, `observation`).
  - `ExperimentIdentity`, **derived** from the header by
    `EpisodeHeader::identity()` rather than stored beside it, and
    `Comparability::{SameConditions, Different, Indeterminate}` naming the
    offending fields in a fixed order.
  - `PracticeEnvelope` / `TaskIdentity` / `PracticeEvent` — section 11's
    reserved, typed, versioned envelope, written through
    `Recorder::set_practice` and `Recorder::push_practice_event`.
  - `MAX_EPISODE_BYTES` = 8 MiB → `MAX_EPISODE_FRAMES` = **13 443**, and
    `Recorder::due()` is `false` once full so a capped recording stops paying
    for a `Diagnostics` evaluation as well as for memory.
  - Both decoders now `validate()` before handing anything back: bad magic,
    unsupported version, a frame width that disagrees with the declared schema,
    an oversize header or frame count, truncation, a header whose
    `schema_version` disagrees with the file's, **any non-finite scalar**, a
    non-positive `dt`, and a schema-2 frame with no block.
  - 19 unit tests (was 9).
- **`crates/sailgym-physics/tests/fixtures/episode-schema1.{json,bin}`** — new.
  A genuine schema-1 episode: twelve samples of `close_hauled` at 10 Hz, a
  seven-key header and 38-scalar frames, 10 441 B and 6 080 B. RV60's fixture.
- **`web/src/sim/scenarioTypes.ts`** — the mirror: `FrameDiagnostics`,
  `Recorded<T>`, `recordedValue`, the five identity records and four version
  constants.
- **`docs/v2/recording-format.md`** — new, and normative for the schema: the
  versions, the header, the frame, the **omitted-diagnostics table**, identity
  and comparison, the binary layout with its rejection table, the byte budget
  with its provenance, and the migration fixtures.

### Task 10.2 — capture at sample time through the existing boundary (P-group A, section agent)

- **`crates/sailgym-wasm/src/lib.rs`** — `start_recording` now goes through
  `EpisodeHeader::manual`, so the header records the resolved catalogue, the
  full initial state and the controls in force, and marks the research-only
  slots `NotApplicable`. **Five** new read-only accessors, all coarse-grained
  (brief §24): `recording_capacity`, `recording_bytes_per_frame`,
  `recording_full`, `episode_identity_json` and `episode_comparability_json`.
  **The codec, the identity and the cap all stay in Rust**; the wrapper
  marshals.
- **`web/src/sim/diagnostics.ts`** — `PartialDiagnostics`,
  `DiagnosticsSample` (`source`, `t`, `values`), `liveDiagnosticsSample`,
  `isRecorded` and `NOT_RECORDED`. **The type is the enforcement**: every field
  reads `T | undefined`, so a consumer cannot use one without deciding what to
  show when it is absent.
- **`web/src/sim/episodeIo.ts`** — `readEpisodeIdentity`, `compareEpisodes`,
  `identityNamesABaseline`, `describeIdentity`, `readRecordingLimit`,
  `recordingSeconds`, `megabytes`. Every verdict is the core's; this file
  displays them.
- **`web/src/sim/loadWasm.ts`** — **no change needed**, and §6.3 says why.

### Task 10.3 — one live-or-replay display selection (P-group B, section agent)

- **`web/src/sim/replay.ts`** — the boundary. `selectInspection` →
  `InspectionView`; `diagnosticsFromFrame` (the two tiers of
  `docs/v2/recording-format.md` §4); `windFromFrame`; `replayWindField`;
  `createReplaySource` no longer throws on an empty episode and its accessors
  return `null`. **Diagnostics are never interpolated**: `sampleAt` sets
  `diag: null` and the view takes the preceding recorded sample's block, with
  that sample's own timestamp.
- **`web/src/sim/useSimulation.ts`** — `setInspecting(on)`. While inspecting
  the frame loop does not call `clock.tick`, so **no route back to `advance`
  remains** however the clock controls are used; `applyControls` pushes an idle
  `Controls`; both transitions pause and clear.
- **`web/src/App.tsx`** — one `selectInspection` call, and `sim.snapshot`,
  `sim.diagnostics` and `wind.windAtBoat()` appear on that line and nowhere
  else in the render. The replay notice, the `[data-testid="inspection"]`
  probe, the visible recording limit and the episode-identity badge.
- **`web/src/ui/Hud.tsx`**, **`web/src/ui/DebugPanel.tsx`**,
  **`web/src/render/ForceOverlay.tsx`**, **`web/src/ui/WindReadout.tsx`** — all
  four take a `DiagnosticsSample`. The panel renders a row for an **absent**
  field rather than dropping it; an overlay whose vector *or* application point
  is missing is not drawn and its legend row reads `Not recorded`.
- **`web/src/wind/{WindLayer.tsx, ArrowOverlay.tsx, useWindField.ts}`** —
  documentation only (§6.4).

### Task 10.4 — charts follow recorded time (P-group B, section agent)

- **`web/src/ui/Charts.tsx`** — `replayChartData(source, t)`, a **pure
  function** of the source and the playhead: nothing accumulates, so playing
  twice or scrubbing backwards produces identical points. Every point is a
  recorded sample at its own timestamp (`frameAt`, never `sampleAt`).
  `useChartSampler` gained a `live` flag that freezes the live buffer during
  replay — it neither appends nor clears. A series the episode does not carry
  is empty and labelled.
- **`web/src/ui/Timeline.tsx`** — `timelineState`, the pure transport rule,
  with defined disabled/constant behaviour for 0- and 1-frame episodes; the
  episode's sampling resolution and schema on the page.
- **`web/tests/unit/replay.test.ts`** — 7 → **28 tests**, in eight `describe`
  blocks. The v1 player tests are kept and extended; the new ones are the
  negative half — that an absent field comes back **absent**.

### Task 10.5 — end-to-end truth test and handoff (P-group S, section agent)

- **`web/tests/e2e/replay-truth.spec.ts`** — new, **9 tests** × 3 desktop
  browsers. §3 states what each one measures.
- **`docs/v2/progress/10-handoff.md`** — this file.

---

## 3. The measured results

### 3.1 The adversarial test, which is the section's point

A matching number proves nothing on its own: the live run and the episode agree
until something separates them. So `replay-truth.spec.ts` **makes the live
simulation conspicuously different and then asserts nothing on screen moved**.

| Test | What is altered under the replay | What must not move |
|---|---|---|
| every consumer follows the episode | `sail.area` 7.06 → 3.0, wind mode → uniform, then the clock asked to run | pose, HUD, wind readout, sheet tension, `GZ`, rope length — compared as a whole object, twice |
| wind field hidden | — | deck layers must be **0** in replay and > 0 again on exit |
| scrubbing | forward → backward → forward | every stop equals its stored frame to 1e-9; the return visit equals the first |
| diagnostics are a sample | playhead put **between** two samples | the panel, the overlay and the HUD all name the same sample time, at or before the playhead and within one logging interval |
| charts | played twice, scrubbed both ways | identical point counts for the same playhead |
| legacy episode | schema-1 document built and re-imported | 5 named fields `Not recorded`, 4 named fields still shown |
| entry/exit | helm held down across the transition | clock stops on entry and **stays paused** on exit |
| recording bound | — | `frames × 624 ≤ 8 MiB`, visible on the page |

### 3.2 Every display consumer, and where its numbers come from

Task 10.5's acceptance asks for this list by name. **Every one takes the single
`InspectionView`**; none reads the live simulator while `source` is `replay`.

| Consumer | Live | Replay | If the episode does not carry it |
|---|---|---|---|
| boat pose (`BoatSvg`, `snapshot` probe) | `sim.snapshot` | interpolated recorded pose | — always recorded (the full F3 state) |
| sail camber (`alpha_sail`) | diagnostics | recorded sample | flat sail drawn; notice lists the field (§9 item 6) |
| mainsheet sag (`sheet_rope_length`) | diagnostics | recorded sample | rope drawn at `lSheet`, i.e. neither taut nor slack |
| HUD, 7 readouts | diagnostics | recorded sample + `data-sample-t` | `Not recorded` per readout |
| wind readout | `wind_at_boat()` | recorded vector | vector shown; speed/bearing `Not recorded` (F6.1) |
| spatial wind field + arrows | live grid | **hidden** | reason shown on the page |
| force overlay, 16 items | diagnostics | recorded sample | overlay not drawn; legend row `Not recorded` |
| overlay legend | diagnostics | recorded sample | `data-recorded="false"` per row |
| charts, 8 series | live ring buffer | episode samples ≤ playhead | series empty and labelled |
| debug panel, 51 rows | diagnostics | recorded sample + timestamp | row rendered, `Not recorded`, `data-recorded="false"` |
| heel indicator | diagnostics | recorded `capsize` / frame flag | peak `Not recorded` |
| capsize state | diagnostics | frame's `capsized` flag | always recorded (schema-1 field) |
| timeline | — | episode extent, `log_hz`, schema | disabled for 0- and 1-frame episodes |
| episode identity badge | — | `Sim.episode_identity_json` | "cannot be labelled a same-conditions experiment" |

### 3.3 The recorded subset, and what it costs

| | schema 1 | schema 2 |
|---|---|---|
| scalars per sample | 38 | **78** |
| bytes per sample (binary) | 304 | **624** |
| cap at the 8 MiB budget | — | **13 443 samples** |
| at 20 Hz, the default | — | **672 s ≈ 11.2 min** |
| at 5 / 10 / 50 Hz | — | 44.8 / 22.4 / 4.5 min |

The 8 MiB is a **memory budget, not a physical quantity and not a threshold
chosen from how a recording looked**: it is the point past which holding the
frame block, its JSON form and the decoded JavaScript objects at once stops
being comfortable in a browser tab. It is stated in bytes and the frame cap is
*derived*, so the cap follows the frame width the next time the schema grows.

### 3.4 Fields not recorded, by episode schema

Fifteen `Diagnostics` fields are deliberately omitted from **every** episode —
`steps`, `course_over_ground`, `leeway_angle`, the six `cl_*`/`cd_*`, the
`boom_moment` breakdown (its total is `moments[3]`), `sheet_hull`, the three
energies and `hull_model_warning`. None is read by the HUD, the force overlay,
the charts or the capsize readout, and each costs bytes in every sample.
`recording::tests::the_omitted_diagnostics_are_named_at_their_source` asserts
that **every** `Diagnostics` field is either carried by a frame or named in
that list, so a field added to `diagnostics.rs` cannot drift into being
silently absent.

A **schema-1** episode additionally lacks everything the block would have
carried: measured, **39 of the 51** fields are unavailable. The **12** it does
carry are still shown — `t`, `true_wind_world`, `sheet_tension`, the three
moments, `heel_deg`, and the five that come straight out of the recorded F3
state (`velocity_body`, `yaw_rate`, `roll_rate`, `beta`, `beta_dot`).

### 3.5 Visual and display settings, and their provenance

Every one is a count of screen pixels, a string, or a rule about what may be
displayed. **None reaches the physics core**, and none was chosen to make a
demonstration look better (brief §43, and the PRD's extension of it to visual
constants).

| Setting | Where | Why |
|---|---|---|
| `NOT_RECORDED = 'Not recorded'` | `sim/diagnostics.ts` | one spelling, so a test can grep for it and a reader cannot mistake it for a value |
| replay notice position/opacity | `App.tsx` | 8 px inset, 0.88 alpha — over the water, under the boat's own controls, `pointerEvents: none` so it cannot take a drag |
| unavailable legend swatch `#ccd` | `render/ForceOverlay.tsx` | the panel's own border grey; a greyed row must not read as a colour-coded quantity |
| unavailable text `#889` | `ui/DebugPanel.tsx`, `ui/Charts.tsx` | the existing `#667` label grey, one step lighter |
| scrub rounding to 3 dp | `tests/e2e/replay-truth.spec.ts` | a test-side constant: `<input type="range">` refuses text it would normalise. Milliseconds are finer than any logging rate the UI offers |

---

## 4. Parameters changed

**None.** `parameters.rs` is byte identical, `provenance.rs` compares the same
83 F7 rows on every gate run, and the two F18.1c overrides section 08 recorded
are still the only ones. No coefficient — physical or visual — was tuned.

`scenarios/` is untouched.

---

## 5. The identity this section leaves behind

```
model_version : 2            (unchanged — no F6/F7 clause moved)
source tree   : 55f3a73fd06aaa2001133f328676fd75b9c49943
predecessor   : 99d1899974ad92919888f0487f1e540a90a5880e   (section 08's)
state         : dirty at the time of writing; clean once committed
```

Computed through a **temporary index file**, twice, with nothing staged and the
real index untouched — the method section 08 §9 established. `git status` is
unchanged by it.

**And this is the thing section 11 and section 02 most need to know.** See
§6.2: the source id moved even though no equation did.

---

## 6. What needs a human, and what deviated

### 6.1 The source identity now moves when the *recording schema* changes — **ruling wanted**

F18.1d defines the source identity as the content of the whole of
`crates/sailgym-physics/src`. `recording.rs` lives there. So this section —
which changed **no equation, no coefficient and no frame convention** — moved
the physics source tree id from `99d1899…` to `55f3a73…`, and
`ModelIdentity::is_comparable_with` therefore reports that an episode recorded
before this change and one recorded after it describe **different models**.
They do not.

This is F18.1d working exactly as written (it is a *content* id, and it is
deliberately conservative), and it is the right default — a false "same
conditions" is far worse than a false "cannot tell". But it has a consequence
worth a decision now rather than in section 02:

- every schema, serialisation or diagnostics change invalidates comparability
  with every earlier baseline;
- F16.4's conformance bundle keys on the same id, so a bundle will be declared
  stale by an edit that cannot affect a single number in it.

Three options, not chosen here: leave it (conservative, noisy); narrow the id
to the modules that carry the model (`foil.rs`, `dynamics.rs`, `forces/`,
`aero/`, `hydro/`, `rigging/`, `stability/`, `parameters.rs`, `integrator.rs`,
`state.rs`, `frames.rs`, `constants.rs`) and record the split in F18.1d; or add
a second, narrower id beside the existing one. **Each changes what F18.1d
means, so none was done unilaterally.**

### 6.2 `MODEL_VERSION` was **not** bumped, deliberately

Section 08 §8 item 5 says to bump it "when F6 or F7 changes in a way that makes
a previously recorded episode describe a different boat". Neither changed, so
it is still 2. Flagged because a schema bump is the kind of change that invites
a version bump reflexively, and that would have been wrong: the recording
format and the model contract are different things, which is why
`EPISODE_SCHEMA_VERSION` and `IDENTITY_VERSION` exist separately.

### 6.3 Task 10.2 wrote no `loadWasm.ts` change — **confirmation wanted**

The task owns `web/src/sim/loadWasm.ts` and the honest outcome was to leave its
behaviour alone: `SimHandle` is re-exported from the `wasm-pack`-generated
declarations, so the five new `Sim` methods were typed the moment gate step 6
regenerated them. Only a doc paragraph was added, recording the finding. This
is the third section to make it — section 01 §1 for six geometry fields,
section 09 §7.1 for six actuator limits — and it is now a pattern rather than a
coincidence.

### 6.4 Three owned files took documentation only

`web/src/wind/{WindLayer.tsx, ArrowOverlay.tsx, useWindField.ts}` are in task
10.3's `Owns:` list. The correct change was **none**: the decision about
whether the wind field may be drawn belongs at the selection boundary
(`replayWindField`) and in `App.tsx`, not in three layers that have no idea
what is being replayed and should not. Each gained a paragraph saying so.

### 6.5 Five files were written that appear in no task's `Owns:` list

Each is flagged rather than hidden, and each was forced — every one held a test
assertion or a struct literal that the schema bump makes **false**, so leaving
it would have left the gate red.

| File | Why |
|---|---|
| `crates/sailgym-physics/tests/invariants.rs` | `deterministic_replay` builds an `EpisodeHeader` as a struct literal and compares `FRAME_LEN` scalars. Both had to move with the schema. |
| `web/tests/unit/scenarioTypes.test.ts` | The parity test **is** the schema contract: field counts, the new `FrameDiagnostics` pair, and the one-line identity records. |
| `web/tests/e2e/replay.spec.ts` | It freezes the header and frame key lists and the version-rejection message, all three of which schema 2 changes. |
| `crates/sailgym-physics/tests/fixtures/` | New directory. RV60's checked-in legacy fixtures; no task owns a fixture path. |
| `web/tests/unit/replay.test.ts` (task 10.4's, edited during group S) | A one-line header fix so the gate was green **at the group-S boundary** rather than only after group B. Rewritten properly in its own group. |

### 6.6 No task was delegated

Groups A and B contain one and two tasks. The two group-B tasks share a data
flow — `Charts.tsx` consumes the `ReplaySource` and `diagnosticsFromFrame` that
task 10.3 defines — so splitting them would have had a subagent rediscover the
selection boundary from cold. Sections 01, 08 and 09 recorded the same thing;
it is now four sections old and worth fixing in the next PRD's task template,
along with the `Owns:` gaps in §6.5.

### 6.7 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` here, as in sections 01 §5.5, 08 §6.7 and 09
§7.7. `check.ps1` was **not edited** by this section, and neither was
`check.sh`, so both should be unaffected — but `check.ps1` remains unverified
on Windows.

---

## 7. Defects this section introduced, both caught by the gate

### 7.1 A render loop from a handle that changes identity every frame

`useSimulation` returns a fresh object every render. The recording-limit effect
and the two identity memos were keyed on that object, so they re-ran on every
frame; the effect then set a fresh `RecordingLimit`, which re-rendered, which
re-ran the effect. Chromium reported **"Maximum update depth exceeded"** and
`debug.spec.ts` failed on the console-error trap.

Keyed on `sim.ready` and the **stable** `withSim` callback instead. This is
section 01's RV4 lesson in a new place: *memoise on values, never on the
handle*.

### 7.2 Switching scenarios paused the simulation

`selectScenario` calls `exitReplay()` unconditionally, so that a scenario can
never change underneath a replay. `exitReplay` now calls
`setInspecting(false)`, which **pauses the clock** — so every scenario switch
paused the live run. Measured: `t` stayed at 0 for the full 20 s poll.

Caught by `scenarios.spec.ts` in **all three browsers** (5 failures). Fixed by
returning early when no replay is open. The guard is `inspectingRef`, the same
ref the frame loop reads, so the two cannot disagree.

**Both defects were in the integration task, not in the physics**, and both
were found by tests that existed before this section started — which is the
argument for running the whole gate at every group boundary rather than the
subset that looks relevant.

---

## 8. Risks that fired

**RV57 — mixed timelines. FIRED, and it is the section's whole subject.** The
defect was present in shipped v1: the world view followed the episode while the
HUD, the wind, the force overlay, the charts and the diagnostics panel followed
the live simulator — `Timeline.tsx`'s own note said so in as many words. The
mitigation the PRD names is the one implemented: a single selection boundary
(`selectInspection`) and an adversarial E2E that changes the live run under the
replay and asserts nothing moves (§3.1).

**RV58 — false comparability. Did not fire, and is structurally closed.** The
identity is the full canonical record, not a parameter digest: a changed
equation moves the source tree id, a changed parameter moves the catalogue, and
both travel. `Unknown` is not equal to anything including itself, so a legacy
episode is `Indeterminate` against every other episode **and against itself** —
which is what the page displays, in as many words, rather than calling it a
same-conditions experiment. Measured in
`same_conditions_needs_every_field_and_a_clean_source` for a changed equation,
seed, `dt`, initial condition, task threshold and parameter, each on its own.

**RV59 — replay becomes re-simulation. Did not fire, and the compiler is the
guard.** No force is evaluated to fill a gap: `PartialDiagnostics` makes every
field `T | undefined`, so a consumer that wants one must handle its absence,
and the absence is rendered as `Not recorded`. The two places a *picture* needs
a diagnostic — the sail's drawn camber and the rope's drawn sag — fall back to
the neutral drawing for a legacy episode (§9 item 6), which claims no angle of
attack and no tension, and the notice says which fields are missing.

**RV60 — migration loses recordings. Did not fire.** `SUPPORTED_SCHEMA_VERSIONS`
is `[1, 2]`, the checked-in fixtures decode in both codecs and re-encode **in
schema 1** with the 38-scalar width preserved, and every rejection path leaves
the episode already loaded untouched — asserted in Rust for eleven fault kinds
and in the browser for truncation and a non-finite value.

**R7 (v1) — build-sensitive artefacts. Unchanged.** `ToolchainInfo` is still
recorded and is still **metadata**, not identity: a rebuilt compiler does not
change the equations. F18.3 says so explicitly and `EpisodeHeader::identity()`
omits it, along with `log_hz` and `created_utc`.

---

## 9. What the next section must know

1. **There is exactly one display selection.** `selectInspection` in
   `sim/replay.ts`, called once in `App.tsx`. A new panel takes the
   `InspectionView`; it does **not** take `sim.diagnostics`. Those three live
   reads appear on one line of `App.tsx` and `replay-truth.spec.ts` is what
   keeps them there.

2. **`undefined` is the answer, and the compiler enforces it.**
   `PartialDiagnostics` is `Partial<Diagnostics>` on purpose. Do not add a
   `?? 0`, do not reach for the live record, and do not compute the value —
   render `NOT_RECORDED`. The two picture-only fallbacks are named in item 6.

3. **Diagnostics are never interpolated; the pose is.** The view carries the
   **preceding recorded sample's** block and that sample's own timestamp, and
   the panel, the overlay and the HUD all publish it as `data-sample-t`. A
   score computed from an interpolated frame would be a score for a state
   nobody simulated — section 11 must evaluate on **recorded samples and
   physics-step indices**, which is what `PracticeEvent.step` is for.

4. **Section 11 writes through the existing recorder.** `Recorder::set_practice`
   and `Recorder::push_practice_event`, into the `PracticeEnvelope` already
   reserved in the header. Do **not** open a second recorder, do not add a
   second `Episode` type in the physics crate, and do not bump 1 → 2 blindly:
   the envelope is typed and versioned so that adding a task needs no schema
   change at all. An event without an envelope is refused.

5. **`ExperimentIdentity` is what "retry restores the exact conditions" means.**
   `identity().compare(&other)` returns `SameConditions` only when every
   compared field is known on both sides and agrees. Two attempts at a task are
   comparable iff that verdict says so — `Sim.episode_comparability_json` is
   the call, and the answer is the core's.

6. **A legacy episode draws a flat sail and a taut rope.** `alpha_sail` and
   `sheet_rope_length` are the only two diagnostics the *drawing* needs, and a
   schema-1 frame has neither, so `BoatSvg` gets `0` and `lSheet` — the neutral
   figure, claiming no angle of attack and no tension — while the notice says
   what is missing. If that ever needs to be better, the honest route is to
   read the episode's **own** recorded catalogue from its header, not the live
   one.

7. **The spatial wind field is hidden in replay, and the reason is a statement
   about this build.** `replayWindField` checks three conditions in order and
   the third — "this build does not reconstruct a recorded wind field" — is the
   one a clean schema-2 episode gets. Reconstructing it needs a second
   `ProceduralWind` built from the episode's own config and seed; reusing the
   live one is the defect, not the feature.

8. **`[data-testid="snapshot"]` is read as numbers.** `fixtures.ts`'s
   `readSnapshot` turns every `data-*` on that element into a `Number`, so a
   word there becomes `NaN` in every spec that reads it — which is exactly how
   `params.spec.ts` failed when `data-source` was added. Non-numeric
   inspection state lives on `[data-testid="inspection"]`.

9. **Leaving replay leaves the run paused.** Deliberate, and it is the PRD's
   rule. `setInspecting(false)` pauses and clears; resuming is the viewer's
   decision. `exitReplay` returns early when no replay is open — §7.2 is why.

10. **The recording cap is real and is 11.2 minutes at 20 Hz.** A task that
    wants a longer attempt must either lower `log_hz` or raise
    `MAX_EPISODE_BYTES` **with a stated reason** — the budget is documented in
    `docs/v2/recording-format.md` §8 and the cap is derived from it, so raising
    it is a one-line change and a paragraph, not a search through the code.

11. **`docs/v2/README.md`'s open item V-H is discharged, and this section did
    not edit the index to say so.** V-H reads "10 defines recording
    compatibility; 11 adds task metadata without a second recorder". The
    compatibility contract is `docs/v2/recording-format.md`, and the envelope
    of §2 task 10.1 is the "without a second recorder" half, reserved and
    tested. The row is left for the human to close, as sections 01 §5.2, 08 §8
    item 8 and 09 §9 item 11 each left theirs.

---

## 10. Commands

```
scripts/check.sh                                     # nine steps — §11
cargo test -p sailgym-physics --lib recording -- --nocapture
cargo test -p sailgym-physics --test invariants deterministic_replay
pnpm --dir web test:unit
pnpm --dir web exec playwright test replay-truth.spec.ts --project=chromium
```

---

## 11. The gate

`scripts/check.sh`, from this section's working tree, exit 0:

| step | | time |
|---|---|---|
| 1 | `cargo fmt --check` | 0 s |
| 2 | `cargo clippy --all-targets -- -D warnings` | 1 s |
| 3 | `cargo test -p sailgym-physics` | 61 s |
| 4 | `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance` | 42 s |
| 5 | `cargo test -p sailgym-physics --test regression` | 0 s |
| 6 | `wasm-pack build` | 9 s |
| 7 | `pnpm --dir web typecheck` | 1 s |
| 8 | `pnpm --dir web test:unit` | 1 s |
| 9 | `pnpm --dir web test:e2e` | 558 s |
| | **all steps passed** | **673 s** |

Counts: **227 Rust unit tests** (was 217), 27 invariants, 4 convergence, 8
determinism, 5 no_shortcuts, 6 provenance, 6 regression, 1 symmetry, 5 wind, 1
boom. **20 vitest files, 208 tests** (was 20 / 187). **313 Playwright tests in
20 files** across Chromium, Firefox, Edge and mobile-Chromium (was 286 in 19),
of which **27** are the new truth suite — nine tests × the three desktop
browsers. All three brief §46 demonstrations pass in all three desktop
browsers.

Browser frame budgets from the same run, unchanged budgets and no measurable
cost from the selection boundary: Chromium `svg` p50 **1.5 ms** / p95 **3.6 ms**
against 12 ms, `wasm` p50 0.2 ms / p95 **0.3 ms** against 8 ms, `frame` p95
0.7 ms, `deck` p95 0.5 ms; 0.00 % dropped frames on the sail, overlay and chart
scenes; physics independence 3.9999 vs 3.9883 sim-s/s at 60 and 20 fps, a
**0.29 %** drift.



`pwsh scripts/check.ps1` was not run — §6.7.
