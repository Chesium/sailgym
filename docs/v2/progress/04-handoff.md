# v2 Section 04 — Handoff (`sailgym-course`: routes, marks, guidance, passage)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F15 (and the new
**F15.5**), F14.1 and F12′, and `docs/v2/prds/04-course.md` before starting
anything that touches courses, agents or episodes.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. `crates/sailgym-physics/` is untouched —
`git diff --name-only crates/sailgym-physics/` is empty, which is the
section's acceptance criterion 3 — so F1–F9 are exactly as sections 08 and 02
left them, `STATE_LEN` is still 13, the F8.3 snapshot layout is unchanged,
`parameters.rs` is byte identical, `scenarios/` is untouched and **the physics
source tree id does not move**. F8.2's WASM surface is unchanged: no route can
be set from the browser yet, and that is a tracked debt, not an omission.

Status: **complete**; `scripts/check.sh` is green end to end (§5). Tasks 4.5
and 4.6 are **absent** — `brief.md` S6 is deferred and no separate selection
arrived with the dispatch (§1, §7). Everything else the PRD asks for landed.

---

## 1. The implementation decision this section needed, and where it is recorded

`docs/v2/brief.md` §5 says a changed normative contract needs the selected S
rows, the exact F deltas, the decision source and date, and the required
validation recorded "in the section handoff or this table". The PRD opens
**BLOCKED** on `brief.md` S3 (and S6 for tasks 4.5–4.6) having a recorded
implementation decision. This is that record, in the same place and the same
shape sections 02 and 03 used.

| item | resolution |
|---|---|
| **decision source** | the human's dispatch of `docs/v2/prds/04-course.md` as the section PRD, 2026-09-22 — the same act `docs/v2/README.md` records as having resolved 02's and 03's deltas on 2026-09-21 |
| **S rows selected** | **S3** (courses and controller interface), course half only. **S6 is not selected**: the brief marks it *Deferred*, `README.md` V-F excludes tasks 4.5–4.6 "unless separately selected", and the dispatch selected nothing separately |
| **D1 — F15 in full** | implemented. Where the implementation departed from F15.1–F15.4 is recorded in the new **F15.5**, not left to be inferred |
| **D2 — step 3 gains the crate** | implemented: `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course`, five sites moved together, **step count unchanged** so `[ValidateRange(1, 11)]` did not move |
| **D3 — brief S3/S6** | S3 in, S6 out; this table is the record, since no section-04 task owns `docs/v2/brief.md` |
| **validation performed** | 48 tests in the new crate (42 unit + 6 replay), the replay validation over all six committed goldens, three demonstrations that the named guards can go red, and four full runs of the eleven-step gate (§5) |

**No signature is claimed for anything beyond that.** In particular this
section makes no physical claim of any kind: it adds geometry, and geometry is
not validation.

---

## 2. What landed, file by file

### Task 4.1 — contracts: the crate, the types, the workspace (P-group S)

- **`crates/sailgym-course/Cargo.toml`** — new crate. Dependencies:
  `sailgym-physics` (for `Vec2` and `BoatState`), `serde`, `serde_json`, and
  nothing else. `sailgym-physics` with `features = ["testkit"]` as a
  **dev**-dependency, so `mirror_state` reaches the tests and nothing that
  ships.
- **`crates/sailgym-course/src/lib.rs`** — the crate document: what is here,
  what is deliberately not here (no physics, no forces, no agent, no episode,
  no obstacles), and the passage rule stated once.
- **`crates/sailgym-course/src/route.rs`** — `Rounding`
  (`Port | Starboard | Either | Gate(Vec2, Vec2)`), `Mark`, `Route`, `Leg`,
  `RouteError`, `Route::validate`, `Route::mirrored`, `position`, and a
  `vec2_serde` shim. 9 unit tests.
- **`Cargo.toml`** (workspace) — `crates/sailgym-course` in `members`,
  `sailgym-course` in `workspace.dependencies`. **`Cargo.lock`** — one new
  package, no new third-party dependency.

### Task 4.2 — passage (P-group A)

- **`crates/sailgym-course/src/passage.rs`** — `passed`, `passed_between`,
  `advance`, `left_normal`, and the four-clause rule with `Gate` as a directed
  segment crossing. Pure functions, no interior mutability, no floating-point
  equality. 17 unit tests, one per clause failing in isolation, the named
  corner-cut regression, and the boundary cases.

### Task 4.3 — guidance (P-group A)

- **`crates/sailgym-course/src/guidance.rs`** — `CourseParams`, `Guidance`,
  `guidance`, `guidance_at`. Signed cross-track is defined against the line
  and computed **here, once**. 8 unit tests including the four-quadrant sign
  sweep and the bit-exact mirror assertion.

### Task 4.4 — progress and replay-driven validation (P-group B)

- **`crates/sailgym-course/src/progress.rs`** — `Progress`, `Passage`,
  `Tracker`. 8 unit tests.
- **`crates/sailgym-course/tests/replay.rs`** — 6 tests driving passage and
  guidance from the committed golden trajectories.
- **`crates/sailgym-course/tests/replay/routes.json`** — the six hand-laid
  routes and the passage sequence each one produces, committed as data.

### Task 4.7 — the gate (P-group S)

- **`scripts/check.sh`** (both sites), **`scripts/check.ps1`** (`$Steps`),
  **`CLAUDE.md`** (the step-3 row and the layout block),
  **`docs/v2/README.md`** (the delivery table, the status paragraph, the gate
  paragraph, V-E and V-F), **`docs/v2/00-foundations.md`** (the delta table,
  F12′ in three places, and the new F15.5).
- **`docs/v2/progress/04-handoff.md`** — this file.

### Tasks 4.5 and 4.6 — absent

No `obstacle.rs`, no `raycast.rs`, no feature flag and no commented-out
module. Section acceptance criterion 6 asks for absence rather than a disabled
feature, and RV23 is why.

---

## 3. The decisions worth arguing with

### 3.1 The route is a circuit, and F15 did not say which way leg 0 points

`Route` is `marks` and `laps` and carries no start point, so something has to
say where leg 0 comes from. It comes from the **last** mark. With `laps > 1`
that is literally where the boat has just been; with `laps == 1` it is a
convention.

The alternative — orienting mark 0's plane by the *outgoing* leg 0→1 — reads
better for a one-lap course and then makes lap 1 and lap 2 disagree about the
same mark, which is the kind of difference nobody finds until a policy
exploits it. A course that wants a distinct start makes the start a mark,
which is also how a start line is spelled: a `Rounding::Gate`.

### 3.2 The single-mark route arrives at a disc, and that is the only proximity test in the crate

F15.1 makes "drag a point and sail at it" a one-mark `Route` with
`Rounding::Either` and a large radius. Such a route has **no incoming leg**,
so it has no perpendicular plane, no direction and no side, and the only thing
"arrival" can mean is the step's segment reaching the mark's disc.

That is a proximity test, and F15.3 forbids proximity tests — so the branch is
fenced: **`Route::validate` refuses `Port`, `Starboard` or `Gate` on a route
with no incoming leg**, naming the mark. The disc branch is unreachable for
any mark that has a side, and `route::tests::validate_names_the_offending_mark`
asserts the refusal. Without that fence this would have been a way to spell a
radius check, which is exactly what RV19 warns about.

### 3.3 `radius` is the clearance, not the test

F15.3 says what `radius` may not be used for and not what it is for. Here a
sided crossing must be at a lateral offset of at least `radius` on the
required side. Grazing the buoy tangentially on the correct side counts
(asserted exactly, at `−2.0` against a `2.0 m` radius); sailing over the top
of it does not (asserted at `−1.9`). So a mark's size constrains the passage
without ever deciding it, and the corner-cut regression has a fixture a naive
radius check genuinely accepts.

### 3.4 Cross-track is positive to port of the leg, and the mirror is bit-exact except at zero

`signed_cross_track = d̂ × (p − a)`, positive when the boat lies to port of the
leg's direction of travel — the side `+y_H` points to (F2) for a boat on the
leg's heading. Asserted on both sides of an east-running leg, in all four
quadrants of leg bearing, and under the mirror.

**One carve-out, and it is in the test's documentation rather than hidden in a
tolerance.** For a boat exactly on the line the error is `+0.0` on both sides
of the mirror, while `−(+0.0)` is `−0.0`, whose bits differ. IEEE 754 says the
two are equal, so the zero rows assert equality with zero and every other row
asserts `to_bits()`. The sweep contains such a row **on purpose** — a point
placed exactly on one leg — and the test fails if it never fires, so the
carve-out cannot quietly become the whole assertion.

### 3.5 The gate's direction is a three-way comparison

A gate's crossing direction comes from the sign of `d̂ · n̂_gate`. A leg exactly
parallel to the gate line gives no sign, and then the boat's own displacement
is the only thing that says which way "through" is. That is written as a
`partial_cmp` with three arms, not a two-way `if`, for the reason F16.3 gives
about `delta_r_self_centre`: a two-way `where` is wrong at zero. The `None`
arm catches a NaN with it, and
`passage::tests::a_gate_exactly_parallel_to_its_leg_falls_back_to_the_boats_own_direction`
is the case.

### 3.6 The lookahead distance is a placeholder and says so

`CourseParams::DEFAULT_LOOKAHEAD = 20.0` m. It is a **course parameter, not a
physical coefficient** (F14.9), so it may be tuned — and it has not been,
because tuning it needs a controller to tune it against. Its provenance note
says it is a round number deliberately **not** derived from any boat dimension,
so no reader can mistake it for one. It is the only number this crate
introduces that anyone could want to change, it never reaches the physics core,
and the PRD tracks it as a debt repaid by section 05.

---

## 4. The measured results

### 4.1 The crate

```
cargo test -p sailgym-course
  42 unit tests  (route 9, passage 17, guidance 8, progress 8)
   6 replay tests
```

### 4.2 The replay validation — the six committed routes

Each route is three marks laid out over the recorded track plus a fourth
behind the start, so that leg 0 — which a circuit takes from the last mark —
runs forward along the boat's own first heading. The **side** of each mark was
not chosen and then checked: the routes were first driven with every rounding
`Either`, the side the boat actually passes on was measured, and the rounding
was set to it. The measured clearances are committed in `routes.json` beside
each event, and
`the_committed_routes_are_valid_and_the_clearances_are_as_recorded` asserts
the sign agrees with the rounding and the magnitude exceeds `1.5 × radius`.

| golden | marks | passages (leg, sample, t) | clearance / gate fraction |
|---|---|---|---|
| `beam_reach_capsize` | stbd, gate, port, either | (0, 75, 15.0 s) · (1, 95, 19.0 s) · (2, 150, 30.0 s) | +5.376 m · 0.500 · −7.528 m |
| `close_hauled` | port, gate, stbd, either | (0, 60, 12.0 s) · (1, 76, 15.2 s) · (2, 150, 30.0 s) | −2.988 m · 0.500 · +2.551 m |
| `free_sail` | port, stbd, gate, either | (0, 70, 14.0 s) · (1, 117, 23.4 s) · (2, 136, 27.2 s) | −5.939 m · +8.481 m · 0.500 |
| `gybe` | port, stbd, gate, either | (0, 28, 5.6 s) · (1, 73, 14.6 s) · (2, 111, 22.2 s) | −5.922 m · +3.899 m · 0.500 |
| `sheet_release_recovery` | port, port, stbd, either | (0, 73, 14.6 s) · (1, 105, 21.0 s) · (2, 142, 28.4 s) | −5.703 m · −4.994 m · +6.731 m |
| `tack` | stbd, gate, either, either | (0, 23, 4.6 s) · (1, 41, 8.2 s) | +2.855 m · 0.500 |

`tack` finishes on **leg 2 of 4** — the recorded 30 s never reaches the third
mark, which is why that mark carries `Either`: no side could be measured for
it, so none is asserted. A course the boat does not finish is the realistic
case and it is deliberately one of the six.

### 4.3 The three properties the acceptance names directly

| property | measurement | result |
|---|---|---|
| passage is monotone over all six goldens | every step of every golden driven: `next ≥ leg` and `next ≤ leg + 1` | **900 steps**, no violation |
| cross-track has one definition | compared against a second formula — the perpendicular component after subtracting the along-track projection — at every sample of every golden | **906 samples**, worst disagreement **1.776e-15 m** |
| `physics` does not depend on `course` | `cargo tree -p sailgym-physics --edges all` | neither `sailgym-course`, `sailgym-task` nor `wasm-bindgen` appears |

### 4.4 Two assertions the PRD did not ask for, kept because they are cheap

- **`reordering_one_mark_changes_the_sequence`** — swapping marks 0 and 1 of
  each committed route changes what the rule decides, on all six tracks. The
  PRD asks for the reorder as a one-off demonstration (§5.3 below is that);
  as a permanent test it says the committed sequences describe the **route**
  and not merely the track.
- **`flipping_one_required_side_loses_that_passage`** — for each of the
  **12** marks carrying a side, requiring the other one changes the sequence.
  The sided clause is therefore load-bearing on recorded tracks and not only
  on hand-built fixtures.

---

## 5. Proven able to fail — three demonstrations, and their revert

Section acceptance criterion 4 asks for two; there are three, because the
committed replay data deserved one of its own. Each was applied, measured,
and reverted; the suite is green afterwards (§6).

### 5.1 The corner-cut regression (task 4.2)

The sided clause was replaced with the forbidden radius check of F15.3 —
`distance_to_segment(m, prev, cur) <= mark.radius` — which is exactly the
shortcut RV19 names.

```
test passage::tests::a_corner_cut_that_a_radius_check_would_accept_is_rejected ... FAILED
  panicked at crates/sailgym-course/src/passage.rs:342:9:
  a corner cut inside the mark's radius on the wrong side is not a passage (F15.3)
```

Five more passage tests and one progress test went red with it, and so did
**two of the six replay tests** — the recorded tracks notice the shortcut too.
Reverted; 48 tests green.

### 5.2 The mirror assertion (task 4.3)

A one-sided saturation was introduced —
`d.cross(from_a).max(-5.0)` — which is the shape of defect RV21 describes:
"sometimes it sails badly to port". A whole-sign error would **not** do, since
it is still antisymmetric, and saying so is part of what the test means.

```
test guidance::tests::mirroring_the_route_and_the_state_negates_the_cross_track_bit_for_bit ... FAILED
  panicked at crates/sailgym-course/src/guidance.rs:381:17:
  leg 0, point 1: -5 is not the bit-exact negation of 27.63679757318457
test guidance::tests::the_mirror_holds_through_mirror_state_itself ... FAILED
```

The four-quadrant sign sweep stayed green, which is the point of having both.
Reverted.

### 5.3 The one-mark reorder (task 4.4)

Marks 0 and 1 of the `gybe` route were swapped in the **committed data**.

```
test the_six_goldens_produce_their_committed_passage_sequences ... FAILED
  gybe: passage sequence (leg, sample) does not match the committed one
test the_committed_routes_are_valid_and_the_clearances_are_as_recorded ... FAILED
```

Reverted.

---

## 6. The gate

Four full runs, one after each group, all `exit 0`. Step 9 is the browser
suite and dominates every one of them.

| run | tree | wall | result |
|---|---|---|---|
| baseline | `4b89b6a`, before any edit | 760 s | all eleven steps ok |
| after group S (4.1) | the crate, `route.rs` only | 756 s | all eleven ok |
| after group A (4.2, 4.3) | + `passage.rs`, `guidance.rs` | 755 s | all eleven ok |
| after group B (4.4) | + `progress.rs`, `tests/replay.rs` | 754 s | all eleven ok |
| after group S (4.7) | + the gate edits, step 3 now three crates | 755 s | all eleven ok |

**The baseline matters and it was measured**, not assumed: the PRD says to
compare no-change guards against this section's starting revision, and at
`4b89b6a` with a clean tree the whole chain was already green, so nothing in
§5's red runs can be blamed on something that was already broken. No
pre-existing failure was found.

Note that until task 4.7 the new crate was **not** in step 3 — it was compiled
by step 2's `clippy --all-targets` and its tests were run directly. From 4.7
on, `scripts/check.sh 3` runs all three crates.

### 6.1 The five sites, moved together

v2 F12′ requires every site that spells the chain out to move together "or the
gate lies about itself". All five moved:

| site | what changed |
|---|---|
| `scripts/check.sh` | the `step_names` array **and** the `run_step` case, plus the comment that says why step 3 grows rather than a step appearing |
| `scripts/check.ps1` | the `$Steps` entry and the same comment |
| `CLAUDE.md` | the step-3 table row, and `crates/sailgym-course/` in the layout block |
| `docs/v2/README.md` | the gate paragraph, the delivery table, the status paragraph, V-E and V-F |
| `docs/v2/00-foundations.md` F12′ | the chain listing, the "step 3 grows" clause, the retained-entries clause and the delta table |

`[ValidateRange(1, 11)]` in `check.ps1` is **deliberately unchanged**: the step
count did not move, so RV17 does not arise.

### 6.2 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` here, as in sections 01, 02, 08, 09, 10 and 11.
`check.ps1` **was** edited by this section, so the edit is **unverified**: it is
the same one-entry substitution made to `check.sh`, in the one place
`check.ps1` spells the command, and `$Steps` still has eleven entries.
Section 03 §6.3 showed that unpacking PowerShell into a scratch directory is
possible; this section did not do it, because the edit is a single string and
the risk it carries is smaller than section 03's five-site step-count change.
Recorded rather than glossed.

---

## 7. No physics changed, and no coefficient was tuned

```
$ git diff --name-only crates/sailgym-physics/
(empty)
$ git status --porcelain crates/sailgym-physics/ crates/sailgym-task/ crates/sailgym-wasm/ crates/sailgym-bench/ scenarios/
(empty)
```

`parameters.rs` is byte identical, `provenance.rs` compares the same F7 rows
on every gate run, and the two F18.1c overrides section 08 recorded are still
the only ones. **Nothing in this crate is a physical coefficient**, and the
one number that could be mistaken for a tunable — the lookahead distance — is
a course parameter with a provenance note saying it is an untuned placeholder
(§3.6). The other numbers this section introduces are mark positions and
radii in a **test fixture**, which are evidence rather than coefficients, and
they are committed as data with their derivation recorded in the file itself.

The user's dispatch adds that a visual constant is governed by the same
discipline as a physical one. This section introduces no visual constant at
all: it has no UI.

---

## 8. Section acceptance criteria, one by one

| # | criterion | result |
|---|---|---|
| 1 | `scripts/check.sh` green with the currently implemented steps | **pass** — eleven steps, four runs, §6 |
| 2 | `cargo tree -p sailgym-physics` does not mention `sailgym-course` | **pass** — asserted by `route::tests::physics_does_not_depend_on_the_course_crate`, which runs in step 3 |
| 3 | `git diff --name-only crates/sailgym-physics/` is empty | **pass** — §7 |
| 4 | the corner-cut regression and the mirror assertion each demonstrated able to fail, and reverted | **pass** — §5.1, §5.2, and a third demonstration in §5.3 |
| 5 | `--test replay` passes against all six committed goldens | **pass** — §4.2 |
| 6 | if S6 was not signed, 4.5 and 4.6 are absent and the handoff says so | **pass** — S6 was not selected (§1); there is no `obstacle.rs` and no `raycast.rs` |
| 7 | `docs/v2/progress/04-handoff.md` per F13.6 | this file |

Task-level acceptance:

| task | criterion | result |
|---|---|---|
| 4.1 | `cargo test -p sailgym-course` runs; `cargo tree` clean; `Route` JSON round-trip is the identity | pass — `a_route_json_round_trip_is_the_identity` asserts value → text → value equality **and** text-identical re-serialisation, the shape `state.rs` uses |
| 4.2 | `cargo test -p sailgym-course passage`; one case per clause failing in isolation; the named corner-cut regression; monotone over the six goldens; the exactly-on boundaries asserted | pass — 17 tests, plus the monotonicity clause measured in `--test replay` over 900 steps |
| 4.3 | `cargo test -p sailgym-course guidance`; sign asserted against F2 on both sides and in all four quadrants; one-mark route gives `line: None` and `target` = the mark; the mirror negates bit-for-bit | pass — 8 tests, with the signed-zero carve-out stated (§3.4) |
| 4.4 | `cargo test -p sailgym-course --test replay`; six goldens; sequences committed as data; a one-mark reorder makes it fail | pass — §4.2, §5.3 |
| 4.7 | `scripts/check.sh 3` runs both crates' tests; `scripts/check.sh` green | pass — step 3 now runs all **three** crates |

---

## 9. Risks

**RV19 — passage degrades to a radius check. Did not fire, and §5.1 is the
proof it would be caught.** No shipped passage test asserts only distance. The
one proximity test in the crate is fenced by `Route::validate` (§3.2).

**RV20 — cross-track acquires a second definition. Did not fire, and it is
closed structurally for now.** `signed_cross_track` is computed in exactly one
function; `replay.rs` checks it against an independently written formula at
906 samples and the worst disagreement is 1.776e-15 m. Section 05 does not own
`guidance.rs`, which is the other half of the mitigation.

**RV21 — a sign error that only shows as "sometimes it sails badly to port".
Did not fire.** The mirror assertion is bit-exact, reuses
`testkit::mirror_state`, and §5.2 shows it catches a one-sided defect that the
four-quadrant sweep does not.

**RV22 — `sailgym-course` acquires a reverse dependency. Did not fire**, and
the assertion runs in the gate rather than living in review.

**RV23 — obstacles land and then S6 is ruled out. Did not fire.** Nothing was
built.

**RV24 — `laps` implemented by duplicating marks. Did not fire.** Asserted at
the representation (`route.rs`) and over a driven three-lap course
(`progress.rs`): 4 marks, 12 legs, each mark passed three times.

**R3 (v1) — sign-convention drift.** The risk this section is most exposed to.
Three independent mitigations are in place: the F2-referenced sign assertions,
the four-quadrant sweep, and the bit-exact mirror.

---

## 10. What deviated from the PRD

### 10.1 `src/lib.rs` takes one line per later module, and task 4.1 owns it

Rust has no way for task 4.2 to declare its own module without editing the
crate root, which is task 4.1's file. `lib.rs` therefore gained
`pub mod passage;` and `pub mod guidance;` during group A and
`pub mod progress;` during group B. The file carries a comment saying so.
This is the same class of `Owns:`-list gap sections 10 and 11 recorded
(11 §7.4, 10 §6.5); it is now the sixth section to hit one, and the PRD
template is still where it should be fixed.

### 10.2 `tests/replay/routes.json` appears in no `Owns:` list

Task 4.4 owns `tests/replay.rs` and the PRD says the sequence "is committed as
data alongside the test", so the data file is required by the task and absent
from its list. Recorded rather than absorbed.

### 10.3 The passage rule needed three clauses the PRD did not state

§3.1 (leg 0 comes from the last mark), §3.2 (arrival at a disc for the
single-mark route, fenced by validation) and §3.5 (the three-way gate
orientation). Each is a place where F15 is silent and something had to be
decided; each is recorded in F15.5 rather than only in a doc comment.

### 10.4 The half-open crossing convention is this section's choice

`s ≤ 0` then `s > 0`. F15.3 says "crossed the plane" and does not say what
happens to a boat sitting exactly on it. The convention is the same shape as
`frames::wrap_pi`'s `(−π, π]` and it is asserted directly: a boat parked
exactly on the plane is not passed on any step, and one starting exactly on it
passes on the step that moves it off. Without a stated convention that boat
passes on every step or on none, depending on how the comparison happened to
be written.

### 10.5 No task was delegated

Group A holds two tasks and group B one. 4.2 and 4.3 share the leg geometry
that 4.1 put in `route.rs`, and both are small; the section agent executed
them, as sections 01, 02, 08, 09, 10 and 11 each recorded. It is now the
seventh consecutive section to say this, which is probably worth taking as
evidence about the task sizes rather than about the agents.

### 10.6 Group order was honoured, and the gate was run on the group's own tree

S → gate → A → gate → B → gate → S → gate. The group-A and group-B sources
were **written** before their groups began, but they were kept out of the
crate — moved outside the repository, not merely undeclared — for the group-S
gate run, so that run measured a tree containing task 4.1 and nothing else.
Recorded because "the gate was green after group S" means nothing if the group
was not what the gate saw.

---

## 11. What the next section must know

1. **`sailgym-course` is the geometry and nothing else.** No outcome, no
   score, no termination. Section 11's handoff asks that later course and env
   work **consume** `sailgym-task`'s `Outcome` and `PracticeEvent` rather than
   define new completion semantics, and this crate deliberately does not: the
   most it says is `Progress` and `Passage`. Section 06 should decide
   terminations, and it should reuse `TaskRun`'s vocabulary where it fits.

2. **`signed_cross_track` is computed in `guidance.rs` and must stay there.**
   Section 05 does not own that file. An agent that recomputes cross-track
   from `Guidance::target` has created RV20's second definition, and the only
   thing that would catch it is a reviewer.

3. **Drive the tracker at the physics step rate.** `Tracker::observe` takes at
   most one mark per step, and a mark whose plane is crossed twice inside one
   step is missed **permanently** — the plane test is a crossing, so the boat
   never gets another chance at it.
   `progress::tests::a_step_that_crosses_two_planes_passes_one_mark_and_misses_the_other`
   is the case. The replay validation drives at 5 Hz because the goldens are
   sampled at 5 Hz and the committed routes have metres of clearance; a live
   episode has no such excuse.

4. **The lookahead distance is untuned and section 05 owns the tuning.**
   `CourseParams::DEFAULT_LOOKAHEAD` is 20 m with a provenance note saying it
   is a placeholder. Tuning it is permitted (F14.9) and requires nothing more
   than a line in the handoff — it is not a coefficient. Do not tune anything
   in `parameters.rs` instead.

5. **`determinism.rs::no_wall_clock` does not scan this crate.** F14.8 says
   the existing grep "extends to the new crates rather than being rewritten",
   and that extension belongs to section 05, which owns the agent crates and
   has the reason. `sailgym-course` reads no clock today —
   `grep -r "Instant\|SystemTime" crates/sailgym-course` is empty — but
   nothing enforces it.

6. **There is no WASM surface.** `set_route` needs F8.2 amending and no
   section-04 task owns `crates/sailgym-wasm`, so the web app cannot set a
   route. The PRD records this as a debt for the section that adds the picker
   UI; F15.1's lookahead point is the shape that UI should use.

7. **The repository root `README.md` now lies about the gate three times.**
   Its nine-step table spells step 3 without `-p sailgym-task` (section 11's
   change) and without `-p sailgym-course` (this one), step 4 without
   `--test conformance` (section 02's), and the chain as nine steps with no 10
   or 11 (section 03's). No task in any of those sections owns that file
   (F13.2). The changes it needs:

   | line | should read |
   |---|---|
   | 194 | `\| 3 \| ` + "`cargo test -p sailgym-physics -p sailgym-task -p sailgym-course`" + ` \| The physics core, the practice evaluator and the course layer are correct **and build on the host with no WASM toolchain**. \|` |
   | 195 | step 4 with `--test conformance` |
   | the chain | eleven steps, with 10 and 11 |
   | the layout block | `crates/sailgym-task/` and `crates/sailgym-course/` |

   `docs/v1/00-foundations.md` F12 is deliberately **not** edited: a v1 clause
   is amended by a recorded v2 delta and never in place.

8. **`build.rs` still has RV52**, as sections 09, 10 and 11 each reported. It
   did not fire here because this section touches no physics source, so the
   compiled identity never went stale relative to a new commit. It will fire
   again for the next section that does.

9. **Four `README.md` open items are still waiting for a human** — V-D, V-G,
   V-H, and now the obstacle half of V-F. This section edited V-E and V-F to
   record what it did; it did not close anything, for the same reason sections
   01, 08, 09, 10 and 11 each left theirs.

---

## 12. Commands

```
cargo test -p sailgym-course
cargo test -p sailgym-course passage
cargo test -p sailgym-course guidance
cargo test -p sailgym-course --test replay -- --nocapture
cargo tree -p sailgym-physics --edges all
scripts/check.sh
scripts/check.sh 3
```

The replay data was generated once, by a design script kept outside the
repository, and is committed. It is **not** regenerated by any command in the
repository, and that is deliberate: a fixture a command can rewrite is a
fixture that gets rewritten when it goes red. Changing it means editing it and
saying why in a handoff — which is also how `tests/golden/` would be treated
if `gen_golden` did not refuse a dirty tree.
