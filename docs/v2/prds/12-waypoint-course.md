# v2 Section 12 — Waypoint courses: numbered overlay, course challenges and a rule-sailor baseline

**Planning revision (2026-10-02):** proposed after 07. **Status: proposed,
unblocked 2026-10-02.** The options for D1–D7 below were selected by the user
on 2026-10-02 — the recommended option in each case — and are recorded in
`../brief.md` §5. As for sections 02–07, they resolve at dispatch and are
recorded in `docs/v2/progress/12-handoff.md` §1. Selecting an option is not
evidence of implementation, and no number in this PRD marked *provisional* has
been measured.

Source: `../README.md` ("a recorded ghost and one rule sailor on a short course
are the next product slice; their web integration still needs a PRD");
`../discussions/autopilot-suggestions.md` (rule sailor, recorded ghost);
`../discussions/learning-loop.md`; `../discussions/unified-agent-interface.md`
§§5, 7; `../discussions/deferred-features.md` rows *Recorded ghost* and *Rule
sailor → polar racer → planner*.

> **Unblocked 2026-10-02.** The block was "not dispatchable until `../brief.md`
> S3's web-integration half has a recorded implementation decision". Brief §5
> now records it, with the selected option for each of D1–D7, so task 12.1 may
> be dispatched. The deltas themselves resolve **by** that dispatch, as
> sections 02–07's did, and not before it. S5 is **not** required: the ghost is
> a recording, and S5's own row says a recorded ghost needs neither `VecEnv` nor
> a live fleet. S6 stays deferred.

Read first: `../../v1/00-foundations.md` in full, `../README.md`,
`../brief.md`, `../00-foundations.md` (F14, F15, F18.3, F18.4, F19), the
dependency handoffs `../progress/04-handoff.md`, `05-handoff.md`,
`06-handoff.md`, `11-handoff.md`, then `../practice-validation.md` §3 (how this
boat tacks), then this PRD.

## Goal

A numbered waypoint course in the browser, a practice challenge that asks the
player to sail through each waypoint in order, and an easy-to-read
**rule-sailor** baseline that sails the same course under the same conditions —
watchable as a replay, raced as a ghost, and measured headless across seeds.

Almost all the machinery exists. `sailgym-course` has gates and ordered, sided,
directed passage; `sailgym-agent` has the `Agent` contract and a `guidance`
sensor; `sailgym-env` runs an agent on a route and evaluates it over seeds;
`sailgym-task` scores practice attempts; the web app has a camera, an overlay
pattern (`render/Trajectory.tsx`) and the try → inspect → retry loop. This
section adds the open-route start, the controller, the course challenge and the
browser wiring — and nothing else.

## The experience

| Step | What the player sees | Decided where |
|---|---|---|
| Choose | A **Courses** group under the three skills in the practice panel: title, one-line goal, waypoint count | `practice_tasks_json` (Rust) |
| Sail | Numbered waypoints, gate bars, leg lines; the next waypoint highlighted; an edge chevron with number and distance when it is off screen; a **Show course** camera button | `practice_state_json`'s `course` block (Rust); TypeScript only projects |
| Pass | ✓ and the split time on that waypoint. A miss: ✗ and "go back through 2" | `sailgym-task` events |
| Race | The baseline as a translucent, non-interacting ghost and its track | the baseline's **recorded** episode |
| Finish | Total time and per-waypoint splits against the baseline | task events; `compare_conditions` verdict (Rust) |
| Watch | **Watch baseline** opens the baseline's episode in the existing replay viewer, captioned with what it is doing ("beating · port tack", "tacking", "reaching to 3") | `run_baseline` (Rust) |
| Inspect / Retry | Section 11's flows unchanged; Inspect jumps to the first miss, or the finish | existing |

## Why this shape

Three mistakes this section exists to prevent.

1. **"Through the waypoint" quietly becoming a radius check.** It is the
   tempting implementation and it is RV19 in a new place. `Rounding::Either`
   is no substitute either: on a leg it requires only a directed crossing of
   the mark's **perpendicular line**, with no lateral bound at all
   (`passage.rs`'s `plane_crossed` returns `true` once the side check is
   `None`), so an `Either` waypoint 200 m off the track is "passed". A waypoint
   is therefore a **gate**: a directed crossing of a segment, which F15.3
   already defines and which F19.4's probe already turns into a miss.
2. **A baseline that is not a baseline.** One that reads privileged
   information, runs under different conditions, or is helped by a physics
   edit gives a number nobody can interpret. The rule sailor reads only
   unprivileged observation columns, runs under the attempt's frozen contract,
   and `parameters.rs` does not move.
3. **A second decision loop or a second scorer.** The agent runs inside Rust,
   through `sailgym-env`'s `Episode`, never in TypeScript and never with a
   boundary crossing per decision (brief §24). The human and the baseline are
   scored by the **same** `sailgym-task` evaluator. A cut is defined once, in
   `sailgym-course`.

## What this section does **not** do

- **No physical change.** `git diff --name-only crates/sailgym-physics/` names
  only `src/recording.rs` (D4), which gains a comparison method and no
  physics. `parameters.rs` is byte identical, F3–F9 are unchanged,
  `STATE_LEN` is still 13, and `scenarios/` is untouched.
- **No live second boat.** The ghost is a recording: no shared clock, no
  collision, no right-of-way, no wind shadow (`brief.md` §3, S5 not selected).
- **No `Helm` and no `Setpoint` action.** Task 5.6 stays deferred. The rule
  sailor is an `ActionSpace::Rates` agent with its own inner loops, which is
  what `Rates` means: it replaces tactic **and** helm (F14.2).
- **No autopilot on the player's boat.** "Autopilot takes the helm" needs the
  engaged/released contract F14.10 §2 says does not exist.
- No course editor or click-to-place waypoints, no polar racer, no planner, no
  Python exposure of the rule sailor, no reward, no training.
- No obstacles (S6) and no new gate step.
- **No course logic in TypeScript.** No passage decision, gate post, waypoint
  state or split is computed in `web/`; it projects, formats and subtracts
  (section 11's rule for `PracticePanel.tsx`, applied to the overlay).

## Normative deltas

Each is **selected (2026-10-02) and resolves at dispatch**, recorded in the
handoff §1 and then in `../00-foundations.md` in the house style of F15.5 and
F19.

### D1 — F15: a route may have an explicit start

```rust
pub struct Route {
    pub marks: Vec<Mark>,
    pub laps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Vec2>,
}
```

- With `start: Some(s)`, **leg 0 runs from `s`**. Every later leg, including
  the first leg of lap 2, runs from the previous mark as today.
- With `start: None` nothing changes. The JSON of every existing route is byte
  identical, because the field is not written. That covers
  `crates/sailgym-course/tests/replay/routes.json`, every research envelope's
  `ResearchIdentity::route`, and the Python binding's route documents.
- `Route::new(marks, laps)` keeps its signature and sets `start: None`.
  F15.1's lookahead point keeps `start: None`, so its disc branch is unchanged.
- `validate` refuses a non-finite start, and a start equal to `marks[0]`
  (leg 0 would have zero length). With a start, a single-mark route **has** an
  incoming leg, so a sided rounding or a gate is valid on it. `laps > 1` still
  needs two marks.
- `mirrored` mirrors the start.

This amends F15.5 §1 ("a course that wants a distinct start makes the start a
mark"). Both spellings remain valid. A circuit is still a circuit.

### D2 — F15: a waypoint is a gate square to its incoming leg

No change to F15.3. `sailgym-course` gains one constructor:

```rust
/// An open course from `start` through `points` in order: one gate per
/// point, `2·half_width` wide, square to the leg arriving at it, centred on it.
pub fn Route::waypoints(start: Vec2, points: &[Vec2], half_width: f64) -> Result<Route, RouteError>
```

Waypoint `i`'s posts are `pᵢ ± half_width · n̂ᵢ`, with `n̂ᵢ` the left normal of
the leg arriving at `pᵢ` (from `start` for `i = 0`). `Mark::position` is `pᵢ`.
`Mark::radius` is `half_width`, which passage ignores for gates and the overlay
draws. `laps` is 1.

**Square and centred is load-bearing.** F19.4's probe is the plane through the
mark's own position, perpendicular to the leg. For a gate square to its leg and
centred on its mark, the probe line and the gate line coincide, so "crossed the
line outside the posts" is exactly a cut (`episode.rs`'s `probe_route`
documentation says so).

### D3 — F18.4: course challenges, and a miss is not the end

A course challenge succeeds when the last waypoint is passed. A cut emits a
`waypoint_missed` event and the attempt **continues**: the player must come
back behind the gate line and cross through it, which F15.3's directed crossing
already requires. Capsize fails the attempt (`FailureReason::Capsized`, as in
11), and the time limit times it out.

**`sailgym-env` is unchanged in this respect.** F19.2's
`Terminated(MarkMissed)` still ends a research episode. The practice evaluator
and the episode runner are allowed to disagree about what a miss *costs*. They
must not disagree about what a miss *is*, which is why D2's cut is defined once.

### D4 — F18.3: same conditions, different controller

```rust
impl ExperimentIdentity {
    /// `compare` with `action` and `observation` excluded: "same conditions,
    /// different controller". Never a substitute for `compare`.
    pub fn compare_conditions(&self, other: &Self) -> Comparability;
}
```

`compare` refuses a hand-flown attempt against an agent run (`action` is
`NotApplicable` on one side and a value on the other; `recording.rs`'s own test
asserts `Different(["action"])`). That is right for section 11's two-attempt
comparison, and it stays. A baseline exists to be compared across controllers,
so it gets its own, named verdict. Splits against the baseline and the ghost
are shown only on `SameConditions` under `compare_conditions`.

### D5 — F8.2: the WASM surface grows, coarse-grained

`sailgym-wasm` gains dependencies on `sailgym-course`, `sailgym-agent` and
`sailgym-env`.

*Measured 2026-10-02:* `cargo build -p sailgym-env --target wasm32-unknown-unknown`
succeeds with the workspace's rayon 1.12. Only `VecEnv` uses rayon, and the
browser never constructs one.

| Method | Kind | Contract |
|---|---|---|
| `run_baseline(log_hz)` | new | Requires an active course attempt. Runs the rule sailor through an `Episode` under the attempt's **frozen initial contract**, with the same course challenge attached. Returns `{episode, narration, conditions}`: the recorded episode, the agent's mode changes as `[{t, mode, side, waypoint}]`, and the `compare_conditions` verdict against the attempt. One call per attempt, never per decision. |
| `episode_course_json(episode_json)` | new | The course recorded in an episode's `TaskIdentity`, or `null`. Replay draws the **recorded** course, never today's catalogue (F18.3). |
| `practice_tasks_json` | extended | Course challenges listed after the skills, each with `kind: "course"` and its waypoint count. |
| `practice_state_json` | extended | A `course` block: `waypoints: [{n, x, y, posts, state}]` with `state ∈ {passed, next, pending, missed}`, `next`, `distance_to_next` and `splits`. |
| `episode_comparability_json` | extended | Adds `conditions` (the `compare_conditions` verdict) beside the existing verdict. |

`narration` is presentation data for this run only and is **not** part of the
episode. Exporting it would need an envelope change this section does not make.

### D6 — F14: the rule sailor's home and contract

The rule sailor lives in `crates/sailgym-agent/src/pilot/`, not in a new crate,
so gate step 3's list does not change. Its contract:

- `AgentSpec { id: "rule_sailor", version: 1, action_space: Rates, cadence: Cadence::new(10) }`.
- It reads observation columns **by name** at `reset`. It never reads
  `leg_bearing_vs_wind` (privileged) or the accelerations.
- Its gains are F14.9 tunables, not coefficients. Any change to what it decides
  from the same observation bumps `version`.

### D7 — F14.1: `task → course`

The course challenge needs passage and cuts, so `sailgym-task` depends on
`sailgym-course`. Arrows become `task → {course, physics}`. There is no cycle,
because `course → physics` only. `sailgym-physics` still depends on nothing.
*D7 follows from the option selected for D3. It was not listed separately in
the planning discussion, and the handoff records it as such.*

## Waypoint passage, stated once

> Waypoint *n* is passed at the first step at which the boat crosses the
> segment of length `2w` centred on it and square to the leg arriving at it, in
> the leg's direction, with waypoint *n − 1* already passed. Crossing that
> line in the leg's direction **outside** the segment is a miss. Crossing it
> backwards is neither.

```
                       ┃ post  p + w·n̂
   prev ─────────────▶ ● p     ← through here, left to right: passed
                       ┃ post  p − w·n̂
            ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─   the same line, outside the posts: missed
```

That sentence is `passage::passed_between` on the real route and
`passage::cut_between` (new, D2) on the probe. Both live in `sailgym-course`,
and both the env and the task call them.

## The rule sailor, stated once

Five layers, each small enough to read in one sitting, each with its own tests.

| Layer | Reads | Does |
|---|---|---|
| **Navigator** | `bearing_to_target`, `cross_track`, `awa` | If the target lies outside the no-go zone **measured against the apparent wind** — the burgee, not a privileged true wind — steer at the guidance target. Otherwise beat close-hauled on the current tack. Tack when the other tack fetches the target, or when `\|cross_track\|` exceeds the corridor on the side being sailed towards, never sooner than `min_tack_interval` after the last manoeuvre. |
| **Manoeuvres** | `awa`, `yaw_rate`, elapsed decisions | A state machine: `sailing → tacking → settling → sailing`, likewise for gybing, plus `recovering` from irons on timeout. **The tack is the measured recipe of `practice-validation.md` §3: full helm and full haul together.** Helm alone never completes a tack in this model. The gybe hauls the boom in before the crossing. |
| **Helm** | `delta_r`, `yaw_rate` | Heading error and yaw-rate damping give a target rudder angle. A P-loop on `delta_r` gives the rate command, because a zero command lets the rudder self-centre (F14.10 §2). |
| **Trim** | `awa`, `l_sheet` | A sheet-length table over `\|awa\|`, tracked through the sheet rate. The table is **measured** by headless sweeps (task 12.3), not invented. v1's close-hauled measurement (2.0 m ≈ 1.78 m/s against 0.40 m/s fully hauled) is the first data point, not the table. |
| **Heel guard** | `heel`, `roll_rate` | Ease on heel, or on fast outward roll. Release past a higher threshold. Wait (hysteresis) before trimming back in. It overrides trim and never the helm. |

Named tunables, all provisional until 12.3 measures them: `no_go`,
`close_hauled_awa`, `min_tack_interval`, `corridor_half_width`, `tack_hold`,
`tack_timeout`, `irons_timeout`, the trim table, `heel_ease`, `heel_release`
and the helm gains.

There is no speed column in tier 0, and the rule sailor does not get one. It
detects a stalled boat by timeout and by `awa`, as a sailor without a log does.

`AgentDebug` reports `mode`, `side`, the target rudder angle and the target
sheet length. `run_baseline` turns mode changes into the narration. No
controller reads it (F14).

## Shipped courses

All three use `free_sail` unchanged: a uniform 5 m/s northerly, the boat at
rest at the origin heading east, sheet fully eased. Coordinates are F2 (`x`
east, `y` north), in metres. Geometry is task configuration, not physics, so
12.3/12.6 may move a waypoint and must say why. Every number below is
**provisional**.

| Course id | Waypoints from start `(0, 0)` | Teaches | `w` |
|---|---|---|---|
| `reach` | (30, 0) → (60, −10) → (90, 0) | Steering and trim only. Every leg is on port tack, with no tack and no gybe. | 5 m |
| `triangle` | (0, 40) → (30, 10) → (0, −10) | A beat, a port broad reach, a gybe at 2, then a starboard broad reach | 5 m |
| `windward_leeward` | (0, 40) → (0, 0) | A beat and a dead run back to the start | 5 m |

`w` is a little over one hull length. A course file carries `schema_version`,
`id`, `title`, `description`, `scenario`, `start`, `half_width` and `waypoints`,
and is embedded with `include_str!` the way `scenario.rs` embeds `scenarios/`.

## Tasks

### 12.1 — Contracts: open routes, the waypoint builder, cuts and the course catalogue

**Owns:** `crates/sailgym-course/src/route.rs`, `crates/sailgym-course/src/passage.rs`,
`crates/sailgym-course/src/waypoints.rs`, `crates/sailgym-course/src/catalogue.rs`,
`crates/sailgym-course/src/lib.rs`, `crates/sailgym-course/tests/waypoints.rs`,
`courses/reach.json`, `courses/triangle.json`, `courses/windward_leeward.json`,
`crates/sailgym-task/Cargo.toml`, `crates/sailgym-bench/Cargo.toml`,
`crates/sailgym-wasm/Cargo.toml`, `Cargo.toml`, `Cargo.lock`
**P-group: S**

D1, D2 and the catalogue. `passage::cut_between(route, leg_index, prev, cur)` is
the probe test of F19.4, moved here and made public, so that a cut has one
definition. `CourseId` is a `Copy` enum (`Reach`, `Triangle`,
`WindwardLeeward`), so a `TaskSpec` can carry it and stay `Copy`. Every manifest
change in the section lands here, as 05's and 06's first tasks did.

Acceptance: `cargo test -p sailgym-course`, with:

- leg 0 of an open route runs from `start`, and later laps run from the last mark;
- every existing route's JSON is **byte identical** (`--test replay` unchanged, `routes.json` round-trips unchanged);
- each new `validate` refusal in isolation;
- the builder's posts are square to the incoming leg and centred, asserted exactly on axis-aligned legs;
- table-driven `cut_between`: through the gate is a pass and no cut; outside the posts is a cut; backwards is neither; exactly through a post is a pass (inclusive, as `gate_crossed` already is);
- the mirror of an open route mirrors its start, and its passage decisions mirror;
- every catalogue course validates, names a scenario in `scenario::shipped_names()`, and starts at that scenario's initial `(x, y)`;
- every catalogue waypoint is a `Gate` (RV65).

`cargo tree -p sailgym-physics` mentions no v2 crate. `cargo build -p sailgym-wasm --target wasm32-unknown-unknown` succeeds with the new dependencies.

### 12.2 — Env: open routes, one definition of a cut, and recorded practice

**Owns:** `crates/sailgym-env/src/episode.rs`, `crates/sailgym-env/tests/course.rs`
**P-group: S**

- **The probe drops `start` today.** `probe_route` rebuilds the route with
  `Route::new(marks, laps)`, so with D1 the probe's leg 0 would run from the
  last mark while the real route's runs from the start. A boat sailing cleanly
  through waypoint 1 could then be terminated as `MarkMissed`. Replace the
  private probe with `passage::cut_between`.
- When a task is attached and `log_hz` is set, write its practice envelope and
  events into the recording through `Recorder::set_practice` and
  `Recorder::push_practice_event`, exactly as `Sim` does. A baseline episode
  then carries its waypoint events.
- Add `Episode::agent_debug()` for `run_baseline`'s narration. It is read by no
  controller.

Acceptance: `cargo test -p sailgym-env`:

- every existing `MarkMissed` test passes unchanged;
- `--test course`: a clean pass through every gate of an open route ends `Finished`, and a cut on leg 0 ends `Terminated(MarkMissed)`;
- **the named regression:** a probe that drops `start` produces a false `MarkMissed` on a clean leg-0 pass. Demonstrated able to fail, then reverted (RV66);
- the recorded practice events equal `Episode::task_events()` field for field;
- a free sail records no practice envelope;
- `no_f7_literal_appears_in_the_env_crate` still passes.

### 12.3 — The rule sailor

**Owns:** `crates/sailgym-agent/src/lib.rs`, `crates/sailgym-agent/src/pilot/mod.rs`,
`crates/sailgym-agent/src/pilot/rule_sailor.rs`, `crates/sailgym-agent/tests/rule_sailor.rs`,
`crates/sailgym-env/tests/rule_sailor.rs`, `crates/sailgym-bench/src/bin/trim_sweep.rs`
**P-group: A**

D6 and the layers above. The trim table comes from `trim_sweep`, which holds
each `|awa|` on both tacks, sweeps the sheet length and records settled speed
and heel. The measured table and its rationale go in the module documentation
**and** in `docs/v2/baseline-validation.md` §1 (handed to 12.6, which owns that
file, as a table in this task's handoff note).

Acceptance: `cargo test -p sailgym-agent rule_sailor` and `cargo test -p sailgym-env --test rule_sailor`:

- **Mirror, bit for bit, at the decision level:** over observation sequences recorded from all three courses, the mirrored sequence fed to a fresh rule sailor yields the mirrored action at every decision (rudder negated; sheet and release unchanged), with internal state mirrored. The parity of each column read is stated in a table in the test. A tolerance here is RV72 firing.
- **Privilege:** replacing `leg_bearing_vs_wind` with arbitrary values changes no action, bit for bit (RV67).
- **Missing columns:** a layout without a required column gives the all-zero action and a `missing_fields` debug note, not a panic. A panic in WASM is a dead page.
- **Unit tests per layer:** the helm's rudder sign follows F2 in all four quadrants of heading error; no tack within `min_tack_interval`; tack timeout leads to `recovering`; the heel guard eases at `heel_ease` and re-trims only after its hysteresis.
- **Closed loop:** the rule sailor finishes all three shipped courses under their browser conditions with no capsize and no miss (RV68).
- F9.7 holds with the rule sailor attached: identical decision logs under six chunkings.
- No wall clock (the existing `no_wall_clock_in_the_v2_crates` grep covers the new files), and no F7 literal in `pilot/`.

### 12.4 — Same conditions, different controller

**Owns:** `crates/sailgym-physics/src/recording.rs`
**P-group: A**

D4. A method and its tests, nothing else.

Acceptance: `cargo test -p sailgym-physics --lib recording`:

- `compare_conditions` ignores `action` and `observation`, and nothing else;
- it still refuses a differing seed, parameter, wind, initial state, scenario, task, model or `dt`, each in isolation;
- every existing `compare` test passes unchanged.

Gate step 4 is unaffected. `git diff --name-only crates/sailgym-physics/` names this file only.

### 12.5 — Course overlay and ghost, as pure presentation

**Owns:** `web/src/render/CourseOverlay.tsx`, `web/src/render/GhostBoat.tsx`,
`web/src/render/Camera.ts`, `web/tests/unit/courseOverlay.test.ts`,
`web/tests/unit/camera.test.ts`
**P-group: A**

`CourseOverlay` takes Rust-decided data through its own props interface: points,
posts, states, `next`, splits. It draws:

- numbered circles to scale, with labels at a fixed pixel size;
- gate bars, and leg lines with the current leg solid;
- states as classes with `data-testid="waypoint-n"` and `data-state`;
- the edge chevron with number and distance when the next waypoint is off screen.

`GhostBoat` draws a translucent plan-view hull and track at a pose it is
handed. Interpolating that pose reuses `sim/replay.ts`'s
`createReplaySource(episode).sampleAt(t)`, read only and display only.
`Camera.ts` gains a pure `fitBounds(points, viewport, margin)` for
**Show course**. Nothing here decides a passage.

Acceptance: `pnpm --dir web test:unit`:

- label count and states follow the props;
- the chevron sits on the correct edge in all eight octants and is absent when the waypoint is in view;
- label size is constant across zoom;
- `fitBounds` contains every point with its margin and clamps to `MIN_ZOOM`/`MAX_ZOOM`;
- `pnpm --dir web typecheck`.

### 12.6 — Course challenges, and the baseline measured

**Owns:** `crates/sailgym-task/src/lib.rs`, `crates/sailgym-task/src/course.rs`,
`crates/sailgym-task/tests/course.rs`, `crates/sailgym-bench/src/bin/course_bench.rs`,
`docs/v2/baseline-validation.md`
**P-group: B**

D3 and D7. The task side:

- `TaskSpec::WaypointCourse { course: CourseId, time_limit_s, … }`, with task ids `course_reach`, `course_triangle` and `course_windward_leeward`.
- `TASK_IDS` stays the three skills, and a new `COURSE_TASK_IDS` lists the courses. `practice_tasks_json` is therefore unchanged until 12.7 lists them, and the gate stays green between groups.
- Events: `waypoint_passed` and `waypoint_missed`, with `value` = the 1-based waypoint number. The metric is elapsed seconds.
- `TaskSpec::thresholds()` includes the course geometry under `course.*` keys. The `TaskIdentity` is then self-describing: two attempts on different geometry are refused, and replay can redraw the recorded course. No schema change, as in 11.
- `TASK_VERSION` is unchanged, because no existing rule changes meaning.

`course_bench` runs the rule sailor over two suites:

- uniform wind × 8 initial headings;
- `free_sail`'s wind in `Spatial` mode × 16 seeds.

For each it reports finish rate, mean and p90 time, capsizes, misses, rudder and sheet travel, and the sensitivity to the guidance lookahead at 10, 20 and 40 m. Time limits are set from these measurements by a rule recorded in the document, provisionally three times the baseline's browser-condition time. The document is written by `cargo run --release -p sailgym-bench --bin course_bench -- --write docs/v2/baseline-validation.md`.

Acceptance: `cargo test -p sailgym-task`:

- table-driven traces for success, miss-then-recover, capsize and timeout;
- identical outcomes and event steps under six batch sizes;
- task-on and task-off physics bit identical;
- a mark cut reported by the task coincides with one reported by `cut_between` (RV66).

The measurement document exists, states the commit and machine it was generated on, and reports, rather than targets, the suite's numbers.

### 12.7 — The WASM surface

**Owns:** `crates/sailgym-wasm/src/lib.rs`, `crates/sailgym-wasm/tests/boundary.rs`,
`web/src/sim/useSimulation.ts`, `web/src/sim/scenarioTypes.ts`, `web/src/sim/loadWasm.ts`,
`web/src/wasm/*`, `web/tests/unit/scenarioTypes.test.ts`
**P-group: C**

D5 exactly.

Acceptance:

- `run_baseline`:
  - its `conditions` verdict against the attempt is `SameConditions`, asserted for every shipped course;
  - two calls in one attempt return identical episodes;
  - it refuses, with a reason, when no course attempt is active;
  - if the attempt's frozen contract cannot be reproduced by an `Episode`, it refuses with a reason rather than running under other conditions (RV70).
- `episode_course_json` returns the recorded course for a course episode and `null` for a legacy or free-sail one.
- **Measured** wall time of `run_baseline` for the longest shipped course in Chromium on the gate machine, recorded in the handoff, within **2 s** (RV71).
- `wasm-pack build` (gate step 6), `pnpm --dir web typecheck` and `pnpm --dir web test:unit` pass.
- The schema test parses each extended JSON shape.

### 12.8 — Practice panel and world wiring

**Owns:** `web/src/App.tsx`, `web/src/ui/PracticePanel.tsx`, `web/src/ui/store.ts`,
`web/src/ui/Timeline.tsx`
**P-group: D**

The flow in *The experience*:

- the Courses group in the chooser;
- the overlay during an attempt and in replay (from `episode_course_json` and the recorded events up to the replay time);
- the baseline computed at attempt start and retry, with a "computing baseline" state when it is not instant;
- the ghost and the splits only on a `SameConditions` verdict, otherwise a plain sentence saying why;
- **Watch baseline** into the replay viewer with the narration caption, and back to the attempt;
- **Show course**;
- timeline markers at each passage.

Acceptance: `pnpm --dir web typecheck` and `pnpm --dir web test:unit`. The panel works at 360×640 and by keyboard. Section 11's three skills and free sail are reachable and unchanged.

### 12.9 — Verification, records and handoff

**Owns:** `web/tests/e2e/course.spec.ts`, `docs/v2/00-foundations.md`, `docs/v2/README.md`,
`docs/v2/brief.md`, `docs/v2/discussions/deferred-features.md`, `CLAUDE.md`,
`docs/v2/progress/12-handoff.md`
**P-group: S**

`course.spec.ts` covers:

- choose `reach`: three numbered waypoints, `next` = 1, chevron or in-view label present;
- Watch baseline: the replay shows every waypoint `passed` at the end and the outcome `succeeded`;
- back to the attempt: the ghost is visible and moves as the clock runs;
- Retry restores the same conditions;
- the same flow at 360×640;
- no console or page errors.

Record the deltas in `00-foundations.md` (an F15.6 for D1, D2 and the cut; F18.3/F18.4 notes for D3 and D4; F8.2 for D5; F14.11 for D6 and D7), in the style of F15.5. Also:

- the evidence column of brief §5's section-12 row (the row itself was written when the section was unblocked on 2026-10-02);
- the `deferred-features.md` statuses for *Recorded ghost* and *Rule sailor*;
- `courses/` in `CLAUDE.md`'s layout;
- the index row.

Acceptance: `scripts/check.sh` green, all eleven steps, with step 9 running `course.spec.ts` in Chrome, Edge and Firefox. The handoff follows F13.6.

## Section acceptance criteria

1. `scripts/check.sh` green, eleven steps. **No step added and step 3's crate list unchanged** (F19.7's five retained entries intact).
2. `cargo tree -p sailgym-physics` mentions no v2 crate. `cargo tree -p sailgym-course` mentions no `task`, `agent` or `env` (F14.1 with D7).
3. `git diff --name-only crates/sailgym-physics/` names only `src/recording.rs`. `parameters.rs` is byte identical and `scenarios/` is untouched (RV69).
4. The rule sailor finishes all three shipped courses under browser conditions. The suite in `baseline-validation.md` is reported, with any course below an 80 % finish rate on the uniform-wind suite named in the handoff as RV68 firing.
5. The waypoint-gate cut table (12.1), the probe-start regression (12.2), the decision-level mirror test and the privilege test (12.3) are each **demonstrated able to fail**, and reverted.
6. Existing JSON is byte identical: `routes.json`, research envelopes, and Python step 11 green with no change under `python/`.
7. `course.spec.ts` green in all three browsers, at desktop size and at 360×640.
8. `docs/v2/progress/12-handoff.md` per F13.6, including the measured `run_baseline` time and the measured trim table.

## Risks

| # | Risk | Mitigation | Fires when |
|---|---|---|---|
| **RV65** | Waypoint passage degrades to proximity — an `Either` mark or a disc | D2's builder emits only gates. 12.1 asserts every catalogue waypoint is a `Gate` square to its leg. | a shipped waypoint's `Rounding` is not `Gate` |
| **RV66** | The real route and the probe disagree, because a copy of the route drops `start` or re-derives the cut | `cut_between` is the one definition. 12.2's named regression and 12.6's agreement test. | `sailgym-env` or `sailgym-task` builds a probe route itself |
| **RV67** | The baseline reads privileged information | 12.3's column-perturbation test | an action changes when `leg_bearing_vs_wind` changes |
| **RV68** | The rule sailor cannot tack or run reliably in this model (`practice-validation.md` §3: helm-only tacks never complete) | The measured helm-and-haul recipe is built in, with closed-loop finish tests and the measured suite | a shipped course is not finished under browser conditions, or the uniform-wind suite is below 80 % |
| **RV69** | A coefficient is tuned to make the baseline look good (v1 brief §43) | Section criterion 3. The baseline's gains are F14.9 tunables and versioned. | any F7 value changes in this section |
| **RV70** | Human and baseline are compared under different conditions | The verdict comes from Rust's `compare_conditions`. Ghost and splits are hidden otherwise. | splits are shown beside a non-`SameConditions` verdict |
| **RV71** | The page freezes computing the baseline | Measured in 12.7, with a 2 s budget and a visible computing state | the measured time exceeds the budget |
| **RV72** | A sign error in the rule sailor shows up only as "sometimes it sails badly to port" (F11 R3) | 12.3's bit-exact decision-level mirror test | the mirror test is weakened to a tolerance |
| **RV73** | Course logic grows in TypeScript | Overlay props are Rust-decided states. 12.5 owns no passage code. | a passage, post, waypoint state or split is computed in `web/` |

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| The guidance lookahead is still the 20 m placeholder. `CourseParams` is not in `ResearchIdentity`, so changing it silently changes what an observation means. | scope; 12.6 measures sensitivity only | the section that records `CourseParams` in the identity |
| Narration is not exported with the episode | D5; exporting it needs an envelope version | the section that records agent intent |
| No speed sensor; stalls are detected by timeout and `awa` | scope; a new sensor changes RL layouts | a selected sensor study |
| Browser courses use uniform wind only; spatial wind is headless only | scope | a later course pack |
| No autopilot on the player's boat | the engaged/released contract does not exist (F14.10 §2) | task 5.6's successor |
| No course editor; no click-to-place waypoints | scope | a later UX section, reusing `Route::waypoints` |
| The rule sailor is not exposed to Python as a baseline policy | scope | a section 07 follow-on |
| Polar racer, planner | `deferred-features.md` order | after this baseline is measured |
