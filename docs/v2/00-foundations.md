# v2 — Foundations (Normative deltas)

**This document contains no tasks and is not executed by an agent.**

**Status: proposed deltas, not implemented contracts.** The [index](README.md) schedules 08–11 before research sections 02–07. The brief records separate scope decisions. v1 foundations remain the shipped reference; explicit v2 corrections require a recorded decision and evidence, not a silent reinterpretation. No signature is supplied by this planning update.

F18 records the playable milestone. F14–F17 remain later research proposals, with the corrections below incorporated. Section-number order is not delivery order.

Numbering continues from v1: F1–F13 are v1's, F14 onward are v2's.

| Delta | Subject | Status |
|---|---|---|
| **F12′** | The gate grows to eleven steps | proposed; sections 02 and 03 |
| **F14** | Agent interface — sensors, actions, cadence, helm | blocked on V-A |
| **F15** | Task and course — routes, marks, guidance, passage | blocked on V-A |
| **F16** | Conformance, digests and the tolerance contract | proposed; section 02 |
| **F17** | The Python boundary | proposed; sections 03 and 07 |
| **F18** | Corrected model, input, replay and tasks | proposed; sections 08–11 |

---

## F12′. The gate

After 11, step 3 includes sailgym-task. Only when 03 lands does the chain become **eleven** steps. Steps 1–9 keep their numbers and their
meaning; step 4's test list gains one entry.

```
 1. cargo fmt --check
 2. cargo clippy --all-targets -- -D warnings
 3. cargo test -p sailgym-physics -p sailgym-task
 4. cargo test -p sailgym-physics --test invariants --test no_shortcuts \
                                  --test convergence --test symmetry \
                                  --test provenance --test conformance     ← + conformance
 5. cargo test -p sailgym-physics --test regression
 6. wasm-pack build crates/sailgym-wasm --target web --out-dir ../../web/src/wasm
 7. pnpm --dir web typecheck
 8. pnpm --dir web test:unit
 9. pnpm --dir web test:e2e
10. uv run ruff check python && uv run ruff format --check python           ← new
11. scripts/py-test.sh                                                     ← new
```

Two properties of this shape are deliberate:

- **Step 4 grows rather than a step 12 appearing.** `--test conformance` proves
  a property of the physics crate, which is what step 4 is for. A separate step
  would make a red step 4 no less ambiguous and would cost another F12 change.
- **Step 11 delegates to a script**, exactly as step 6 delegates to
  `build-wasm.sh`. Later sections add `maturin develop` in front of `pytest`
  inside `scripts/py-test.{sh,ps1}` **without amending F12 again**. This is the
  whole reason the step is a script and not a command line.

Every site that spells the chain out moves together, or the gate lies about
itself: `scripts/check.sh` (the `step_names` array **and** the `run_step` case),
`scripts/check.ps1` (the `$Steps` array **and** `[ValidateRange(1, 9)]` →
`(1, 11)`), the table in `CLAUDE.md`, the chain in `README.md`, and F12 itself.

Additions to the pinned stack: `uv`, `ruff`, `pytest`, `jax`, `maturin`, `pyo3`,
`rayon`. **`rayon` may not appear in `sailgym-physics`** — see F16.5.

All gate changes remain proposed. Section 11 adds task coverage before research gates; retain it in all later crate lists.

---

## F14. Agent interface

*Blocked on V-A. Source: `discussions/unified-agent-interface.md`,
`discussions/ablation-spaces.md`. Implemented by section 05.*

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

---

## F15. Task and course

*Blocked on V-A. Source: `discussions/unified-agent-interface.md` §5.
Implemented by section 04.*

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

---

## F16. Conformance, digests and the tolerance contract

*Proposed for sections 02 and 03, after 08 and 10. S1/S2 and these deltas require a recorded implementation decision; neither section is unconditionally dispatchable.*

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

## F17. The Python boundary

*Implemented by sections 03 and 07.*

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

## F18. Playable milestone contracts (proposed)

### F18.1 Corrected reduced model — section 08

Retain 13 state scalars and force-based RK2 motion. Resolve GZ peak/root/domain constraints, unintended sheet preload and positive slack tension in the shared physics modules. Record exact changed F6/F7 clauses, equation/parameter rationale, before/after evidence and a new model/source identity. Do not choose replacement coefficients by tutorial success. No new degrees of freedom or empirical certification is implied.

### F18.2 Input ownership — section 09

The web composes device input once into normalized Controls. Rust limits actuator rates. Release overrides sheet rate; each pointer owns one pad; lifecycle transitions clear transient commands. Actual-state gauges and rate feedback are distinct. Position-target control waits for explicit engaged/released semantics in a shared Rust adapter.

### F18.3 Recording and comparison — section 10

Extend the existing versioned Episode codec with the diagnostic/identity data needed for truthful display. Decode schema-1 files with unavailable fields explicit. Replay selects one recorded source for every consumer and does not recompute missing forces using current physics. Canonical identity contains model/source, parameters, initial state/controls, integrator/dt, wind/seed and applicable task/action/observation contracts. Unknown is not equal to known. Sampled inspection is not action resimulation.

### F18.4 Guided tasks — section 11

A small pure sailgym-task evaluator owns practice outcomes outside physics. Evaluate on physics steps; thresholds and ordered events are versioned task data. Retry restores exact conditions, not live edited defaults. Record outcomes/events and compare only compatible attempts. Later course/env work reuses these semantics. Only two attempts are retained in session memory; no persistence framework is required.
