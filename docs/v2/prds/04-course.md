# v2 Section 04 — `sailgym-course`: routes, marks, guidance, passage

**Planning revision (2026-09-20):** later than playable milestone 08–11; IDs are preserved, not dispatch order. Read the revised index/brief/F18 and dependency handoffs. All required scope/normative decisions remain proposed. Compare no-change guards against this section's starting revision, not the pre-08 baseline.

Source discussions: `../discussions/unified-agent-interface.md` §§5, 11.1;
`../discussions/ablation-spaces.md` §2.4.

> **BLOCKED.** This PRD may not be dispatched until `../brief.md` S3 (and S6, for
> the obstacle tasks) has a recorded implementation decision, answering `../README.md` V-A. Write
> it, review it, do not execute it. Tasks 4.5 and 4.6 are additionally gated on
> S6 alone and may be cut without affecting the rest of the section.

Read first: `../../v1/00-foundations.md` in full, `../README.md`, `../brief.md`,
`../00-foundations.md` (F14.1, F15), `../progress/11-handoff.md`, then this PRD.

## Goal

The first of the three crates cross-stack §8 points 1–3 stand on. `Route`,
`Mark`, `Rounding`, `Guidance`, passage, progress — the task definition that
makes a rule sailor, a polar racer and an RL policy answerable to the same
thing.

It is first because it is the only one of the three that is **testable with no
controller at all**: feed it the recorded golden trajectories and assert its
passage decisions.

## Why this shape

Two mistakes this section exists to prevent, both of which are cheap now and
expensive later.

1. **Two UX modes becoming two code paths.** The web app wants "drag a point,
   sail at it" and "here is a course of marks". F15.1 makes the first a one-mark
   route, so there is one passage implementation and one set of tests.
2. **Cross-track error acquiring two definitions.** Jaulin & Le Bars' controller
   needs the *line*; a pursuit controller needs the *point*. F15.2 carries both,
   so neither is derived from the other inside an agent.

## What this section does **not** do

- No agent, no controller, no helm, no observation. Section 05.
- No episode, no termination, no `Outcome`. Section 06.
- **No Rust outside `crates/sailgym-course/`** except the workspace manifest and
  the gate scripts, both of which are `S` tasks. `git diff --name-only crates/sailgym-physics/`
  must be empty.
- Reuse 11 task outcome semantics when integrating; this crate adds geometry, not a second practice evaluator. No scoring beyond progress and elapsed time. Finish rate, capsize counts and
  control effort are evaluation concerns and belong to section 06.
- **No collision physics, under any circumstance** (`brief.md` §3). Even if S6 is
  approved, contact is a termination reason computed by section 06, never a
  force.

## Normative deltas

### D1 — F15 (task and course). **Open; blocking the section.**

`../00-foundations.md` F15 in full. F15.3 is the load-bearing clause: passage is
ordered, sided and directed, and a radius check is forbidden.

### D2 — F12′ step 3′s Rust test list gains the new crate. **Open.**

A new gate step is **not** requested. `cargo test -p sailgym-course` is added to
step 3's command as `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course`, which
keeps "the Rust core is correct" as one step and keeps a red step 3 meaning one
thing. Sections 05 and 06 extend the same list rather than each adding a step.

### D3 — `brief.md` S3, and S6 for tasks 4.5–4.6. **Open; record the implementation decision.**

## Passage, stated once

The rule, because it is the only part of this section that is subtle and it is
the part a policy will attack:

> A mark is passed at the first step at which **all four** hold: the previous
> mark is already passed; the boat has crossed the plane through the mark
> perpendicular to the incoming leg; it crossed on the side `rounding` requires;
> and it crossed in the direction the leg runs.

A radius check satisfies none of these on its own. It permits cutting the
corner, and `../discussions/unified-agent-interface.md` is right that a policy
will learn to — which then shows up as an unexplained improvement in completion
time that survives review because nobody re-reads the passage test.

A **gate** is not two marks. It is a directed crossing of the segment between
its two posts, and it is passed or it is not; there is no per-post state.

Every passage decision is a pure function of `(previous state, current state,
route, leg_index)`. There is no hysteresis, no timer and no "nearly". A boat
that crosses back out has not un-passed the mark — passage is monotone in
`leg_index`, and the test suite asserts it.

## Tasks

### 4.1 — Contracts: the crate, the types, the workspace

**Owns:** `crates/sailgym-course/Cargo.toml`, `crates/sailgym-course/src/lib.rs`,
`crates/sailgym-course/src/route.rs`, `Cargo.toml`, `Cargo.lock`
**P-group: S**

```rust
pub enum Rounding { Port, Starboard, Either, Gate(Vec2, Vec2) }
pub struct Mark { pub position: Vec2, pub radius: f64, pub rounding: Rounding }
pub struct Route { pub marks: Vec<Mark>, pub laps: u32 }
```

Dependencies: `sailgym-physics` (for `Vec2` and `BoatState` only), `serde`,
`serde_json`. **Nothing else, and no dependency in the other direction ever**
(F14.1).

`Route` is `Vec`-ordered, never a map (F9.3). `laps` repeats the mark list; it
does not duplicate it, so a 3-lap 4-mark course has 12 legs and 4 marks.

Acceptance: `cargo test -p sailgym-course` runs; `cargo tree -p sailgym-physics`
does not mention `sailgym-course`; JSON round-trip of a `Route` is the identity,
in the shape `state.rs`'s round-trip tests already use.

### 4.2 — Passage

**Owns:** `crates/sailgym-course/src/passage.rs`
**P-group: A**

The four-clause rule above, plus `Gate` as a directed segment crossing. Pure
functions; no interior mutability; no floating-point equality.

Acceptance, and this is the task's whole value:

- `cargo test -p sailgym-course passage` — a table-driven suite with one case
  per clause **failing in isolation**: correct side but wrong direction, correct
  direction but previous mark unpassed, plane crossed outside the segment for a
  gate, and the corner-cut case that a radius check would wrongly accept.
- **The corner-cut case is the named regression.** A trajectory that passes
  within `radius` of the mark but on the wrong side must be rejected, and the
  test says so in its name.
- Passage is monotone in `leg_index` over all six golden trajectories.
- Exactly-on-the-plane and exactly-on-the-segment-end cases are asserted, not
  left to chance — this is F16.3's discipline applied to task geometry.

### 4.3 — Guidance

**Owns:** `crates/sailgym-course/src/guidance.rs`
**P-group: A**

F15.2's struct, derived from `(Route, leg_index, BoatState)` each decision.
Signed cross-track is defined against `line` and computed **once**, here.
Lookahead distance is a course parameter, not a physical coefficient (F14.9),
and is therefore tunable.

Acceptance: `cargo test -p sailgym-course guidance` — sign of `signed_cross_track`
is asserted against the F2 convention explicitly, on both sides of the line, in
all four quadrants of leg bearing; a one-mark route with `Rounding::Either`
produces `line: None` and `target` equal to the mark position; mirroring the
route and the state about the wind axis negates `signed_cross_track` bit-for-bit,
reusing `testkit::mirror_state`.

The mirror assertion is the important one. F11's R3 names sign-convention drift
as the highest-probability defect class in this repository, and guidance is
nothing but signs: which side of the line, which way to turn, which tack.

### 4.4 — Progress and replay-driven validation

**Owns:** `crates/sailgym-course/src/progress.rs`,
`crates/sailgym-course/tests/replay.rs`
**P-group: B**

`Progress { leg_index, laps_done, distance_to_next, elapsed }`, plus the test
that makes this section cheap: drive passage and guidance from the **recorded
golden trajectories** in `crates/sailgym-physics/tests/golden/*.json` and assert
the decisions. No controller, no simulation, no new source of truth — the
trajectories are already committed and already regression-tested.

Acceptance: `cargo test -p sailgym-course --test replay` — for each of the six
goldens, a hand-checked route produces a stated passage sequence; the sequence is
committed as data alongside the test; and a deliberate one-mark reorder makes it
fail.

### 4.5 — Obstacles *(gated on `brief.md` S6 alone; cuttable)*

**Owns:** `crates/sailgym-course/src/obstacle.rs`
**P-group: B**

```rust
pub enum Obstacle { Circle { c: Vec2, r: f64 }, Segment { a: Vec2, b: Vec2 } }
```

Geometry only. No forces, ever (F15.4). `Segment` covers a shoreline, a pier and
a course boundary with one primitive.

Acceptance: `cargo test -p sailgym-course obstacle` — containment and distance
are exact for both primitives, including the degenerate zero-length segment and
the zero-radius circle.

### 4.6 — Ray casting *(gated on `brief.md` S6 alone; cuttable)*

**Owns:** `crates/sailgym-course/src/raycast.rs`
**P-group: B**

Analytic against both primitives. Iterates a `Vec` in **index order**, never a
set (F9.3) — determinism is the reason, and a `HashSet` here would be invisible
until a trajectory diverged on a different machine.

Do not build this unused. Tasks 4.5–4.6 remain deferred unless S6 selects a concrete obstacle task and sensor consumer. Other boats are not automatically obstacles in a non-interacting ghost race.

Acceptance: `cargo test -p sailgym-course raycast` — ranges match a closed-form
reference at 10⁴ sampled `(origin, direction)`; a ray exactly tangent to a circle
and a ray exactly along a segment are both asserted; ray order is stable across
runs, asserted by hashing the returned range vector.

### 4.7 — The gate

**Owns:** `scripts/check.sh`, `scripts/check.ps1`, `CLAUDE.md`,
`docs/v2/00-foundations.md`, `docs/v2/README.md`
**P-group: S**

D2: step 3 becomes `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course`. Both sites
in `check.sh`, the `$Steps` array in `check.ps1`, the `CLAUDE.md` table, the
chain in `docs/v2/README.md`, F12. **Step count does not change**, so
the implemented ValidateRange is untouched: nine before 03, eleven after 03. This section has no Python/JAX dependency.

Acceptance: `scripts/check.sh 3` runs both crates' tests; `scripts/check.sh` green.

## Section acceptance criteria

1. `scripts/check.sh` green with the currently implemented steps (nine before 03, eleven afterward).
2. `cargo tree -p sailgym-physics` does not mention `sailgym-course` (F14.1).
3. `git diff --name-only crates/sailgym-physics/` is empty.
4. The corner-cut regression in 4.2 and the mirror assertion in 4.3 each
   demonstrated able to fail, and reverted.
5. `--test replay` passes against all six committed goldens.
6. If S6 was not signed, 4.5 and 4.6 are absent and the handoff says so — not
   present and disabled.
7. `docs/v2/progress/04-handoff.md` per F13.6.

## Risks

| # | Risk | Mitigation | Fires when |
|---|---|---|---|
| **RV19** | Passage degrades to a radius check under time pressure, and a later policy exploits it. | 4.2's named corner-cut regression; F15.3 forbids it normatively. | any passage test asserts only distance |
| **RV20** | Cross-track acquires a second definition inside an agent in section 05. | F15.2 carries both `line` and `target`; `guidance.rs` is the only file computing it, and it is not owned by section 05. | `signed_cross_track` is computed anywhere outside `guidance.rs` |
| **RV21** | A sign error in guidance shows up only as "sometimes it sails badly to port". | 4.3's mirror assertion, reusing `testkit::mirror_state`, bit-for-bit. | the mirror test is weakened to a tolerance |
| **RV22** | `sailgym-course` acquires a reverse dependency and ends F8.1's host-testability property. | 4.1's `cargo tree` assertion in the section criteria, not just in review. | `cargo tree -p sailgym-physics` mentions any v2 crate |
| **RV23** | Obstacles land, S6 is later ruled out, and the code stays as dead weight nobody will delete. | 4.5 and 4.6 are separately gated and explicitly cuttable; criterion 6 requires absence, not a feature flag. | an obstacle module ships behind a disabled flag |
| **RV24** | `laps` is implemented by duplicating marks, so a 3-lap course has 12 `Mark`s and mark identity is lost. | 4.1 states the representation; 4.4's progress test asserts `marks.len()` is unchanged by `laps`. | `Route::marks.len()` depends on `laps` |

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| No WASM surface; the web app cannot set a route yet | scope — `set_route` needs F8.2 amending, and no task owns `sailgym-wasm` | the section that adds the picker UI |
| No polar table, no laylines | scope | the polar-racer section |
| Scoring is progress and elapsed time only | scope | section 06's evaluation report |
| Lookahead distance has no tuned default; it is a course parameter with a placeholder | 4.3 — tuning it needs a controller to tune against | section 05 |
