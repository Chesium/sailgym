# v2 Section 11 — Handoff (guided practice: try, inspect, retry)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F18.4 and F12′,
`docs/v2/practice-validation.md`, `docs/v2/recording-format.md` and
`docs/v2/progress/10-handoff.md` before starting anything that touches
practice, courses or episodes.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. `crates/sailgym-physics/` is untouched —
`git diff --stat crates/sailgym-physics/ scenarios/` is empty — so F1–F7 are
exactly as section 08 left them, `STATE_LEN` is still 13, the F8.3 snapshot
layout is unchanged, `parameters.rs` is byte identical, `recording.rs` is byte
identical, `scenarios/` is untouched and **the physics source tree id does not
move** (§5). F8.2's method set grew by five practice methods (§2, task 11.2)
and F9 is unchanged.

Status: **complete**; `scripts/check.sh` is green end to end (§12). One
pre-existing failure was found at the section's starting revision and is
recorded separately (§9). Eight things deviated from the PRD as written and §7
states each one; one item needs a human ruling (§7.1).

---

## 1. What the section is, in one paragraph

Before this, sailgym was a simulator you could sail and a replay you could
scrub, with nothing to aim at. Now there are three challenges — *get moving*,
*complete a tack*, *recover from excessive heel* — each a shipped scenario plus
a deterministic evaluator that watches **every physics step**, emits ordered
events keyed by step index, and ends with one of four outcomes. The attempt is
recorded through the envelope section 10 reserved, so the result and the
episode are the same object: **Inspect** puts the replay playhead on the event
the challenge is about, **Retry** restores the exact initial contract the
attempt was started under, and two attempts are compared only when the core's
own `ExperimentIdentity::compare` says they may be. No scoring, no threshold
and no rule lives in TypeScript, and none of it lives in `sailgym-physics`
either.

---

## 2. What landed, file by file

### Task 11.1 — three task specifications and the evaluator (P-group S, section agent)

- **`crates/sailgym-task/`** — new crate, in the workspace, depending on
  `sailgym-physics` and on nothing else. `src/lib.rs` (1380 lines) carries:
  - `TaskId` / `TASK_IDS` / `TaskSpec` / `TASK_VERSION` = 1, with
    `TaskSpec::shipped(id)` as **the only configuration the application
    offers** — the browser picks a challenge by id and Rust supplies the
    thresholds, so there is no path by which a player lowers a bar (RV61);
  - `GetMovingConfig`, `CompleteTackConfig`, `RecoverHeelConfig`, and
    `TaskSpec::validate`, which rejects a non-finite or non-positive value and
    an out-of-order pair, naming the field;
  - `StepObservation` (a **value**, not a borrow of `Simulation` — v2 F14.7's
    trap) and `true_wind_angle`, which reuses `frames::world_to_body` and
    `diagnostics::apparent_wind_angle`'s convention on the true wind;
  - `Outcome` = `Running | Succeeded | Failed(reason) | TimedOut` and
    `FailureReason` = `BackwardDrift | WrongWay | RepeatedJitter |
    LateRelease | Capsized`. **`TimedOut` carries no reason**: the events say
    what happened;
  - `TaskRun` — `start(spec, &initial)`, `observe(&obs)`, terminal for ever
    once it is terminal — with the three state machines, a `Hold` timer over
    **simulated** seconds, `envelope()` for `Recorder::set_practice`,
    `highlight()` for **Inspect**, and `report()`;
  - 6 unit tests.
- **`crates/sailgym-task/tests/practice.rs`** (1030 lines, **17 tests**) —
  three kinds, and §3 says what each measures: 14 table-driven synthetic
  traces, 9 scripted baseline runs against the real `Simulation`, and 4
  contract tests.
- **`Cargo.toml`** — `crates/sailgym-task` in `members`, `sailgym-task` in
  `workspace.dependencies`. **`Cargo.lock`** — one new package, no new
  third-party dependency.
- **`docs/v2/practice-validation.md`** — new, and the evidence behind every
  threshold: the sweeps, the numbers they were chosen from, what each one
  separates, the scripted runs the gate keeps, and the coverage table saying
  which outcome has which kind of evidence.

### Task 11.2 — runtime and recording integration (P-group A, section agent)

- **`crates/sailgym-wasm/src/lib.rs`** — `Attempt`, `Conditions`,
  `AttemptStatus`, and five methods: `practice_tasks_json`, `start_practice`,
  `retry_practice`, `cancel_practice`, `practice_state_json`. All
  coarse-grained (brief §24): the whole challenge list in one call, the whole
  attempt in one call.
  - `advance` now runs **one** loop that both evaluates and records
    (`observe_step`), so there is no second clock and no second stepping path.
    The fast path — no recorder, no attempt — is still the batched
    `inner.advance(n)`.
  - `start_recording`'s body moved into `begin_recording`, which attaches the
    practice envelope **before** the first sample, because
    `push_practice_event` refuses an event with no envelope.
  - `reset` and `restart` end an active attempt as `cancelled`;
    `set_parameter`, `reset_parameters` and `set_wind` end it as
    `conditions_changed` — and `set_parameter` only when the edit was actually
    accepted.
- **`crates/sailgym-wasm/Cargo.toml`** — `sailgym-task` added. The arrow runs
  `wasm → task → physics`.
- **`crates/sailgym-physics/src/recording.rs`** — **byte identical**, and §7.2
  says why that is the right outcome rather than an omission.
- **`web/src/sim/scenarioTypes.ts`** — `PracticeChallenge`,
  `PracticeOutcome`, `PracticeProgress`, `PracticeReport`, `PracticeStatus`,
  `PracticeState`, `readPracticeChallenges`, `readPracticeState`,
  `lastEvent`.
- **`web/src/sim/useSimulation.ts`** — `challenges` (read once at load),
  `practice` (polled every frame, re-parsed only when the text moves),
  `startPractice`, `retryPractice`, `cancelPractice`, `stopRecording`. Start
  and retry go through `armAttempt`, which clears every held input, clears the
  trajectory and starts the clock.
- **`web/src/sim/loadWasm.ts`** — documentation only (§7.3).

### Task 11.3 — prompt, result and two-attempt comparison (P-group B, section agent)

- **`web/src/ui/PracticePanel.tsx`** — new, 623 lines. Four states: chooser,
  sailing, result, attempts. Three one-line challenge selectors and the
  instruction, goal and Start of the **selected** one (§7.6 is why); a
  progress line with a hold bar while sailing; a result with one metric, the
  observed terminal event in a sentence, a collapsed event list and Retry; and
  the attempts card with Inspect, the core's verdict and the measured deltas.
- **`web/src/ui/store.ts`** — `PracticeAttempt`, `MAX_REMEMBERED_ATTEMPTS` =
  2, `attempts`, `rememberAttempt`, `forgetAttempts`. **Not persisted**: it is
  absent from `partialize` and explicitly cleared in `merge`.
- **`web/src/ui/Charts.tsx`** — `PRACTICE_SERIES`, `practiceCompareData`,
  `CompareChart`. Every point is a recorded sample; the x axis is elapsed
  **task** time so the two attempts line up from the moment each began; the y
  axis is shared.
- **`web/src/ui/Timeline.tsx`** — an optional `events` prop renders one button
  per recorded practice event, each jumping the playhead to that event's own
  recorded time.
- **`web/src/App.tsx`** — the wiring: the capture effect, the two comparison
  memos, the four handlers, `enterReplay(from?, at?)` for **Inspect**, the
  panel in the `instruments` slot, and `showTheBoat()` (§7.7).

### Task 11.4 — user-flow verification and gate integration (P-group S, section agent)

- **`web/tests/e2e/practice.spec.ts`** — new, **12 tests** × 3 desktop
  browsers = 36. §4 states what each measures.
- **`scripts/check.sh`**, **`scripts/check.ps1`** — step 3 is now
  `cargo test -p sailgym-physics -p sailgym-task`. Still **nine** steps.
- **`CLAUDE.md`** — the step-3 row, `crates/sailgym-task/` in the layout, and
  a row in the normative-documents table for the v2 documents, which the file
  did not mention at all.
- **`docs/v2/00-foundations.md`** — F12′ records step 3 as implemented and
  says what did not move with it; the delta table records F18.2, F18.3 and
  F18.4 as implemented by 09, 10 and 11; **F18.4a** records what section 11
  actually built.
- **`docs/v2/progress/11-handoff.md`** — this file.

---

## 3. The measured results — the evaluator

`docs/v2/practice-validation.md` is the full record. The headline numbers, all
from `cargo test -p sailgym-task --test practice -- --nocapture`:

### 3.1 The scripted baseline runs the gate keeps

| challenge | script | outcome | measured |
|---|---|---|---|
| get moving (`free_sail`) | haul 1.0 s | **Succeeded** t = 5.635 s, step 1127 | `speed_reached` 2.630 s at 1.2002 m/s; top speed 1.6806 m/s |
| get moving | no input | **TimedOut** 45.00 s | top speed **0.6075 m/s** |
| get moving | haul to the stop | **TimedOut** 45.00 s | top speed **1.0436 m/s** |
| complete a tack (`tack`) | helm −1 and sheet −1 for 8 s | **Succeeded** t = 15.590 s | `approach` 1.050 s · `crossing` 6.005 s · `settled` 15.590 s |
| complete a tack | helm only, sheet untouched | **TimedOut** 45.00 s | crossed at 6.665 s, never settled |
| complete a tack | helm + haul for 3 s | **Failed(WrongWay)** 39.425 s | `reversal` 17.765 s · `bore_away` 39.425 s at −120.0° |
| recover from heel (`sheet_release_recovery`) | release 4–7 s | **Succeeded** t = 7.135 s | peak heel **1.1536 rad = 66.1°** |
| recover from heel | sheet held | **Failed(LateRelease)** 7.870 s | 1.3094 rad = 75.0°, no `release` event |
| recover from heel | release from 7.5 s | **Failed(Capsized)** 9.350 s | `release` 7.510 s; peak 1.8259 rad |
| recover from heel | release at 0 / 2 / 4 / 6 s | all **Succeeded** | peaks **29.70° < 62.49° < 66.10° < 70.30°**, strictly monotone |

The last row is the section in miniature: the outcome rewards reacting at all,
and the **metric** rewards reacting sooner, so two attempts are worth
comparing.

### 3.2 The three properties the acceptance names directly

| property | measurement | result |
|---|---|---|
| batching cannot change a score (RV62) | the same script driven **1, 2, 3, 5, 10 and 37** steps per call, all three challenges | outcome, elapsed step count and every event's `(id, step, value)` identical in all six arms |
| the evaluator is an observer | each challenge's scenario run twice under one script, with and without a `TaskRun` | **13/13 state scalars bit-identical** over 4000 steps, all three challenges |
| `physics` does not depend on `task` | `cargo tree -p sailgym-physics --edges all` | neither `sailgym-task`, `sailgym-wasm` nor `wasm-bindgen` appears |
| the WASM path's own batching | `invariants::advance_batching_invariant` (unchanged, section 10's) | `advance(n)` equals `n × advance(1)`, which is what makes the attached-evaluator loop identical to the batched fast path (F9.7) |

The third row is why the two `advance` shapes in `crates/sailgym-wasm` are
interchangeable: with no recorder and no attempt the wrapper calls
`inner.advance(n)`, and with either one it calls `inner.advance(1)` n times.
F9.7 says those agree and `advance_batching_invariant` measures it, so
"task-on and task-off physics are bit-identical for the same controls" is two
existing tests rather than a new claim.

### 3.3 The finding that nearly sank the tack challenge

**The shipped `tack` scenario cannot be tacked with the helm alone.** 1 344
swept scripts — full and partial rudder, held for 2 to 12 s, with and without
counter-helm, with and without easing — either never cross head to wind or
cross and then hang at `TWA` between +5° and +25° making **sternway** at −0.2
to −0.5 m/s for forty seconds. The fastest genuine tack any of them produced
took **47 s**, nearly all of it going backwards.

A random search over eight five-second segments of `(rudder, sheet)` command
found an **11.4 s** tack, and the whole difference is the sheet: hauling in
through the turn keeps the sail — a stalled plate at a large angle of attack —
producing force through head to wind, and the surge speed bottoms out at
**0.355 m/s** instead of reversing.

**No coefficient was touched.** The challenge's script and thresholds were
chosen to match what the boat does; the boat was not changed to match the
challenge (brief §43, RV61). It is also the better lesson: *keep the sheet in
and keep turning* is what the model actually rewards.

---

## 4. The measured results — the browser

`web/tests/e2e/practice.spec.ts`, 12 tests in Chromium, Firefox and Edge.

| test | what it measures |
|---|---|
| the chooser offers exactly three challenges | three selectors, each naming a shipped scenario, each showing a goal with numbers in it before anything starts |
| goal → attempt → result → inspect → retry | the whole loop with a real mainsheet drag; the trimmed attempt **succeeds** (measured 1.7026 m/s top speed at 7.42 s in Chromium), Inspect opens the replay with the playhead on the highlight event's own recorded time, Retry leaves the replay and starts again |
| free sail and debugging stay reachable | abandoning returns the chooser and the v1 record controls; Debug Mode still opens with the charts and the panel |
| recorded event times agree with the runtime | the page's event list and the **episode's envelope** are equal object for object, and every event's `t` equals `step · dt` to 1e-9 |
| a paused clock consumes no task time | elapsed seconds and step count frozen across two wall seconds of pause, and rising again after |
| a replay cannot advance an attempt | the episode played through twice; the result object is unchanged |
| the same initial snapshot and identity | two attempts' `initial_state`, `initial_controls`, resolved catalogue, wind and seed compared object for object; the core's verdict is `same_conditions` |
| a held release button does not survive | `Space` held across a cancel; the sheet does not pay out in the new attempt |
| two attempts compare, with measured deltas | `data-comparable="true"`, a finite metric delta, both traces on one axis; and choosing a different challenge forgets both |
| a parameter edit ends the attempt | a live `sail.area` edit through the parameter panel → `conditions_changed` on the page |
| no real-boat claim | seven forbidden words absent from the chooser and from all three sailing views; the standing note present |
| keyboard at 360 × 640 | zero horizontal overflow before and during an attempt; Start and Retry reached by focus and pressed with `Enter`; the helm steers from the keyboard mid-attempt |

### 4.0 What the browser does **not** measure, and where it is measured instead

Task 11.3's "Comparison rejects differing seed, parameter, model, task version
and dt" is not asserted in the browser, and cannot usefully be: two remembered
attempts are the same challenge under a restored contract by construction, so
the rejecting cases do not arise there. They are measured in Rust, by section
10's `recording::tests::same_conditions_needs_every_field_and_a_clean_source`,
which changes the **model** (both the source id and a dirty source), the
**seed**, **`dt`**, the **initial state**, the **`task`** record and a
**parameter**, each on its own, and asserts `Different` naming that one field.
`task` is compared by whole-record structural equality, so a `TASK_VERSION`
bump alone is rejected by the same path. What the browser asserts is that the
page shows the core's verdict and shows a delta only for `same_conditions`.

### 4.1 The observed play session

Driven through the browser by the section agent, scripted rather than watched
over a human's shoulder. **It is not a user study and no completion rate is
claimed.** What it recorded:

- **1280 × 720, first load.** The `main` row is 335 px tall with 682 px of
  content; the world view fills the row exactly, so the practice panel is
  below the visible band and needs a scroll. This is **not new** — the
  recording controls, the heel indicator and the raw snapshot line have always
  been there — but it is new that something a player must press is.
  §7.7 is the mitigation.
- **The chooser reads in one pass**: three names, then one instruction and one
  goal for the selected one, then Start. Screenshot kept at
  `02-desktop-chooser.png` during the session.
- **First attempt, get moving**, trimmed by dragging down on the boat:
  `Get moving — Done · Top speed 1.66 m/s · 7.14 s of task time · 1428
  physics steps`.
- **Second attempt, over-sheeted** to the stop: `Out of time · Top speed
  1.08 m/s · 45.00 s`, and the comparison card then read *"Same conditions, so
  these two are comparable. Top speed −0.58 m/s, task time +37.86 s on the
  newer attempt."* with both traces on one axis. That is the loop working.
- **360 × 640, recover from excessive heel**, using the on-screen release
  button: `Attempt over · Peak heel 75.0° · 7.87 s`, explained as *"the boat
  still had the sheet in at 7.87 s — 75.0 °"*. Horizontal overflow: **0**.

What was awkward, honestly: on the phone the result, the goal and the attempts
card together are about two screens of scrolling; the world view and the
result are never visible at once. Nothing was measured that says how much that
matters.

---

## 5. The identity this section leaves behind

```
model_version : 2                                                  (unchanged)
source tree   : 55f3a73fd06aaa2001133f328676fd75b9c49943           (unchanged)
predecessor   : 55f3a73fd06aaa2001133f328676fd75b9c49943           (section 10's)
state         : clean
```

**The physics source tree id did not move**, because
`crates/sailgym-physics/src` was not touched — including `recording.rs`, which
task 11.2 owned and which needed no change (§7.2). Section 10 §6.1 flagged
that a schema change moves the id and invalidates comparability with every
earlier baseline; section 11 adds a whole dependent feature and moves nothing,
which is some evidence that the envelope was reserved at the right level of
generality.

`TASK_VERSION` is **1**. It is not `MODEL_VERSION` and must not be confused
with it: bumping it says *these attempts were scored under different rules*,
and `ExperimentIdentity::compare` then refuses to compare across it.

---

## 6. Parameters changed

**None.** `crates/sailgym-physics/src/parameters.rs` is byte identical,
`tests/provenance.rs` compares the same 83 F7 rows on every gate run, the two
F18.1c overrides section 08 recorded are still the only ones, and
`scenarios/` is untouched. No coefficient — physical or visual — was tuned to
make a challenge passable; §3.3 is the case where that pressure was real and
what was done instead.

### 6.1 Every number this section introduced, and where it lives

| kind | where | governed by |
|---|---|---|
| task thresholds (19 of them) | `sailgym-task`, versioned, visible, recorded with the episode | `practice-validation.md`, and a `TASK_VERSION` bump |
| `PRACTICE_LOG_HZ` = 20 | `App.tsx` | a recording rate; the evaluator runs on every step whatever it is |
| `MAX_REMEMBERED_ATTEMPTS` = 2 | `ui/store.ts` | v2 F18.4 says two |
| `COMPARE_WIDTH` 300, `COMPARE_HEIGHT` 80, two chart colours | `ui/Charts.tsx` | counts of screen pixels and a `#rrggbb`; presentation only |
| the `<details>` list's 120 px cap | `ui/PracticePanel.tsx` | a scroll cap in screen pixels |

None of them reaches the physics core. The first row is the one that looks
most like tuning and is the one `practice-validation.md` exists to justify,
threshold by threshold, against measured runs.

---

## 7. What deviated from the PRD, and what needs a human

### 7.1 The root `README.md` still spells step 3 the old way — **ruling wanted**

v2 F12′ says every site that spells the chain out must move together "or the
gate lies about itself", and names `README.md` among them. Task 11.4 owns
`scripts/check.{sh,ps1}`, `CLAUDE.md` and `docs/v2/00-foundations.md` — not
`README.md`. F13.2 is normative and says to stop and report rather than edit a
file the task does not own, so that is what this is.

The change needed is one table row, `README.md` line 194:

```diff
-| 3 | `cargo test -p sailgym-physics` | The physics core is correct **and builds on the host with no WASM toolchain**. |
+| 3 | `cargo test -p sailgym-physics -p sailgym-task` | The physics core and the practice evaluator are correct **and build on the host with no WASM toolchain**. |
```

and, in the same file's layout block at line 300, a line for
`crates/sailgym-task/`. `docs/v1/00-foundations.md` F12 (line 916) is
**deliberately** left alone: a v1 clause is amended by a recorded v2 delta and
never in place.

### 7.2 Task 11.2 wrote no `recording.rs` change — **confirmation wanted**

The task owns `crates/sailgym-physics/src/recording.rs` and the honest outcome
was to leave it byte identical. Section 10 reserved `PracticeEnvelope`,
`TaskIdentity` and `PracticeEvent`, gave `Recorder` `set_practice` and
`push_practice_event`, and made `EpisodeHeader::identity()` read the envelope's
`TaskIdentity` — so a whole dependent feature landed with **no schema change,
no version bump and no new type in the physics crate**, which is exactly what
the PRD asked for ("without adding a task-crate dependency to physics; if 10
did not provide an envelope, record a new explicit schema delta"). 10 did
provide it, so no delta was recorded. Flagged because a task with an empty
diff on a file it owns deserves to be looked at rather than assumed — this is
the third section to make that flag (09 §7.1, 10 §6.3).

### 7.3 `loadWasm.ts` took documentation only

Fourth time in four sections (01 §1, 09 §2.2, 10 §6.3). `SimHandle` **is** the
`wasm-pack`-generated declaration, so the five new `Sim` methods were typed the
moment gate step 6 regenerated them. A paragraph was added recording that this
is now a rule rather than a coincidence.

### 7.4 Two files were written that appear in no task's `Owns:` list

Both were forced, and neither is hidden.

| File | Why |
|---|---|
| `Cargo.toml` (workspace) | task 11.1 owns it; listed here only because the **workspace** manifest and the **crate** manifest are different files and the PRD's list reads as the crate's. The workspace edit is two lines and is what makes `cargo test -p sailgym-task` resolve at all. |
| `Cargo.lock` | task 11.1 owns it. One new package, no new third-party dependency. |

Neither is a deviation in substance; both are recorded because the `Owns:`
lists are how this project stays honest.

### 7.5 The 11.4 spec was written and run during group B

`web/tests/e2e/practice.spec.ts` belongs to task 11.4, which is the final
group-S task. It was written *during* group B because a UI with no browser
test is a UI nobody has run, and three defects in the panel were found by it
before the group-B gate (§8). Group order was otherwise honoured: S, full
gate, A, full gate, B, full gate, S, full gate.

### 7.6 The chooser is three lines, not three cards

The PRD says "Show one short instruction and measurable goal before starting".
The first implementation showed three cards, each with its own instruction and
goal, which added about eighteen rows to a panel that already sits below the
world view (§4.1). It now shows three one-line selectors and the instruction
and goal of the **one** that is selected — which is what the PRD's singular
actually asks for, and about five rows.

### 7.7 A scroll was added that the layout should not need

`App.tsx`'s `showTheBoat()` scrolls the world cell back to the top of the
`main` row after Start and after Retry, because pressing either leaves the
boat off the top of the scrollport (§4.1, measured). It changes no layout, no
size and nothing in the core.

**The better answer is a layout slot above the world**, and `ui/Layout.tsx` is
owned by no section-11 task, so it was not touched. Whichever PRD next owns
that file should consider one: the practice strip is the first thing in this
application that a player must *press* while watching the boat, and every
other instrument is something they only *read*.

### 7.8 No task was delegated

Groups A and B contain one task each, and group S's two tasks are the contract
and the integration. There was nothing to parallelise. Sections 01, 08, 09 and
10 recorded the same thing; it is now five sections old and the `Owns:`-list
gaps (§7.4, and 10 §6.5) are worth fixing in the next PRD's task template
together.

### 7.9 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` here, as in 01 §5.5, 08 §6.7, 09 §7.7 and 10
§6.7. **`check.ps1` *was* edited by this section**, which makes this weaker
than the previous four flags: the step-3 line was changed by the same one-line
substitution as `check.sh` and the `$Steps` array is still ten entries with
`[ValidateRange(1, 9)]` untouched, but it has not been executed. `--fast` /
`-Fast` were not touched, and §7.10 of section 09 still stands — `--fast`
skips the `mobile-chromium` project, and now also runs `practice.spec.ts` only
in Chromium.

---

## 8. Defects this section introduced, all caught before the group-B gate

### 8.1 Pressing Start scrolled the boat off the screen

`practice.spec.ts`'s mainsheet drag moved the mouse to the centre of the world
view and nothing happened. Measured: after Playwright scrolled the Start
button into view, the world view's box was at `y = −145` — the drag was
landing on the header. The test now scrolls the world back first, and §7.7
made the application do the same thing for a player.

### 8.2 Inspect was disabled for an attempt with no highlight event

An attempt that times out before anything happens has no `speed_reached`, no
`crossing` and no `heel_max`, so `highlight` was `null` and the button was
disabled — on exactly the attempt a player would most want to look at. Inspect
is now enabled whenever a replay is not already open, and `App.tsx` opens the
episode at the highlight, else the last event, else the start.

### 8.3 The practice poll could swallow an update

`useSimulation`'s frame loop compares `practice_state_json()` with the last
text it saw. The first version updated that ref **before** the `renderHz`
early-return, so a frame skipped by the throttle marked the text as seen and
never published it — the result would simply never have arrived on a throttled
page. The ref is now written at publish time. Found by reading, not by a test;
`?renderHz=` is only set by `perf.spec.ts`, which starts no attempt.

---

## 9. Pre-existing failures, recorded separately

### 9.1 `identity::rebuilding_unchanged_source_keeps_it_stable` failed at `a2d69d7`

Measured **before any section-11 edit**, with the working tree stashed and the
tree clean at `a2d69d7`:

```
the compiled source id is 99d1899974ad92919888f0487f1e540a90a5880e
but git now says 55f3a73fd06aaa2001133f328676fd75b9c49943;
build.rs did not re-run when src changed (RV52)
```

It is the **same stale build artefact** section 09 §8.1 diagnosed and section
10 did not fix: the identity is a function of git HEAD, `build.rs` declares
only `cargo:rerun-if-changed=src`, and committing section 10's work moved HEAD
without touching a file mtime. `touch crates/sailgym-physics/build.rs` re-ran
it and the test passes; that is what was done here too, and it changes no file
content.

**This is now the third consecutive section to hit it**, and it will recur for
every build that predates a commit — every CI cache, every developer who
pulls. The remedy is in `crates/sailgym-physics/build.rs` (watch the git refs
as well as the sources, or fall back to hashing `src/` directly), which is
owned by no section-11 task and was not changed. Section 09 recommended
section 10 fix it first; this handoff repeats the recommendation, more loudly.

---

## 10. Risks that fired

**RV61 — a tutorial tunes the physics. Did not fire, and §3.3 is the case
where it could have.** The tack challenge was unreachable under the obvious
technique and remains unreachable under it; the sweep found a different
technique instead of a different boat. `parameters.rs` and `scenarios/` are
byte identical, `provenance.rs` compares 83 F7 rows every gate run, and the
task thresholds live in a different crate with their own version.

**RV62 — a score that depends on browser FPS. Did not fire, and it is closed
structurally.** The evaluator runs inside the one `advance` loop, on completed
physics steps; every duration is compared against simulated seconds taken from
the observations themselves rather than accumulated from a `dt`; and
`identical_control_sequences_agree_under_six_batch_sizes` measures the
property at 1, 2, 3, 5, 10 and 37 steps per call. The browser half is the
paused-clock test: two wall seconds of pause consume no task time.

**RV63 — a retry changes the conditions. Did not fire.** `retry_practice`
writes back a frozen `Conditions` field by field — catalogue, wind
configuration, seed, complete F3 state, controls — and is deliberately **not**
`Sim::restart`, which keeps a live brief §31 edit. A parameter, catalogue or
wind change during an attempt ends it as `conditions_changed` instead. The
browser test compares two attempts' recorded headers object for object and
then asks the core, which answers `same_conditions`.

**RV64 — a misleading recovery lesson. Did not fire.** A declared capsize ends
every challenge, in the evaluator, before any other rule. The recovery
challenge's threshold — 75° with the sheet still in — fires at 7.87 s, 1.6 s
*before* the capsize is declared at 9.45 s, so the result can say *the sheet
was still in at 75° of heel* rather than only *you capsized*; and when the
boat does go over the result adds "This simulation does not right a boat that
has gone over." Nothing in the lesson text implies a capsized dinghy comes
back up.

**RV57/RV58/RV59 (section 10) — did not re-fire.** Every practice number on
the page comes through `selectInspection`'s view or through the evaluator's
report; the comparison plot reads recorded samples with
`diagnosticsFromFrame` and plots nothing a legacy episode does not carry; the
comparison verdict is the core's.

**R2 (v1) — the boat may be too tender to sail.** Relevant and not fired: all
three challenges are set on scenarios section 08 already validated, and the
one that proved hardest (§3.3) was hard for a hull-drag reason, not a
stability one.

---

## 11. What the next section must know

1. **The evaluator is the contract, and it is outside physics.**
   `sailgym-task` depends on `sailgym-physics` and nothing depends on it but
   the wrapper. `sailgym-course` and `sailgym-env` (v2 F15, F14.1) should
   **consume `Outcome` and `PracticeEvent`**, not define their own completion
   semantics — that is what F18.4 asks for and what makes a course's "mark
   passed" and a task's "settled" the same kind of thing.

2. **A task threshold is not a coefficient, and the crate boundary is the
   distinction.** Thresholds may be tuned, with a `TASK_VERSION` bump and a
   line in `practice-validation.md`. `parameters.rs` may not (brief §43). v2
   F14.9 says the same thing about controller gains. If those three ever end
   up in one crate, the distinction stops being checkable.

3. **Evaluate on steps, never on frames or on an interpolated pose.**
   `PracticeEvent.step` exists for this, `StepObservation` is a value so that
   nothing can reach the per-`advance` force cache (v2 F14.7), and the
   batching test is what keeps it true. Section 10 §9 item 3 said the same
   thing from the replay side.

4. **`Sim::restart` keeps a live parameter edit; `Sim::retry_practice` does
   not.** They are different on purpose. Anything that means "put this back
   exactly as it was" must freeze resolved values, not re-resolve a document.

5. **The envelope did not need to grow, and should not.**
   `EPISODE_SCHEMA_VERSION` is still 2 and `PRACTICE_ENVELOPE_VERSION` still
   1. A second recorder, a second `Episode` type or a per-feature header field
   is what `docs/v2/recording-format.md` §6 exists to prevent.

6. **The practice panel is below the world view and that is a layout problem,
   not a panel problem.** §7.7. The first thing a player must *press* while
   watching the boat now lives in a slot designed for things they only *read*.

7. **`build.rs` still has RV52.** §9.1. Third section running. Fix it before
   section 02 keys a conformance bundle on the same id.

8. **The three challenges are one scenario each, and there are three
   scenarios left over.** `close_hauled`, `gybe` and `beam_reach_capsize` carry
   no challenge. `close_hauled` is where **backward drift** is reachable —
   over-easing from close-hauled reaches `u = −0.44 m/s` — which is the one
   shipped failure rule with only synthetic evidence
   (`practice-validation.md` §2.3, §5). A fourth challenge on `close_hauled`
   would close that gap cheaply.

9. **`docs/v2/README.md`'s open item V-H is discharged, and this section did
   not edit the index to say so.** V-H reads "10 defines recording
   compatibility; 11 adds task metadata without a second recorder". Both
   halves hold: §7.2 is the evidence that no second recorder was added. The
   row is left for the human to close, as sections 01 §5.2, 08 §8 item 8, 09
   §9 item 11 and 10 §9 item 11 each left theirs. **Four rows are now
   waiting** — V-D, V-G, V-H and (with this section) the gate half of V-E.

10. **M-next's four sections are done.** §13 lists 08–11's acceptance criteria
    against what was measured, which is what the PRD asks the release handoff
    to carry.

---

## 12. The gate

`scripts/check.sh`, from this section's working tree, exit 0:

| step | | time |
|---|---|---|
| 1 | `cargo fmt --check` | 0 s |
| 2 | `cargo clippy --all-targets -- -D warnings` | 0 s |
| 3 | `cargo test -p sailgym-physics -p sailgym-task` | 63 s |
| 4 | `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance` | 41 s |
| 5 | `cargo test -p sailgym-physics --test regression` | 0 s |
| 6 | `wasm-pack build` | 10 s |
| 7 | `pnpm --dir web typecheck` | 0 s |
| 8 | `pnpm --dir web test:unit` | 1 s |
| 9 | `pnpm --dir web test:e2e` | 631 s |
| | **all steps passed** | **746 s** |

Counts: **227 Rust unit tests** in `sailgym-physics` (unchanged) plus
**6 unit + 17 integration** in the new `sailgym-task`; 27 invariants, 4
convergence, 8 determinism, 5 no_shortcuts, 6 provenance, 6 regression, 1
symmetry, 5 wind, 1 boom — all unchanged. **20 vitest files, 208 tests**
(unchanged). **349 Playwright tests in 21 files** (was 313 in 20), of which
**36** are the new practice suite — twelve tests × the three desktop browsers.
All three brief §46 demonstrations pass in all three desktop browsers.

Browser frame budgets from the same run, unchanged budgets and no measurable
cost from the per-frame practice poll: Chromium `svg` p50 **1.6 ms** / p95
**4.6 ms** against 12 ms, `wasm` p50 **0.2 ms** / p95 **0.3 ms** against 8 ms
— the extra `practice_state_json()` call per frame does not move it — `frame`
p95 0.7 ms, `deck` p95 0.5 ms; 0.00 % dropped frames on the sail, overlay and
chart scenes; physics independence 3.9999 vs 4.0116 sim-s/s at 60 and 20 fps,
a **0.29 %** drift.

The browser's own successful `get_moving` attempts from this run, one per
desktop browser, all through the real mainsheet drag:

```
succeeded · top speed 1.8086 m/s · 7.36 s of task time · 1472 physics steps
succeeded · top speed 1.7827 m/s · 7.80 s of task time · 1560 physics steps
succeeded · top speed 1.7026 m/s · 7.42 s of task time · 1484 physics steps
```

They differ because the drag is a wall-clock gesture and each browser reaches
the target rope length a few physics steps apart — which is the *player's*
input differing, not the evaluator: the same control sequence gives the same
answer at the same step, and §3.2 is the measurement of that.

```
scripts/check.sh
cargo test -p sailgym-task --test practice -- --nocapture
pnpm --dir web test:e2e --project=chromium practice.spec.ts
```

`pwsh scripts/check.ps1` was not run — §7.9, and it **was** edited this time.

---

## 13. M-next release: 08–11 against their acceptance criteria

The PRD asks the release handoff to list 08–11's criteria, schema
compatibility and hardware test coverage. Each section's own handoff is the
detail; this is the roll-up.

| section | criteria | evidence |
|---|---|---|
| **08** physics consistency | **passed**, per `progress/08-handoff.md` §11 | `physics-validation.md`; four-harmonic `GZ`, unilateral sheet, geometric sheet stop, `ModelIdentity`. Two items still want a human ruling (08 §6) |
| **09** touch and readability | **passed**, per `progress/09-handoff.md` §10 | one input-composition path; measured rates 1.500000 / 6.000000 m/s and 2.090000 rad/s against the catalogue; four viewports with no overflow. **Hardware coverage outstanding** (09 §9 item 9) |
| **10** trustworthy replay | **passed**, per `progress/10-handoff.md` §11 | one display selection; the adversarial truth suite; schema 2 with `Recorded<T>`; canonical `ExperimentIdentity`. One item wants a ruling (10 §6.1) |
| **11** guided practice | **passed**, §§3, 4, 12 above | three challenges, the evaluator in the gate, the browser loop in three browsers. Two items want a ruling (§7.1, §7.2) |

**Schema compatibility.** `EPISODE_SCHEMA_VERSION` is **2** and
`SUPPORTED_SCHEMA_VERSIONS` is `[1, 2]`; section 11 changed neither.
`IDENTITY_VERSION` is 1, `PRACTICE_ENVELOPE_VERSION` is 1,
`SCENARIO_SCHEMA_VERSION` is 1. A schema-1 episode still decodes, still
scrubs, still re-encodes as schema 1, and is still `Indeterminate` against
everything including itself — the checked-in fixtures
(`crates/sailgym-physics/tests/fixtures/episode-schema1.{json,bin}`) assert it
on every gate run. A schema-2 episode written before section 11 has
`practice: null` and therefore `task: NotApplicable`, which is the honest
answer and not a gap.

**Hardware test coverage.** Still **outstanding**, and no claim in any of the
four handoffs rests on one. `mobile-chromium` (`devices['Pixel 5']`, 393 × 851
at DPR 2.75) is emulation: it gives trusted multi-touch, real pointer capture
and a real mobile viewport, and it does not give a digitiser, a finger, palm
rejection, a mobile GPU or retractable browser chrome. Two remedies remain
unverified for that reason — the safe-area padding (`env()` is 0 in the
emulator) and the `100svh` root against a returning address bar (there is no
chrome to retract). Section 11 added nothing that needs a digitiser: the
practice panel is buttons and text, and it was measured at 360 × 640 with zero
horizontal overflow and the whole flow reachable by keyboard.

**What M-next does not claim.** No real-boat accuracy, no validated
hydrodynamics, no completion-rate target, and no user study. The play session
in §4.1 is one scripted walk-through recorded as evidence, not a measurement
of how easily anybody else would find their way.
