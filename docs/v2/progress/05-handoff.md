# v2 Section 05 — Handoff (`sailgym-agent`: sensors, actions, cadence, helm)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F14 (and the new
**F14.10**), F12′, F15.5 and F16.4, and `docs/v2/prds/05-agent.md` before
starting anything that touches agents, observations, episodes or the Python
binding.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. Exactly one file in `crates/sailgym-physics/` changed —
`src/rng.rs`, which gained one constant and one table row (D2) — so F3, F4,
F5, F6, F7, F8 and F9 are exactly as sections 08, 02 and 04 left them:
`STATE_LEN` is still 13, the F8.3 snapshot layout is unchanged,
`parameters.rs` is byte identical, `scenarios/` is untouched, and F8.2's WASM
surface is unchanged. **No agent can be selected from the browser**, and that
is a tracked debt, not an omission.

Status: **complete**; `scripts/check.sh` is green end to end (§6). Task 5.6
(`Helm`) is **deferred by the PRD itself** and nothing stands in for it (§3.2).

---

## 1. The implementation decision this section needed, and where it is recorded

The PRD opens **BLOCKED** on `docs/v2/brief.md` S3 having a recorded
implementation decision, answering `docs/v2/README.md` V-A. `brief.md` §5 says
a changed normative contract needs the selected S rows, the exact F deltas,
the decision source and date, and the required validation recorded "in the
section handoff or this table". This is that record, in the same place and the
same shape sections 02, 03 and 04 used.

| item | resolution |
|---|---|
| **decision source** | the human's dispatch of `docs/v2/prds/05-agent.md` as the section PRD, 2026-09-22 — the same act `docs/v2/README.md` records as having resolved 02's and 03's deltas on 2026-09-21 and 04's on 2026-09-22 |
| **S rows selected** | **S3** (courses and controller interface), **controller half**; section 04 took the course half. S3's "one rule sailor before polar racing or planning" is **not** delivered here: the PRD's own scope section excludes an autopilot, and S3 scopes the web integration separately |
| **D1 — F14 in full** | implemented. Where the implementation departed from F14.1–F14.9 is recorded in the new **F14.10**, not left to be inferred |
| **D2 — `rng.rs` gains `STREAM_AGENT = 4`** | implemented, additively: one constant, one module-doc table row, and `stream_labels_are_distinct` widened to compare every label against every other **and** to assert the four streams' draws themselves differ |
| **D3 — step 3 gains `-p sailgym-agent`** | implemented: `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent`, five sites moved together and **verified** (§6.1), step count unchanged so `[ValidateRange(1, 11)]` did not move |
| **D4 — `brief.md` S3** | the controller half, in; the rule sailor and the web picker, out. This table is the record, since no section-05 task owns `docs/v2/brief.md` |
| **validation performed** | 71 tests in the new crate (62 unit + 9 integration), the six committed goldens reproduced **bit for bit** through the funnel, the F9.7 identity with an agent attached over six chunkings, five demonstrations that the named guards can go red, and six full runs of the eleven-step gate (§6) |

**No signature is claimed for anything beyond that.** This section makes no
physical claim of any kind: it adds an interface, and an interface is not
validation.

---

## 2. What landed, file by file

### Task 5.1 — contracts: the crate, the traits, the stream (P-group S)

- **`crates/sailgym-agent/Cargo.toml`** — new crate. Dependencies:
  `sailgym-physics`, `sailgym-course`, `serde`, `serde_json`, and nothing
  else; `sailgym-physics` with `features = ["testkit"]` as a
  **dev**-dependency, so `mirror_state`, `uniform_wind` and `still_air` reach
  the tests and nothing that ships.
- **`src/lib.rs`** — the crate document: the three determinism traps stated
  once, what is here, and what is deliberately not.
- **`src/spec.rs`** — `ActionVec`, `Action`, `ActionSpace`, `Cadence`,
  `AgentSpec`, `AgentDebug`, the `Agent` trait, `agent_rng`, `SpecError`.
  6 tests.
- **`crates/sailgym-physics/src/rng.rs`** — D2.
- **`Cargo.toml`** (workspace) — `crates/sailgym-agent` in `members`,
  `sailgym-agent` in `workspace.dependencies`. **`Cargo.lock`** — one new
  package, **no new third-party dependency**.

### Task 5.2 — `WorldView` and the `Sensor` trait (P-group A)

- **`src/worldview.rs`** — F14.4's containment boundary. 2 tests.
- **`src/sensor/mod.rs`** — the `Sensor` trait, `FieldSpec` (name, unit,
  bounds, normalisation, noise, privilege), `sensor_stream` and `fnv1a64`.
  3 tests.

### Task 5.3 — the tier-0 sensor suite (P-group A)

- **`src/sensor/imu.rs`** (width 5), **`src/sensor/wind.rs`** (2),
  **`src/sensor/rig.rs`** (`rig_state` 4 and `actuator_state` 2),
  **`src/sensor/guidance.rs`** (5), **`src/sensor/registry.rs`**.
  31 tests.

### Task 5.4 — the observation, and `obs_digest` (P-group B)

- **`src/observation.rs`** — `ObsColumn`, `ObsLayout`, `observe`,
  `sensor_streams`, `obs_digest`, `LAYOUT_VERSION`, `to_identity`. 6 tests.

### Task 5.5 — the actuation funnel and the `rate` adapter (P-group B)

- **`src/actuation/mod.rs`** — the `Actuation` trait, `apply`, `apply_values`,
  `action_identity`, `ActionError`. 4 tests.
- **`src/actuation/rate.rs`** — the identity adapter, `action_for`, and the
  golden bit-identity tests. 4 tests.

### Task 5.6 — `Helm` — absent, not disabled

No `helm.rs`, no `Setpoint` struct, no `Action::Setpoint`, no feature flag and
no commented-out module. §3.2 is why.

### Task 5.7 — `manual`, and the F9.7 identity (P-group S)

- **`src/manual.rs`** — the external action source. 5 tests.
- **`tests/determinism.rs`** — the contract test: the driver, the three
  properties, the extended greps, the dependency assertion and the F7-literal
  audit. 9 tests.

### Task 5.8 — the gate (P-group S)

- **`scripts/check.sh`** (the `step_names` array, the `run_step` case **and**
  the comment that says why step 3 grows), **`scripts/check.ps1`** (`$Steps`
  and the same comment), **`CLAUDE.md`** (the step-3 row and the layout
  block), **`docs/v2/README.md`** (the delivery table, the status paragraph,
  the gate paragraph and V-E), **`docs/v2/00-foundations.md`** (the delta
  table, F12′ in four places, F14's status line, a clause in F14.9 and the new
  **F14.10**).
- **`docs/v2/progress/05-handoff.md`** — this file.

---

## 3. The decisions worth arguing with

### 3.1 `Action` carries a normalised vector, and `decide` takes `&[f64]`

The source discussion sketched `Action::Rates(Controls)` and
`decide(&mut self, obs: &Observation)`. Both are changed, and for the same
reason: a contract is worth more when the type system holds it.

`Action::Rates(ActionVec)` means an agent **cannot** emit `Controls`, so it
cannot go round the F14.5 funnel; `actuation::apply` is the only place a
normalised action becomes `Controls`, and the `rate` arm of a later ablation
is therefore comparable with the `angle` arm rather than being the one arm
that skipped the denormalisation step.

`decide(&mut self, obs: &[f64], rng: &mut Pcg32)` means an agent cannot reach
a `WindField`, a `Route`, a `WorldView` or a `BoatState`, because no such
value is ever in scope. That is F14.4 made structural. The PRD asks for a
`trybuild` case or a documented manual check; the manual check is §5.5, and
`spec::tests::the_agent_trait_cannot_reach_the_world` asserts the signature
permanently so the property cannot be lost by an ordinary edit.

### 3.2 `ActionSpace` has two variants and refuses one of them

F14.2 fixes the enumeration at two. The PRD says "Rates ships first.
Setpoint/Helm remains a design until an engaged/released contract and consumer
are selected; do not scaffold it." Those pull in opposite directions, and the
resolution is that **the enumeration is logged data and the implementation is
not**.

`ActionSpace::Setpoint` exists as a name, serialises as `"setpoint"`, and
`AgentSpec::validate` refuses it with an error that names task 5.6 and the
reason. Nothing beneath it is built: no `Setpoint` struct, no `Helm`, no
adapter, no `Action` variant. The alternative — a one-variant enum now and a
second variant later — would renumber the serialised form of a record that
travels in episode headers, so two runs of the same experiment either side of
task 5.6 would be incomparable for a reason that has nothing to do with the
experiment.

The engagement problem the PRD names is real and is **not** solved here: a
zero rudder-rate command currently means *released, self-centring*
(`delta_r_self_centre = true` is the F7 default), and a position servo needs
*engaged, hold*. There is no way to spell the second today, so there is no
`Helm`.

### 3.3 `WorldView` carries no `Route`, and that is the point

The discussion's `WorldView` had `course: &'a Route`. This one does not.

F15.5 §4 and RV20 require `signed_cross_track` to be computed in
`sailgym-course`'s `guidance.rs` and **nowhere else**, and the section-04
handoff §11.2 warns that "an agent that recomputes cross-track from
`Guidance::target` has created RV20's second definition, and the only thing
that would catch it is a reviewer". Removing the route from the containment
boundary answers that with a type instead of a reviewer: the guidance sensor
has no geometry to recompute from, only the number the course layer already
decided, and `cross_track_is_reported_and_never_recomputed` asserts the
reported value is that number **bit for bit**.

What the `WorldView` gained instead is `controls`, because the actuator inner
loop is part of the plant (F4.3) and `imu` needs it to evaluate the forces
afresh.

### 3.4 Privilege is per column, and exactly one column has it

F14.3 says the surviving `ObsMask` is "one flag per sensor". Section 10's
`ObservationField` already records `privileged` per **column**, and the
`guidance` sensor is the case that decides it: four of its five columns are
the task telling the boat what it is being asked to do — which a sailor is
told before the start — and the fifth, `leg_bearing_vs_wind`, is derived from
the **true** wind, which no instrument on the boat measures. A sensor-level
flag would either hide four honest columns or leak one dishonest one.

So `FieldSpec::privileged` is per column, `Sensor::privileged()` reports *any
column privileged* so F14.3's reading is still available, and
`the_suite_has_one_privileged_column` asserts that the whole tier-0 suite
contains exactly `guidance.leg_bearing_vs_wind`. The flag is not decoration:
`exactly_one_guidance_column_is_privileged_and_it_is_the_true_wind_one`
changes the wind and nothing else, and that column is the only one that moves.

### 3.5 The vane sits at the CE height, and introduces no number

The PRD says the apparent-wind sensor measures "at the masthead". F7 has no
mast height. Inventing one would put a length in this crate that looks like a
boat dimension without being one — and the user's dispatch is explicit that a
non-physical constant is governed by the same discipline as a physical one.

The vane therefore sits at `mast_pos_b + (0, 0, z_ce)`: up the mast, at the
highest **declared** point of the rig, built from two F7 values and no literal
of its own. It follows a live parameter edit rather than freezing a copy
(`the_mount_is_derived_from_f7_and_carries_no_literal_of_its_own` asserts
both). `ApparentWind::at` moves it for a study, because a sensor mounting
position is a sensor setting and not a physical coefficient (F14.9).

### 3.6 `obs_digest` is the canonical record, not a hash

F16.4 says canonical-record equality is sufficient for comparison, that a
compact key is optional, and that one must use an established SHA-256 rather
than handwritten cryptography. Section 10 made exactly this call for
`ExperimentIdentity` and ships no digest at all.

`obs_digest` therefore returns the canonical JSON of the whole `ObsLayout` —
ordered columns, each with its sensor id, that sensor's version, the column's
index within it, and its name, unit, bounds, normalisation, noise and
privilege. It is stable across runs, it changes when any sensor's version,
width or order changes, it round-trips back into an `ObsLayout` so two stored
digests can be compared field by field rather than only as text, and it brings
no second hash implementation into a crate that ships. The repository's one
SHA-256 stays where section 02 put it, behind the `testkit` feature and out of
the browser build.

The cost is honest and recorded: bounds live in `ObsLayout` and **not** in the
episode header, because section 10's `ObservationField` has no bounds field
and this section may change exactly one file in the physics crate. F16.4
requires the full record to travel beside any digest, and `ObsLayout` is that
record.

### 3.7 The release flag is a sign, not a threshold

`Controls::sheet_release` is a boolean and F14.5 requires one `[−1, 1]` box.
Encoding it as the **sign** of a third scalar makes the all-zero action
exactly `Controls::default()` — hands off, nothing commanded. A threshold
anywhere but zero would make "do nothing" a number a policy has to learn, and
would make the neutral action differ between the `rate` arm and any later arm.

### 3.8 The per-sensor substreams are built once and carried

F14.8 requires per-sensor substreams below `STREAM_AGENT`, keyed by the
sensor's own id so that adding a sensor to one ablation arm cannot perturb
another sensor's noise in the same arm. That forces a string→`u64` derivation,
which is FNV-1a 64 here, pinned against the published vectors.

It is **not** a digest and not cryptography: nothing is compared, committed or
relied on for integrity, so F16.4's SHA-256 rule is not in play. It is pinned
anyway, because changing it would silently change every noisy sensor's draws.

The streams are built once per episode by `sensor_streams` and carried across
decisions by the caller. Rebuilding them inside `observe` would hand every
decision the same draws — a noise model that does not move, which is the most
plausible way to ship a broken one and not notice. No noise model ships here;
every column declares `noise: 0.0`.

---

## 4. The measured results

### 4.1 The crate

```
cargo test -p sailgym-agent
  62 unit tests  (spec 6, worldview 2, sensor 3, imu 5, wind 5, rig 5,
                  guidance 9, registry 6, observation 6, actuation 4,
                  rate 4, manual 5, others 2)
   9 integration tests  (tests/determinism.rs)
```

### 4.2 The stop condition — `rate` against the committed goldens

```
cargo test -p sailgym-agent actuation::rate -- --nocapture
  rate: 6 of 6 goldens compared bit for bit
```

**Six of six**, not a skip: this build's toolchain matches the one the goldens
were recorded under (`rustc 1.98.1 (48a229cea 2026-09-01)` /
`x86_64-unknown-linux-gnu` / `debug`), so the R7 branch did not fire and every
sample of every golden was compared with `to_bits()` equality. The companion
test, `the_funnel_changes_no_bit_of_a_trajectory`, needs no committed file and
no toolchain match: it drives each of the six scripts twice, once straight
onto `Simulation::set_controls` and once through the funnel, and compares the
two trajectories bit for bit. That half can never be skipped.

### 4.3 The three properties the acceptance names directly

| property | measurement | result |
|---|---|---|
| F9.7 with an agent attached | `advance(3000)` vs 3000 × `advance(1)`, stub policy at cadence 10 on `close_hauled`, and `manual` every step on `free_sail` at chunks 3000, 7 and 1 | **bit identical** in all 13 state fields, and the decision log identical |
| cadence lands on episode steps | chunkings {1, 3, 7, 10, 13, 200} over 1000 steps of `tack` | decisions at exactly `{0, 10, …, 990}` in **all six**; period 7 gives `{0, 7, …, 994}`, equally chunk-independent |
| manual and policy share the path | the same five-action script, once from a policy and once pushed in from outside, 800 steps of `gybe` | **bit identical** trajectory and decision log; a different pushed action changes the trajectory, so the agreement is not the agreement of two things that ignore their input |

### 4.4 The observation

| property | measurement |
|---|---|
| layout length | 18 columns = 5 + 2 + 4 + 2 + 5, and `ObsLayout::names()` is a `Vec<String>` |
| digest stability | equal across three independent builds of the same suite |
| digest sensitivity | changes on a sensor-version bump, a width change, a suite reorder **and** a field swap inside one sensor |
| additivity | adding `guidance` to `{imu, apparent_wind}` appends 5 columns and leaves all 7 preceding columns equal |
| chunk independence | the full 18-column vector at step 500 is bit identical across chunks {1, 3, 7, 10, 13, 200, 500}; the `imu` block alone likewise |
| privilege | exactly one privileged column in the suite, `guidance.leg_bearing_vs_wind`, at index 16 |

### 4.5 Sensor semantics, asserted rather than described

Every sensor is checked for what it emits, not only for how wide it is:

- **No absolute pose reaches the policy.** By name — no sensor emits `x`, `y`,
  `psi`, `heading`, `position`, `latitude`, `longitude`, `north` or `east` —
  **and by value**: translating the boat 500 m leaves the `imu` and the rig
  sensors bit identical, rotating the boat and the field together leaves the
  `imu` unchanged to 1e-9, and translating **and** rotating the whole problem
  (boat, route and wind) leaves every `guidance` column unchanged to 1e-9.
- **Every sensor mirrors** (R3). Port/starboard mirroring negates the signed
  columns and leaves the magnitudes: roll rate, yaw rate, heel and sway
  acceleration but not surge acceleration; `β`, `β̇`, `δr` and the rudder rate
  command but not `l_sheet` or the sheet slack; AWA but not AWS; cross-track,
  bearing, leg-versus-wind and the rounding side but not the distance.
- **The vane feels the rotation term of F6.2**: a boat spinning on the spot in
  still air measures 0.6 m/s at the mast, because the mast is moving. Omitting
  `ω × r` is the silent physics bug F6.2 warns about, and it would read zero.
- **`actuator_state` distinguishes the two cases that matter**: the same
  `δr = 0.3` with `rudder_rate_cmd = +1` and with `−1` read differently, which
  a policy given only the angle could not tell apart.

### 4.6 The dependency direction

```
$ cargo tree -p sailgym-physics --edges all | grep -c -E 'sailgym-(agent|course|task)|wasm-bindgen'
0
```

Asserted twice in the gate — `spec::tests::physics_does_not_depend_on_the_agent_crate`
in step 3's unit tests and `physics_depends_on_no_v2_crate` in its integration
tests — so it runs on every gate run rather than living in review.

---

## 5. Proven able to fail — five demonstrations, and their revert

The PRD's acceptance asks for one: "the F9.7 test is demonstrated able to fail
by keying cadence off a per-call counter, and reverted". There are five,
because four other guards in this section are the kind that pass for the wrong
reason. Each was applied, measured and reverted; the suite is green afterwards
(§4.1, §6).

### 5.1 Cadence off a per-call counter (RV26) — the one the PRD names

`Episode::advance` was changed to test `self.cadence.decides_at(per_call)`,
where `per_call` counts iterations **within one `advance` call**.

```
test cadence_lands_on_episode_steps_whatever_the_chunking ... FAILED
  decisions did not land on the episode's own step multiples
  left:  [0, 1, 2, 3, …, 999]
  right: [0, 10, 20, 30, …, 990]
test advance_n_equals_n_advance_1_with_an_agent_attached ... FAILED
  stub at cadence 10: field x — 1.6767586502751926 vs 1.6142360192291008
test manual_and_policy_actions_take_the_same_path ... FAILED
```

Three of the eight went red. Reverted.

### 5.2 An observation built once per `advance` call (RV25)

The driver was changed to capture the state once at the top of `advance` and
build every observation inside that call from it — which is exactly what
reading `Simulation::forces` from inside the step loop would give: a breakdown
up to `n` steps stale.

```
test advance_n_equals_n_advance_1_with_an_agent_attached ... FAILED
  stub at cadence 10: field x — 1.608294093008884 vs 1.607679910709557
test cadence_lands_on_episode_steps_whatever_the_chunking ... FAILED
```

Note which test stayed **green**: `the_observation_does_not_depend_on_advance_chunking`,
which observes once outside any driver. The two tests catch different things
and both are needed. Reverted.

### 5.3 The `rate` adapter one ULP off (RV32)

`rudder_rate_cmd: a[0] * (1.0 + f64::EPSILON)` — the smallest perturbation the
adapter could make.

```
test rate_reproduces_the_committed_goldens_bit_for_bit ... FAILED
  free_sail: sample 51, field phi: 0.4690091966187138 vs recorded
  0.46900919661871376 — the `rate` adapter perturbed a bit, and every
  committed golden is now wrong (RV32). This assertion has no tolerance to
  loosen.
  left: 4602120538490407421   right: 4602120538490407420
```

One bit in the last place, caught at sample 51 of 151. `the_funnel_changes_no_bit_of_a_trajectory`
and `the_adapter_is_the_identity_onto_controls` went red with it. Reverted.

### 5.4 A manual source with its own semantics (RV27)

`Manual::decide` was given a "gentler for humans" half-rate rudder — the exact
shape of divergence RV27 names, and one that no source-pattern ban would
catch because there is no branch on "is an agent attached".

```
test manual_and_policy_actions_take_the_same_path ... FAILED
  manual vs policy: field x — 1.1059846703265808 vs 1.1331710020966623
```

Reverted.

### 5.5 An F7 coefficient copied into the agent crate (RV28)

`pub const SMUGGLED: f64 = 7.06;` — the sail area — added to `manual.rs`.

```
test no_f7_literal_appears_in_the_agent_crate ... FAILED
  an F7 coefficient has been copied into the agent crate (RV28, brief §43):
  crates/sailgym-agent/src/manual.rs:44: 7.06 in `pub const SMUGGLED: f64 = 7.06;`
```

Reverted.

### 5.6 The compile-fail check for F14.4 (task 5.2)

Not a revert, because it never compiled. An `Agent` implementation that tries
to reach the world out of its observation:

```rust
fn decide(&mut self, obs: &[f64], _rng: &mut Pcg32) -> Action {
    let _wind = obs.wind.sample(0.0, 0.0, 0.0);
    let _route = obs.guidance;
    …
}
```

```
error[E0609]: no field `wind` on type `&[f64]`
error[E0609]: no field `guidance` on type `&[f64]`
error: could not compile `sailgym-agent` (test "probe_tmp") due to 2 previous errors
```

The PRD permits "`trybuild` or a documented manual check"; this is the
documented manual check, and it costs no dependency. The probe file was
removed. `spec::tests::the_agent_trait_cannot_reach_the_world` is the
permanent half: it asserts `decide` takes `&[f64]` and that the trait body
mentions no `WorldView`, `WindField`, `Route`, `BoatState` or `Guidance`.

---

## 6. The gate

Six full runs, one baseline and one after each group, plus the two runs RV52
cost (§7). Step 9 is the browser suite and dominates every one of them.

| run | tree | wall | result |
|---|---|---|---|
| baseline | `f045240`, clean, before any edit | 756 s | all eleven ok |
| after group S (5.1) | the crate, `spec.rs`, `rng.rs` — **uncommitted** | 773 s | steps 1–8, 10, 11 ok; **step 9 red** (RV52, §7) |
| after group S (5.1) | the same tree, committed, build script **not** re-run | 771 s | step 9 red again (RV52's second half, §7) |
| after group S (5.1) | the same tree, committed, build script re-run | 755 s | all eleven ok |
| after group A (5.2, 5.3) | + `worldview.rs`, the five sensors, the registry | 756 s | all eleven ok |
| after group B (5.4, 5.5) | + `observation.rs`, `actuation/` | 754 s | all eleven ok |
| after group S (5.7) | + `manual.rs`, `tests/determinism.rs` | 757 s | all eleven ok |
| after group S (5.8) | + the gate edits, step 3 now four crates | GATE58 | all eleven ok |

**The baseline matters and it was measured**, not assumed: the PRD says to
compare no-change guards against this section's starting revision, and at
`f045240` with a clean tree the whole chain was already green. No pre-existing
failure was found, which is what makes §7's red runs attributable.

### 6.1 The five sites, moved together — and verified

v2 F12′ requires every site that spells the chain out to move together "or the
gate lies about itself". All five moved, and the result was **verified rather
than asserted**, as section 03 did: both scripts were parsed and their
step-name lists diffed.

```
check.sh : 11 steps
check.ps1: 11 steps
 1 == cargo fmt --check
 2 == cargo clippy --all-targets -- -D warnings
 3 == cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent
 4 == cargo test -p sailgym-physics --test invariants --test no_shortcuts …
 5 == cargo test -p sailgym-physics --test regression
 6 == wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
 7 == pnpm --dir web typecheck
 8 == pnpm --dir web test:unit
 9 == pnpm --dir web test:e2e
10 == uv run ruff check python && uv run ruff format --check python
11 == scripts/py-test.sh
ValidateRange: [ValidateRange(1, 11)]
both scripts' step-3 actions agree with their names
```

The `$E2E` splice in `check.ps1` is resolved to its non-`-Fast` form, which is
what the comparison should use.

| site | what changed |
|---|---|
| `scripts/check.sh` | the `step_names` array, the `run_step` case **and** the comment that says why step 3 grows |
| `scripts/check.ps1` | the `$Steps` entry and the same comment |
| `CLAUDE.md` | the step-3 table row, and `crates/sailgym-agent/` in the layout block |
| `docs/v2/README.md` | the delivery table, the status paragraph, the gate paragraph and V-E |
| `docs/v2/00-foundations.md` F12′ | the delta table, the chain listing, the opening paragraph, the "step 3 grows" clause and the retained-entries clause (now **four** entries) |

`[ValidateRange(1, 11)]` is **deliberately unchanged**: the step count did not
move, so RV17 does not arise.

### 6.2 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` here, as in sections 01, 02, 04, 08, 09, 10 and
11. `check.ps1` **was** edited, so the edit is **unverified as an execution** —
but it is not unverified as text: §6.1 parses both scripts and diffs their
step lists, which is a stronger check than section 04 made of the same
one-entry substitution. Recorded rather than glossed.

---

## 7. RV52 fired, and it has two halves

`docs/v2/progress/04-handoff.md` §11.8 predicted this: "`build.rs` still has
RV52 … It will fire again for the next section that does [touch physics
source]." This is that section, and the prediction was right. Both halves are
recorded because the second one is not obvious and cost a full gate run.

**The defect.** `crates/sailgym-physics/build.rs` bakes
`git status --porcelain -- src` into the binary as `SAILGYM_SOURCE_STATE`. An
uncommitted edit anywhere under `crates/sailgym-physics/src` therefore makes
`ModelIdentity::current()` **dirty**, `ExperimentIdentity::compare` returns
`Indeterminate` because a dirty tree names no baseline (F18.1d), and section
10's practice comparison correctly refuses to call two attempts "the same
conditions". Six E2E tests go red across the three browsers:

```
Locator: getByTestId('practice-compare')
Expected: "true"     Received: "false"        (data-comparable)
Expected: "same_conditions"  Received: "indeterminate"
```

**It was measured, not assumed.** With `src/rng.rs` stashed and the WASM
rebuilt, both specs pass; with it restored and rebuilt, both fail. The D2 edit
is the whole difference.

**Half two: the identity goes stale relative to the commit.** `build.rs`
declares `cargo:rerun-if-changed=src` and nothing else, so **committing does
not re-run it**. After the commit the tree was clean, the gate was run again,
and step 9 failed *identically* — the binary still carried
`SAILGYM_SOURCE_STATE=dirty` from the previous build. Touching any file under
`src` (or `cargo clean -p sailgym-physics`) re-runs the build script and the
gate goes green.

**The remedy this section used**, with the human's approval recorded in the
session: commit the physics-source edit, then force the build script to re-run
before the gate. There is no in-crate fix: `ModelIdentity` is read from
compiled-in environment variables, and weakening `is_comparable_with` to
accept a dirty tree would destroy the property section 10 built it for.

**What section 06 should do about it.** The cheapest honest repair is to make
`build.rs` also watch the things its answer depends on —
`cargo:rerun-if-changed=../../.git/HEAD` and `../../.git/index` — so that a
commit invalidates the baked identity. That is an edit to a physics-crate file
and no task in this section owns it, so it is **reported, not made** (F13.2).
Until then: any section that edits `crates/sailgym-physics/src` must commit
that edit **and** force a rebuild before the gate can be green, and a gate run
that skips the second step will fail in a way that looks like a physics
regression and is not.

---

## 8. No physics changed, no coefficient was tuned, and no gain was introduced

```
$ git diff --name-only f045240 -- crates/sailgym-physics/
crates/sailgym-physics/src/rng.rs
$ git status --porcelain crates/sailgym-task/ crates/sailgym-wasm/ crates/sailgym-bench/ scenarios/
(empty)
```

That is section acceptance criterion 6: **only `src/rng.rs`**, and the change
is one `pub const`, one module-doc table row and a widened distinctness test.
`parameters.rs` is byte identical and `--test provenance` compares the same F7
rows on every gate run.

**Acceptance criterion 8 asks this section to record every gain it introduced.
It introduced none**, and that is not an oversight:

- The `rate` adapter is the **identity**. It has no gain because it does no
  arithmetic — deliberately, since "no arithmetic" is the property the golden
  bit-identity test asserts and it is easier to keep true if it is literally
  true.
- `Helm` is deferred, so its gains do not exist yet.
- No noise model ships, so there are no noise settings.

The crate's only tunable settings are a sensor's **mounting position**
(`ApparentWind::at`, defaulting to a point derived entirely from F7 values)
and a sensor's **version**. Neither is a physical coefficient: brief §43
governs `parameters.rs`, F14.9 says in as many words that it does not govern
controller gains, adapter gains or sensor settings, and the crate boundary is
the distinction. Nothing in this crate reaches a force, a moment or an
equation of motion — `crates/sailgym-agent/` contains no equation of motion at
all, and `no_f7_literal_appears_in_the_agent_crate` now asserts on every gate
run that no F7 value has been copied into it (§5.5 is the proof it can fail).

The section-04 handoff §11.4 left `CourseParams::DEFAULT_LOOKAHEAD = 20.0 m`
untuned and said section 05 owns the tuning. **It is still untuned**, and the
reason is the same one section 04 gave: tuning it needs a controller to tune
it against, and this section deliberately ships no autopilot. It remains a
course parameter with a provenance note saying it is a placeholder, and the
first section that builds a rule sailor should tune it there.

The user's dispatch adds that a **visual** constant is governed by the same
discipline as a physical one. This section introduces no visual constant at
all: it has no UI.

---

## 9. Section acceptance criteria, one by one

| # | criterion | result |
|---|---|---|
| 1 | `scripts/check.sh` green with the eleven-step gate, retaining `sailgym-task` coverage | **pass** — §6; `-p sailgym-task` and `-p sailgym-course` are both still in step 3, now beside `-p sailgym-agent` |
| 2 | external manual and agent actions share the validated actuation path, proven by equal resulting trajectories, not by a source-pattern ban | **pass** — `manual_and_policy_actions_take_the_same_path`, bit identical over 800 steps, demonstrated able to fail (§5.4) |
| 3 | `rate` reproduces a golden trajectory bit for bit | **pass** — **6 of 6** committed goldens, no skip (§4.2), demonstrated able to fail at one ULP (§5.3) |
| 4 | F9.7 holds with an agent attached, and the test was demonstrated able to fail | **pass** — §4.3, §5.1 |
| 5 | `cargo tree -p sailgym-physics` mentions no v2 crate | **pass** — §4.6, asserted in two gate tests |
| 6 | `git diff --name-only crates/sailgym-physics/` names **only** `src/rng.rs` | **pass** — §8 |
| 7 | no F7 literal in `crates/sailgym-agent/`; `--test provenance` still green | **pass** — §8, now a permanent test, demonstrated able to fail (§5.5). `--test provenance`: 6 tests, green in step 4 of every run |
| 8 | this handoff per F13.6, recording every gain introduced and why gains are not physical coefficients | this file; §8 records that the count is **zero**, and why |

Task-level acceptance:

| task | criterion | result |
|---|---|---|
| 5.1 | `cargo test -p sailgym-agent spec`; `cargo test -p sailgym-physics rng` green with the stream assertions extended; `cargo tree -p sailgym-physics` mentions neither `sailgym-agent` nor `sailgym-course` | pass — 6 and 5 tests respectively; the tree assertion runs in step 3 |
| 5.2 | `cargo test -p sailgym-agent sensor`; a compile-fail test or documented manual check that an `Agent` cannot reach a `WindField`; `width() == field_names().len()` for every registered sensor | pass — 35 tests under `sensor`; §5.6 is the documented check plus a permanent signature assertion; `every_registered_sensor_declares_a_consistent_layout` asserts width against **both** `fields()` and `field_names()` for all five |
| 5.3 | `cargo test -p sailgym-agent sensor::`; `imu` uses a fresh `forces::evaluate`, asserted by a source grep in the shape of `no_shortcuts.rs`; no sensor's output depends on `advance` chunking, asserted numerically; translation/rotation tests; no undeclared absolute coordinates or heading; guidance privilege tested separately | pass — `no_sensor_reads_the_cached_force_breakdown` is the grep, `the_imu_does_not_depend_on_advance_chunking` the numeric assertion, §4.5 the semantics, and `exactly_one_guidance_column_is_privileged_and_it_is_the_true_wind_one` the privilege test |
| 5.4 | `cargo test -p sailgym-agent observation`; length equals the sum of widths; the layout is a `Vec<String>`; `obs_digest` stable and changes on version, width or order; adding a sensor does not perturb preceding columns | pass — §4.4 |
| 5.5 | `cargo test -p sailgym-agent actuation`; the golden bit-identity test; an adapter returning a value outside `[−1, 1]` rejected by the contract assertion; `dim()` matches the emitted action width | pass — 8 tests; `Runaway` is an adapter that denormalises into ±4 and is rejected by name and field; `the_declared_dim_matches_the_emitted_action_width` |
| 5.6 | — | **deferred by the PRD**; nothing ships (§3.2) |
| 5.7 | the three tests pass; the F9.7 test demonstrated able to fail by keying cadence off a per-call counter, and reverted | pass — §4.3, §5.1 |
| 5.8 | step 3 becomes the four-crate command; step count unchanged | pass — §6.1 |

---

## 10. Risks

**RV25 — the observation is built from `Diagnostics` and F9.7 dies silently.
Did not fire, and it is closed three ways.** F14.7 is honoured by
construction: `WorldView` carries no `Simulation` and no `ForceBreakdown`, so
there is nothing to read. `no_sensor_reads_the_cached_force_breakdown` greps
for `Simulation`, `Diagnostics`, `.forces()` and `acceleration_body` in
shipped sensor code. `the_observation_does_not_depend_on_advance_chunking`
measures it over seven chunkings. §5.2 shows the driver-level version of the
defect going red.

**RV26 — cadence keys off a per-`advance` counter. Did not fire**, and §5.1 is
the proof it would be caught, over six chunkings rather than one.

**RV27 — external and manual actions acquire different semantics from
policies. Did not fire.** The test is behavioural (§4.3) and §5.4 shows it
catching a half-rate rudder that no source-pattern ban would notice.

**RV28 — an agent gain gets written into `parameters.rs` because it "felt
physical". Did not fire, and could not have**: no gain exists (§8). The audit
now runs in the gate anyway, in both directions — `--test provenance` for
`parameters.rs` and `no_f7_literal_appears_in_the_agent_crate` for the new
crate.

**RV29 — a sensor version is not bumped after a field reorder. Did not fire.**
`version()`'s contract is in the trait doc; the digest test asserts that a
swap of two columns **inside one sensor** changes `obs_digest`, which is the
change a reviewer waves through.

**RV30 — `true_wind` ships unmarked, leaking a velocity estimate. Did not
fire.** It is excluded from the suite with the reason stated in
`sensor/wind.rs`, and `the_tier0_suite_is_the_five_sensors_the_prd_names`
asserts both that it is not registered and that asking for it is an error.

**RV31 — absolute position reaches a policy. Did not fire**, and §4.5 is why:
the absence is asserted by name *and* by rigid-motion invariance, because a
name check alone cannot establish it.

**RV32 — the `rate` adapter perturbs a bit. Did not fire.** §5.3 shows the
stop condition catching one ULP.

**RV52 (from section 04) — fired.** §7.

**R3 (v1) — sign-convention drift.** The risk this section is most exposed to,
because a sensor is nothing but signs. Every sensor carries a mirror test
against `testkit::mirror_state`, every sign is stated against F2 in the doc
comment that declares the column, and the guidance signs are additionally
checked against `sailgym-course`'s own bit-exact mirror.

---

## 11. What deviated from the PRD

### 11.1 `src/lib.rs` takes one line per later module, and task 5.1 owns it

Rust has no way for task 5.2 to declare its own module without editing the
crate root, which is task 5.1's file. `lib.rs` therefore gained `pub mod
sensor; pub mod worldview;` during group A, `pub mod actuation; pub mod
observation;` during group B and `pub mod manual;` during the second group S.
The file carries a comment saying so. This is the same `Owns:`-list gap
sections 10, 11 and 04 recorded; it is now the seventh section to hit one, and
the PRD template is still where it should be fixed.

The same applies one level down: `src/sensor/mod.rs` is task 5.2's and
declares task 5.3's five sensor modules.

### 11.2 Task 5.3's `Owns:` names four files for five sensors

The five sensors are `imu`, `apparent_wind`, `rig_state`, `actuator_state` and
`guidance`; the `Owns:` list names `imu.rs`, `wind.rs`, `rig.rs`,
`guidance.rs` and `registry.rs`. `actuator_state` therefore lives in `rig.rs`
beside `rig_state`, which is where the pairing belongs — both answer "what is
the boat's own machinery doing right now?" — but it is a gap in the list and
is recorded rather than absorbed.

### 11.3 `docs/v2/progress/05-handoff.md` appears in no `Owns:` list

F13.6 requires it and task 5.8 is the task that would own it. The same gap
section 04 recorded for `tests/replay/routes.json`.

### 11.4 The wall-clock grep is extended by a new file, not by editing the old one

F14.8 says the existing `determinism.rs::no_wall_clock` grep "extends to the
new crates rather than being rewritten". Section acceptance criterion 6 says
`git diff --name-only crates/sailgym-physics/` must name **only** `src/rng.rs`
— which forbids editing `crates/sailgym-physics/tests/determinism.rs`, where
that grep lives.

The resolution: `crates/sailgym-agent/tests/determinism.rs` runs the **same
needles** (`Instant`, `SystemTime`, `now(`, `rand::thread_rng`) with the
**same** `#[cfg(test)]` exclusion, over `sailgym-agent` and `sailgym-course`.
It is the same grep with a different scope, not a second grep with different
rules, and `the_grep_would_notice` asserts the scanner itself both finds a
wall clock and ignores one inside a `#[cfg(test)]` item. `sailgym-course` is
included because the section-04 handoff §11.5 asked for it and nothing
enforced it before. The physics crate keeps its own copy, unchanged, over its
own sources.

### 11.5 An extra test the PRD did not ask for

`no_f7_literal_appears_in_the_agent_crate` (§8). Criterion 7 states the
property as something to check once; a permanent test is strictly better, and
`tests/provenance.rs` scans only `crates/sailgym-physics/src`, so without it
the agent crate would be the one place a coefficient could be copied to
unnoticed.

### 11.6 No task was delegated

Group A holds two tasks and group B two. The section agent executed them, as
sections 01, 02, 04, 08, 09, 10 and 11 each recorded. It is now the eighth
consecutive section to say this.

### 11.7 Group order was honoured, and the gate was run on the group's own tree

S → gate → A → gate → B → gate → S(5.7) → gate → S(5.8) → gate. Later groups'
sources were written before their groups began but were kept **outside the
repository** until their group, so each gate run measured a tree containing
that group and nothing later. Recorded because "the gate was green after group
S" means nothing if the group was not what the gate saw.

---

## 12. What the next section must know

1. **`sailgym-agent` is the interface and nothing else.** There is no episode,
   no `Outcome`, no termination, no `VecEnv` and no decision log. The
   `Episode` struct in `tests/determinism.rs` is a **test harness**, not a
   runner: it exists to prove the contract holds and section 06 should build
   the real one rather than promote it. What section 06 should keep from it is
   the shape of `advance`: split the caller's `n` at decision boundaries using
   `Cadence::steps_to_next_decision`, so where the decisions land is a
   property of the episode and never of the caller's chunk size.

2. **Section 11's handoff asked that later course and env work consume
   `sailgym-task`'s `Outcome` and `PracticeEvent` rather than define new
   completion semantics; section 04 repeated it, and this section adds
   nothing that competes with them.** `AgentDebug` is the only new reporting
   type and it is read by no controller (F6.10's discipline).

3. **`ActionSpace::Setpoint` is reserved and refused.** Building `Helm` means
   first writing the engaged/released contract §3.2 describes, then removing
   the `ActionSpace::is_implemented` refusal and adding the `Action` variant.
   Do not add a placeholder `Helm` to make a picker compile.

4. **`obs_digest` is a canonical record, not a hash**, and `ObsLayout` is
   where bounds live. If section 06 or 07 wants a compact key, F16.4 requires
   an established SHA-256 — `sailgym_physics::digest::sha256_hex` exists
   behind the `testkit` feature, and using it from a crate that ships would
   pull `sha2` into that crate's graph. Decide deliberately.

5. **The per-sensor RNG substreams must be built once per episode and carried
   across decisions.** `sensor_streams` builds them; rebuilding them inside
   `observe` would hand every decision the same draws. When a `NoiseModel`
   lands, that is the first thing to get wrong.

6. **There is no WASM surface.** `set_agent`, `agents_json`, `set_route` and
   `agent_debug_json` all need F8.2 amending, and no section-05 task owns
   `crates/sailgym-wasm`. The web app cannot select an agent or set a route.
   The PRD records this as a debt for the section that adds the picker UI.

7. **RV52 has a repair and this section did not make it.** §7. Any section
   that edits `crates/sailgym-physics/src` must commit the edit **and** force
   `build.rs` to re-run before the gate can be green.

8. **The repository root `README.md` now lies about the gate four times.** Its
   nine-step table spells step 3 without `-p sailgym-task` (section 11's
   change), without `-p sailgym-course` (section 04's) and without
   `-p sailgym-agent` (this one), step 4 without `--test conformance`
   (section 02's), and the chain as nine steps with no 10 or 11 (section
   03's). No task in any of those sections owns that file (F13.2). The changes
   it needs:

   | line | should read |
   |---|---|
   | 194 | `\| 3 \| ` + "`cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent`" + ` \| The physics core, the practice evaluator, the course layer and the agent interface are correct **and build on the host with no WASM toolchain**. \|` |
   | 195 | step 4 with `--test conformance` |
   | the chain | eleven steps, with 10 and 11 |
   | the layout block | `crates/sailgym-task/`, `crates/sailgym-course/` and `crates/sailgym-agent/` |

   `docs/v1/00-foundations.md` F12 is deliberately **not** edited: a v1 clause
   is amended by a recorded v2 delta and never in place.

9. **`CourseParams::DEFAULT_LOOKAHEAD` is still untuned** (§8). The first
   section with a controller owns it.

10. **Four `README.md` open items are still waiting for a human** — V-D, V-G,
    V-H, and the obstacle half of V-F. This section edited V-E to record what
    it did; it closed nothing, for the same reason sections 01, 04, 08, 09, 10
    and 11 each left theirs.

---

## 13. Commands

```
cargo test -p sailgym-agent
cargo test -p sailgym-agent spec
cargo test -p sailgym-agent sensor::
cargo test -p sailgym-agent observation
cargo test -p sailgym-agent actuation -- --nocapture
cargo test -p sailgym-agent --test determinism
cargo test -p sailgym-physics rng
cargo tree -p sailgym-physics --edges all
scripts/check.sh
scripts/check.sh 3
```

The golden trajectories this section compares against are **not** regenerated
by anything here: `crates/sailgym-physics/tests/golden/*.json` is read as
data, with a local `serde` struct rather than the generator's own type, so a
change to the generator cannot break this crate's compilation and a change to
the recorded numbers breaks it exactly the way it should.
