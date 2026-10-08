# v2 Section 12 — handoff: waypoint courses, course challenges and a rule-sailor baseline

Completed **2026-10-08**, after section 07, from `f4c3ff2` (`feat: v2-12`, which
had landed task 12.1 alone). Written per F13.6.

> **Read this first if you are the next section.** Three things here will bite
> you. §4.2 is a measured risk that **fired** (RV68) and is not a defect to go
> and fix blind. §9.1 is a gate property of this repository, not of this
> section: a section that touches `crates/sailgym-physics/src` **cannot see a
> green step 3 or step 9 until its work is committed**, and the proof in §9 was
> taken in a throwaway committed worktree because of it. §10 lists four
> `Owns:`-list gaps this section had to cross, each with the file and the
> reason.

---

## 1. The normative deltas, resolved at dispatch

Section 12's seven deltas were **selected by the user on 2026-10-02** (the
recommended option in each case) and recorded in `brief.md` §5. They resolved
**by this section's dispatch on 2026-10-08**, exactly as sections 02–07's did,
and are now written into `docs/v2/00-foundations.md` — which this section's
task 12.9 owns, so unlike section 07 there is no clause left for someone else to
carry.

| Delta | Subject | Where it now lives |
|---|---|---|
| D1 | F15: a route may have an explicit `start` | **F15.6 §1** |
| D2 | F15: a waypoint is a gate square to its incoming leg | **F15.6 §2** |
| D3 | F18.4: course challenges, and a miss is an event | **F18.4b** |
| D4 | F18.3: `compare_conditions` | **F18.3a** |
| D5 | F8.2: `run_baseline`, `episode_course_json`, three extended outputs | **F18.5** |
| D6 | F14: the rule sailor's home and contract | **F14.11 §§1–8** |
| D7 | F14.1: `task → course` | **F14.11 §9** |

The selected S row is `brief.md` §2's **S3, its web-integration half**. **S5 was
not selected** — the ghost is a recording, with no shared clock, no collision,
no right-of-way and no wind shadow — and **S6 stays excluded**: there is no
`obstacle.rs` and no `raycast.rs`, and F15.5 §6 holds.

Validation performed: the eleven-step gate (§9),
[`baseline-validation.md`](../baseline-validation.md) (the measured trim table
and both suites), `course.spec.ts` in Chrome, Edge and Firefox at desktop size
and at 360 × 640, and four tests demonstrated able to fail and reverted (§5).

---

## 2. What landed

### 2.1 Task 12.1 — contracts (landed in `f4c3ff2`, before this session)

`Route::start`, `Route::waypoints`, `passage::cut_between`, the `courses/`
catalogue and the three course documents, with `crates/sailgym-course/tests/waypoints.rs`.
Verified in place: `cargo test -p sailgym-course` is green, every existing route
document is byte identical, `cargo tree -p sailgym-physics` mentions no v2 crate
and `cargo build -p sailgym-wasm --target wasm32-unknown-unknown` succeeds with
the new dependencies.

### 2.2 Task 12.2 — env: one definition of a cut, and recorded practice

`crates/sailgym-env/src/episode.rs`:

- the private `probe_route` is **gone**; `passage::cut_between(tracker.route(), …)`
  is the one definition (F15.6 §3, RV66);
- when a task is attached and `log_hz` is set, the practice envelope is attached
  before the first sample and every event the evaluator appends is pushed into
  the recording, in order — the same shape `Sim::observe_step` has;
- `Episode::agent_debug()` for `run_baseline`'s narration, read by no
  controller;
- **and one thing the PRD did not ask for**: `Episode::begin` now logs its
  initial sample. See §3.2.

`crates/sailgym-env/tests/course.rs`, five tests: a clean pass through four
gates of an open route finishes; a cut on leg 0 terminates as `MarkMissed`, **on
the step the geometry says**; the named regression (§5.2); the recorded practice
events equal `Episode::task_events()` field for field including `step`; a free
sail records no practice envelope.

### 2.3 Task 12.3 — the rule sailor

`crates/sailgym-agent/src/pilot/{mod.rs, rule_sailor.rs}`, five layers, plus
`crates/sailgym-bench/src/bin/trim_sweep.rs`. Ten tests in
`crates/sailgym-agent/tests/rule_sailor.rs` and five in
`crates/sailgym-env/tests/rule_sailor.rs`. See §3.1 for the three design
changes the measurements forced, and §4 for the numbers.

### 2.4 Task 12.4 — `compare_conditions`

`crates/sailgym-physics/src/recording.rs`: one private marker enum, `compare`
delegating to a shared `compare_fields`, and `compare_conditions`. **One field
list, one fixed order**; the two entry points differ by a flag at the last two
notes. Two tests, 19 → 21 recording unit tests. `git diff --name-only
crates/sailgym-physics/` names only this file (criterion 3, RV69).

### 2.5 Task 12.5 — the overlay and the ghost

`web/src/render/CourseOverlay.tsx`, `web/src/render/GhostBoat.tsx`,
`Camera.fitBounds`, with `web/tests/unit/courseOverlay.test.ts` (11 tests) and
three new `fitBounds` tests. The component is a thin map over exported pure
functions, because `web/tests/unit/` runs under vitest in a `node` environment
with no React renderer and this section added no dependency.

### 2.6 Task 12.6 — course challenges, and the baseline measured

`crates/sailgym-task/src/course.rs` and the `TaskSpec::WaypointCourse` wiring in
`src/lib.rs`; `crates/sailgym-task/tests/course.rs` (10 tests);
`crates/sailgym-bench/src/bin/course_bench.rs`; and
[`docs/v2/baseline-validation.md`](../baseline-validation.md), generated.

### 2.7 Task 12.7 — the WASM surface

`crates/sailgym-wasm/src/lib.rs`: `run_baseline`, `episode_course_json`, and the
three extended outputs. `Sim` goes from 36 methods to 38. Five new
`wasm-bindgen-test`s in `tests/boundary.rs`. On the TypeScript side,
`scenarioTypes.ts` gains the shapes and four readers, `useSimulation.ts` gains
`runBaseline` and `episodeCourse`, and `scenarioTypes.test.ts` gains five
shape tests. `loadWasm.ts` **needed no change**, for the fifth time: `SimHandle`
**is** the generated declaration.

### 2.8 Task 12.8 — the panel and the world

`App.tsx` (the overlay live and in replay, the ghost, the baseline with its
computing state, **Watch baseline** with the narration caption, **Show
course**), `PracticePanel.tsx` (the Courses group, the course readout, the
`Baseline` section with splits against the player's), `Timeline.tsx` (markers
labelled `passed 2` / `missed 2` from the event's own `value`). `store.ts` was
**not** changed: a baseline belongs to one attempt and one page, and the store
holds view state that survives a remount.

### 2.9 Task 12.9 — verification and records

`web/tests/e2e/course.spec.ts` (10 tests × 3 browsers), the five foundations
clauses, `brief.md` §5, `README.md`'s index row, `deferred-features.md`,
`CLAUDE.md`'s layout, and this note. Also, at the user's request,
`docs/v3/discussions/core-features-not-in-web.md` and the repository
`README.md`.

---

## 3. What deviated from the PRD, and why

### 3.1 The rule sailor's manoeuvre logic is not the PRD's sketch

The PRD's navigator says: *"Tack when the other tack fetches the target, or when
`|cross_track|` exceeds the corridor on the side being sailed towards, never
sooner than `min_tack_interval` after the last manoeuvre."* That was
implemented, measured on the three shipped courses, and **did not work**. Three
changes followed, each from a measurement:

1. **`reset` starts `min_tack_interval` at zero, not in the distant past, and
   the interval is 300 decisions (15 s).** The first implementation tacked 6 s
   into the `triangle` beat with 1.47 m/s of way on. A separate sweep —
   `trim_sweep`'s settle-then-tack probe — measured that a tack from a settled
   beat completes in **5.9–11.2 s above about 1.6 m/s and does not complete at
   all below it**. So the interval that separates two manoeuvres also has to
   separate the start of an episode from the first one.

2. **A `tack_margin` (11.5°) and a `tack_arc_max` (110°) were added; one new
   tunable replaced the PRD's "the other tack fetches".** Without the margin the
   boat tacked for two degrees of advantage and lost the whole beat. Without the
   arc limit it turned **156° through the wind's eye at 0.37 m/s** rather than
   204° round through the run — a limit cycle that never reached waypoint 2 of
   `triangle`. Bearing away through the run always works in this model, because
   the sail fills throughout; a tack does not.

3. **The helm holds close-hauled while a tack is pending, instead of steering at
   the target.** The first implementation steered at an upwind target it could
   not lay and **luffed from 1.74 m/s to 0.82 m/s** waiting for an interval that
   had not run — so the tack, when allowed, failed. This is the whole of RV68 in
   one line of control logic.

**The corridor rule is kept and is measured to be inert on the shipped
courses.** Sweeping `corridor_half_width` over 6, 8, 10, 12, 14 and 18 m changed
not one outcome or time in the uniform × 8 suite: the tack that happens is
always the one the target crossing the wind's eye asks for, which fires as the
boat crosses the leg line and therefore before any corridor does. What bounds
the zigzag is `min_tack_interval`. It is kept because the PRD's layer table
specifies it and a longer leg will need it; it is recorded here as **not** the
thing doing the work.

A fourth change is smaller but worth naming: **the gybe needed a second local
`wrap_pi`**, because `frames::wrap_pi`'s slow path is not exactly odd and the
mirror test is bit-exact. See F14.11 §7.

### 3.2 One change outside any task's stated scope: the episode recorder's first sample

`Episode::begin` now logs one sample as the recorder is created. This was not in
the PRD, and it had to be done for `run_baseline` to mean anything. The symptom,
the diagnosis and the fix are in F15.6 §4 and in
`docs/v3/discussions/core-features-not-in-web.md` §7.2. In one sentence: section
10's `Recorder::observe` fills the header's initial condition from whichever
sample arrives first, and an `Episode`'s first sample used to be the state after
one physics step — so a recorded research episode's own header said it had
started somewhere it had not, and `ExperimentIdentity::compare` refused it
against the identical conditions recorded in the browser.

The file is `crates/sailgym-env/src/episode.rs`, which task 12.2 owns, so this
is inside the section's write scope. It changes the **frame count** of every
recorded `sailgym-env` episode by one and makes its header honest; no test
asserted the old count.

### 3.3 Three controller gains are written at a value chosen to avoid an F7 collision

`helm_kd` is 0.6 rather than 0.55, `trim_rate_k` 5.5 rather than 6.0 and
`roll_rate_ease` 0.8 rather than 0.85. Each of the three first values collides
with an unrelated number in `parameters.rs` (`stability.gm`,
`sheet.sheet_release_rate`, `sail.oswald`), and the collision would have cost
`no_f7_literal_appears_in_the_pilot_module` an exemption entry. A gain has no
preferred value — nothing measured picks one over its neighbour — and F17.6 §1
calls an empty exemption table the intended steady state. Recorded here rather
than left to be noticed. **No F7 value changed** (RV69).

### 3.4 Where the PRD's D5 sketch was short

D5's `course` block lists `waypoints: [{n, x, y, posts, state}]`. Two more
fields are needed and are emitted: **`radius`**, because the numbered circle is
drawn to scale and halving the post separation would be TypeScript deciding
course geometry (RV65/RV73); and **`start`**, because leg 0 runs from
`Route::start` and without it the first leg line has no beginning. `half_width`
is emitted too, for a page that wants to say how wide a gate is.

### 3.5 `TaskId` gained a variant that carries data

So its `PartialOrd`, `Ord`, `Serialize` and `Deserialize` are written out rather
than derived — deriving would have demanded `CourseId: Ord` and a serde shape
nothing asked for. All four go through `as_str`/`parse`. The three skills'
serialised form is byte for byte section 11's.

### 3.6 `Machine` is no longer `Copy`

`CourseState` owns a `Tracker`, which owns a `Route`. The three skill states are
still `Copy` and are still read out by value, through a reference. `CourseState`
also implements `PartialEq` by hand, because `Tracker` does not and this section
may not add a derive to `sailgym-course` (F13.2) — two course runs are equal when
they have got to the same place by the same passages.

---

## 4. The measurements

### 4.1 The trim table, and the baseline under browser conditions

`cargo run --release -p sailgym-bench --bin trim_sweep` — 11 target angles ×
41 sheet lengths × both tacks, 40 s holds, last quarter averaged. The two tacks
agreed to within **5.1e-5 m/s** of settled speed at every point, which is a
measured number rather than an asserted symmetry. The table is the midpoint of
the admissible plateau (within 2 % of the best speed, heel under 30°) and is
reported **as measured**: it decreases once, at 90.6° → 109.1°, and was not
smoothed. Full table in [`baseline-validation.md`](../baseline-validation.md) §1
and in `rule_sailor.rs`'s own documentation.

Under the browser's own conditions, at the default 20 m lookahead:

| course | baseline | peak heel | `time_limit_s` |
|---|---|---|---|
| `reach` | **42.50 s** | 28.4° | 130 s |
| `triangle` | **105.50 s** | 35.5° | 320 s |
| `windward_leeward` | **91.80 s** | 35.5° | 280 s |

No capsize, no miss, and the limits are 3 × the baseline rounded up to the next
five seconds — a rule recorded in the document, not a number anybody chose.

### 4.2 RV68 **fired**, and here is exactly what fired

Section acceptance criterion 4 asks for any course below an **80 %** finish rate
on the uniform-wind suite to be named here.

| course | uniform × 8 headings | spatial × 16 seeds | below 80 %? |
|---|---|---|---|
| `reach` | **100 %** (8/8) | **100 %** (16/16) | no |
| `triangle` | **38 %** (3/8) | 81 % (13/16) | **yes — RV68 fired** |
| `windward_leeward` | **38 %** (3/8) | 81 % (13/16) | **yes — RV68 fired** |

Traced heading by heading, the five non-finishes on `triangle` are **not** a
controller that cannot sail:

- **four cuts of waypoint 1** (headings 45°, 135°, 225°, 315°): the boat beats
  the 40 m leg and crosses the gate's line 5.7–8.6 m from its centre against
  posts 5 m either side — outside by 0.7 to 3.6 m. It sailed the leg and arrived
  off centre;
- **one drift** (heading 0°): the scenario puts the boat exactly head to wind at
  rest, an exactly mirror-symmetric state. The controller is exactly
  mirror-symmetric — its decision-level mirror test is bit for bit — so it has
  no side to choose and does not invent one. That is the other face of RV72's
  property, not a defect in the layers, and breaking it would mean breaking the
  symmetry.

**And the suite measures a stricter thing than the challenge does.** It runs the
research runner, where F19.2 makes a cut `Terminated(MarkMissed)` and the
episode ends. The practice challenge is D3: a cut emits `waypoint_missed` and the
attempt continues, so a boat that arrived 1 m wide comes back through. The finish
rate above is a lower bound on what a player sees the baseline do. Three
different v3 answers — a beat that lays the mark, a wider gate, a lay-line rule —
are three different decisions, and none of them is this section's to take.

### 4.3 RV71: the measured `run_baseline` wall time

Measured in the browser by `course.spec.ts`, on the longest shipped course
(`triangle`, 105.5 s of simulated time), on the gate machine:

| browser | `run_baseline(course_triangle)` | budget |
|---|---|---|
| Chromium | **13 ms** | 2 000 ms |
| Edge | **16 ms** | 2 000 ms |
| Firefox | **9 ms** | 2 000 ms |

(Taken from the full gate run of §9.2. A standalone `--grep "§12"` run of the
same three browsers measured 12 / 23 / 5 ms, so the figure is stable to within
about 15 ms and is three orders of magnitude inside the budget either way.)

Three orders of magnitude inside the budget. The "computing baseline" state
`App.tsx` publishes is therefore invisible in practice and is kept anyway: it
costs a `setTimeout(0)` and it is what makes the page honest on a slower machine
or a longer course.

### 4.4 Lookahead sensitivity

Over the uniform × 8 suite, at 10, 20 and 40 m: the finish **rate** moves by at
most one run per course (`triangle` 4/8 at 10 m against 3/8 at 20 and 40 m), and
the mean time of the finishers moves a lot — `reach` 63.6 s / 88.2 s / 73.1 s.
The debt is unchanged: `CourseParams` is still not in `ResearchIdentity`, so
changing it silently changes what a `guidance` observation means. This measured
the sensitivity; it did not repay the debt.

---

## 5. Demonstrated able to fail, then reverted

Section acceptance criterion 5 names four. Each was broken deliberately, the
failure recorded, and the file restored.

1. **The waypoint-gate cut table (12.1).** Shipped in `f4c3ff2`;
   `crates/sailgym-course/src/passage.rs`'s own table-driven tests cover
   through-the-gate, outside-the-posts, backwards and exactly-through-a-post.
2. **The probe-start regression (12.2).** Reinstating section 06's
   `probe_route` in `episode.rs` made **two** tests red:
   `a_probe_that_drops_the_start_disagrees_about_leg_0` with *"a boat sailing
   straight at its only waypoint was terminated: Terminated(MarkMissed)"*, and
   `a_cut_on_leg_0_of_an_open_route_terminates_as_mark_missed` with
   `Truncated` where a cut was expected — the false positive and the false
   negative, both measured.
3. **The decision-level mirror test (12.3).** Making the heel guard read the
   **signed** heel instead of `|φ|` — so the boat protects itself only when it
   heels to starboard — failed with *"triangle: decision 320: the sheet command
   must be unchanged"*.
4. **The privilege test (12.3).** Letting `Sensed::read` add
   `0.0 * guidance.leg_bearing_vs_wind` to `awa` failed with *"reach:
   decision 3, scalar 0: the baseline read a privileged column"*.

Additionally, task 12.4 demonstrated its own two tests able to fail — by
excluding `seed` from `compare_conditions`, and by making it compare the
controller fields after all — and task 12.5 demonstrated two
(`label.px` made zoom-dependent; the chevron's octant comparison inverted).

---

## 6. Risks from the F11 and section-12 registers

| Risk | Fired? | Note |
|---|---|---|
| **RV65** waypoint passage degrades to proximity | **yes, and it was the old code** | §5.2's false positive: section 06's probe degraded to the mark's own disc on a single-waypoint open route. Every shipped waypoint is a `Gate`, asserted in `tests/waypoints.rs` |
| **RV66** real route and probe disagree | **yes, and it was the old code** | fixed by one definition (F15.6 §3) |
| RV67 the baseline reads privileged information | no | the column is never resolved; the perturbation test is bit-exact |
| **RV68** the rule sailor cannot tack or run reliably | **yes** | §4.2. Browser conditions pass on all three; the uniform suite is 38 % on the two upwind courses |
| RV69 a coefficient tuned to make the baseline look good | no | `git diff --name-only crates/sailgym-physics/` names only `src/recording.rs`; `parameters.rs` is byte identical; `scenarios/` and `courses/` are untouched |
| RV70 human and baseline compared under different conditions | no | the verdict is Rust's; `run_baseline` refuses before the first step if the contract cannot be reproduced; the ghost and the splits are gated on `same_conditions` |
| RV71 the page freezes computing the baseline | no | §4.3: 5–23 ms against 2 000 ms |
| RV72 a sign error shows up only as "sometimes it sails badly to port" | no | the mirror test is bit for bit, with one documented carve-out at the sign of zero. It needed a second local `wrap_pi` to be achievable at all (F14.11 §7) |
| RV73 course logic grows in TypeScript | no | the overlay's props **are** `practice_state_json`'s block; the one derived thing a replay does is read recorded events |
| F11 **R3** sign-convention drift | watched | the whole of the rule sailor is signs, which is why its mirror test is exact and why no `signum` is used on a quantity that can be zero |
| F11 **R2** the boat is too tender to sail | watched | the trim table's close-hauled entries settle at 24–29° of heel, so the heel guard eases at 35° rather than 30°, and the measured peak on the shipped courses is 35.5° |

---

## 7. Parameters

**None changed.** `crates/sailgym-physics/src/parameters.rs` is byte identical,
`scenarios/` is untouched, and `courses/` is untouched since task 12.1. The
only file under `crates/sailgym-physics/` that changed in the whole section is
`src/recording.rs`, which gained `compare_conditions` and its tests and no
physics.

Every number this section introduced is either an F14.9 controller tunable
(`rule_sailor.rs`, with provenance per field), a task threshold
(`course.rs`'s time limits, derived by a recorded rule from a measurement), a
piece of course geometry (`courses/*.json`, task 12.1) or a visual constant
(`CourseOverlay.tsx`, `GhostBoat.tsx`, `App.tsx`'s `SHOW_COURSE_MARGIN_M`, each
with a "presentation only" note and a reason).

---

## 8. The gate

**No change at all**, which was the point. No step was added, step 3's crate
list is unchanged, F19.7's five retained entries are intact and
`[ValidateRange(1, 11)]` was not touched. The rule sailor lives in
`sailgym-agent` and the course challenge in `sailgym-task` precisely so that it
could — `README.md`'s V-I row records the discharge.

---

## 9. The gate run, and the one thing that makes it awkward

### 9.1 A section that touches physics source cannot see a green gate uncommitted

F16.9 §8 records this for section 02 and it applies here unchanged. F18.1d makes
a **dirty** physics source id comparable with nothing, including itself, so with
`crates/sailgym-physics/src/recording.rs` modified and uncommitted **three**
tests are red:

- `sailgym-env`'s `recording::tests::incompatible_identities_refuse_an_action_resimulation`
  (step 3);
- section 11's two browser tests that assert `data-verdict="same_conditions"`
  (step 9);

and, new to this section, **five of `course.spec.ts`'s ten** — every one that
reads `run_baseline`'s `conditions` verdict, because that verdict is
`compare_conditions` and `compare_conditions` compares the model.

That is the contract working as written. The proof below was therefore taken the
way section 02's was: the same source, committed in a **throwaway git worktree**,
with its own `node_modules` and its own `target`.

### 9.2 The measured runs

**`scripts/check.sh`, the whole chain, in the committed worktree:
all eleven steps green, exit 0, in 865 s.**

| # | Step | Result |
|---|---|---|
| 1 | `cargo fmt --check` | ok, 1 s |
| 2 | `cargo clippy --all-targets -- -D warnings` | ok, 6 s |
| 3 | `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent -p sailgym-env` | ok, 83 s |
| 4 | the six audit targets | ok, 42 s |
| 5 | `--test regression` | ok, 0 s |
| 6 | `wasm-pack build` | ok, 11 s |
| 7 | `pnpm --dir web typecheck` | ok, 1 s |
| 8 | `pnpm --dir web test:unit` | ok, 1 s — **227 passed**, 21 files |
| 9 | `pnpm --dir web test:e2e` | ok, 701 s — **379 passed**, 0 failed, Chrome + Edge + Firefox |
| 10 | `uv run ruff check python && ruff format --check` | ok, 8 s |
| 11 | `scripts/py-test.sh` | ok, 11 s — **69 passed**, no change under `python/` |

Step 9's 379 includes `course.spec.ts`'s ten tests in each of the three
browsers, at desktop size and at 360 × 640, with no console or page errors —
the suite fails the whole run on either (`fixtures.ts`). Step 8's 227 includes
`courseOverlay.test.ts`'s eleven, the three new `fitBounds` tests and the five
new `scenarioTypes` shape tests.

Step 11 is **unchanged**: 69 passed, and `git diff --name-only python/` is
empty for the whole section (criterion 6). Step 4 is unaffected by the one
physics edit, because the conformance bundle's key covers the declared
`model_version` and every fixture's data digest and deliberately not a source
tree id (F16.9 §3).

Existing JSON is byte identical (criterion 6): `git diff --stat courses/
scenarios/ crates/sailgym-physics/src/parameters.rs crates/sailgym-course/` is
empty, so `routes.json`, the research envelopes and the six scenario documents
are all untouched.

### 9.3 What is red in the working tree, and why — measured both ways

`scripts/check.sh` against the **uncommitted** working tree **fails at step 3,
exit 101**, on one test: `sailgym-env`'s
`recording::tests::incompatible_identities_refuse_an_action_resimulation`,
42 passed / 1 failed, with
`Indeterminate(["recording.model"])` where `SameConditions` was expected. The
chain fails fast, so it does not reach step 9's seven other dirty-source
failures; those were measured separately by a `--grep` run before the worktree
was built (section 11's two `data-verdict="same_conditions"` tests, plus five of
`course.spec.ts`'s ten).

Steps **1, 2, 4, 5, 6, 7, 8 and 10 are green in the working tree too**, which is
what says the failure is the identity contract and not the work: in particular
step 4's `provenance::docs_match_source` and
`shipped_values_match_the_f7_table` both pass **after** this section's edits to
`docs/v2/00-foundations.md`, so the F18.1c override block was not disturbed.

The two trees were diffed to make the proof transferable: **every source file,
test, scenario, course document and generated artefact is byte identical between
the working tree and the committed worktree §9.2 measured**; the only
differences are the documentation files task 12.9 wrote after the snapshot
(this note included), and the one gate step that reads any of them is step 4,
re-run above.

There is no action for the next section here beyond committing this work.

---

## 10. `Owns:`-list gaps this section had to cross

Four files needed a change that no section-12 task's `Owns:` list names.
Recorded here rather than absorbed, in the shape sections 05 and 06 used.

1. **`web/tests/e2e/practice.spec.ts`** — its
   *"the chooser offers exactly the three shipped challenges"* test counted
   every `practice-challenge-*` row and asserted three. Task 12.6 says in as
   many words that `practice_tasks_json` is unchanged *"until 12.7 lists
   them"* — which is the change that assertion was pinning. The edit counts
   `[data-kind="skill"]` rows instead and adds a second assertion for the three
   courses; **no other assertion in that file changed**, and the comment at the
   site says so.
2. **`web/src/sim/episodeIo.ts`** — holds `ComparabilityVerdict`, which needed
   an optional `conditions` field for D4's second verdict. **Not edited.**
   `scenarioTypes.ts` (task 12.7's) declares `IdentityVerdict`,
   `EpisodeComparison` and `compareEpisodesWithConditions` instead, with a doc
   comment pointing here. The repair — one optional field on the existing type,
   and `compareEpisodes` returning it — belongs to whoever owns that file next.
3. **`crates/sailgym-agent/src/lib.rs`** — a `pub mod pilot;` line. Rust has no
   way for a later task to declare a module without touching the crate root.
   Task 12.3's `Owns:` names it, so this one is **covered**; it is listed here
   only because sections 04, 05, 06, 10 and 11 each recorded the same shape and
   the pattern is now five sections old. A cleaner answer would be for a crate
   root to be a shared file by convention.
4. **`crates/sailgym-task/src/lib.rs`'s `TaskId::scenario`** — a course
   challenge's scenario is its **document's**, and this method returns
   `&'static str`. Widening the signature would reach into
   `crates/sailgym-wasm/src/lib.rs`, which group B does not own, and would have
   left the workspace uncompilable between groups B and C. The three literals
   are therefore a second copy, and
   `tests/course.rs::every_course_challenges_scenario_is_its_documents` loads
   all three documents and asserts the two agree — F19.6's device, for F19.6's
   reason.

---

## 11. Deliberate debts, as the PRD tracks them

Every row of the PRD's debt table stands, unchanged, plus one:

| Debt | Repaid by |
|---|---|
| The guidance lookahead is still the 20 m placeholder; `CourseParams` is not in `ResearchIdentity` | the section that records `CourseParams` in the identity. §4.4 measured the sensitivity |
| Narration is not exported with the episode | the section that records agent intent (needs an envelope version bump) |
| No speed sensor; stalls are detected by timeout and `awa` | a selected sensor study. §3.1 shows what it costs: the "1.6 m/s to tack" rule is enforced by a **decision counter** standing in for a speed the controller cannot see |
| Browser courses use uniform wind only | a later course pack. The headless suite measures `Spatial`; the browser does not offer it |
| No autopilot on the player's boat | task 5.6's successor |
| No course editor; no click-to-place waypoints | a later UX section, reusing `Route::waypoints` |
| The rule sailor is not exposed to Python as a baseline policy | a section-07 follow-on |
| Polar racer, planner | after this baseline is measured — which it now is |
| **New: the corridor rule is measured inert on the shipped courses** (§3.1) | a course with legs long enough to need it, or a lay-line rule that replaces it |

---

## 12. What the next section must know

1. **RV68 fired, and §4.2 says precisely what fired.** Do not "fix the rule
   sailor" without reading it: four of the five failures are a 0.7–3.6 m gate
   miss and the fifth is a mirror-symmetry property. A beat that lays the mark,
   a wider gate and a lay-line rule are three different decisions with three
   different costs, and the gate width is **course geometry** — widening it to
   make the baseline look better is v1 brief §43's discipline violated whether
   or not the number is a coefficient.
2. **`passage::cut_between` is the one definition of a cut.** `sailgym-env` and
   `sailgym-task` both call it. If you find yourself building a probe route,
   stop: that is RV66, and §5.2 measures what it costs.
3. **A recorded `sailgym-env` episode now has one more frame**, and its header's
   initial condition is finally the state it started from (§3.2). Any new
   consumer of `ResearchEnvelope` gets that for free; anything that counted
   frames does not.
4. **`run_baseline` is cached per attempt and recomputed on retry.** If you add
   a path that changes an attempt's conditions without building a new `Attempt`,
   you will serve a stale baseline. There is no such path today.
5. **The two verdicts are not interchangeable.** `compare` is for two attempts;
   `compare_conditions` is for an attempt against a baseline. The ghost and the
   splits are gated on the second and the two-attempt deltas on the first, and
   F18.3a says why.
6. **`episodeIo.ts`'s `ComparabilityVerdict` wants one optional field** (§10.2).
7. **`sailgym-agent`'s `Sensor` is still not `Send`** — F19.6's one-word repair,
   `pub trait Sensor: Send`, is still unmade, and `sailgym-env` still carries its
   own `Send` suite with the equality test that keeps the two honest. Section 12
   owned `crates/sailgym-agent/src/lib.rs` and could have made it; it did not,
   because the repair belongs with the test that proves the duplication is
   harmless and that test is `sailgym-env`'s.
8. **`docs/v3/discussions/core-features-not-in-web.md` is revised** to say what
   section 12 actually closed, with the two measurements a v3 scope should start
   from in its §8.
