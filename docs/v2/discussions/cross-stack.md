# Gymnasium, and a JAX/Warp second stack — design suggestion

Third note in the series, after `unified-agent-interface.md` and
`ablation-spaces.md`. brief §45 names both "Gymnasium/PettingZoo-style RL
environment" and "vectorized alternate physics backend → JAX / Warp
experiments"; brief §37 adds that the design must not preclude "thousands of
parallel RL environments".

Status: suggestion. Same scope caveat throughout.

---

## 0. Measure before you port — the numbers argue against JAX for throughput alone

From `docs/v1/performance.md` via the section 10 handoff: **native headless runs at
3 144–3 532× real time.** At `dt = 0.005` real time is 200 steps/s, so that is
roughly **630 000 physics steps/s on one core**. At the 20 Hz decision cadence
that is ~63 000 agent-decisions/s per core.

A `rayon`-parallel `VecEnv` over N independent `Simulation`s on a 32-core box
lands around **2 × 10⁷ steps/s**, which is in the same league as GPU-vectorised
simple-physics environments — and it costs one crate, no reimplementation, and
**no loss of bit-identity**.

So the honest framing:

| Reason to build a JAX/Warp stack | Does Rust+rayon already cover it? |
|---|---|
| Raw throughput for PPO-style on-policy training | **Largely yes.** Measure before porting |
| Avoiding host↔device transfer in a GPU training loop | No — this is a real win |
| Differentiable simulation / analytic policy gradients | No — but see §7, the physics fights you |
| Very large batch (10⁵+ envs) on one GPU | No — real win |
| **An independent implementation that cross-checks the first** | No — and this is the most underrated reason |

That last row is the one I would lead with. A second implementation is the
strongest correctness instrument this project could acquire — it catches the
sign errors, unit slips and branch-boundary mistakes that R3 names as the
highest-probability defect class, because two independent ports rarely make the
same mistake. Treat the JAX/Warp work as **verification infrastructure that
also happens to be fast**, and it is worth doing even if rayon wins on
throughput.

Concrete first step, before any port: build `VecEnv` with rayon, benchmark it
against your actual training loop, and find out whether you have a throughput
problem at all.

*(Note on F9.6: "no parallelism inside a single simulation step." Stepping N
**independent** boats on N threads is bit-identical — they share no
accumulator. That stops being true the moment they interact through a shared
wind field, per the swarm note.)*

---

## 1. Gymnasium

### 1.1 Binding

`sailgym-py`, pyo3 + maturin, over `sailgym-env`. **Not** over `sailgym-wasm` —
two independent boundaries with different performance shapes and lifetimes.

Non-negotiables for it to be fast:

- Zero-copy `numpy` in and out. Observations write into a caller-provided
  array, matching the `step_all(actions, obs_out, done_out)` shape from the
  companion note and the `sample_wind_grid` precedent already in the repo.
- **Release the GIL** for the duration of `step_all`. Without this the rayon
  `VecEnv` is pointless from Python.
- No per-step Python object allocation. `info` dicts per step per env will
  dominate your profile; return a struct-of-arrays and let the wrapper
  materialise dicts only when asked.

### 1.2 Spaces come from the sensor layout, not from a hand-written constant

The ablation note makes the observation layout runtime data — the concatenation
of each configured sensor's `field_names()`. So:

```python
observation_space = Box(low, high, shape=(sum(s.width for s in sensors),))
action_space      = Box(-1.0, 1.0, shape=(actuation.dim,))
```

built from what the Rust core reports. Never typed out in Python. This is the
same rule F8 applies to TypeScript — **no physical constant and no layout is
written in Python** — and it is worth stating as such, because Python is where
that discipline usually collapses.

The `obs_digest` from the ablation note should be exposed as an env attribute
and written into every checkpoint. A policy loaded against a different digest
should refuse, not silently misread its input.

### 1.3 Seeding: keep Gymnasium's RNG away from the physics

Gymnasium seeds a per-env `np_random`. This repo's determinism flows from an
explicit `u64` through `rng.rs`'s named streams (F9.2).

Map `reset(seed=k)` straight onto the Rust seed, and **use `np_random` for
nothing that touches the simulation** — not wind, not sensor noise, not
scenario sampling. If you want a randomised scenario per episode, derive it
from the same `u64` through `STREAM_SCENARIO`, which `rng.rs` has already
reserved for exactly this. Two RNGs feeding one episode is how a "deterministic"
env stops being reproducible.

### 1.4 Termination, truncation, and the autoreset trap

Keep `Outcome` (from the companion note) authoritative **inside Rust**, and do
not wrap with Gymnasium's `TimeLimit`. Reason: the vectorised path cannot use
Python wrappers, so a wrapper-supplied truncation would make the single-env and
vector paths disagree about episode boundaries — and they would disagree
silently, in the returns.

The autoreset convention is the specific thing to pin down early. Gymnasium's
vector API and the JAX-native env ecosystem have historically differed over
whether the reset observation appears on the *same* step as `done` or the
*next* one, and Gymnasium 1.0 made the mode explicit/configurable. Check the
convention of the exact version you install rather than assuming — then:

1. Pick one convention and implement it in Rust, once.
2. Make the JAX stack use the same one.
3. Test it numerically: compute discounted returns for a fixed action sequence
   both ways and assert they agree. An off-by-one here shifts every bootstrapped
   value by one step and produces training curves that look merely *mediocre*
   rather than broken, which is the worst kind of bug.

### 1.5 Swarm → PettingZoo

When multi-boat arrives, `ParallelEnv` is the right interface (all agents act
simultaneously, which matches a fleet). Keep the single-agent Gymnasium env as
the N=1 specialisation of the same Rust runtime rather than a separate code
path.

---

## 2. Cross-stack consistency: bit-identity is not available. Say so first.

F9 already scopes its guarantee to "the same build on the same platform" and
explicitly does not claim cross-platform bit-identity. Cross-*stack* is
strictly harder:

- XLA reassociates and fuses arithmetic; a fused multiply-add is not the same
  as a multiply then an add.
- JAX defaults to f32, and f64 on consumer GPUs runs at a small fraction of f32
  throughput, so you will *want* f32 for training.
- GPU reductions are not order-deterministic unless explicitly forced.
- Warp kernels with atomics have the same issue.

So do not write a conformance test that asserts equality. Define a **tolerance
contract**, tiered by how much numerical error has had a chance to accumulate.

### 2.1 Set the tolerances from the convergence study, not from taste

The repo already has a time-step convergence study (`docs/v1/convergence.md`,
`tests/convergence.rs`) establishing RK2's second-order behaviour and hence the
magnitude of the local discretisation error at `dt = 0.005`.

That gives a principled tolerance instead of an arbitrary `1e-6`:

> A second implementation agrees if its divergence from the reference is small
> compared with the discretisation error the reference already carries.

If the two stacks differ by less than, say, 10% of the known `O(dt²)` local
error, they are the same physics expressed differently. If they differ by more,
something is wrong — and you can say *which* something, because the tolerance
has a physical meaning. Pick the fraction once, justify it in the doc, and hold
every tier to a tolerance derived the same way.

### 2.2 Four tiers, because long trajectories are the wrong test

Comparing 60-second trajectories is the instinctive test and a bad one: near
the stability boundary this system is strongly divergent (R2 puts it close to
capsize in normal conditions), so two implementations differing in the last bit
will separate exponentially and the tolerance becomes meaningless — you learn
only that floating point exists.

| Tier | What is compared | Why it is the useful one |
|---|---|---|
| **0 — pure functions** | `foil_force`, `sail_load`, mainsheet `T`, `GZ`, `wrap_pi`, `apparent_wind_at`, `wind.sample` at ~10⁴ sampled inputs | **This is where essentially every porting bug lives.** No accumulation, so tolerances are tight and a failure names the function |
| **1 — one derivative** | `derivative(state, controls, params)` at sampled states | Catches assembly and sign errors in `Generalized::add`, the F6.4 heel rotation, the summation order |
| **2 — short horizon** | 1–5 s trajectories from fixed initial conditions | Catches integrator and clamp-ordering differences before divergence dominates |
| **3 — invariants and ensembles** | The brief §35 invariant suite re-run against the port; ensemble statistics over many seeds | This is what licenses the claim "same environment", which tier 2 cannot give you |

Tier 3 is the one that matters for RL validity and it is cheap, because the
invariants already exist: rest equilibrium, mirror symmetry, the sheet's
unilateral constraint, zero-flow foil behaviour, dissipativity, frame
consistency. **Port the invariant suite, not just the physics.** A JAX stack
that passes brief §35 is trustworthy in a way that one which merely tracks a
trajectory for three seconds is not.

### 2.3 Sample densely at the branch points, not uniformly

Uniform random sampling over the state space will almost never hit the places a
port actually differs. The whole value of tier 0 is in the discontinuities.
Enumerate them and sample each one at, around, and exactly on the boundary:

| Branch | The trap |
|---|---|
| `limit_rate` | Uses a **strict** inequality, and `dynamics.rs` documents that the strictness is load-bearing — a non-strict port freezes the actuator a timestep-dependent distance short of its limit |
| `T = max(0, k·e + c·ė)` | The unilateral sheet. Sample `e ≈ 0` and the damping-dominated case where the bracket goes negative with `e > 0` |
| `delta_r_self_centre` | A **three-way** `partial_cmp` including the exact-zero case. A two-way `where` is subtly wrong at zero |
| Stall blend | `alpha_stall ± stall_blend`; the blend region is where two smooth pieces are joined |
| `wrap_pi` | At exactly `±π`, and the half-open `(−π, π]` convention |
| `phi` unwrapped | `wrap_angles` deliberately leaves `phi` alone (brief §17). A port that wraps everything breaks capsize |
| Zero-flow foil | Division guards at `|V| → 0`; `0/0` gives NaN in one stack and a guarded zero in the other |
| `sheet_release` | Overrides the analogue command — precedence, not addition |

Each row is a test case, and each is a bug a uniform sampler would miss.

---

## 3. The conformance bundle: ship data, not a second reading of the source

Generate a language-neutral artifact from the Rust core and have every stack
test against it. This is the R7 pattern (goldens carry their toolchain)
generalised across languages.

```
conformance/<digest>/
├── manifest.json        toolchain, F7 digest, generator version,
│                        per-tier tolerances and their justification
├── parameters.json      the F7 catalogue, verbatim
├── wind_modes.json      §4.1 — precomputed spectral modes per (config, seed)
├── tier0_<fn>.npz       inputs + expected outputs, per pure function
├── tier1_derivative.npz
└── tier2_trajectories.npz
```

Three properties make it work:

1. **One runner per stack, consuming the same bundle.** Python/JAX, Warp — and
   **Rust too**. The Rust runner is not redundant: it catches a *stale bundle*,
   which is otherwise the failure that makes everyone chase a phantom port bug
   for a day.
2. **The bundle is keyed by the physics digest.** Same digest as the episode
   header from the earlier notes. Change a parameter, the digest changes, and
   the old bundle refuses rather than silently passing.
3. **Regenerating is a gate step.** A physics change that does not regenerate
   the bundle should fail, the way `cargo fmt --check` fails.

---

## 4. What makes the port tractable

### 4.1 Export the wind modes as data — do not reimplement PCG32 in JAX

`ProceduralWind::new` draws `κ_k`, `ϕ_k`, `ω_k` **once at construction** from
`Pcg32::stream(STREAM_WIND)` and stores them in a `Vec<WindMode3>` (12 modes by
default). `sample` is then a pure, branch-free sum over that table.

This is the single luckiest fact for the port. It means:

- The JAX/Warp stack needs **no RNG conformance at all** — no PCG32 port, no
  bit-matching a stream. Export the mode table alongside the parameters and
  read it.
- The wind sampler becomes a `(K, ...)` tensor contraction, which is exactly
  what JAX is good at, and it vectorises across envs and grid points for free.

Reimplementing PCG32 in JAX to regenerate those twelve modes would be days of
work to reproduce something you can serialise in a kilobyte. Don't.

### 4.2 Read the parameters, never retype them

`parameters.json` is already serialisable and `parameter_meta_json` already
exists. Load it into a frozen dataclass registered as a JAX pytree.

Then add the grep-test that F8 already implies for TypeScript: **the JAX physics
module contains no float literals** beyond structural ones (`0.0`, `1.0`,
`0.5`, `2.0`). Section 10 already built a three-tier `no_stray_constants` audit
for the TS side; point the same idea at Python. Retyped constants are the
second-most-common porting bug after signs, and they are trivially automatable
away.

### 4.3 JAX and Warp are for different jobs — don't pick one

They are not competing backends:

**Warp** is imperative, kernel-per-env, supports f64, and imposes no
control-flow restriction. It can keep the Rust branch structure **verbatim** —
the same `if`, the same three-way comparison, the same strict inequality. That
makes it the better *conformance* target: a Warp port that disagrees with Rust
almost certainly disagrees for a real reason, not because a branch became a
`where`.

**JAX** requires branch-free, shape-static code: every branch becomes
`jnp.where` with both sides evaluated, and `lax.scan` for the rollout with
done-masking rather than variable-length episodes. That rewrite is where
semantics drift. But it composes — `vmap`, `jit`, `grad`, and a training loop
that never leaves the device.

Suggested division: **Warp as the high-fidelity fast backend and the primary
conformance witness; JAX as the composable/differentiable one, held to the same
bundle but expected to need the looser tier-2 tolerance.**

---

## 5. f32 is a different environment until you prove otherwise

Run conformance in f64 (`jax_enable_x64`), train in f32 — but **measure the
f32↔f64 divergence on the same bundle and report it as a number**. If f32
divergence exceeds the cross-stack tolerance, your f32 training environment is
not the environment your reference describes, and policy transfer between them
is an open question rather than an assumption.

The specific thing to watch is **R1**: `k_sheet = 2e4 N/m` with `I_b = 12 kg·m²`
gives `ω ≈ 41 rad/s`, about 30 steps per period at `dt = 0.005`. That is the
stiffest mode in the system and the one the risk register already names as the
most likely blow-up. A stiff, lightly-damped oscillator resolved at 30
steps/period is exactly where f32 degrades first. Put a dedicated
sheet-transient case in tier 2 and watch its f32 error specifically.

If f32 proves inadequate, the escalation is **not** to raise `c_sheet` or lower
`k_sheet` — brief §43 forbids tuning a coefficient to make a result look
better, and R1's mitigation order is explicit. Sub-step the rigging DOF in the
port, or run that DOF in f64.

---

## 6. Vectorisation shape

Structure-of-arrays throughout: `state[n_envs, 13]` in the F8.3 order, so the
layout is the same constant everywhere and a transposed batch is impossible.

`lax.scan` over a fixed rollout length with in-graph autoreset and done-masking.
Fixed-length rollouts are not a compromise here — they are what on-policy
algorithms want anyway.

Terminations become masks, not early exits. Note the consequence for the
capsize observer: F6.10 requires `capsized` to be set after `|φ| > φ_capsize`
holds *continuously* for `t_capsize`, which is a small piece of per-env state
that must be carried through the scan, not recomputed. It is exactly the kind
of thing a port drops, and nothing in a short trajectory test would notice.

---

## 7. If the goal is differentiable simulation, read this first

The physics is full of things that are non-differentiable or that kill the
gradient outright:

- `T = max(0, ...)` — **zero gradient whenever the sheet is slack**, which is a
  large and important part of the state space.
- `limit_rate` and the actuator clamps — zero gradient at saturation.
- Stall blending — smooth by construction, so this one is fine.
- The soft boom limit stop — one-sided spring, so a kink at `±β_max`.
- The capsize observer — a threshold on accumulated time; no gradient at all.

None of this makes differentiable sim impossible, but a naive
analytic-policy-gradient run will produce gradients that are zero exactly when
the interesting thing is happening (the sheet going slack, the actuator
saturating). If that is the goal, plan for smoothed surrogates *for the
gradient path only*, kept strictly separate from the forward model, and
conformance-tested as a distinct backend with its own digest. Do not soften the
forward physics to make gradients nicer — that is brief §43 with extra steps.

---

## 8. Order

1. **`VecEnv` with rayon, benchmarked against your real training loop.** You may
   discover there is no throughput problem. Bit-identical, one crate.
2. `sailgym-py` (pyo3/maturin): single env, then vector. Spaces from the sensor
   layout; seeding through the Rust `u64`; `Outcome` authoritative in Rust.
3. Pin the autoreset convention and test returns computed both ways.
4. The conformance bundle generator + the **Rust** runner. Before any port —
   this is what a port is tested against, and it must be trustworthy first.
5. Port `wind.sample` alone to JAX, against tier 0. One function, modes as
   data, no RNG. It proves the whole toolchain — bundle, loader, tolerance,
   digest check — on the easiest possible target.
6. Warp port of the full step, verbatim branch structure, tiers 0→3.
7. JAX port, branch-free, tiers 0→3 at the looser tolerance.
8. Port the brief §35 invariant suite to both. This is what licenses "same
   environment", and it is the step most likely to be skipped under deadline.

Steps 4 and 5 are the ones that make the rest cheap. Building the conformance
machinery against `wind.sample` — a pure, branch-free, 12-term sum with no RNG
dependency once the modes are data — means that when the hard port lands, the
only new thing being tested is the physics.
