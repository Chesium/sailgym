# Ablating action and observation spaces — design suggestion

Extends `unified-agent-interface.md`. That note fixed *where* a policy cuts into
the control stack; this one is about making the cut itself a **swept
experimental variable**, across action spaces (rate / angle / force) and sensor
suites (IMU → +wind → +wind field → +lidar → +priors).

Status: suggestion. Same scope caveat as the companion note — brief §44 defers
RL, obstacles and multiple boats; §11 additionally defers hand force
explicitly.

---

## 0. The finding that shapes everything

The six action spaces you named are **not** six variants of one thing. They fall
into three tiers with wildly different costs, and the tier boundaries are set by
normative documents, not by effort.

| Action space | Tier | What it costs |
|---|---|---|
| Rate commands (`Controls`, native) | **0** | nothing — it is the existing interface |
| Rudder **angle** `δr_cmd` | **0** | an adapter above `Controls`; no physics change |
| Sheet **length** `L_cmd` | **0** | same |
| Sail/boom **angle** `β_cmd` | **✗** | **forbidden.** F6.8: "the boom angle is never assigned"; F6.9: no tack state exists. See §1.3 |
| Tiller **force / torque** | **2** | a new second-order DOF **and** a new foil sub-model that does not exist |
| Sheet **hand force** | **2** | a new DOF, and brief §11 explicitly defers hand force |

Tier 0 is an afternoon. Tier 2 is a physics milestone with a normative
amendment. Design the interface so tier 0 lands now and tier 2 slots in later
without a rewrite — but do not plan as if they are comparable in size.

### The tier-2 detail worth knowing before you promise anyone tiller force

`hydro/rudder.rs` applies the blade load at a **fixed** point `p.rudder.pos_b`.
There is no chordwise centre of pressure that migrates with angle of attack.
So the hydrodynamic moment *about the rudder stock* is identically zero in this
model: **there is currently no tiller feel to feel.** Force-controlling the
tiller against zero resistance is just a badly-conditioned rate controller.

Tiller-force control therefore needs two separate additions:

1. A CP-offset model in `foil.rs` — `x_cp(α)` so the load has a moment arm about
   the stock. This is new physics and belongs in an F5 amendment.
2. `δr` promoted from first-order to second-order:
   `I_tiller·δ̈r = τ_hand + τ_hydro(δr, flow) − c_tiller·δ̇r`, which adds a state
   variable.

Same shape for sheet force: F6.8 computes `T` from the extension `ℓ(β) − L`,
with `L` commanded. Force control inverts that — `L` becomes the dynamic
response to the mismatch between the sailor's pull and the rope tension — which
is another new DOF, plus the mechanical-advantage and friction model that
brief §11 defers.

### How to add DOFs without detonating F3

Do **not** extend `BoatState` from 13 to 15. `STATE_LEN = 13` and the F8.3
index order are normative, and the golden regression files, the episode schema
and `web/src/sim/snapshot.ts` all index into them.

Instead, put the new DOFs in a **separate, parallel** actuator state:

```rust
pub trait ActuatorModel {
    const ACT_LEN: usize;
    const ACT_FIELDS: &'static [&'static str];
    fn derivative(&self, act: &ActState, st: &BoatState, cmd: &Command,
                  p: &BoatParameters) -> ActStateDot;
    /// What the force model sees. For the default model this is a pure read of
    /// `st.delta_r` / `st.l_sheet` — no new numbers at all.
    fn effective(&self, act: &ActState, st: &BoatState) -> Effective;
}
```

with `RateActuator` (the current behaviour, `ACT_LEN = 0`) as the default. Then:

- `ACT_LEN = 0` means the default model adds no state, no arithmetic and no
  bit-difference — **the existing goldens stay valid**, which is the property
  that makes this affordable.
- F3 and F8.3 are untouched, so `snapshot.ts` and the recording schema are
  untouched.
- `ForceActuator` is a second implementation with `ACT_LEN = 2`, published
  through its own `actuator_snapshot()` and its own field list.

Prove the first property with a test: `RateActuator` reproduces a golden
trajectory bit-for-bit. If it does not, the refactor is wrong and you find out
on day one rather than after the ablation has run.

---

## 1. The action side: one funnel, adapters above it

```
Policy ─ normalised aᵢ ∈ [−1,1]ᵏ ─► Actuation adapter ─► Controls ─► physics
                                     (the swept variable)
```

```rust
pub trait Actuation {
    fn id(&self) -> &'static str;
    fn dim(&self) -> usize;
    /// Bounds are always [−1,1]; denormalisation lives here and nowhere else.
    fn to_controls(&mut self, a: &[f64], st: &BoatState, p: &BoatParameters) -> Controls;
    fn reset(&mut self);
}
```

### 1.1 Normalise at the boundary, always

Every action space presents `[−1, 1]^k`. The adapter denormalises. This is not
cosmetic: if the angle arm hands the network radians and the rate arm hands it
normalised commands, you have confounded *action space* with *action scaling*,
and action scaling alone will dominate your results. It is the single most
common way an action-space ablation produces a meaningless ranking.

### 1.2 The tier-0 adapters

| `id` | dim | Mapping |
|---|---|---|
| `rate` | 2 (+1 release) | identity onto `Controls` |
| `angle` | 2 | inner rate loop: `rudder_rate_cmd = clamp(kδ·(δr_cmd − δr), −1, 1)`, similarly for `L` |
| `angle_bangbang` | 2 | `sign(δr_cmd − δr)` with a deadband — the crude inner loop, worth having as a control |

The `angle` adapter has a gain `kδ` that is **an agent parameter, not a physical
coefficient** — brief §43 does not apply to it. Keep it in the agent crate, and
sweep it, because "angle control beats rate control" can easily be "I picked a
good `kδ`".

### 1.3 Boom angle: what to do instead

Commanding `β` directly is forbidden and should stay forbidden — it is exactly
the shortcut that makes the boom stop being dynamic, and section 07's
`no_shortcuts` audits exist to catch that class of thing.

If you want the "the sailor thinks in boom angle" arm, implement it honestly as
`beta_target → L` through the F6.8 geometry: invert `ℓ(β)` to get the sheet
length at which the boom *could* sit at `β`, and command that length. The boom
still has to get there through its own dynamics, it will overshoot, and in a
gust it will not arrive at all — which is the physically correct outcome and,
incidentally, the interesting thing for a policy to learn.

Label it `sheet_length_for_beta`, not `beta`, so nobody later reads the
experiment log and believes the boom was commanded.

### 1.4 Hold the plant fixed across arms — including self-centring

`delta_r_self_centre = true` is the F7 default, and an angle adapter will fight
it: with no command the tiller runs back to neutral at 90°/s, so the inner loop
must hold a nonzero command just to stand still. Turning it off for the angle
arm and leaving it on for the rate arm confounds action space with actuator
physics, and the confound is large.

Set `delta_r_self_centre = false` for **every arm of the ablation, including
rate**, and record that this is a non-default catalogue. Or run the whole grid
twice, on and off, and report it as a second factor — it is a legitimately
interesting one.

### 1.5 Every arm is comparable at the `Controls` layer

A useful consequence of the funnel: whatever the action space, the realised
`Controls` trajectory exists. **Log it for every arm.** Then "the angle arm
wins" can be checked against "the angle arm's realised rudder trajectory is
smoother/faster", which distinguishes a genuine action-space advantage from an
inner loop that happened to be well tuned.

---

## 2. The observation side: a sensor list, not a mask

The companion note proposed an `ObsMask` over a fixed field set. For this study
that is the wrong shape — a 2-D lidar has a configurable ray count, so its width
is not a subset of anything fixed. Generalise it:

```rust
pub trait Sensor {
    fn id(&self) -> SensorId;
    fn version(&self) -> u32;
    /// Fixed for the episode, known at reset. Drives the layout.
    fn width(&self) -> usize;
    /// Names for each of `width()` scalars. Emitted into the log as data.
    fn field_names(&self) -> Vec<String>;
    /// The one place privileged world access is permitted.
    fn sense(&mut self, w: &WorldView, rng: &mut Pcg32, out: &mut [f64]);
}
```

The observation vector is the ordered concatenation of the configured sensors.
`ObsMask` survives as one flag per sensor: *is this one privileged?*

### 2.1 `WorldView` is the containment boundary

```rust
pub struct WorldView<'a> {
    pub st: &'a BoatState,
    pub p: &'a BoatParameters,
    pub wind: &'a dyn WindField,
    pub course: &'a Route,
    pub obstacles: &'a [Obstacle],
    pub others: &'a [BoatState],     // empty until swarm
    pub t: f64,
}
```

Sensors see everything; the agent sees only the concatenated vector. That single
line is what makes "privileged information" enforceable by construction instead
of by discipline — there is no path from an `Agent` to a `WindField`.

### 2.2 The suite

| Sensor | Width | Notes |
|---|---|---|
| `imu` | 6 | `p`, `r`, `φ`, body accelerations `ax, ay`, and heading if you allow a magnetometer. **No velocity, no position.** |
| `speed_log` | 1 | `u` through the water. A real boat has one; separating it from the IMU is a genuinely interesting ablation axis |
| `apparent_wind` | 2 | masthead vane + anemometer: AWA, AWS. **This is what a real boat measures** |
| `true_wind` | 2 | derived on a real boat from apparent + boat speed. Mark **privileged** if `speed_log` is absent |
| `rig_state` | 4 | `β`, `β̇`, `L`, sheet slack — proprioception, cheap, and the policy is lost without it |
| `actuator_state` | 2 | `δr` and the rate command in force. Required: the inner loop is part of the plant |
| `wind_probe` | `2·n_rings·n_sectors` | field sampled on a polar pattern ahead. Reading gusts. Configurable, privileged-ish |
| `lidar2d` | `n_rays` (×2 with class) | horizontal ray cast. §2.4 |
| `guidance` | 5 | cross-track, bearing-to-target relative to heading, distance, leg bearing vs. wind, rounding side |
| `polar_prior` | 2–3 | target speed and target TWA from the measured polar at current TWS. An explicit "prior" |
| `layline_prior` | 2 | bearing to each layline for the next mark |

Two things worth stating out loud because they are the interesting part of your
IMU-only arm:

- **IMU-only means the policy must estimate its own speed.** No `u`, no `v`, no
  position. It has angular rates and accelerations and must integrate them,
  under noise, with no drift correction. That is a real partial-observability
  problem and probably wants a recurrent policy or a frame stack. Budget for
  that — it is not the same learning problem as the other arms, and a flat MLP
  will simply fail rather than fail informatively.
- **True wind is not a sensor.** Real boats compute it from apparent wind and
  boat velocity. If you hand a policy true wind while denying it a speed log,
  you have handed it a free velocity estimate through the back door. Either
  mark it privileged or derive it in the sensor from the sensed quantities.

### 2.3 The IMU costs one force evaluation, and it must be a fresh one

Body accelerations are derived, never integrated (brief §5). `Diagnostics`
carries `acceleration_body`, but it is computed from `Simulation::forces` — the
cached breakdown refreshed **once per `advance(n)` call**. Reading it from
inside the step loop gives an acceleration up to `n` steps stale, and the
observation then depends on how the caller chunked its calls.

So `imu` must call `evaluate(...)` itself at the decision instant. At 20 Hz
decisions over 200 Hz physics that is one extra evaluation per ten steps — about
10% on top of RK2's two per step, which is affordable. But it must be the fresh
call, not the cache, or the determinism identity quietly dies in the one place
nobody tests.

### 2.4 Lidar, and getting it without collision physics

Obstacles do not exist yet. The minimum that makes the sensor real:

```rust
pub enum Obstacle {
    Circle { c: Vec2, r: f64 },
    Segment { a: Vec2, b: Vec2 },     // shoreline, pier, course boundary
}
```

in `sailgym-course` (it is task geometry, not physics). Ray casting is analytic
against both primitives, deterministic given a fixed ray order — iterate a `Vec`
in index order, never a set (F9.3).

**You do not need collision physics to run the lidar ablation.** Sense
obstacles, and score contact as a *termination* (`TerminationReason::Collision`)
rather than simulating contact forces. That keeps you inside the spirit of
brief §44's deferral, costs nearly nothing, and answers the question you
actually care about — can a policy use range data to avoid things — without a
contact model.

Parameters to expose, because each is an ablation axis of its own: `n_rays`,
angular span, max range, and whether returns carry a class channel.

And the unification worth designing for now: **when swarm arrives, other boats
are just obstacles.** `WorldView::others` feeds the same ray cast. Do not build
a separate boat-detection sensor.

### 2.5 Noise, bias, latency and dropout are first-class

"How much does sensor *quality* matter" is at least as interesting as "which
sensors", and it is nearly free once the sensor abstraction exists:

```rust
pub struct NoiseModel {
    pub bias_sigma: f64,      // drawn once at reset, constant for the episode
    pub white_sigma: f64,
    pub dropout_p: f64,       // hold last value
    pub latency_steps: u32,
}
```

Each sensor draws from `Pcg32::stream` keyed by **its own** `SensorId` — the
same named-stream discipline `rng.rs` already uses for the wind. That is what
lets you add a lidar to arm 7 without perturbing the IMU noise in arm 7, which
would otherwise silently make arm 7 incomparable to arm 3.

`STREAM_AGENT = 4` from the companion note, with per-sensor substreams derived
below it.

---

## 3. What makes this actually extensible

Five mechanisms, in descending order of how much grief they save.

### 3.1 An arm is a config file, not a code path

```json
{
  "arm_id": "imu+aw_rate",
  "action":  { "id": "rate" },
  "sensors": [
    { "id": "imu",           "noise": { "white_sigma": 0.01, "bias_sigma": 0.002 } },
    { "id": "apparent_wind", "noise": { "white_sigma": 0.05 } },
    { "id": "rig_state" },
    { "id": "actuator_state" },
    { "id": "guidance" }
  ],
  "cadence_steps": 10,
  "physics": "ilca7_no_selfcentre",
  "course_set": "wl_train_v1",
  "seeds": [1, 2, 3, 4, 5, 6, 7, 8]
}
```

A 6 × 5 grid is 30 files and zero new code. If adding an arm requires touching
Rust, the abstraction has failed and the study will quietly shrink to the three
arms someone had time to implement.

### 3.2 The observation layout is data, not a constant

The companion note proposed `const OBS_FIELDS`. For this study it must be
**runtime data**: the concatenation of each sensor's `field_names()`, written
into the episode header. Every logged observation vector is then
self-describing, and six months later you can still tell which column was
`lidar[37]`.

### 3.3 A layout digest, checked before any comparison

Hash the ordered `(sensor_id, version, width)` triples into an `obs_digest`, and
the `(action_id, dim, physics_digest)` into a `contract_digest`. Put both in the
header.

Then comparing two runs is a digest check, not an act of faith. The failure this
prevents is specific and common: you widen the lidar from 32 to 64 rays halfway
through, rerun some arms and not others, and compare a v1 arm to a v2 arm
without noticing. Numbers come out, they look plausible, and they mean nothing.

### 3.4 Fixed-width union mode, optional

Variable-width observations are honest — each arm gets its own network, and
that is the right default. But if you want one policy across arms (multi-task,
curriculum, or sensor-dropout robustness training), offer `--pad-to-union`: the
vector is the union of all configured sensors, absent ones are zeroed, and a
per-sensor presence bit is appended.

Offer it; do not default to it. Zero-padding changes what "absent" means to the
network, and a zero is a perfectly plausible IMU reading.

### 3.5 The sensor registry is a `Vec`, and versions are sticky

Sensors resolve by id through an ordered `Vec`, never a `HashMap` (F9.3). And
bump `version()` on *any* change to what a sensor emits, including a field
reorder. The digest depends on it, so a bumped version invalidates comparisons
loudly instead of silently.

---

## 4. Experimental hygiene this design should enforce

Things the code can check, so a reviewer does not have to:

| Hazard | The guard |
|---|---|
| Action scaling confounds action space | bounds are `[−1,1]^k` in the trait contract; assert it |
| Actuator physics confounds action space | `physics_digest` in the header; refuse cross-digest comparison |
| Decision rate differs between arms | `cadence_steps` in the digest |
| Obs layout drifted mid-study | `obs_digest` |
| Policy learned the course, not sailing | no absolute `x, y, ψ` in any non-privileged sensor; held-out courses in eval |
| Policy exploited a physics quirk | freeze and digest the F7 catalogue; the open `stability.gm` and `sheet.l_sheet_min` decisions in `parameters.md` are exactly what a strong policy will find |
| Privileged info leaked | the `WorldView` containment boundary; per-sensor privileged flag reported in every results table |
| Stale polar prior | the `polar_prior` sensor carries the digest of the physics its table was measured on, and refuses on mismatch |

Report per arm, always: finish rate, completion time, capsizes, collisions,
missed marks, and **realised control effort** — on held-out wind seeds and
held-out courses. A reward curve will not tell you the IMU-only arm learned to
sail conservatively rather than well.

---

## 5. Suggested order

1. `Actuation` trait + `rate` adapter. Prove the existing behaviour is
   unchanged bit-for-bit. Nothing else moves until this is green.
2. `Sensor` trait, `WorldView`, layout-as-data, digests. Port the companion
   note's fixed observation into sensors: `imu`, `apparent_wind`, `rig_state`,
   `actuator_state`, `guidance`.
3. Config-driven arms + the runner. **Run the grid with the rule sailor, not a
   policy.** It is cheap, deterministic, and it shakes out layout and digest
   bugs before any GPU time is spent.
4. `angle` and `angle_bangbang` adapters, plus `sheet_length_for_beta`. That is
   the tier-0 action sweep complete.
5. Noise models. Then `wind_probe`, `polar_prior`, `layline_prior`.
6. Obstacles + `lidar2d` + collision-as-termination.
7. Tier 2, if the results justify it: CP-offset foil model, second-order
   actuator DOFs, `force` adapter. This needs an F5/F7 amendment and human
   sign-off; it is a physics milestone, not an adapter.

Step 3 is the one people skip and should not. A rule sailor sweeping all 30 arms
overnight will find every layout bug, every digest mismatch and every leaked
privileged field, for the price of some CPU.
