# A unified agent interface — design suggestion

Companion to `autopilot-suggestions.md`, which argues *which* controllers to
build. This one argues *what they plug into*: one interface that serves the web
autopilot picker, waypoint/lookahead guidance, later swarm work, and RL
training and policy deployment, without four divergent copies of the control
stack.

Status: **suggestion, not a PRD.** Nothing here is normative and nothing here
may be treated as amending `00-foundations.md`.

---

## 0. Scope, first — this needs a human decision before it is built

brief §44 lists as explicitly out of scope for v1: *multiple boats*, *RL
training*, *multi-agent sailing*, *collisions between boats*, *obstacle
avoidance*. brief §45 then lists the same items as the intended future
direction, and the brief outranks everything on scope.

So this work is a **post-v1 scope extension**, not a section 11. Per
`CLAUDE.md` it is escalated rather than decided by an agent. Concretely, before
any of it is implemented someone needs to sign off on:

1. A v2 brief (or a brief §44 amendment) that moves autopilots, courses and the
   RL environment from "deferred" to "in scope".
2. A foundations addendum — call it **F14 (agent interface)** and **F15 (task /
   course)** — because the items below *are* conventions and API surfaces, and
   F8.2 currently enumerates the entire WASM API. Adding `set_agent` to `Sim`
   without amending F8.2 puts the code in contradiction with a normative
   document.
3. New gate steps, which means editing `scripts/check.{ps1,sh}` and the
   `CLAUDE.md` gate table. No existing task owns those files.

Everything below assumes that sign-off has happened.

---

## 1. The one idea

The control stack is a chain, as `autopilot-suggestions.md` sets out:

```
route → guidance → tactic → setpoint → helm → Controls
```

The mistake to avoid is letting each consumer cut the chain in a different
place: the web autopilot at `route`, the RL policy at `Controls`, a future
swarm agent somewhere else. Then nothing is comparable, the helm gets
reimplemented, and "which layer was the policy substituted at" becomes
archaeology.

**Make the substitution point an explicit, logged, enumerated value.**

| Substitution point | Agent replaces | Shared machinery below it |
|---|---|---|
| `ActionSpace::Rates` | tactic + helm | none |
| `ActionSpace::Setpoint` | tactic only | the one shared `Helm` |

Two variants, not five. Everything above the cut — route, guidance, mark
progression — is the **environment's** job, never the agent's, for every agent
including the rule sailor. That is what makes the rule sailor, the polar racer
and an RL policy answerable to the same task definition.

`ActionSpace::Setpoint` is the interesting one and the reason to build the
`Helm` as a separate object: it is the mode in which a policy and a rule sailor
are driving *identical* actuator dynamics, so a difference in finish time is a
difference in tactics, not in how well each one happened to compensate for
rudder self-centring.

---

## 2. The interface

```rust
/// Everything an agent may see. One struct, one flat encoding, one schema.
pub struct Observation { /* §3 */ }

/// What an agent emits. The runner converts `Setpoint` through the shared Helm.
#[derive(Clone, Copy, Debug)]
pub enum Action {
    /// The native action space: F3 `Controls`, rate commands, verbatim.
    Rates(Controls),
    /// Heading and trim targets; the shared `Helm` turns these into `Controls`.
    Setpoint(Setpoint),
}

#[derive(Clone, Copy, Debug)]
pub struct Setpoint {
    /// rad, desired heading in the world frame, F2 sign convention.
    pub psi_cmd: f64,
    /// m, desired mainsheet length. Not a boom angle: the boom is dynamic
    /// (F6.9) and no controller may command it directly.
    pub l_sheet_cmd: f64,
    /// Emergency ease, mapped straight onto `Controls::sheet_release`.
    pub release: bool,
}

/// Fixed, declarative metadata. Serialised into the episode header and into
/// the autopilot picker; never inferred.
pub struct AgentSpec {
    pub id: &'static str,        // "rule_sailor", "polar_racer", "policy_onnx", "manual"
    pub version: u32,
    pub action_space: ActionSpace,
    pub cadence: Cadence,
    pub obs_mask: ObsMask,
}

pub trait Agent {
    fn spec(&self) -> AgentSpec;
    /// Called once per episode. The *only* place an agent may seed itself.
    fn reset(&mut self, ctx: &EpisodeCtx);
    /// Pure given `(self, obs)`. No wall clock, no global RNG, no allocation.
    fn decide(&mut self, obs: &Observation) -> Action;
    /// UI-rate introspection only. Never read by the runner.
    fn debug(&self) -> AgentDebug { AgentDebug::default() }
}
```

Object-safe, so `Box<dyn Agent>` works and the registry is a `Vec`, not a
`HashMap` (F9.3).

Three constraints worth writing into the trait's doc comment, because each one
is a determinism bug waiting to happen:

- **`decide` may not read a wall clock.** `tests/determinism.rs` already greps
  the physics crate for `Instant`/`SystemTime`; extend that grep to the agent
  crate rather than writing a second one.
- **Agent randomness goes through `Pcg32::stream`.** `rng.rs` already reserves
  `STREAM_NOISE = 3` for exactly this; allocate `STREAM_AGENT = 4` in the same
  table. This is what lets you add a jittery "easy" NPC without shifting the
  wind field by a single bit — which is the whole point of the named-stream
  design already in the repo.
- **`debug()` is not an input.** Same discipline as `CapsizeState` (F6.10):
  written for observers, read by no controller.

`manual` is an `Agent` too — it reads the browser's held keys out of
`EpisodeCtx` and returns `Action::Rates`. Making the human one implementation
of the trait means there is no "agent attached / not attached" branch in the
runner, and no second code path for the case you will exercise most.

---

## 3. Observation — one struct, flat, masked

This is the piece that most repays getting right, because it is simultaneously
the RL observation vector, the rule sailor's input, and the thing you log.

Follow the pattern the repo already has for `BoatState` and `EpisodeFrame`:

```rust
pub const OBS_LEN: usize = /* … */;
pub const OBS_FIELDS: [&str; OBS_LEN] = [ /* … */ ];

impl Observation {
    pub fn to_array(&self) -> [f64; OBS_LEN];
    pub fn from_array(a: &[f64; OBS_LEN]) -> Self;
}
```

with the same round-trip and field-order tests `state.rs` already carries. That
one decision gives you the RL obs buffer, the WASM crossing and the log schema
for free, and stops them drifting apart.

### Contents

Egocentric and task-relative, so it transfers across courses and wind seeds:

| Block | Fields |
|---|---|
| Motion | `u`, `v`, `r`, `p`, `phi`, leeway angle |
| Wind | apparent wind angle and speed at the sail; true wind speed and angle relative to heading |
| Rig | `beta`, `beta_dot`, `l_sheet`, normalised sheet slack |
| Actuator | `delta_r`, and the rate command in force (needed, because the helm's integrator is part of the plant) |
| Guidance | signed cross-track error, bearing to the lookahead point relative to heading, distance to the next mark, leg bearing relative to the wind, required rounding side |
| Privileged | true wind at points other than the boat, wind derivatives, exact forces, future wind |

No absolute `x`, `y`, `psi` in the public block. A policy that learns the course
geometry by absolute position has learned the course, not sailing.

### Build it from state, not from `Diagnostics`

Tempting, and wrong. `Diagnostics` reads `Simulation::forces`, the **cached**
breakdown refreshed once per `advance(n)` call — see the comment in
`simulation.rs:advance`. An observation built on it inside the step loop would
see a breakdown from up to `n` steps ago, and the observation would then depend
on how the caller happened to chunk its calls. That breaks F9.7 in the one
place nobody would think to test.

So define:

```rust
pub fn observe(
    st: &BoatState,
    p: &BoatParameters,
    wind: &dyn WindField,
    g: &Guidance,
    mask: ObsMask,
) -> Observation
```

Pure in its five arguments, no `ForceBreakdown`, no `Simulation`. Cheap enough
to call at 20 Hz for hundreds of boats, and trivially correct under F9.7.

Secondary benefit: the RL observation is no longer coupled to the 50-field
debug record, which is free to keep changing as a debug record should.

### `ObsMask`, not two structs

One struct with a mask, and the privileged block zeroed unless the mask permits
it. The mask goes in the episode header, so "privileged benchmark" is a data
flag you can filter on rather than a code path someone forgets to label.

---

## 4. Cadence, and why it is the same mechanism for autopilots and RL

`autopilot-suggestions.md` proposes 20 Hz decisions over 200 Hz physics. That is
the same requirement as "the autopilot must not run every physics step", so
build it once:

```rust
pub struct Cadence { pub period_steps: u32 }   // 10 → 20 Hz at dt = 0.005
```

Rules:

1. A decision happens exactly when `episode_step % period_steps == 0`, keyed off
   the **episode** step counter — never off a counter that resets per
   `advance` call, and never off elapsed time.
2. Between decisions the last `Controls` is held (zero-order hold).
3. `period_steps` is frozen at `reset` and recorded in the header.

That gives `advance(n) == n × advance(1)` with an agent attached, and it should
be asserted, not assumed: the existing F9.7 test re-run with a rule sailor
driving is the cheapest high-value test in this whole proposal. Note the
existing `advance` refreshes forces once per call as an optimisation *because*
the breakdown depends only on the final state; adding a per-step agent hook is
precisely the change that could invalidate that reasoning, so the test earns its
keep.

---

## 5. Guidance — unify waypoints and the lookahead point

The web UX wants both "drag a point, sail at it" and "here is a course of
marks". Do not build two things.

```rust
pub struct Mark {
    pub position: Vec2,
    pub radius: f64,
    pub rounding: Rounding,      // Port | Starboard | Either | Gate(Vec2, Vec2)
}

pub struct Route { pub marks: Vec<Mark>, pub laps: u32 }

/// What the agent actually sees. Derived by the environment each decision.
pub struct Guidance {
    /// The current leg as a directed line. `None` for a bare lookahead point.
    pub line: Option<(Vec2, Vec2)>,
    /// The point to steer at — lookahead along the line, or the raw point.
    pub target: Vec2,
    pub signed_cross_track: f64,
    pub leg_index: u32,
    pub rounding: Rounding,
}
```

Both UX modes are `Route`s: a dragged lookahead point is a one-mark route with
`Rounding::Either` and a large radius. One code path, one set of tests, and the
"trajectory" case is just more marks.

Carry **both** `line` and `target`. Jaulin & Le Bars' controller needs the line
(cross-track is defined against a line, not a point); a simple pursuit
controller needs the point. Deriving one from the other at the agent is how you
end up with two subtly different definitions of cross-track error.

Two details `autopilot-suggestions.md` is right to flag, and which belong in the
**course crate**, not in any agent:

- Mark passage is *ordered, sided, and directed*. A radius check lets a boat cut
  the corner and lets a policy learn to. Use: crossed the mark's perpendicular
  plane, on the required side, in the required direction, with the previous mark
  already passed.
- Gates are a directed segment crossing, not two marks.

---

## 6. Crates and dependency direction

```
crates/
├── sailgym-physics/   unchanged. Knows nothing about agents. (F8.1)
├── sailgym-course/    Route, Mark, Rounding, passage, progress, scoring
├── sailgym-agent/     Observation, Action, Agent, Helm, Cadence, registry,
│                      rule sailor, polar racer, polar tables
├── sailgym-env/       Episode runner: Simulation + Route + Agent → step/reset,
│                      termination vs truncation, decision logging, VecEnv
├── sailgym-wasm/      + the additive surface of §7
└── sailgym-bench/     unchanged
```

Arrows point one way: `env → {agent, course, physics}`, `agent → {course,
physics}`, `physics → nothing`. `sailgym-physics` gaining a dependency on an
agent crate is the failure mode to watch for; it would end the "builds and tests
on the host with plain `cargo test`" property that F8.1 exists to protect.

Why new crates rather than modules of `sailgym-physics`:

- Maneuver state machines are *the sailor's intent*, not physics.
  `autopilot-suggestions.md` is right that the boom crossing must emerge from
  the physics; putting a tack FSM inside the physics crate is how that stops
  being true.
- Gate steps 3–5 mean "the physics is correct". Diluting them with controller
  tests makes a red step 3 ambiguous.

**A rule worth writing down explicitly**, because it is the obvious place
brief §43 gets blurred: *autopilot gains are not physical coefficients.* Tune
them freely. `parameters.rs` remains untouchable. The distinction is the crate
boundary, which is why the boundary is worth having.

### The polar table is a measurement, and measurements go stale

The polar racer's tables are generated *from this simulator*, so they are only
valid for the physics that produced them. Give the generated polar the same
treatment as the golden trajectories under R7: store it with the toolchain
record **and a digest of the serialised F7 catalogue**, and refuse-with-a-clear-
message on mismatch rather than silently racing on a stale polar. The
alternative is a polar racer that quietly gets worse after a parameter edit and
nobody knows why for three weeks.

---

## 7. The WASM surface

brief §24 and F8.2 require this to stay coarse. The decisive consequence: **the
agent runs inside `advance`, in Rust.** JavaScript sets a goal and calls
`advance(n)` once per frame, as it does today. It must never be called back per
decision — at 20 Hz with a 4× clock that is 80 boundary crossings a second for
one boat, and it scales with the fleet.

Additive to `Sim`, in the existing idiom:

```rust
/// The catalogue of selectable agents, with their tunable parameters and
/// ranges — the exact pattern `parameter_meta_json` already uses, so the
/// picker and its sliders are generated from Rust, not written in TS.
pub fn agents_json(&self) -> Result<JsValue, JsValue>;

/// `{"id": "...", "params": {...}, "seed": 0}`, or `null` for manual.
pub fn set_agent(&mut self, spec_json: &str) -> Result<(), JsValue>;

pub fn set_route(&mut self, route_json: &str) -> Result<(), JsValue>;
pub fn route_progress(&self) -> Box<[f64]>;      // flat, F8.3-style layout

/// Intent, maneuver state, laylines, target point. UI rate, never physics rate.
pub fn agent_debug_json(&self) -> Result<JsValue, JsValue>;
```

`agents_json` mirroring `parameter_meta_json` matters more than it looks: it is
what keeps autopilot tuning ranges from being duplicated in TypeScript, the same
way F7/F8 keep physical parameters out of TypeScript. Same rule, same reason.

And extend `Scenario` with `agent: Option<AgentSpec>` and `route:
Option<Route>`, bumping `schema_version` to 2 — the six existing scenarios are
all `schema_version: 1` and the loader already rejects unknown versions, so this
is a real bump with a real migration, not a free field.

---

## 8. Reserving swarm without building it

The reservations that cost nothing now and save a rewrite later:

**Make the env crate plural from day one, and the WASM object singular.**
`sailgym-env` holds `Vec<BoatRuntime>` with N = 1 as the ordinary case; `Sim`
stays exactly the single-boat object it is. Adding a `Fleet` export later is
then additive, not a refactor. Getting this backwards — a single-boat env that
`Fleet` has to work around — is the expensive mistake.

**One environment, many boats.** Wind field, clock and seed live in the episode,
not per boat. The physics is already shaped for this: wind is a field sampled at
a position, so N non-interacting boats in one field is nearly free. That gets
you ghost races and fleet replays immediately, which is most of the near-term
value.

**Fleet snapshot = `n_boats × STATE_LEN`, row-major, F8.3 order, one call.** One
typed array per frame. Do not invent a second state layout.

**Boat-to-boat interaction, when it comes, enters through `WindField`, not
through a new force.** `WindField` is already a trait with one `sample`
contract, and a wind shadow is exactly "the air behind that boat is slower". A
`ShadowedWind<'a>` decorator wrapping `ProceduralWind` and subtracting wakes is
the seam. A new force term in `forces.rs` is not — it would break the fixed
summation order of F9.4 and put fleet state inside the physics crate.

One caveat to record: F9.6 forbids parallelism *inside a single step*. Stepping
N **independent** boats in parallel is bit-identical, because they share no
accumulator. That stops being true the moment they interact through a shared
wind field, so if wind shadowing arrives, the parallel path has to go back to a
fixed index order — or to an explicit two-phase sample-then-step, which is the
better answer anyway.

Out of scope and worth saying so loudly: collisions, right-of-way, avoidance.
brief §44 defers all three and none of them is needed for ghost racing.

---

## 9. The RL contract

Freeze it early, as `autopilot-suggestions.md` says. What that means concretely:

**Termination is an enum, not a bool.** Gymnasium distinguishes task
termination from time-limit truncation and conflating them biases value
bootstrapping:

```rust
pub enum Outcome {
    Running,
    Finished { time: f64 },
    Terminated(TerminationReason),   // Capsized, OutOfBounds, MarkMissed
    Truncated,                       // step budget only
}
```

**Log every decision, not a time sample.** The existing recorder samples at
`log_hz` and would miss intervening actions — a real gap for RL, where the
action sequence *is* the artifact. Since decisions are at a fixed cadence with
zero-order hold, `(step_index, action)` pairs are a complete and small record.
Add a `decisions` array alongside `frames` in `Episode`; keep the sampled frames
for inspection. Do not try to make one array serve both.

**The episode header is the comparison contract.** `EpisodeHeader` already
carries `scenario`, `parameters`, `dt`, `log_hz` and `ToolchainInfo` — extend
it, do not invent a parallel structure: add `agent_spec`, `action_space`,
`obs_mask`, `cadence`, `route`, and a **digest of the serialised F7
catalogue**. That digest is what makes "the physics was frozen for this
comparison" checkable, which matters precisely because of the open
`stability.gm` and `sheet.l_sheet_min` decisions in `parameters.md` — a stronger
policy is exactly the thing that will find and exploit them.

**Evaluation reports more than return.** Finish rate, completion time,
capsizes, missed marks, failed maneuvers, on held-out wind seeds and held-out
courses. Reward alone will not tell you the policy learned to cut a mark.

**Vectorisation shape, for later.** `VecEnv { envs: Vec<Episode> }` with

```rust
fn step_all(&mut self, actions: &[f64], obs_out: &mut [f32], done_out: &mut [u8]);
```

Flat caller-provided buffers, no per-env allocation — the pattern
`sample_wind_grid` already proves. Note `obs_out: &mut [f32]` is fine and F9.5
is not violated: F9.5 forbids `f32` *intermediates in physics*; this is an
output buffer, same as the wind grid's.

**Python binding is a fifth crate (`sailgym-py`, pyo3) over `sailgym-env`, not
anything in `sailgym-wasm`.** Keep the two boundaries independent; they have
different performance shapes and different lifetimes.

---

## 10. Acceptance criteria, in the house style

Commands and numbers, never "looks right" (F13.4):

| Test | Asserts |
|---|---|
| `cargo test -p sailgym-agent obs_round_trip` | `to_array`/`from_array` is the identity, `OBS_FIELDS.len() == OBS_LEN` |
| `cargo test -p sailgym-env determinism` | `advance(n) == n × advance(1)` **with an agent attached**, bit for bit |
| `cargo test -p sailgym-env cadence` | decisions land on `step % period == 0` regardless of call chunking |
| `cargo test -p sailgym-env mirror` | a route mirrored about the wind axis yields the mirrored trajectory |
| `cargo test -p sailgym-agent helm` | heading tracking error < X° at steady state, against known rudder self-centring |
| `cargo test -p sailgym-env completion` | rule sailor finishes a windward–leeward course from ≥ N/M seeded starts |
| `cargo test -p sailgym-env --test regression` | golden autopilot trajectories, with R7's toolchain-fingerprint skip |

The mirror test deserves emphasis. R3 (sign-convention drift) is named as the
highest-probability defect class in this repo, and tack/gybe logic is nothing
but signs — which tack, which way the boom goes, which side of the line. The
existing symmetry machinery in `tests/symmetry.rs` extends to this almost for
free and will catch a class of bug that a completion-rate test will only ever
report as "sometimes it sails badly to port".

---

## 11. Suggested order

1. `sailgym-course` — `Route`, `Guidance`, passage. Testable with no controller
   at all: feed it recorded trajectories and assert passage decisions.
2. `sailgym-agent` — `Observation`, `Action`, `Agent`, `Cadence`, `Helm`, and
   `manual` as the first implementation. The gate should be green with the human
   re-plumbed through the trait before a single autopilot exists.
3. `sailgym-env` — the runner, termination/truncation, decision logging.
4. The rule sailor, to the completion-rate criterion.
5. WASM surface + picker UI.
6. Polar generation (a `sailgym-bench` binary) and the polar racer.
7. `VecEnv`, then `sailgym-py`.

Step 2 is the load-bearing one: if `manual` goes through the trait and the gate
stays green, the interface is real. If it needs a bypass, the interface is
wrong, and you have found that out before writing any controller.
