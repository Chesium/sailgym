# v2 — Foundations (Normative deltas)

**This document contains no tasks and is not executed by an agent.**

**Status: proposed deltas, except F18.1, which section 08 has implemented.** The [index](README.md) schedules 08–11 before research sections 02–07. The brief records separate scope decisions. v1 foundations remain the shipped reference; explicit v2 corrections require a recorded decision and evidence, not a silent reinterpretation. No signature is supplied by this planning update — and none is supplied by F18.1a–d either: they record what section 08 implemented and the evidence for it ([`physics-validation.md`](physics-validation.md), [`progress/08-handoff.md`](progress/08-handoff.md)), and the handoff names the two decisions that still want a human ruling.

F18 records the playable milestone. F14–F17 remain later research proposals, with the corrections below incorporated. Section-number order is not delivery order.

Numbering continues from v1: F1–F13 are v1's, F14 onward are v2's.

| Delta | Subject | Status |
|---|---|---|
| **F12′** | The gate grows to eleven steps | **implemented**: step 3 by sections 11, 04 and 05, step 4 by section 02, steps 10 and 11 by section 03 |
| **F14** | Agent interface — sensors, actions, cadence, helm | **implemented** by section 05; see F14.10. `Helm` and `ActionSpace::Setpoint` (task 5.6) remain deferred |
| **F15** | Task and course — routes, marks, guidance, passage | **implemented** by section 04; see F15.5. Obstacles (F15.4's `Obstacle`, S6) remain excluded |
| **F16** | Conformance, digests and the tolerance contract | **implemented** by section 02; see F16.9 |
| **F17** | The Python boundary | F17.1 **implemented** by section 03; F17.2–F17.5 remain proposed, section 07 |
| **F18** | Corrected model, input, replay and tasks | F18.1 **implemented** by section 08, F18.2 by 09, F18.3 by 10, F18.4 by 11 |

---

## F12′. The gate

**The chain is eleven steps, and every part of it is implemented.** Section
11 added `-p sailgym-task` to step 3, section 04 added `-p sailgym-course`
and section 05 `-p sailgym-agent` to it on **2026-09-22 by human approval** —
each its dispatch as the section PRD, whose task 4.7 and task 5.8 instruct
the five-site edit explicitly — without changing the step count, so
`[ValidateRange(1, 11)]` did not move again; section 02 added
`--test conformance`
to step 4 on **2026-09-21 by human approval**, the same approval with a date
that section 01's D1 received, recorded in
`docs/v2/prds/02-conformance-bundle.md` D1 and in
`docs/v2/progress/02-handoff.md`. Section 03 added **steps 10 and 11 on
2026-09-21 by human approval** — its dispatch as the section PRD, whose task
3.7 instructs the five-site edit explicitly — recorded in
`docs/v2/prds/03-jax-wind.md` D1 and in `docs/v2/progress/03-handoff.md`.
`[ValidateRange(1, 9)]` in `check.ps1` became `[ValidateRange(1, 11)]` in the
same edit; without it `scripts/check.ps1 -Step 10` would fail with a
parameter error rather than running the step (RV17). Steps 1–9 keep their
numbers and their meaning.

```
 1. cargo fmt --check
 2. cargo clippy --all-targets -- -D warnings
 3. cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent   ← done, 11, 04, 05
 4. cargo test -p sailgym-physics --test invariants --test no_shortcuts \
                                  --test convergence --test symmetry \
                                  --test provenance --test conformance     ← done, section 02
 5. cargo test -p sailgym-physics --test regression
 6. wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
 7. pnpm --dir web typecheck
 8. pnpm --dir web test:unit
 9. pnpm --dir web test:e2e
10. uv run ruff check python && uv run ruff format --check python          ← done, section 03
11. scripts/py-test.sh                                                     ← done, section 03
```

`--fast` covers steps 1–9 only. The pre-commit subset was not re-derived when
the Python steps landed; section 03 records it as a tracked debt, to be repaid
whenever the Python suite gets slow enough that skipping it matters. Selecting
a step explicitly always runs it.

Three properties of this shape are deliberate:

- **Step 3 grows rather than a step 10 appearing.** Step 3 is the step that
  proves the pure Rust is correct *and builds on the host with no WASM
  toolchain* (F8.1), and `sailgym-task` has exactly that property: it is pure,
  it depends on `sailgym-physics` and nothing depends on it except the
  wrapper. A separate step would have made the chain ten steps for a crate
  that answers the same question step 3 already asks. `sailgym-course` joined
  it in section 04 and `sailgym-agent` in section 05, for the same reason and
  by the same edit, and **section 06 extends this same list rather than
  adding a step** — which is why the step count has not moved since
  section 03.
- **Step 4 grows rather than a step 12 appearing.** `--test conformance` proves
  a property of the physics crate, which is what step 4 is for. A separate step
  would make a red step 4 no less ambiguous and would cost another F12 change.
  What it proves is **staleness**: the committed bundle is recomputed from
  current source and compared bit for bit, so a physics change that does not
  regenerate the bundle fails the way `cargo fmt --check` fails.
- **Step 11 delegates to a script**, exactly as step 6 delegates to
  `build-wasm.sh`. Later sections add `maturin develop` in front of `pytest`
  inside `scripts/py-test.{sh,ps1}` **without amending F12 again**. This is the
  whole reason the step is a script and not a command line.

Every site that spells the chain out moves together, or the gate lies about
itself: `scripts/check.sh` (the `step_names` array **and** the `run_step` case),
`scripts/check.ps1` (the `$Steps` array **and** `[ValidateRange(1, 9)]` →
`(1, 11)`), the table in `CLAUDE.md`, the chain in `README.md`, and F12 itself.
Section 02's step-4 edit moved four of those five — `check.sh` at both sites,
`check.ps1`'s `$Steps`, `CLAUDE.md`'s table and this clause — and left the
repository root `README.md` alone for the same ownership reason recorded
below. Section 03's steps 10 and 11 moved all five, `[ValidateRange]`
included, and verified the result rather than asserting it: both scripts were
parsed and their step-name lists diffed, agreeing at all eleven positions
(`docs/v2/progress/03-handoff.md` §6).

Additions to the pinned stack: `uv`, `ruff`, `pytest`, `jax`, `maturin`, `pyo3`,
`rayon`. **`rayon` may not appear in `sailgym-physics`** — see F16.5.

No gate change remains proposed. Four entries must be retained in every
later revision of the chain: `-p sailgym-task`, `-p sailgym-course` and
`-p sailgym-agent` in step 3, and `--test conformance` in step 4. A section
that rewrote the chain and dropped one would take the whole practice
evaluator, the whole course layer, the whole agent interface, or the whole
bundle-freshness check, out of the gate silently.

**One site did not move with the rest**, and it is recorded rather than
edited: the nine-step table in the repository's root `README.md` still spells
step 3 as `cargo test -p sailgym-physics`, step 4 without
`--test conformance`, and the chain as nine steps with no 10 or 11. No
section-11, section-02 or section-03 task owns that file (F13.2), so
`docs/v2/progress/11-handoff.md`, `docs/v2/progress/02-handoff.md` and
`docs/v2/progress/03-handoff.md` report the exact changes it needs — now four
lines rather than two. `docs/v1/00-foundations.md` F12 is deliberately **not**
edited: a v1 clause is amended by a recorded v2 delta — this one — and never
in place.

---

## F14. Agent interface

*Implemented by section 05 on 2026-09-22, after 04. The implementation
decision brief §5 asks for — the selected S row (S3, controller half), the
exact F deltas, the decision source and date, and the validation performed —
is recorded in [`progress/05-handoff.md`](progress/05-handoff.md) §1, which
brief §5 names as one of the two places it may live. **F14.10 below records
where the implementation departed from F14.1–F14.9 and why.** Source:
`discussions/unified-agent-interface.md` §§2–4,
`discussions/ablation-spaces.md` §§1–3.*

### F14.1 Crates and the direction of dependency

```
sailgym-physics/   knows nothing about tasks or agents. Corrected by 08. (F8.1)
sailgym-task/      pure practice evaluation (11), reused by later env
sailgym-course/    Route, Mark, Rounding, Guidance, passage, Obstacle   (F15)
sailgym-agent/     Sensor, Actuation, Action, Agent, Helm, Cadence, registry
sailgym-env/       episode runner, Outcome, decision log, VecEnv
sailgym-py/        pyo3 binding over sailgym-env                        (F17)
sailgym-wasm/      09–11 input/inspection/tasks; unchanged within 02–06
sailgym-bench/     unchanged except the conformance generator           (F16)
```

Arrows point one way: `py → env → {agent, course, task, physics}`,
`agent → {course, physics}`, `physics → nothing`. **A dependency from
`sailgym-physics` onto any of the others is a defect**, because it ends the
"builds and tests on the host with plain `cargo test`" property that F8.1
exists to protect.

### F14.2 The substitution point is enumerated, not implicit

An agent replaces the control stack at exactly one of two points, and which one
is declarative data recorded in the episode header:

| `ActionSpace` | The agent replaces | Shared machinery below it |
|---|---|---|
| `Rates` | tactic and helm | none — this is F3 `Controls`, verbatim |
| `Setpoint` | tactic only | the one shared `Helm` |

Rates ships first. Setpoint/Helm remains a design until an engaged/released contract and consumer are selected; do not scaffold it. Everything above the cut — route, guidance, mark progression — is
the **environment's** job for every agent, including rule-based ones.

### F14.3 Observation layout is runtime data, not a constant

Start with one concrete versioned layout. A selected configurable-sensor study may extend it; no speculative lidar/noise registry is required. The observation vector is the **ordered concatenation** of each configured
sensor's outputs. Its layout is the concatenation of each sensor's
`field_names()`, recorded in the episode header. There is no `const OBS_FIELDS`
and no `OBS_LEN`: a ray-casting sensor's width is configurable and is therefore
not a subset of any fixed field set.

`ObsMask` survives only as **one flag per sensor: is this one privileged?**

### F14.4 `WorldView` is the containment boundary

Sensors receive a `WorldView` and see everything in it. The agent sees only the
concatenated vector. There is no path from an `Agent` to a `WindField`, a
`Route` or another boat's state. This restricts direct access, but derived guidance can still leak privileged information. Validate output semantics and mark any true-wind-derived guidance explicitly.

### F14.5 Action bounds are always `[−1, 1]^k`

Every actuation adapter presents `[−1, 1]^k` and denormalises internally.
Denormalisation lives in the adapter and nowhere else. An adapter that hands a
policy radians has confounded action *space* with action *scaling*.

**The boom angle `β` is never commanded.** F6.8 and F6.9 stand: an adapter that
wants "the sailor thinks in boom angle" inverts `ℓ(β)` to a sheet length and
commands that, and is named `sheet_length_for_beta` so no reader of an
experiment log believes the boom was commanded.

### F14.6 Cadence

```rust
pub struct Cadence { pub period_steps: u32 }   // 10 → 20 Hz at dt = 0.005
```

1. A decision happens exactly when `episode_step % period_steps == 0`, keyed off
   the **episode** step counter — never off a per-`advance`-call counter, never
   off elapsed time.
2. Rate actions hold Controls between decisions. A later setpoint action holds the target while its shared adapter updates on a separately declared simulation cadence.
3. `period_steps` is frozen at `reset` and recorded in the header.

F9.7 (`advance(n) == n × advance(1)`) must continue to hold **with an agent
attached**, and is tested, not assumed.

### F14.7 Observation is computed, never read from the cache

```rust
pub fn observe(st, p, wind, guidance, sensors) -> Observation
```

pure in its arguments. It may **not** read `Simulation::forces`, which is the
breakdown cached once per `advance(n)` call: an observation built on it would
depend on how the caller chunked its calls, which breaks F9.7 in the one place
nobody would think to test. A sensor needing accelerations calls
`forces::evaluate` itself at the decision instant.

### F14.8 Agent randomness

`rng.rs` gains `STREAM_AGENT: u64 = 4` in its existing named-stream table
(`STREAM_WIND = 1`, `STREAM_SCENARIO = 2`, `STREAM_NOISE = 3`). Per-sensor
substreams derive below it, keyed by the sensor's own id, so adding a sensor to
one ablation arm cannot perturb another sensor's noise in the same arm.

`decide` may not read a wall clock. The existing `determinism.rs::no_wall_clock`
grep extends to the new crates rather than being rewritten.

### F14.9 Autopilot gains are not physical coefficients

brief §43 governs `parameters.rs`. It does **not** govern controller gains,
adapter gains or sensor noise settings, which may be tuned freely. The crate
boundary is the distinction, which is why the boundary is worth having.

**Section 05 introduced no gain at all**, which is the cleanest form the rule
can take: the `rate` adapter is the identity and has nothing to tune, `Helm`
is deferred, and no noise model ships. The crate's only tunable settings are a
sensor's mounting position and its version, neither of which reaches a force.

---

### F14.10 What section 05 implemented, and where it departed

*Recorded here because F14.1–F14.9 were a proposal and this is what they
became. The evidence is [`progress/05-handoff.md`](progress/05-handoff.md).
**One file in `crates/sailgym-physics/` changed**, `src/rng.rs`, and it gained
one constant and one table row (D2); `git diff --name-only
crates/sailgym-physics/` names only that file, which is the section's
acceptance criterion 6. F3, F4, F5, F6, F7, F8 and F9 are unchanged:
`STATE_LEN` is still 13, the F8.3 snapshot layout is untouched,
`parameters.rs` is byte identical, there is no new WASM surface and this crate
holds no equation of motion, no coefficient and no frame conversion of its
own.*

1. **`Action` carries a normalised vector, not `Controls`.** The source
   discussion sketched `Action::Rates(Controls)`. F14.5 supersedes it: every
   adapter presents `[−1, 1]^k`, so an agent emitting `Controls` would have
   gone *round* the funnel rather than through it, and the `rate` arm of an
   ablation would be the one arm whose numbers were not comparable with the
   others. `Action::Rates` therefore carries `ActionVec` — at most
   `ACTION_MAX_DIM = 4` scalars, `Copy`, bounds-checked at construction — and
   `actuation::apply` is the one place it becomes `Controls`.

2. **`ActionSpace` keeps both variants; one of them is refused.** F14.2 fixes
   the enumeration at two and the enumeration is *logged data*, so a header
   written today and one written after task 5.6 lands must use the same
   vocabulary. `ActionSpace::Setpoint` is therefore a reserved name that
   `AgentSpec::validate` refuses, naming the deferral. **Nothing beneath it is
   scaffolded**: there is no `Setpoint` struct, no `Helm`, no adapter and no
   `Action` variant. Task 5.6 is deferred exactly as the PRD instructs —
   engaged/released semantics have no contract, because a zero rudder-rate
   command currently means *released, self-centring* and a position servo
   needs *engaged, hold*.

3. **`Agent::decide` takes `&[f64]`, which makes F14.4 structural.** The agent
   sees the concatenated observation vector because that is the only thing in
   scope: no `WorldView`, no `WindField`, no `Route`, no `BoatState`. `reset`
   takes the layout's field names and a `Pcg32` rather than an `EpisodeCtx`,
   which does not exist until section 06 — and which the PRD forbids a manual
   source to reach in any case.

4. **`WorldView` carries `controls` and `guidance`, and deliberately no
   `Route`.** The discussion had `course: &Route`. F15.5 §4 and RV20 require
   `signed_cross_track` to be computed in `sailgym-course`'s `guidance.rs` and
   nowhere else, and the surest way to stop a sensor recomputing it is to make
   the route unreachable from a sensor. `controls` is present because the
   actuator inner loop is part of the plant (F4.3) and because `imu` needs it
   to evaluate the forces afresh. `others: &[BoatState]` is present and empty.
   There are no obstacles: `brief.md` S6 is deferred and section 04 shipped
   none.

5. **Privilege is per column, and the sensor-level flag is derived.** F14.3
   says "one flag per sensor". Section 10's `ObservationField` already records
   `privileged` per column, and `guidance` emits four sensed columns and one
   derived from the true wind: a sensor-level flag would either hide four
   honest columns or leak one dishonest one. `Sensor::privileged()` reports
   *any column privileged*, so F14.3's reading is still available.

6. **The vane's mount is derived from F7 and introduces no literal.** The PRD
   says "at the masthead"; F7 has no mast height. The vane therefore sits at
   `mast_pos_b + (0, 0, z_ce)` — the highest *declared* point of the rig —
   which follows a live parameter edit rather than freezing a copy, and is
   configurable per instance because a sensor mounting position is not a
   physical coefficient (F14.9).

7. **`obs_digest` is the canonical record, not a hash.** F16.4 permits a
   compact key and requires an established SHA-256 if one is used; section 10
   ships no digest and compares canonical records. `obs_digest` returns the
   canonical JSON of the whole `ObsLayout`, so no second hash implementation
   enters a crate that ships, and the repository's one SHA-256 stays behind
   section 02's `testkit` feature and out of the browser build.

8. **Bounds live in `ObsLayout`, not in the episode header's
   `ObservationField`.** Section 10's record carries name, unit,
   normalisation, noise and privilege and no bounds, and this section may not
   widen it (see the note above about `rng.rs`). F16.4 requires the full
   record to travel beside any digest, and `ObsLayout` is that record.

9. **`observe` takes a `WorldView` and an explicit list of per-sensor RNG
   substreams.** F14.7 writes `observe(st, p, wind, guidance, sensors)`; the
   first four travel together as the F14.4 boundary given a name. The streams
   are built once per episode by `sensor_streams` and carried across
   decisions: rebuilding them inside `observe` would hand every decision the
   same draws, which is the most plausible way to ship a broken noise model
   and not notice. No noise model ships here; every column declares
   `noise: 0.0`.

10. **The wall-clock grep is extended by a new file in the agent crate.**
    F14.8 asks for the existing grep to be extended rather than rewritten, and
    acceptance criterion 6 forbids editing
    `crates/sailgym-physics/tests/determinism.rs`. So
    `crates/sailgym-agent/tests/determinism.rs` runs the **same needles** and
    the same `#[cfg(test)]` exclusion over `sailgym-agent` and
    `sailgym-course` — the latter because `progress/04-handoff.md` §11.5 asked
    for it and nothing enforced it before. The physics crate keeps its own
    copy, unchanged, over its own sources.

11. **The `rate` adapter is three scalars, and the release flag is a sign.**
    `sheet_release` is a boolean and F14.5 requires one box; encoding it as
    the sign of a third scalar makes the all-zero action exactly
    `Controls::default()`, so "do nothing" is not a number a policy has to
    learn. `angle`, `angle_bangbang` and `sheet_length_for_beta` are not
    built.

12. **The gate's step 3 grew again**, to
    `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent`.
    The step count did not change, so `[ValidateRange(1, 11)]` is untouched.
    Section 06 extends the same list.

---

## F15. Task and course

*Implemented by section 04 on 2026-09-22, after 03. The implementation
decision brief §5 asks for — the selected S row (S3, with S6 excluded), the
exact F deltas, the decision source and date, and the validation performed —
is recorded in [`progress/04-handoff.md`](progress/04-handoff.md) §1, which
brief §5 names as one of the two places it may live. **F15.5 below records
where the implementation departed from F15.1–F15.4 and why.** Source:
`discussions/unified-agent-interface.md` §5.*

### F15.1 One route type, two UX modes

A dragged lookahead point is a one-mark `Route` with `Rounding::Either` and a
large radius. There is no second code path for "sail at this point".

### F15.2 Guidance carries both a line and a point

```rust
pub struct Guidance {
    pub line: Option<(Vec2, Vec2)>,   // the current leg, as a directed line
    pub target: Vec2,                 // lookahead along it, or the raw point
    pub signed_cross_track: f64,
    pub leg_index: u32,
    pub rounding: Rounding,
}
```

Cross-track error is defined against the **line**, once. Deriving the line from
the point (or the reverse) inside an agent is how two subtly different
definitions of cross-track error come to exist.

### F15.3 Mark passage is ordered, sided and directed

A mark is passed when the boat has crossed the mark's perpendicular plane, on
the required side, in the required direction, with the previous mark already
passed. **A radius check is not sufficient** and is forbidden: it permits
cutting the corner, and a policy will learn to. A gate is a directed segment
crossing, not two marks.

### F15.4 Course geometry is not physics

`Route`, `Mark`, `Obstacle` and ray casting live in `sailgym-course`. Obstacle
ray casting iterates a `Vec` in index order, never a set (F9.3). Contact, if
scored at all, is a **termination**, never a force.

### F15.5 What section 04 implemented, and where it departed

*Recorded here because F15.1–F15.4 were a proposal and this is what they
became. The evidence is [`progress/04-handoff.md`](progress/04-handoff.md).
**No Rust outside `crates/sailgym-course/` changed** except the workspace
`Cargo.toml`, `Cargo.lock` and the two gate scripts:
`git diff --name-only crates/sailgym-physics/` is empty — the section's
acceptance criterion 3 — and `crates/sailgym-task`, `crates/sailgym-wasm` and
`crates/sailgym-bench` are untouched. F3, F4, F5, F6, F7, F8 and F9 are
unchanged; this crate holds no equation of motion, no coefficient and no
frame conversion of its own.*

1. **The route is a circuit, which F15 did not say.** `Route` is `marks` and
   `laps` and carries no start point, so leg `i` runs from mark `(i − 1) mod n`
   to mark `i mod n` and **leg 0 comes from the last mark**. With `laps > 1`
   that is literally where the boat has just been; with `laps == 1` it is a
   convention, and a course that wants a distinct start makes the start a
   mark — which is also how a start line is spelled, as a `Rounding::Gate`.
   The alternative, orienting mark 0's plane by the *outgoing* leg, would
   have made lap 1 and lap 2 disagree about the same mark.

2. **The single-mark route of F15.1 arrives at a disc, and that is the only
   place in the crate where proximity decides anything.** A dragged lookahead
   point has no incoming leg, so it has no perpendicular plane and no side;
   `Leg::from` is `None`, `Guidance::line` is `None`, and passage is the
   step's segment reaching the mark's disc. F15.3's prohibition is not
   weakened by it, because `Route::validate` **refuses** `Port`, `Starboard`
   or `Gate` on a route with no incoming leg — so the disc branch can never
   be reached by a mark that has a side.

3. **`radius` earns its place in the sided clause.** F15.3 forbids a radius
   check as the passage test and says nothing about what `Mark::radius` is
   for. Here it is the **clearance**: a sided crossing must be at a lateral
   offset of at least `radius` on the required side, so grazing the buoy
   tangentially counts and sailing over the top of it does not. A mark's size
   therefore constrains the passage without ever deciding it.

4. **Cross-track error is positive to port of the leg's direction of
   travel** — `d̂ × (p − a)` with `Vec2::cross`. That is the side `+y_H`
   points to (F2) for a boat on the leg's heading, which is why that sign was
   chosen rather than its negation. It is computed in `guidance.rs` and
   nowhere else (RV20), and the mirror of a route and a state negates it
   **bit for bit**, with one stated carve-out: `−(+0.0)` is `−0.0`, whose bits
   differ from the `+0.0` a mirrored zero produces, so at zero IEEE equality
   is asserted instead.

5. **A gate is oriented by its leg, with one documented fallback.** The
   crossing direction comes from the sign of `d̂ · n̂_gate`, as a **three-way**
   `partial_cmp`; a leg exactly parallel to the gate line gives no
   orientation, and the boat's own displacement is then what says which way
   "through" is. That is the `delta_r_self_centre` trap of F16.3 in a new
   place, and it is handled rather than asserted away.

6. **Tasks 4.5 and 4.6 are absent, not disabled.** `brief.md` S6 is deferred
   and `README.md` V-F excludes them from default delivery, so there is no
   `obstacle.rs`, no `raycast.rs` and no feature flag standing in for them
   (RV23, section acceptance 6).

7. **There is no WASM surface, no scenario field and no recording field.**
   F8.2 is unchanged and no route can yet be set from the browser; the PRD
   tracks that as a debt for the section that adds the picker UI.

8. **The gate's step 3 grew again**, to
   `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course`. The
   step count did not change, so `[ValidateRange(1, 11)]` is untouched.
   Sections 05 and 06 extend the same list rather than each adding a step.

---

## F16. Conformance, digests and the tolerance contract

*Implemented by section 02 on 2026-09-21, after 08 and 10; section 03 remains
proposed. The implementation decision brief §5 asks for — the selected S row
(S1), the exact F deltas, the decision source and date, and the validation
performed — is recorded in [`progress/02-handoff.md`](progress/02-handoff.md)
§1, which brief §5 names as one of the two places it may live. **F16.9 below
records where the implementation departed from F16.1–F16.8 and why.***

### F16.1 Bit-identity across stacks is not available, and is not claimed

F9's guarantee is already scoped to "the same build on the same platform".
Cross-**stack** is strictly harder: XLA reassociates and fuses arithmetic, a
fused multiply-add is not a multiply then an add, GPU reductions are not
order-deterministic unless forced, and JAX defaults to f32.

**No conformance test may assert equality across stacks.** What is asserted is
a tolerance, tiered by how much error has had a chance to accumulate.

### F16.2 Quantity-specific tolerance contract

Conformance is implementation agreement, not real-world model validation. Record finite-input domains, units, absolute/relative bounds, near-zero handling and derivation per column. No “32 ulp relative” ambiguity: an ULP distance and a relative error are distinct metrics. An initial 32-ULP budget is a proposal for well-scaled tier-0 values, not a universal pass threshold near cancellation.

| Tier | Comparison | Required evidence |
|---|---|---|
| 0 | Pure functions | Absolute floor near zero plus relative/ULP bounds away from zero; separately measured custom-kernel/reduction error over the tested domain |
| 1 | Derivative components | Direct component error with derivative units; no dimensionally invalid reuse of a state-error tolerance |
| 2 | 1–5 s trajectories | Fresh dt/dt2/dt4 reference study after 08, per scenario and state quantity; target port error below 10% of measured reference discretization error with justified numerical floors |
| 3 | Port invariants | Each invariant's actual criterion plus branch/event tests; agreement is scoped to tested regimes |

RK2 local state error is O(dt³), global error O(dt²) in smooth regimes. Branch crossings need dedicated event-aware evidence. Long divergent capsize trajectories are not a pointwise oracle. A tolerance change requires a new derivation, never merely a green port test.

### F16.3 Sampling is dense at the branch points, not uniform

Uniform sampling over the state space almost never hits the places a port
actually differs. Each of the following is a named sampler and a test case, and
each is sampled at, around and **exactly on** the boundary:

| Branch | The trap |
|---|---|
| `limit_rate` (`dynamics.rs`) | a **strict** inequality whose strictness is load-bearing and documented at its source |
| `T` with explicit slack boundary from 08 (`mainsheet.rs`) | the unilateral sheet; sample `e ≈ 0` and the damping-dominated case where the bracket goes negative with `e > 0` |
| `delta_r_self_centre` (`dynamics.rs`) | a **three-way** `partial_cmp` including the exact-zero case; a two-way `where` is wrong at zero |
| stall blend (`foil.rs`) | `alpha_stall ± stall_blend`, where two smooth pieces are joined |
| `wrap_pi` (`frames.rs`) | exactly `±π`, the half-open `(−π, π]` convention, and the bit-identical in-range fast path |
| `phi` unwrapped (`state.rs`) | `wrap_angles` leaves `phi` alone (brief §17); a port that wraps everything breaks capsize |
| zero-flow foil (`foil.rs`) | the `EPS_FLOW` guard; `0/0` is NaN in one stack and a guarded zero in the other |
| `sheet_release` | overrides the analogue command — precedence, not addition |

### F16.4 The bundle is data, keyed by a digest

```
conformance/<digest>/
├── manifest.json        toolchain, digest, generator version,
│                        per-tier tolerances and their justification
├── parameters.json      the F7 catalogue, verbatim
├── wind_modes.json      the precomputed spectral modes per (config, seed)
├── tier0_<fn>.npy
├── tier1_derivative.npy
└── tier2_trajectories.npy
```

Three properties are normative:

1. **One runner per stack, consuming the same bundle — including Rust.** The
   Rust runner is not redundant: it is what catches a *stale bundle*, which is
   otherwise the failure that sends everyone hunting a phantom port bug.
2. **Keyed by complete bundle identity.** Parameter or equation changes invalidate it; a stale bundle refuses.
3. **Freshness checking is a gate step** (F12′ step 4). A physics change that does not
   regenerate the bundle fails, the way `cargo fmt --check` fails.

The bundle key must cover the model/source identity from 08, resolved parameters, integrator/dt, fixture inputs/wind modes, generator/schema and tolerance-contract versions. Reuse 10's canonical records. A parameter-only key cannot reject changed equations. Canonical-record equality is sufficient for comparisons; if a compact artifact key is needed, use an established SHA-256 implementation, not handwritten cryptography. Serialization/order is explicitly versioned.

Observation identity includes ordered fields, units, bounds, normalization, sensor config/noise/privilege and versions. Action identity includes adapter/gains/engagement semantics/cadence. Experiment identity also includes initial state/controls, task/thresholds, wind/seed and outcome rules. Keep the full records beside optional digests. Unknown metadata prevents strict comparison while permitting display.

Verification must reject stale records even if the directory and manifest agree with each other. Compare against the selected source/contract identity; toolchain-incompatible bit checks are explicitly inconclusive, never certified passes. Regeneration is an explicit command; the gate checks freshness without rewriting committed artifacts.

### F16.5 Parallelism

F9.6 forbids parallelism **inside a single simulation step** and is unchanged.
Stepping N **independent** boats on N threads is bit-identical, because they
share no accumulator, and is permitted in `sailgym-env` and `sailgym-bench`.

It stops being permitted the moment boats interact through a shared field. If
that ever arrives, the parallel path returns to a fixed index order, or to an
explicit two-phase sample-then-step.

**`rayon` may not appear in `sailgym-physics`.**

### F16.6 The wind kernel is not `libm`

`environment/wind.rs`'s `wave` is a hand-rolled cosine — Cody–Waite reduction
and a polynomial — agreeing with `f64::cos` only to about `1e-14`, and
`sample_inner` accumulates in a fixed two-slot pairwise order required by F9.4.
A port calling a library cosine with a vectorised reduction reproduces neither.

Ports therefore ship **two arms**: one reproducing the kernel and the summation
order verbatim, held to the tier-0 tolerance; and one using the host library's
cosine, held to a **measured** bound. The divergence between the two arms is
reported as a number, not as a pass or a fail.

### F16.7 The wind modes are data, and the RNG is never ported

`ProceduralWind::new` draws `κ_k`, `ϕ_k`, `ω_k` **once at construction** and
stores them; `sample` is a pure, branch-free sum over that table. The table is
exported in the bundle. **No stack other than Rust implements PCG32**, and no
conformance test compares RNG streams across stacks.

### F16.8 f32 is a different environment until measured

Conformance runs in f64. If a stack trains in f32, the f32↔f64 divergence is
measured on the same bundle and **reported as a number**. If it exceeds the
cross-stack tolerance, the f32 environment is not the environment the reference
describes, and policy transfer between them is an open question rather than an
assumption.

If f32 proves inadequate, the escalation is **not** to raise `c_sheet` or lower
`k_sheet` — brief §43 forbids it and R1's mitigation order is explicit. Sub-step
the rigging DOF in the port, or run that DOF in f64.

---

### F16.9 What section 02 implemented, and where it departed

*Recorded here because F16.1–F16.8 were a proposal and this is what they
became. The evidence is [`conformance.md`](conformance.md),
[`throughput.md`](throughput.md) and
[`progress/02-handoff.md`](progress/02-handoff.md). **F3, F4, F5, F6, F7, F8
and F9 are unchanged** — no equation, no coefficient, no state field and no
summation order moved; `git diff` over `forces/`, `dynamics.rs`,
`integrator.rs` and `parameters.rs` is empty, and `--test regression` measures
a worst `|Δ|` of exactly `0.0`.*

1. **The bundle is `conformance/<key>/`**, `.npy` v1.0 rather than the
   discussion note's `.npz`: `.npz` is a zip container and this workspace's
   dependency graph is `serde`, `serde_json` and a test-only `sha2`. Every
   `.npy` is 2-D `<f8`, leading columns inputs and trailing columns outputs,
   with the column names in `manifest.json`. The debt is tracked in the
   section PRD and is repaid only if a stack appears that cannot read `.npy`.

2. **`sha256_hex` is `sha2`, an optional dependency behind the `testkit`
   feature.** F16.4 requires "an established SHA-256 implementation, not
   handwritten cryptography"; gating it on `testkit` keeps the default and
   `wasm-pack` builds on the dependency graph they had.

3. **The key covers the bundle's *contract*, and deliberately excludes
   `model.source`.** F16.4 asks the key to cover the model/source identity;
   section 02 keys on the declared `model_version` plus every fixture's
   **data digest**, and records the full `ModelIdentity` — `state` included —
   in the canonical record beside it. The reason is that a source tree id
   changes on every commit that touches `crates/sailgym-physics/src`,
   including the commit that adds the bundle, so a directory keyed on it
   would be stale the instant it was committed; whereas a changed equation
   changes the *numbers*, which changes the data digest, which changes the
   key. That is strictly stronger than a source id for RV10's purpose, and
   `digest::tests::an_equation_change_with_identical_parameters_invalidates_the_bundle`
   measures it. `BundleIdentity::is_release_baseline` is false for a dirty or
   unknown source, so such a bundle still cannot certify a release.

4. **Tolerances are measured, per tier, and none is retyped.** F16.2's rule is
   stated once per tier in the manifest; the numbers that instantiate it are
   measured on the generating build. Tier 2 is a **fresh** dt/dt2/dt4 study on
   section 08's model, per case and per state quantity, with the bound at 10 %
   of the measured reference discretization error and a stated numerical
   floor. **No order is asserted anywhere**, because F18.1b's tension law is
   discontinuous at take-up. Nothing is read from `docs/v1/convergence.md`.

5. **F16.6's gap is now a measurement, not a characterisation.** F16.6 says
   the `wave` kernel agrees with `f64::cos` "to about `1e-14`". Measured over
   `θ ∈ [−10⁵, 10⁵]` at 700 000 points the worst gap is **4.44e-16** — about
   two ULP of the result, two decades tighter than the clause's figure. The
   clause's number was a bound and remains a safe one; the manifest carries
   the measurement, and section 03 consumes that rather than choosing its own.
   The fixed two-slot pairwise reduction of `sample_inner` differs from a
   Kahan-compensated sum of the same modes by at most **8.88e-16 m/s**.

6. **F16.5's parallelism argument is now a fact.** `rayon` entered
   `sailgym-bench` only; `crates/sailgym-bench/src/bin/vec_bench.rs` asserts
   that the parallel run's final states are `to_bits()`-identical to the
   serial run's at every N and every thread count, and
   `determinism.rs::no_rayon_in_physics` is not needed because
   `no_wall_clock`'s existing scan already covers the crate — the dependency
   simply is not there, and `sailgym-physics/Cargo.toml` is the record.

7. **There is no tier-3 runner**, as the section PRD scoped. The brief §35
   invariants exist for Rust in `tests/invariants.rs`; agreement claimed by
   this bundle is scoped to the regimes its samplers visit.

8. **One consequence for every later section, found and measured by the
   gate.** The two section-11 browser tests that assert
   `data-verdict="same_conditions"` are red for **any** uncommitted edit
   under `crates/sailgym-physics/src`, because F18.1d makes a dirty identity
   comparable with nothing, including itself. That is the contract working as
   written, not a defect, but it means **a section that touches physics
   source cannot see a green step 9 until its work is committed**. Measured:
   the same source committed in a throwaway worktree runs step 9 at 349
   passed, exit 0, against 343 passed / 6 failed uncommitted.
   `docs/v2/progress/02-handoff.md` §7 records the method and the command
   that follows the commit.

---

## F17. The Python boundary

*F17.1 implemented by section 03 on 2026-09-21; F17.2–F17.5 remain proposals
for section 07. **F17.6 below records what section 03 implemented and where
it departed.***

### F17.1 The F8 rule, restated

**Binding/wrapper Python contains no physical equations or independently defined parameter/layout values.** Rust exports those contracts. The explicit exception is `python/sailgym_jax`: it is an independent verification implementation and therefore contains the equations it tests. Its kernel constants cite the Rust source; physical parameters come from the identified bundle. This exception does not extend to `python/sailgym` wrappers.

Audit the two scopes separately with non-vacuity checks. A named constant is not automatically valid merely because it is named. Independent ports may share model mistakes; conformance remains distinct from physical validation.

### F17.2 Two independent boundaries

`sailgym-py` binds `sailgym-env`. It does **not** bind, wrap or reuse
`sailgym-wasm`. The two boundaries have different performance shapes and
different lifetimes and are kept apart deliberately.

### F17.3 Seeding

`reset(seed=k)` maps straight onto the Rust `u64`. Gymnasium's `np_random`
is used for **nothing that touches the simulation** — not wind, not sensor
noise, not scenario sampling. A randomised scenario derives from the same `u64`
through `STREAM_SCENARIO`. Two RNGs feeding one episode is how a deterministic
environment stops being reproducible.

### F17.4 Episode boundaries are authoritative in Rust

`Outcome` is decided in Rust. Gymnasium's `TimeLimit` wrapper is **not** used:
the vectorised path cannot use Python wrappers, so a wrapper-supplied truncation
would make the single-env and vector paths disagree about episode boundaries —
silently, in the returns.

The autoreset convention (whether the reset observation appears on the step that
reports `done` or the next one) is **pinned once, in Rust**, checked against the
convention of the installed Gymnasium version rather than assumed, and asserted
numerically: discounted returns for a fixed action sequence computed both ways
must agree.

### F17.5 Performance non-negotiables

The low-level batch API uses caller-owned contiguous `numpy` buffers, writing into caller-provided arrays in the shape
`sample_wind_grid` already establishes; the GIL **released** for the duration of
`step_all`; and avoids per-environment object churn. The public Gymnasium adapter still returns its required tuples/dicts; measure its overhead separately. Stable buffer addresses prove reuse, not absence of temporary allocations. Releasing the GIL enables Python concurrency; Rust rayon can execute internally even while the caller holds it.

`obs_out: &mut [f32]` does not violate F9.5, which forbids f32 **intermediates
in physics**. This is an output buffer, exactly like the wind grid's.

### F17.6 What section 03 implemented, and where it departed

*Recorded here because F17.1 was a proposal and this is what it became. The
evidence is [`conformance.md`](conformance.md) and
[`progress/03-handoff.md`](progress/03-handoff.md). **No Rust file changed**:
`git diff --name-only crates/` is empty for the whole section, which is its
acceptance criterion 3.*

1. **The two scopes are audited separately, by
   `python/tests/test_no_stray_constants.py`**, in the three tiers of
   `provenance.rs::no_stray_constants`: a float literal is structural
   (`0.0`, `1.0`, `0.5`, `2.0`), or the right-hand side of a module-level
   named constant carrying a doc string, or in an exemption table with a
   stated reason. The exemption table is **empty**, and that is the intended
   steady state.

2. **F17.1's "kernel constants cite the Rust source" is enforced literally.**
   A second pass requires every named constant that introduces a number to
   name a `<file>.rs:<line>` in its doc string, and asserts the cited file
   exists. Fourteen constants in `sailgym_jax/wind.py` carry such a
   citation. The **line numbers are not checked against their content** and
   will drift if `wind.rs` is edited; that is a known limitation, recorded in
   the handoff, and the cheaper alternative — citing only the file — would
   have made the citation unfalsifiable.

3. **The audit carries two anti-vacuity floors and a proven failure**, both
   copied from the Rust audit: `files > 3` and `scanned > 12` (currently 4
   and 19), and the demonstration that pasting `1.225` into `wind.py` makes
   it red at the file and line.

4. **A fourth pass asserts the absence of the mode-table generator** under
   `python/sailgym_jax/` — the four words F16.7 and RV15 name. That is
   section 03's acceptance criterion 6, and it is why those words appear
   nowhere in that package, not even in a comment saying they must not.

5. **The expected bundle identity lives in the test suite, not in the
   loader.** F16.4 requires verification against "the selected
   source/contract identity", and F17.1 forbids wrapper scope from carrying
   independently defined parameter or layout values — `dt` is a number Rust
   owns. `Bundle.load(root, expect=...)` therefore takes the expectation as a
   required keyword argument with no default, and `python/tests/conftest.py`
   is where this suite states which bundle it was written against.

6. **`jax_enable_x64` is enabled and asserted at `conftest` import** (RV14),
   and the loader returns `float64` arrays explicitly, refusing any fixture
   whose stored dtype is not `float64`.

## F18. Playable milestone contracts (proposed)

### F18.1 Corrected reduced model — section 08

Retain 13 state scalars and force-based RK2 motion. Resolve GZ peak/root/domain constraints, unintended sheet preload and positive slack tension in the shared physics modules. Record exact changed F6/F7 clauses, equation/parameter rationale, before/after evidence and a new model/source identity. Do not choose replacement coefficients by tutorial success. No new degrees of freedom or empirical certification is implied.

The three deltas below are **resolved** and implemented by section 08. Their
evidence is [`physics-validation.md`](physics-validation.md); their handoff is
[`progress/08-handoff.md`](progress/08-handoff.md). F3 (state and controls),
F4 (equations of motion), F5 (the foil model), F8 (the WASM surface) and F9
(determinism) are **unchanged** — `STATE_LEN` is still 13, the snapshot layout
is untouched, and no physics moved to TypeScript.

#### F18.1a — D1: the righting-arm curve (replaces F6.7's fit)

`GZ` is a **four**-term odd harmonic series, solved once at parameter-build
time from four constraints instead of three:

```
GZ(φ)   = Σ_{n=1..4} c_n · sin(n φ)
GZ'(φ)  = Σ_{n=1..4} n · c_n · cos(n φ)
∫₀^φ GZ = Σ_{n=1..4} c_n · (1 − cos(n φ)) / n

Σ n·c_n            = GM        slope at the origin
Σ c_n sin(n φ_p)   = GZ_max    the peak's value
Σ n·c_n cos(n φ_p) = 0         the peak is a stationary point       ← the new constraint
Σ c_n sin(n φ_v)   = 0         the vanishing angle
```

The human-facing tunables are unchanged: `GM`, `φ_p`, `GZ_max`, `φ_v`.
`GZ(0) = 0` and oddness are structural. The harmonics are built from one
`sin_cos` by the Chebyshev recurrences, so every term of `GZ` still carries
exactly one factor of `sin φ` and `gz(−φ)` is the **bit-exact** negation of
`gz(φ)`. `gz`, `dgz` and `gz_integral` are the one curve, its analytic
derivative and its analytic integral — never three approximations.

**Supported heel domain: `φ ∈ [−π, π]`**, extended to every real `φ` by the
curve's own `2π` periodicity (`state.phi` stays unwrapped, F3). A valid
configuration has exactly three equilibria per half-turn: `φ = 0` stable,
`φ = ±φ_v` **unstable**, `φ = ±π` stable (turtled). Righting a turtled boat
remains deferred, so `±π` is an accepted end state.

`GzCurve::fit` **rejects**, naming the offending field:

1. `GM` or `GZ_max` not finite and positive, or `0 < φ_p < φ_v < π` violated;
2. a singular system;
3. `GZ ≤ 0` anywhere on `(0, φ_v)`;
4. `GZ` not unimodal on `(0, φ_v)`, or its maximum not at `φ_p`;
5. `GZ ≥ 0` anywhere on `(φ_v, π)` — **no positive stability between the
   vanishing angle and inversion**;
6. `|GZ(φ)| > hull.beam / 2` anywhere — a righting arm is a horizontal lever
   between two points inside the hull, so half the beam is a generous
   geometric envelope.

Rule 6 makes the fit depend on `hull.beam`, so the one entry point is
`GzCurve::fit_catalogue(&BoatParameters)`; `GzCurve::fit(gm, φ_p, GZ_max, φ_v, gz_limit)`
remains for tests and for callers that hold the four tunables directly. Every
path that admits parameters from outside the crate — `Scenario::to_parameters`,
`Simulation::set_parameter`, `Simulation::set_parameters` — calls it, and a
rejected edit leaves the previous simulation and catalogue intact.

Rule 4 **supersedes** F6.7's "reject parameter sets that produce a
non-monotonic `GZ` on `[0, φ_p]`" together with its "the peak is pinned in
value but not exactly in location". Those two clauses contradicted each other
while the peak's location was unconstrained; the fourth harmonic pins the
location, so both now say the same thing. The contradiction recorded in
`docs/v1/progress/07-handoff.md` is hereby closed.

#### F18.1b — D2: the unilateral sheet (replaces F6.8's `T`)

```
T = if e > 0 { max(0, k_sheet·e + c_sheet·ė) } else { 0 }
```

**The boundary is `e > 0`, strictly.** The slack set `{e ≤ 0}` is closed and
carries `T = 0` on all of it — `e = 0` included, for either sign of `ė` and for
`|ė|` arbitrarily large. Stiffness and damping are both properties of stretched
rope; an element at its natural length transmits nothing. The `max` is retained
and still load-bearing: it is what stops a **taut** rope from pushing while it
is eased faster than it is stretched.

`T` is therefore discontinuous at take-up (`T → c_sheet·ė` as `e → 0⁺`,
`T = 0` at `e = 0`). That is the contact-impact discontinuity every unilateral
spring–damper has. Convergence across it is measured with event-aware checks,
never with a smooth-order assertion.

**Geometry-consistent minimum.** With `a = mast.x − block.x`,
`b = mast.y − block.y`, `h = mast.z + z_boom − block.z`, `R = √(a² + b²)`:

```
ℓ(β)² = a² + b² + h² + d_sheet² − 2·d_sheet·R·cos(β − atan2(b, a))
β_min = atan2(b, a)                    (for d_sheet > 0)
ℓ_min = ℓ(β_min) = √( h² + (R − d_sheet)² )
```

`rigging::mainsheet::min_rope_path(&p) -> (ℓ_min, β_min)` is the single
definition and returns `rope_path_length(β_min, &p)`, so the bound and the path
are the same evaluation. `BoatParameters::validate` requires
`ℓ_min ≤ l_sheet_min < l_sheet_max`, and `Scenario::validate` requires
`l_sheet_min ≤ initial_state.sheet_length ≤ l_sheet_max` against the scenario's
own resolved catalogue. **Prestretch is not modelled**: a rope shorter than the
path it must follow is a rigging fault, not a trim setting.

#### F18.1c — F7 parameter deltas

Two F7 defaults change. Both are ASSUMED values whose previous numbers are
shown infeasible in `physics-validation.md` §1.5 and §2.1–2.2; neither was
chosen from a scenario, a tutorial or a demonstration (v1 brief §43).
`tests/provenance.rs::shipped_values_match_the_f7_table` reads the table below
as an explicit override of the F7 table in `docs/v1/00-foundations.md`; every
other F7 row is still compared against F7 itself.

<!-- BEGIN F7-OVERRIDES -->

| Path | v1 (F7) | v2 | Tag | Reason |
|---|---|---|---|---|
| `stability.gm` | 1.00 | 0.55 | ASSUMED | 1.00 m makes `φ_p` a local **minimum** of the four-constraint curve (maxima at 31.80° and 61.20° straddle it), so rule 4 rejects it. The admissible interval for F7's `φ_p`, `GZ_max`, `φ_v` under rules 3–6 is `[0.535, 0.561]` m; 0.55 m is the value `docs/v1/progress/07-handoff.md` and `hydrostatics::tests::consistent_curve` already recorded as self-consistent for this set. `Δ·g·GZ_max = 406 N·m` is unchanged. |
| `sheet.l_sheet_min` | 0.90 | 1.0404326023342405 | ASSUMED | 0.90 m is 0.1404 m below `ℓ_min`, the shortest path the rig can take, and F4.3 clamps `L` to it — a permanent 2.81 kN preload at the one boom angle where `dℓ/dβ = 0` and the element has no damping. The new value is `min_rope_path` for the F7 geometry, to the last bit. |

<!-- END F7-OVERRIDES -->

Two shipped scenarios move `initial_state.sheet_length` from `0.9` to
`1.0404326023342405` for the same reason. No other parameter, in F7 or in a
scenario, is changed by section 08.

#### F18.1d — D3: model identity

`sailgym_physics::identity` exposes a `ModelIdentity` record for sections 10
and 02 to reuse. It carries a declared `MODEL_VERSION` — the contract version,
bumped by hand when F6/F7 changes — and a **source** identity taken from git:
`git rev-parse HEAD:crates/sailgym-physics/src`, the content-addressed tree id
of the physics source, captured by `build.rs`. Properties:

* it changes when any physics source file's content changes, and **only** then
  — an unrelated commit that leaves `src/` alone leaves it alone;
* an uncommitted edit under `src/` marks the identity **dirty**, and a build
  outside a git checkout marks it **unknown**;
* `dirty` and `unknown` are not equal to any known baseline and must not
  authorise a comparison against one.

No cryptographic primitive is implemented here; git's own object hashing is the
implementation. Parameters travel separately: a parameter digest cannot detect a
changed equation, and an implementation identity cannot detect a changed
parameter. Both are required.

### F18.2 Input ownership — section 09

The web composes device input once into normalized Controls. Rust limits actuator rates. Release overrides sheet rate; each pointer owns one pad; lifecycle transitions clear transient commands. Actual-state gauges and rate feedback are distinct. Position-target control waits for explicit engaged/released semantics in a shared Rust adapter.

### F18.3 Recording and comparison — section 10

Extend the existing versioned Episode codec with the diagnostic/identity data needed for truthful display. Decode schema-1 files with unavailable fields explicit. Replay selects one recorded source for every consumer and does not recompute missing forces using current physics. Canonical identity contains model/source, parameters, initial state/controls, integrator/dt, wind/seed and applicable task/action/observation contracts. Unknown is not equal to known. Sampled inspection is not action resimulation.

### F18.4 Guided tasks — section 11

A small pure sailgym-task evaluator owns practice outcomes outside physics. Evaluate on physics steps; thresholds and ordered events are versioned task data. Retry restores exact conditions, not live edited defaults. Record outcomes/events and compare only compatible attempts. Later course/env work reuses these semantics. Only two attempts are retained in session memory; no persistence framework is required.

#### F18.4a — what section 11 implemented

Recorded here because the clause above was a proposal and this is what it
became. The evidence is [`practice-validation.md`](practice-validation.md) and
[`progress/11-handoff.md`](progress/11-handoff.md). **F3, F4, F5, F6, F7, F8.3
and F9 are unchanged** — no equation, no coefficient and no frame convention
moved, `STATE_LEN` is still 13, `parameters.rs` is byte identical and
`scenarios/` is untouched.

1. **`crates/sailgym-task`** is the evaluator: three challenges
   (`get_moving`, `complete_tack`, `recover_from_heel`), `TaskSpec`,
   `TaskRun`, and `Outcome` = `Running | Succeeded | Failed(reason) |
   TimedOut`. `TimedOut` carries no reason; the **events** say what happened.
   The dependency runs `task → physics` and never the other way, which
   `cargo tree -p sailgym-physics` asserts in the gate (F14.1, F8.1).

2. **`TASK_VERSION`** is 1, and is bumped by hand whenever a threshold or an
   outcome rule changes meaning. It travels inside the episode's
   `TaskIdentity`, so `ExperimentIdentity::compare` refuses to compare two
   attempts scored under different rules (F18.3).

3. **Thresholds are task configuration, not coefficients.** Every one is
   visible through `TaskSpec::thresholds()`, recorded with the episode, and
   was chosen from scripted runs of section 08's baseline. `parameters.rs` did
   not move to make a challenge passable, and where the boat turned out not to
   do what a challenge assumed, the challenge changed
   (`practice-validation.md` §3.1).

4. **The WASM surface grew by five read-only-or-lifecycle methods**, all
   coarse-grained (brief §24): `practice_tasks_json`, `start_practice`,
   `retry_practice`, `cancel_practice`, `practice_state_json`. F8.2 is
   otherwise unchanged.

5. **Recording needed no schema change.** Section 10's reserved
   `PracticeEnvelope` is written through `Recorder::set_practice` and
   `Recorder::push_practice_event`; `EPISODE_SCHEMA_VERSION` is still 2,
   `PRACTICE_ENVELOPE_VERSION` is still 1, and `recording.rs` is byte
   identical.

6. **Retry restores the frozen initial contract** — the resolved catalogue,
   the complete F3 state, the controls, the wind configuration and the seed —
   rather than re-resolving a document. It is deliberately not `Sim::restart`,
   which keeps a live brief §31 edit. A parameter, catalogue or wind change
   during an attempt ends it as `conditions_changed`, and a reset or a
   scenario change ends it as `cancelled`; neither produces a comparable
   result.
