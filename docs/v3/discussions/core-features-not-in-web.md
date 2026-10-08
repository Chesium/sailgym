# Core features the web front end cannot show, control or reach

Written 2026-10-08 against `f4c3ff2` (`feat: v2-12`, task 12.1 only) and
**revised the same day, after v2 section 12 shipped in full**. This is an
**inventory, not a plan**. It lists what the Rust crates (and, through them,
Python) can do that the browser cannot display, drive or get to, where each
item is reachable today, and whether a PRD already plans its browser half. It
is starting material for v3 scoping. Selecting any of it still needs a scope
row and an F8.2 delta, recorded the way section 12's D5 was.

**What the revision changed.** Section 12 closed most of §§1–3's *browser*
gaps, but it closed them **for one controller on one shape of course**, which is
a narrower thing than the crates can do and is worth being precise about. The
rows below now distinguish three states rather than two: reachable from the
browser, reachable **only through the baseline** (the page can watch the
research machinery run, and cannot configure it), and headless. §7's latent
defect is fixed and a **second** one, found by section 12's own work, is
recorded beside it.

Every claim below was checked against the source. File and symbol names are
given so they can be re-checked.

---

## 0. The boundary today

The browser reaches Rust through one object, `Sim`, in
`crates/sailgym-wasm/src/lib.rs`. It has **38** methods — section 12 added
`run_baseline` and `episode_course_json` — and `web/src` calls 37 of them. The
exception is `recording_full`, which `loadWasm.ts` mentions only in a comment
(see §5). The front end therefore uses nearly all of its interface; what it
cannot reach is mostly what was never put behind that interface.

`sailgym-wasm` **now calls all five pure crates**: `sailgym-physics`,
`sailgym-task`, `sailgym-course`, `sailgym-agent` and `sailgym-env`. The last
three were named in its `Cargo.toml` by task 12.1 and are used by `src/lib.rs`
since task 12.7 (section 12, D5, recorded as F18.5).

They are reached through **one** method. `run_baseline(log_hz)` builds a
`sailgym_env::Episode` from the active attempt's frozen contract, attaches the
course challenge, runs `sailgym_agent::pilot::RuleSailor` to the end and hands
back the recorded episode, a narration and a `compare_conditions` verdict —
one call per attempt, never one per decision (brief §24). Nothing else about an
episode, an agent or a route is configurable from the page.

That remains deliberate. F8.2 enumerates the WASM API, and each research
section recorded its missing browser surface as a tracked debt:

- section 04 §11.6: "the web app cannot set a route" — **still true**; the page
  can draw the three shipped courses and cannot author one;
- section 05: "no agent can be selected from the browser" — **still true**; the
  page can run *the* rule sailor and cannot choose a controller;
- sections 06 and 07: "no episode can be started from the browser" — **partly
  closed**; the page starts exactly one kind of episode, the baseline's, and
  cannot open a `ResearchEnvelope`.

| Crate | Browser | Python (`sailgym`) | Native only |
|---|---|---|---|
| `sailgym-physics` | **most of it** — §5 lists the exceptions | through the env | `sailgym-bench` binaries |
| `sailgym-task` | **the three skills and the three courses**, nothing configurable | no | tests |
| `sailgym-course` | **the three shipped courses, drawn**: numbered gates, posts, leg lines, passage states, splits. No route authoring, no sided or multi-lap mark, no `CourseParams` control | a route as `Spec(route_json=…)` | tests |
| `sailgym-agent` | **the rule sailor, through `run_baseline` only**: its episode, its mode narration. No controller choice, no observation view, no cadence or sensor control | the sensor list and the observation layout; the only actor is the external `Manual` source | `env_bench`, `trim_sweep`, `course_bench`, tests |
| `sailgym-env` | **one episode shape, through `run_baseline`**: a baseline on a shipped course, recorded and replayable. No `VecEnv`, no `ResearchEnvelope`, no reward, no autoreset, no decision log on the page | `SailgymEnv`, `SailgymVectorEnv` | `env_bench`, `course_bench`, tests |

In one sentence: **the browser is a hand-flown physics simulator with practice
challenges, an honest replay, and one baseline it can race against. Everything
*configurable* about a course, a controller or an episode is still headless.**

---

## 1. Courses — `sailgym-course` (section 04, task 12.1)

| Feature | Where | Notes |
|---|---|---|
| `Route { marks, laps, start }`, `Mark { position, radius, rounding }` | `route.rs` | `Route::validate`, `Route::mirrored` (port/starboard mirror) |
| `Rounding::{Port, Starboard, Either, Gate(a, b)}` | `route.rs` | sided marks and directed gate crossings. The only proximity test in the crate is the disc that a single mark with no incoming leg arrives at — F15.1's "sail at this point" |
| Multi-lap circuits | `route.rs` | `laps > 1` needs two or more marks |
| **Open routes** — leg 0 runs from an explicit start | `Route::with_start` | 12.1, D1; the JSON of every existing route is unchanged |
| **Waypoint builder** — one gate per point, square to its incoming leg, centred on the point | `Route::waypoints(start, points, half_width)` in `waypoints.rs` | 12.1, D2 |
| **Course catalogue** — `CourseId::{Reach, Triangle, WindwardLeeward}` | `catalogue.rs`, `courses/*.json` | all three on `free_sail`, 5 m gates, geometry marked *provisional* |
| Passage — ordered, sided, directed | `passage::{passed, passed_between, advance}` | |
| Cuts — crossing the gate line outside the posts | `passage::{cut, cut_between}` | 12.1; meant to be the **one** definition (RV66), see §7 |
| Progress tracking, step-indexed | `progress::{Tracker, Progress, Passage}` | must be driven at the physics rate (section 04 §11.3) |
| Guidance — signed cross-track (+ to port), bearing and distance to target, lookahead point | `guidance::{guidance, guidance_at, CourseParams}` | lookahead `DEFAULT_LOOKAHEAD = 20 m` is still a placeholder |

**In the browser, since section 12:** the three shipped courses are drawn —
numbered circles to scale, gate bars, leg lines with the current one solid, an
edge chevron with the number and distance when the next waypoint is off screen,
and **Show course** to fit the camera to the whole thing. Passage states, the
next waypoint, the distance to it and the splits all arrive decided from Rust in
`practice_state_json`'s `course` block; a replay redraws the **recorded** course
from the episode's own identity (`episode_course_json`), never today's
catalogue.

**Still not in the browser:** authoring a route (no editor, no click-to-place);
sided (`Port`/`Starboard`) and multi-lap marks — the page only ever sees
`Rounding::Gate` on a one-lap open route; `Mark::radius` as a *clearance* rather
than a gate half-width; `CourseParams::lookahead`, which is not displayed and
cannot be changed; and `Guidance` itself — the player sees the next waypoint and
its distance, and never the cross-track error or the lookahead point the
controller steers at.

**Reachable today from:** Python, as `Spec(route_json=…)` on an episode, where
a route turns into `Finished { time }` or `Terminated(MarkMissed)`. Also from
Rust tests, including the replay validation over the six committed goldens.

**Delivered by section 12** (2026-10-08):

- 12.5 — `CourseOverlay` (numbered waypoints, gate bars, leg lines, an
  off-screen chevron) and `Camera.fitBounds` for **Show course**;
- 12.6 — `TaskSpec::WaypointCourse` and the `waypoint_passed` /
  `waypoint_missed` events;
- 12.7 — the `practice_state_json` `course` block and `episode_course_json`;
- 12.8 — the Courses group in the practice panel.

**Not planned anywhere:**

- a course editor or click-to-place waypoints (a listed section-12 debt, still
  open);
- sided-mark and multi-lap courses in the browser, since section 12 ships gates
  only;
- anything that exposes or tunes the guidance lookahead.

---

## 2. The agent interface — `sailgym-agent` (section 05)

| Feature | Where | Notes |
|---|---|---|
| Tier-0 sensor suite, **18 columns** | `sensor/` | see the table below |
| Per-column privilege flag | `FieldSpec::privileged` | exactly one column, `guidance.leg_bearing_vs_wind`, because it is derived from the true wind |
| Observation layout and its canonical record | `observation::{ObsLayout, observe, obs_digest}`, `LAYOUT_VERSION` | `obs_digest` is the canonical JSON of the layout, not a hash |
| Action funnel `[−1, 1]^3 → Controls` | `actuation::{apply, rate::Rate}` | release is the **sign** of the third scalar, so the all-zero action is hands-off |
| `Agent` trait, `AgentSpec`, `Cadence`, `AgentDebug` | `spec.rs` | an agent sees only `&[f64]`; it cannot reach the wind, the route or the state (F14.4) |
| Deterministic agent randomness | `STREAM_AGENT`, per-sensor substreams | sensors take a `&mut Pcg32`, but no noise model ships |
| `Manual` external source | `manual.rs` | **the only `Agent` implementation that ships**; every other `impl Agent` is a test stub |

| Sensor | Columns |
|---|---|
| `imu` | `roll_rate`, `yaw_rate`, `heel`, `accel_surge`, `accel_sway` |
| `apparent_wind` | `awa`, `aws` (the vane sits at the sail's CE height) |
| `rig_state` | `beta`, `beta_dot`, `l_sheet`, `sheet_slack` |
| `actuator_state` | `delta_r`, `rudder_rate_cmd` |
| `guidance` | `cross_track`, `bearing_to_target`, `distance_to_target`, `leg_bearing_vs_wind` (privileged), `rounding_side` |

**In the browser, since section 12:** one controller, through one call. The
rule sailor (`src/pilot/rule_sailor.rs`) runs inside `run_baseline`, and the
page shows its **mode narration** — "beating · port tack · to 1", from
`AgentDebug` — captioned over the replay and listed in the panel. The whole
agent stack is exercised behind that call: the five tier-0 sensors, the
observation layout, the `[−1, 1]^k` funnel, the `rate` adapter and the cadence.

**Still not in the browser:** the player's own inputs go straight to
`Sim::set_controls` and never pass through the agent funnel, so a human run and
an agent run still cannot be compared on what each one *saw*; no observation
vector is displayed; no controller can be **chosen**; the cadence, the sensor
list and the tunables are fixed; and `AgentDebug`'s other notes — the target
rudder angle and the target sheet length the rule sailor is holding — cross the
boundary and are not drawn.

**What does not exist yet, anywhere** (these are not just hidden):

- **the rule sailor** — task 12.3; `crates/sailgym-agent/src/pilot/` does not
  exist;
- `Helm` / `ActionSpace::Setpoint` — task 5.6, deferred. The name is reserved
  and `AgentSpec::validate` refuses it;
- sensor noise, bias, latency and dropout models;
- the polar racer and the rollout planner.

**Delivered by section 12** (2026-10-08): the rule sailor (12.3), run inside
one `run_baseline` call (12.7), shown as a ghost and a narrated replay (12.5,
12.8). That is the first time any agent runs in the browser build, and it
remains the only one.

**Still not planned anywhere:** a live view of the observation vector, or
picking a sensor suite or an agent from the page.

---

## 3. The episode runner — `sailgym-env` (section 06)

| Feature | Where | Notes |
|---|---|---|
| `Episode` = scenario + sensors + route + bounds + step budget + autoreset + reward + optional task observer | `episode.rs` | `EpisodeConfig` |
| `Outcome::{Running, Finished { time }, Terminated(reason), Truncated}` | `outcome.rs` | termination and truncation are never merged (RV34) |
| `TerminationReason::{Capsized, OutOfBounds, MarkMissed}` | `outcome.rs` | capsize is the simulator's own accumulator, read and not recomputed |
| Sailing area | `Bounds::{Unbounded, Rect}` | |
| Autoreset conventions | `AutoresetMode::{NextStep, SameStep}` | proven equivalent on discounted returns, to the bit |
| Rewards | `Reward` trait, `ZeroReward` | no reward function ships |
| Complete decision log | `DecisionLog`, `decision_log::replay` | every `(step, action)` pair; replays bit for bit |
| Research envelope | `ResearchEnvelope`, `ResearchIdentity` | `to_json` / `from_json`, `comparable_with`, `resimulate_actions_against` |
| Sampled recording inside an episode | `EpisodeConfig::log_hz`, `Episode::take_envelope` | wraps an ordinary `sailgym_physics::recording::Episode` |
| Batched independent episodes | `VecEnv` | rayon; bit-identical to serial at every N and thread count |
| Evaluation over seeds × routes | `evaluate::{EvalSuite, evaluate, EvaluationReport}` | |

**In the browser, since section 12:** one episode shape. `run_baseline` builds
an `Episode` with a route, a task and a recorder, runs it to a terminal
`Outcome`, and the page shows the outcome word, the course time and the splits.
A cut inside that episode is `Terminated(MarkMissed)` and the page says the
baseline did not finish.

**Still not in the browser:** the player cannot start an episode of their own;
`VecEnv`, the reward hook, the autoreset conventions, the decision log, the
bounds and `ResearchEnvelope` are all unreachable; and the one episode the page
does run is not configurable — its scenario, route, task and seed are the
attempt's frozen contract and nothing else.

**Reachable today from:** Python, through `SailgymEnv` / `SailgymVectorEnv`
(section 07), and `env_bench` for throughput. **Three parts are reachable from
nowhere but Rust code that builds an `EpisodeConfig` itself:**

- **The sampled recording and the research envelope.** No shipped caller sets
  `log_hz` (`env_bench` sets it to `None`), the Python `Spec` takes no
  `log_hz`, and nothing outside `sailgym-env`'s own tests calls
  `take_envelope` or `ResearchEnvelope::viewing`.
- **The decision log, as data.** Python can switch it on (`log_decisions=True`)
  but has no getter to read it back.
- **A practice task as an observer.** `EpisodeConfig::task` exists, but the
  Python `Spec` does not take one.

The consequence: **an episode run by a policy cannot be watched in the replay
viewer today.** The viewer already decodes the physics `Episode` that an
envelope wraps. What is missing is a way to produce the envelope and a way to
open one.

**Delivered by section 12** (2026-10-08):

- 12.2 makes an episode write its practice envelope and events into the
  recording — and log its **first** sample, so its own header's initial
  condition is honest (§7.2);
- 12.7's `run_baseline` runs an `Episode` inside one WASM call and hands its
  recording to the page — the first browser path into `sailgym-env`, used for
  the baseline only.

**Not planned anywhere:**

- opening a research envelope written by Python or Rust in the replay viewer;
- showing `Outcome`, the termination reason or decision markers on the
  timeline;
- `evaluate` reports in any UI.

---

## 4. The practice evaluator — `sailgym-task` (section 11)

**In the browser:** the three skills **and**, since section 12, the three
waypoint courses — their thresholds as `practice_tasks_json` reports them (a
course's geometry included, under `course.*`), live progress, events, the final
report, **Inspect** at the highlight event (for a course: the **first** miss, or
the finish), **Retry** under the frozen contract, and the two-attempt
comparison. This crate is the one research-adjacent crate that is fully wired.

**Not reachable from the browser:**

- **Custom thresholds.** `GetMovingConfig`, `CompleteTackConfig` and
  `RecoverHeelConfig` are public and validated, but the page can only ask for
  `TaskSpec::shipped(id)`. This is deliberate (RV61: no path by which a player
  lowers a bar) and should stay that way unless a "custom drill" mode is scoped
  with its own identity rules.
- **Challenges on the other three scenarios.** `close_hauled`, `gybe` and
  `beam_reach_capsize` carry none. Section 11 §11.8 notes that a
  `close_hauled` challenge would give the backward-drift failure rule its first
  non-synthetic evidence.
- **Course challenges.** Not implemented yet (12.6).

---

## 5. Physics features with partial browser exposure

| Feature | In Rust | In the browser |
|---|---|---|
| **Wind spectrum** | `WindConfig` has eight fields: `mode`, `speed`, `bearing_deg`, `variation`, `length_scale`, `time_scale`, `modes` (K), `spectral_slope` | `mode` in the header; `speed`, `bearing_deg` and `variation` in the parameter panel (`BRIEF_31_WIND`). **`length_scale`, `time_scale`, `modes` and `spectral_slope` cannot be edited** except by editing a scenario file |
| **Wind seed** | `Simulation::new(params, seed)`; scenarios carry a seed; `Sim::new` accepts `seed` | **No control.** The seed changes only by switching scenario, so a gust pattern cannot be re-rolled or chosen |
| **Integrator** | `Integrator::{SemiImplicitEuler, Rk2Midpoint, Rk4}` (`Rk4` is the convergence reference) | **Not selectable.** `sim.integrator` is a string, `leafPaths` skips it, `set_parameter` never accepted it, and scenario `parameter_overrides` are `f64`-only. The only route is a full `parameters` document passed to `new Sim(…)`, which no UI does |
| **Model curves** | `GzCurve::{sample, gz, dgz}`, `foil::{cl, cd}` over the whole ±180°, `mainsheet::{rope_path_length, drope_dbeta, min_rope_path}` | Only the **current operating point**: the `gz` scalar, the righting-moment overlay, the sail angle-of-attack chart. Editing `stability.*` in the panel shows no curve, so the four-harmonic fit, and why `validate` refuses some edits, stay invisible |
| **Arbitrary scenarios** | `Scenario::load` validates any document, and `Sim::reset` accepts one (and the older ad-hoc `{seed, state, wind}` shape) | Only the six shipped scenarios. There is no "load scenario file", no "save the current state as a scenario", and no "start a run from this replay frame", though the episode header carries everything that last one needs |
| **Recording cap reached** | `Recorder::due()` turns false once `MAX_EPISODE_FRAMES` (13 443) is reached; `Sim::recording_full()` reports it | The cap is shown **before** recording (`readRecordingLimit`), but `recording_full` is never called. When the cap is hit, the "■ Stop (n)" counter simply stops rising, and nothing says the recording has ended. A one-line wiring gap, the cheapest item on this page |
| Model identity, experiment identity, comparability | `identity::ModelIdentity`, `ExperimentIdentity::compare` | **Reachable**: the identity badge and the attempt comparison |
| Diagnostics, parameter catalogue, `delta_r_self_centre` and other boolean parameters | `diagnostics`, `parameters` | **Reachable**: Debug Mode and the generated parameter panel |
| Conformance digest, `GzRepresentation`, `.npy` codec | behind the `testkit` feature | Not in the browser build, **by design** — verification artifacts, not a gap |

---

## 6. Headless by design — not candidates for the web

- **`python/sailgym`** — the Gymnasium binding is, today, the only interactive
  way to drive the agent and env stack. A reward is a Python callable over the
  observation at the end of the decision period.
- **`python/sailgym_jax`, `python/sailgym_conformance`** — a verification port
  of the wind field and the bundle loader. Agreement checks, not product
  features.
- **`sailgym-bench`** — `bench`, `convergence`, `gen_golden`,
  `gen_conformance`, `vec_bench`, `env_bench`. Measurement and fixture
  generators.

---

## 7. Two latent defects found this way, both now fixed

### 7.1 The env probe dropped `Route::start` — fixed by task 12.2

`crates/sailgym-env/src/episode.rs` built its missed-mark probe with a private
`probe_route`, which called `Route::new(marks, laps)` and so **dropped
`Route::start`**. Since 12.1 a route can carry an explicit start, and nothing on
the env side took it into account.

Measured, after it was fixed, by
`crates/sailgym-env/tests/course.rs::a_probe_that_drops_the_start_disagrees_about_leg_0`,
which reconstructs the old probe and compares it with `passage::cut_between`
over a real `free_sail` track. It is **worse than this note first guessed**, in
two directions:

- **A false positive.** On a *single*-waypoint open course the dropped-start
  probe has no incoming leg at all, so `Route::leg(0).from` is `None` and
  `Either` degrades to arrival at the mark's own **disc** — a radius check,
  which is what F15.3 forbids and RV65 names. It fires while the boat is still
  short of the gate line: on the measured track, 300 steps before the crossing.
  A boat sailing straight at its only waypoint was terminated as `MarkMissed`.
- **A false negative.** On the shipped `reach` course the probe's leg 0 ran
  *west*, from the last waypoint to the first, where the real leg 0 runs east.
  A genuine eastward cut of waypoint 1 — 12 m to port of it, outside the posts —
  went **unreported**, so the episode sailed on with the mark missed.

**Fixed:** `passage::cut_between` is now the one definition, in
`sailgym-course`, and it builds its probe from the leg the real route computed,
so the start and the laps travel with it by construction (F15.6 §3). The
regression was demonstrated able to fail and reverted.

### 7.2 A recorded episode's header said it started one step late — fixed by task 12.2

Section 10's `Recorder::observe` fills the header's `initial_state` and
`initial_controls` from whichever sample arrives **first**. `Sim::begin_recording`
has always logged one sample as the recording starts; `Episode::begin` did not,
so an `Episode`'s first sample was the one its advance loop logged — the state
after **one physics step**. A recorded research episode therefore said in its own
header that it had started somewhere it had not, and
`ExperimentIdentity::compare` refused it against the identical conditions
recorded in the browser.

- **Who could hit it:** anyone comparing a `sailgym-env` recording with any
  other recording of the same conditions — which is exactly what a baseline is
  for. Nothing before section 12 did that, which is why it had never shown up.
- **How it surfaced:** `run_baseline`'s `compare_conditions` verdict came back
  `different`, with reasons `initial_state, initial_controls`, on a baseline
  built from the attempt's own frozen contract. RV70's own check had already
  passed, because the *state* was right and only the *record of it* was wrong.
- **Fixed:** `Episode::begin` logs its initial sample, so a recorded episode's
  frame 0 is the state it was started from (F15.6 §4).

---

## 8. What section 12 closed, and what it left

Section 12 shipped on 2026-10-08. Every row below is now a fact rather than a
plan; `docs/v2/progress/12-handoff.md` is the record and
`docs/v2/baseline-validation.md` the measurement.

| Gap | Closed by | Left open |
|---|---|---|
| No course in the browser | 12.5, 12.7, 12.8 | **course editor; sided and multi-lap marks; the lookahead placeholder; `Guidance` itself (cross-track, lookahead point) is still not displayed** |
| No agent in the browser | 12.3, 12.7 | **choosing an agent; an observation view; the rule sailor's rudder and sheet targets; `Helm` / autopilot on the player's boat** |
| No episode in the browser | 12.7 (`run_baseline`, the baseline only) | **starting an episode of your own; opening a `ResearchEnvelope`; `VecEnv`; reward; the decision log and the outcome on the timeline; bounds** |
| No course challenge | 12.6 | **challenges on `close_hauled`, `gybe`, `beam_reach_capsize`; configurable thresholds** |
| Same conditions, different controller | 12.4 (`compare_conditions`) | — |
| Env probe drops `start` (§7.1) | 12.2 | — |
| An episode's header started one step late (§7.2) | 12.2 | — |
| Narration not exported with the episode | — (a listed 12 debt) | needs an envelope version bump |
| Browser courses use uniform wind only | — (a listed 12 debt) | spatial and gust wind on courses. The headless suite measures `Spatial` and the browser does not offer it |
| Rule sailor not exposed to Python | — (a listed 12 debt) | a section-07 follow-on |
| `CourseParams` not in `ResearchIdentity` | — (a listed 12 debt) | 12.6 measured the sensitivity; recording it is the repair |
| Wind spectrum, seed, integrator, curves, scenario import (§5) | — | all of it |
| Recording cap reached is never shown (§5) | — | all of it |

**Two things section 12 measured that a v3 scope should start from.**

1. **The rule sailor finishes all three courses under browser conditions, and
   its uniform-wind finish rate from arbitrary initial headings is 100 % on
   `reach` and 38 % on the two upwind courses** — RV68 fired and is named in the
   handoff. Four of the five non-finishes are a cut of waypoint 1 by 0.7–3.6 m
   outside a 10 m gate; the fifth is an exactly head-to-wind start, which a
   bit-exact mirror-symmetric controller cannot escape without breaking its own
   symmetry. A better beat, a wider gate or a lay-line rule are three different
   v3 answers and they are not the same decision.
2. **The suite measures a stricter thing than the challenge does.** It runs the
   research runner, where a cut ends the episode (F19.2); the practice challenge
   lets the attempt continue (F18.4b). A v3 reading of either number has to say
   which rule it was taken under.

---

## 9. Candidate v3 work — suggestions, not decisions

Ordered by value for the "sail, inspect, retry" direction, against what each
one costs at the boundary. Every item must respect the standing rules: no
physical equation in TypeScript (F8), coarse-grained boundary calls (brief
§24), an F8.2 delta per new method, no course logic in `web/` (RV73), and
replay showing **recorded** values, never a fresh evaluation of the model now
loaded (section 10).

0. **Call `recording_full`** (§5). The page should say when a recording has
   stopped at its cap. It is a few lines in `RecordControls.tsx`, needs no new
   boundary method, and could be done in any section.
1. ~~Finish section 12 as written, 12.2 first.~~ **Done** (2026-10-08). What it
   left is §8's "Left open" column, and the most valuable items in it are the
   two the measurement points at: a beat that lays the mark, and a course the
   player can author.
2. **A research-episode viewer.** Open a `ResearchEnvelope` in the existing
   replay viewer, with the outcome and termination reason, decision markers on
   the timeline, and the route drawn by 12.5's `CourseOverlay`. This is the
   *inspect* half of the loop for policies, and most of the machinery exists.
   - **Needs:** `log_hz` and an envelope getter on the Python `Spec`/env (or a
     native CLI that writes envelopes); one WASM decode method;
     `docs/v2/recording-format.md` extended to cover the envelope.
3. **An observation inspector in Debug Mode.** Show the tier-0 observation
   vector with units, bounds and the privileged flag. The live view is one
   coarse method over `sailgym_agent::observe`.
   - **Caveat:** in replay it can show only observations that were
     **recorded**. Recomputing them from a recorded state would break section
     10's rule, so the replay half needs the observation in the recording, or
     it reads `Not recorded`.
4. **A complete wind panel.** Add `length_scale`, `time_scale`, `modes`,
   `spectral_slope` and the seed, with a "re-roll seed" button. It uses the
   existing `set_wind` / scenario path, and an attempt in progress ends as
   `conditions_changed` exactly as it does today.
5. **Model-curve plots in the parameter panel.** Draw `GZ(φ)` from
   `GzCurve::sample` beside the `stability.*` rows, and CL/CD(α) beside each
   foil, from one coarse `curves_json`. This makes a refused edit explainable
   at a glance.
6. **Scenario import and export.** Load a scenario file, save the current state
   as a scenario, and start a run from a replay frame. `Sim::reset` already
   accepts the document, so this is mostly UI and error messages. A started
   run must carry its own identity, not the replay's.
7. **Later, and blocked on contracts that do not exist yet:**
   - an autopilot on the player's boat — needs task 5.6's engaged/released
     contract;
   - live fleets (S5) and obstacles (S6);
   - a selectable integrator — a reference tool, and arguably better left out
     of the product page.
