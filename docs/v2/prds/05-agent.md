# v2 Section 05 — `sailgym-agent`: sensors, actions, cadence, helm

**Planning revision (2026-09-20):** later than playable milestone 08–11; IDs are preserved, not dispatch order. Read the revised index/brief/F18 and dependency handoffs. All required scope/normative decisions remain proposed. Compare no-change guards against this section's starting revision, not the pre-08 baseline.

Source discussions: `../discussions/unified-agent-interface.md` §§2, 3, 4;
`../discussions/ablation-spaces.md` §§1, 2, 3.
Prepares `../discussions/cross-stack.md` §1.2, which builds the Gymnasium spaces
from what this section reports.

> **BLOCKED.** Not dispatchable until `../brief.md` S3 has a recorded implementation decision,
> answering `../README.md` V-A.

Read first: `../../v1/00-foundations.md` in full, `../README.md`, `../brief.md`,
`../00-foundations.md` (F14, F16.4), `../progress/04-handoff.md`, then this PRD.

## Goal

One interface that the web autopilot picker, a rule sailor, a polar racer and an
RL policy all plug into, so that a difference between two of them is a
difference in tactics rather than in how each happened to reimplement the helm.

The section is done when externally supplied manual actions and agent decisions share validated action semantics, cadence and logging. A human is an external action source, not a policy with access to hidden episode state.

## Why this shape

The mistake to avoid is letting each consumer cut the control chain
(`route → guidance → tactic → setpoint → helm → Controls`) in a different place.
Then nothing is comparable and "which layer was the policy substituted at"
becomes archaeology. F14.2 makes the substitution point an enumerated, logged
value with exactly two variants.

Begin with one concrete, versioned observation layout for the rate-only baseline. The configurable design is enabled only for an actual study; do not create unused sensor/adapter implementations. Two design decisions for that extension, in favour of
`ablation-spaces.md`:

- **A sensor list, not an `ObsMask` over a fixed field set** (F14.3). The
  observation layout is runtime data. `cross-stack.md` §1.2 builds
  `observation_space` from `sum(s.width for s in sensors)`; a ray-casting
  sensor's width is configurable and is not a subset of anything fixed. There is
  no `OBS_LEN`.
- **An actuation funnel with normalised bounds** (F14.5). Every adapter presents
  `[−1, 1]^k`. An ablation that hands one arm radians and another normalised
  commands has measured action *scaling*, not action *space*.

## What this section does **not** do

- No autopilot. The manual external source and a test stub exercise the shared actuation path. The
  rule sailor, the polar racer and the polar tables are later sections.
- No episode, no `Outcome`, no `VecEnv`. Section 06.
- No WASM surface, no picker UI. F8.2 enumerates the entire WASM API and no task
  here owns `sailgym-wasm`.
- **No new state variables.** `STATE_LEN = 13` and the F8.3 order are normative
  and untouched. The `Actuation` trait is deliberately shaped so a future
  second-order actuator adds explicitly versioned actuator state recorded alongside initial state rather than silently widening `BoatState` — but that model is not built here (`ablation-spaces.md` §0 tier 2).
- No lidar, no wind probe, no noise models. Listed in 5.3 as the extension
  points they are; built later.
- No physical coefficient is added or changed. Controller and adapter gains are
  not physical coefficients (F14.9) and live in this crate, never in
  `parameters.rs`.

## Normative deltas

### D1 — F14 (agent interface). **Open; blocking the section.**

`../00-foundations.md` F14 in full.

### D2 — `rng.rs` gains `STREAM_AGENT: u64 = 4`. **Open.**

A physics-crate file, edited additively by task 5.1, which therefore owns it.
The named-stream table already reserves 1–3 (`STREAM_WIND`, `STREAM_SCENARIO`,
`STREAM_NOISE`) and the module doc carries the table; both move together. This
is what lets a jittery agent be added without shifting the wind field by a bit.

### D3 — F12′ step 3 gains `-p sailgym-agent`. **Open.** As section 04's D2.

### D4 — `brief.md` S3. **Open; record the implementation decision.**

## The three determinism traps, stated once

Each of these is a bug that no ordinary test would catch, so each gets a named
assertion in this section rather than a comment.

**1. The observation must not read the force cache.** `Diagnostics` and
`Simulation::forces` hold a breakdown refreshed **once per `advance(n)` call**.
An observation built on it inside the step loop would see a breakdown up to `n`
steps old, so the observation would depend on how the caller happened to chunk
its calls — and F9.7 would break in the one place nobody tests. F14.7 requires
`observe` to be pure in its arguments, and any sensor needing accelerations
calls `forces::evaluate` itself at the decision instant. At 20 Hz over 200 Hz
physics that is one extra evaluation per ten steps, about 5 % in force-evaluation count on top of RK2's
two per step.

**2. Cadence keys off the episode step counter.** Not off a per-`advance`
counter, not off elapsed time (F14.6). The existing F9.7 identity re-run with an
agent attached is the cheapest high-value test in this whole proposal, and it
earns its keep precisely because `advance` refreshes forces once per call *as an
optimisation whose justification a per-step agent hook could invalidate*.

**3. Agent randomness goes through `Pcg32::stream(STREAM_AGENT)`,** with
per-sensor substreams below it. Two RNGs feeding one episode is how a
deterministic environment stops being reproducible.

## Tasks

### 5.1 — Contracts: the crate, the traits, the stream

**Owns:** `crates/sailgym-agent/Cargo.toml`, `crates/sailgym-agent/src/lib.rs`,
`crates/sailgym-agent/src/spec.rs`, `crates/sailgym-physics/src/rng.rs`,
`Cargo.toml`, `Cargo.lock`
**P-group: S**

`Action`, `ActionSpace`, `AgentSpec`, `Cadence`, and the `Agent`
trait — object-safe, so `Box<dyn Agent>` works and the registry is a `Vec`, not
a `HashMap` (F9.3).

Three constraints go in the trait's **doc comment**, because each is a
determinism bug waiting to happen: `decide` may not read a wall clock; agent
randomness goes through `STREAM_AGENT`; `debug()` is written for observers and
read by no controller, the same discipline `CapsizeState` already has under
F6.10.

D2 here: `STREAM_AGENT = 4`, plus the module-doc table row.

Acceptance: `cargo test -p sailgym-agent spec`; `cargo test -p sailgym-physics rng`
still green with the stream-distinctness assertions extended to the new constant;
`cargo tree -p sailgym-physics` mentions neither `sailgym-agent` nor
`sailgym-course`.

### 5.2 — `WorldView` and the `Sensor` trait

**Owns:** `crates/sailgym-agent/src/sensor/mod.rs`,
`crates/sailgym-agent/src/worldview.rs`
**P-group: A**

F14.4's containment boundary, and the trait: `id`, `version`, `width`,
`field_names`, `sense(&mut self, &WorldView, &mut Pcg32, &mut [f64])`.

`WorldView` is the **one** place privileged world access is permitted. Sensors
see everything in it; the agent sees only the concatenated vector. That single
structural fact is what makes "privileged information" enforceable by
construction instead of by discipline.

`others: &[BoatState]` is present and **empty** from day one, so the swarm case
is additive rather than a refactor (`unified-agent-interface.md` §8).

Acceptance: `cargo test -p sailgym-agent sensor`; a compile-fail test (`trybuild`
or a documented manual check) that an `Agent` cannot reach a `WindField`;
`width() == field_names().len()` asserted for every registered sensor.

### 5.3 — The tier-0 sensor suite

**Owns:** `crates/sailgym-agent/src/sensor/imu.rs`,
`crates/sailgym-agent/src/sensor/wind.rs`,
`crates/sailgym-agent/src/sensor/rig.rs`,
`crates/sailgym-agent/src/sensor/guidance.rs`,
`crates/sailgym-agent/src/sensor/registry.rs`
**P-group: A**

Five sensors, chosen because together they are what a real dinghy measures plus
the task:

| id | width | note |
|---|---|---|
| `imu` | 5 | `p`, `r`, `φ`, body `ax`, `ay` (width 5; no absolute heading). **No velocity, no position.** Calls `forces::evaluate` itself — trap 1 |
| `apparent_wind` | 2 | AWA, AWS at the masthead. This is what a boat actually measures |
| `rig_state` | 4 | `β`, `β̇`, `L`, normalised sheet slack |
| `actuator_state` | 2 | `δr` and the rate command in force — the inner loop is part of the plant, so a policy is lost without it |
| `guidance` | 5 | from `sailgym-course`: cross-track, bearing to target relative to heading, distance, leg bearing vs. wind, rounding side |

**No absolute `x`, `y`, `psi` in the policy observation.** Sensors may read state to derive body-relative quantities; a source-access grep alone cannot establish absence of privileged information. Guidance derived from true wind must be marked privileged or use sensed estimates. A policy that
learns the course geometry by absolute position has learned the course, not
sailing.

`true_wind` is **not** in this suite, deliberately. Real boats derive it from
apparent wind and boat velocity; handing it to a policy that has no speed log is
handing it a free velocity estimate through the back door. When it is added it
is marked privileged, or derived inside the sensor from sensed quantities only.

Registry resolves by id through an ordered `Vec`, never a `HashMap` (F9.3), and
`version()` is bumped on **any** change to what a sensor emits, including a
field reorder — the digest depends on it, so a bump invalidates comparisons
loudly rather than silently.

Acceptance: `cargo test -p sailgym-agent sensor::` — each sensor's `field_names`
length equals `width`; `imu` uses a **fresh** `forces::evaluate` and not
`Simulation::forces`, asserted by a source grep in the shape of
`no_shortcuts.rs`; no sensor's output depends on `advance` chunking, asserted
numerically; translation/rotation tests verify declared output semantics, and the emitted field layout contains no undeclared absolute coordinates/heading. Test guidance privilege separately.

### 5.4 — The observation, and `obs_digest`

**Owns:** `crates/sailgym-agent/src/observation.rs`
**P-group: B**

The ordered concatenation, `observe(...)` pure in its arguments (F14.7), the
layout emitted as data, and `obs_digest` over the ordered
field records including IDs/versions, units, bounds, normalization, noise and privilege, using section 10 canonical identity. A compact hash is optional; do not introduce another hash implementation.

Acceptance: `cargo test -p sailgym-agent observation` — layout length equals the
sum of widths; the layout is a `Vec<String>`, not a constant; `obs_digest` is
stable across runs and **changes** when any sensor's version, width or order
changes; adding a sensor does not perturb the preceding columns.

### 5.5 — The actuation funnel and the `rate` adapter

**Owns:** `crates/sailgym-agent/src/actuation/mod.rs`,
`crates/sailgym-agent/src/actuation/rate.rs`
**P-group: B**

F14.5's trait, bounds `[−1, 1]^k` **asserted in the trait contract**, and the
`rate` adapter: the identity onto `Controls`.

The property that makes everything after this affordable, and it is asserted on
day one, not assumed: **`rate` reproduces a golden trajectory bit-for-bit.** If
it does not, the funnel is wrong and the section stops.

`angle`, `angle_bangbang` and `sheet_length_for_beta` are **not** built here.
`sheet_length_for_beta` is named in F14.5 so that when it is built, nobody
reading an experiment log believes the boom was commanded.

Acceptance: `cargo test -p sailgym-agent actuation` — the golden bit-identity
test; an adapter returning a value outside `[−1, 1]` is rejected by the contract
assertion; `dim()` matches the emitted action width.

### 5.6 — `Helm`

**Owns:** `crates/sailgym-agent/src/helm.rs`
**P-group: B**

The shared `Setpoint → Controls` object. It exists as a separate object for one
reason: in `ActionSpace::Setpoint` a policy and a rule sailor drive **identical**
actuator dynamics, so a difference in finish time is a difference in tactics and
not in how well each compensated for rudder self-centring.

Its gains are agent parameters (F14.9) and are tuned freely.

Do not implement a position servo by requiring a tiny nonzero command at equilibrium. Zero currently means released/self-centering, so engaged hold needs an explicit Rust contract first. **Task 5.6 is deferred** until that delta and a concrete setpoint consumer are selected; the rate-only section can finish without it. No placeholder Helm implementation ships. Gains and tracking bounds then become versioned adapter configuration, and feedback runs every physics step or a declared adapter cadence distinct from policy decisions.

### 5.7 — `manual`, and the F9.7 identity with an agent attached

**Owns:** `crates/sailgym-agent/src/manual.rs`,
`crates/sailgym-agent/tests/determinism.rs`
**P-group: S**

The section's contract task. An external action source receives normalized manual Controls through a setter/step argument. It has no access to browser held keys or hidden EpisodeCtx. Route both external and policy actions through the same validation/adapter/logging path; selecting their source is legitimate, not a forbidden branch.

The tests:

- **F9.7 with an agent attached**: `advance(n)` equals `n × advance(1)`,
  bit-for-bit, with `manual` and with a stub agent at cadence 10.
- **Cadence**: decisions land on `step % period == 0` regardless of how the
  caller chunks `advance`, asserted over chunkings `{1, 3, 7, 10, 13, 200}`.
- **Wall clock**: `determinism.rs::no_wall_clock`'s grep **extended** to the new
  crates rather than a second grep written.

Acceptance: all three pass; the F9.7 test is demonstrated able to fail by keying
cadence off a per-call counter, and reverted.

### 5.8 — The gate

**Owns:** `scripts/check.sh`, `scripts/check.ps1`, `CLAUDE.md`,
`docs/v2/00-foundations.md`, `docs/v2/README.md`
**P-group: S**

D3: step 3 becomes `cargo test -p sailgym-physics -p sailgym-task -p sailgym-course -p sailgym-agent`.
Step count unchanged.

## Section acceptance criteria

1. `scripts/check.sh` green with the current nine/eleven-step gate, retaining sailgym-task coverage.
2. **External manual and agent actions share the validated actuation path**, proven by equal resulting trajectories; no source-pattern ban replaces this behavioral test.
3. `rate` reproduces a golden trajectory bit-for-bit (5.5).
4. F9.7 holds with an agent attached, and the test was demonstrated able to fail.
5. `cargo tree -p sailgym-physics` mentions no v2 crate.
6. `git diff --name-only crates/sailgym-physics/` names **only** `src/rng.rs`.
7. No literal from the F7 catalogue appears in `crates/sailgym-agent/`;
   `--test provenance` still green.
8. `docs/v2/progress/05-handoff.md` per F13.6, recording every gain introduced,
   with the explicit statement that gains are not physical coefficients and why.

## Risks

| # | Risk | Mitigation | Fires when |
|---|---|---|---|
| **RV25** | The observation is built from `Diagnostics`, and F9.7 dies silently. | F14.7; 5.3's source grep; 5.7's chunking test. | an observation reads stale cached force data |
| **RV26** | Cadence keys off a per-`advance` counter and `advance(n) ≠ n × advance(1)`. | 5.7's chunking test over six chunkings, demonstrated able to fail. | any cadence test uses a single chunking |
| **RV27** | External/manual actions acquire different semantics from policies. | 5.7 and criterion 2 compare equal actions through both sources. | Equal actions produce different controls or trajectories |
| **RV28** | An agent gain gets written into `parameters.rs` because it "felt physical". | F14.9; criterion 7's grep; `--test provenance` is unchanged and still gates. | any new field appears in `parameters.rs` in this section |
| **RV29** | A sensor version is not bumped after a field reorder, and two incomparable runs compare cleanly. | 5.3's rule; 5.4's digest test asserts order changes the digest. | `obs_digest` is unchanged across a reorder |
| **RV30** | `true_wind` is added as an ordinary sensor, leaking a velocity estimate to an IMU-only arm. | Excluded from the suite here, with the reason stated; when added it is privileged or derived. | a `true_wind` sensor ships unmarked |
| **RV31** | Absolute position reaches a policy, which then learns the course. | 5.3 output-semantic tests and explicit guidance privilege. | an undeclared absolute field or privileged derivation reaches the policy |
| **RV32** | The `rate` adapter perturbs a bit and every committed golden silently becomes wrong. | 5.5's bit-identity test is a **stop condition**, not a warning. | the golden test needs a tolerance |

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| Only the `rate` adapter; no `angle`, `angle_bangbang`, `sheet_length_for_beta` | scope — the funnel is the point, the arms are the study | the action-space ablation section |
| Only five sensors; no `wind_probe`, `lidar2d`, `polar_prior`, `layline_prior`, `speed_log` | scope | the observation ablation section |
| No noise, bias, latency or dropout models | scope; `NoiseModel` is designed for in F14.8's substream scheme but not built | the sensor-quality ablation |
| No `ActuatorModel` / parallel actuator state; a force-controlled tiller has no CP-offset model to push against | `ablation-spaces.md` §0 tier 2 — this is a physics milestone with an F5 amendment, not an adapter | only if the ablation results justify it |
| `Helm` and position tracking are deferred pending engagement semantics | 5.6 deferred | the rule-sailor section, which is the first thing that stresses them |
