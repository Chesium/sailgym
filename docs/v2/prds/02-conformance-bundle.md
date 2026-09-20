# v2 Section 02 — The conformance bundle: generator, Rust runner, and the throughput question

Source discussion: `../discussions/cross-stack.md`, §§0, 2, 3, 4.1.
Answers §8 point **4**, and §0's "measure before you port".

Read first, in order: `../../v1/00-foundations.md` in full, `../README.md`,
`../brief.md`, `../00-foundations.md` (F12′, F16), then this PRD. The discussion
note is background; where it and this PRD differ, **this PRD wins**.

## Goal

Generate a language-neutral data artifact from `sailgym-physics` that any second
implementation can be tested against, and a **Rust** runner that consumes it.
Then answer, with a number, the question `cross-stack.md` §0 says to answer
before porting anything: *is there a throughput problem at all?*

Nothing in this section ports any physics. Nothing in this section is Python.

## Why this shape

Three reasons, in descending order of how much they matter.

1. **The Rust runner is the point of the whole section.** A bundle that no
   trusted implementation checks is a bundle that silently goes stale, and a
   stale bundle is the failure that sends a porting team hunting a phantom bug
   for a day. The runner is written *before* any port exists, so that when the
   port lands the only new thing being tested is the port.
2. **The bundle is data, not a second reading of the source.** This is R7's
   pattern — goldens carry their toolchain — generalised across languages.
3. **§0 argues against porting for throughput alone.** `../../v1/performance.md`
   records 3 144–3 532× real time on one core. Task 2.7 measures what rayon adds
   across independent boats, so section 03 onwards is a decision rather than an
   assumption.

## What this section does **not** do

- No Python, no JAX, no Warp. Section 03.
- **No physics changes.** Three accessors and one new module are added; no
  equation, no parameter, no state field, no summation order moves. `git diff`
  on `forces/`, `dynamics.rs`, `integrator.rs` and `parameters.rs` must be empty
  at the end of the section, and task 2.8 asserts it.
- No tier-3 runner. The brief §35 suite is already in `tests/invariants.rs` for
  Rust; porting it is section 03's problem for the wind slice and a later
  section's for the rest.
- No `obs_digest` or `contract_digest`. Those need F14 and are section 05's.
- No `sailgym-env`. Task 2.7 measures `Vec<Simulation>` directly, on purpose:
  the throughput answer is wanted *before* the env crate is designed, not after.

## Normative deltas

### D1 — F12′ step 4 gains `--test conformance`. **Open; blocking task 2.8.**

Step 4 proves "the physical invariants still hold". A stale-bundle check is a
property of the physics crate and belongs there. Recorded in
`../00-foundations.md` F12′; needs the same human approval, with a date, that
section 01's D1 received. Tasks 2.1–2.7 do not depend on it.

### D2 — F16 (conformance, digests, tolerance contract). **Open; blocking the section.**

`../00-foundations.md` F16 in full. The load-bearing clauses for this section
are F16.2 (the 10 % rule), F16.3 (branch-point sampling), F16.4 (bundle layout
and `physics_digest`), F16.5 (rayon is permitted outside `sailgym-physics`) and
F16.7 (modes as data).

### D3 — `brief.md` S1. **Open; the human must sign.**

S1 (conformance bundle) is not in v1 §44's deferred list and v1 §45 names the
vectorised backend directly, so `../README.md` V-A — which names the four notes
proposing agents, courses, RL and swarm — does not reach it. **This reading is
stated so it can be overruled.** If the human rules otherwise, this section
stops.

## The tolerance contract, stated once

F16.2 fixes the rule; this section fixes the numbers, in
`docs/v2/conformance.md`, generated the way `../../v1/convergence.md` and
`../../v1/performance.md` are generated — by a command in the repository, never
typed by hand.

| Tier | Tolerance |
|---|---|
| 0 | 32 ulp relative, per column. Where the reference uses the non-libm `wave` kernel (F16.6), a **measured** bound instead, computed by task 2.4 and written into the manifest |
| 1 | 10 % of the per-step `O(dt²)` term implied by `../../v1/convergence.md` |
| 2 | 10 % of the measured `dt = 0.005` error in `../../v1/convergence.md`, per scenario and per quantity. At the time of writing that table reads, in position: `close_hauled` 9.2e-5 m, `beam_reach_capsize` 7.9e-3 m, `gybe` 7.6e-6 m — **read them from the file, do not retype them** |

The manifest carries the tolerance *and its justification*, as a string, per
tier. A number without its derivation is how a tolerance gets widened three
months later by someone who does not know what it meant.

## Tasks

### 2.1 — `physics_digest`

**Owns:** `crates/sailgym-physics/src/digest.rs`,
`crates/sailgym-physics/src/lib.rs`
**P-group: S**

SHA-256, implemented in-crate. In-crate for F9.2's reason: a digest that changes
because a dependency changed its algorithm is worse than no digest.

```rust
/// Hex SHA-256 over the serialised parameter catalogue. F16.4.
pub fn physics_digest(p: &BoatParameters) -> String;
pub fn sha256_hex(bytes: &[u8]) -> String;
```

Input, in this order and no other: `serde_json::to_string(p)?`, a `\n`, then one
line per `ParamMeta` from `parameters::catalogue()` spelled `"{path}\t{tag}\t{unit}"`.
`catalogue()` returns a `Vec` in source order, serde emits struct fields in
declaration order, and `serde_json` carries `float_roundtrip`, so the input is
canonical without any sorting step. Do not sort it — a sort is a second
convention that can drift from the first.

Acceptance:

- `cargo test -p sailgym-physics digest` — the three NIST SHA-256 sample vectors
  (`""`, `"abc"`, the 448-bit message) match their published digests.
- `physics_digest` is stable across two calls and across a JSON round-trip of
  the same `BoatParameters`.
- Changing any one parameter through `set_path` changes the digest.
- `cargo test -p sailgym-physics --test determinism no_hash_iteration` still
  passes. (The implementation uses `[u32; 64]`, not a hash container.)
- `cargo test -p sailgym-physics --test provenance no_stray_constants` still
  passes. The round constants are `u32` and carry no decimal point, which that
  audit does not scan; **if this turns out to be false, stop and report it — do
  not add an `EXEMPT` entry.**

### 2.2 — The wind modes become readable data

**Owns:** `crates/sailgym-physics/src/environment/wind.rs`
**P-group: A**

Additive only. No draw order, no normalisation, no summation changes — this task
must not alter a single bit of any `sample` output, and task 2.6 proves it by
the existing goldens staying green.

- `WindMode3` gains public accessors (`k()`, `amp()`, `omega()`, `phase()`) and
  `Serialize`.
- `ProceduralWind::modes(&self) -> &[WindMode3]`.
- `wave` becomes `pub` under `#[cfg(any(test, feature = "testkit"))]`, with a
  doc comment stating F16.6: it is not `f64::cos`, it agrees with it to about
  `1e-14`, and a port must either reproduce it or be held to a measured bound.

This is F16.7, and it is the single most valuable thing in the bundle: it means
**no stack other than Rust ever implements PCG32.** Reimplementing a stratified
Fisher–Yates draw in JAX to regenerate twelve numbers you can serialise in a
kilobyte is days of work for nothing.

Acceptance: `cargo test -p sailgym-physics --test wind` and
`cargo test -p sailgym-physics --test regression` unchanged and green;
`modes().len() == mode_count()`; a `ProceduralWind` rebuilt from its serialised
modes reproduces `sample` bit-for-bit at 1 000 sampled `(x, y, t)`.

### 2.3 — The GZ curve becomes readable data

**Owns:** `crates/sailgym-physics/src/stability/hydrostatics.rs`
**P-group: A**

Additive accessors for the solved coefficients `c1, c2, c3`, so the bundle can
ship the curve **and** so `GzCurve::fit` becomes its own tier-0 case. `fit`
solves a 3×3 system by Cramer's rule with no pivoting — deliberately, for
determinism — which is exactly the kind of thing a port silently replaces with a
library solve that pivots.

Acceptance: `cargo test -p sailgym-physics stability` unchanged and green; a
curve rebuilt from its three coefficients reproduces `gz(phi)` bit-for-bit over
`phi ∈ [−π, π]` at 4 096 samples.

### 2.4 — The generator

**Owns:** `crates/sailgym-bench/src/conformance/mod.rs`,
`crates/sailgym-bench/src/conformance/npy.rs`,
`crates/sailgym-bench/src/conformance/samplers.rs`,
`crates/sailgym-bench/src/bin/gen_conformance.rs`,
`crates/sailgym-bench/Cargo.toml`, `docs/v2/conformance.md`
**P-group: B**

`cargo run --release -p sailgym-bench --bin gen_conformance`. Enables
`sailgym-physics/testkit`.

**Format: plain `.npy`, v1.0, not the note's `.npz`.** A `.npy` file is a
64-byte-aligned header plus raw little-endian data — about forty lines to write
and about thirty to read — and `np.load` opens it directly. `.npz` is a zip
container, which would mean a zip dependency in a workspace whose entire
dependency graph is `serde` and `serde_json`. Recorded as a deviation from the
note in the handoff.

The bundle, per F16.4, at `conformance/<digest>/`. Every `.npy` is 2-D,
`(n_rows, n_cols)`, `<f8`, and every column is **named in the manifest** — the
column names are the contract, and a port that reads columns by position is one
insertion away from silently comparing the wrong thing.

Tier 0 functions, each its own file. All are free functions today except where
noted:

| File | Function | Note |
|---|---|---|
| `tier0_wrap_pi` | `frames::wrap_pi` | including the in-range bit-identical fast path |
| `tier0_wave` | `environment::wind::wave` | testkit-gated; F16.6 |
| `tier0_wind_sample` | `ProceduralWind::sample` | method; modes shipped in `wind_modes.json` |
| `tier0_apparent` | `aero::apparent::{true_wind_body, apparent_wind_at, apparent_wind_cg}` | no parameters at all — the cleanest target in the repo |
| `tier0_foil` | `foil::{smoothstep, angle_of_attack, cl, cd, foil_force}` | `foil_force` takes `rho` explicitly, not `&BoatParameters` |
| `tier0_sail` | `aero::sail::sail_load` | flatten `SailOutput` to named columns by hand |
| `tier0_sheet` | `rigging::mainsheet::{boom_attach_point, rope_path_length, drope_dbeta, sheet_output}` | `l_sheet_dot` is an input, not derived |
| `tier0_gz` | `GzCurve::fit`, `gz`, `dgz`, `righting_moment` | |
| `tier0_hydro` | `hydro::{local_flow, foil_hydro_load}`, `hydro::hull::hull_loads` | |

**Do not add `serde` derives to `SailOutput`, `SheetOutput`, `FoilLoad`,
`HullLoads`, `Generalized` or `ForceBreakdown`.** The generator flattens them to
named `f64` columns by hand, because the bundle needs explicit column names
anyway and a derive would put a second, unnamed layout in the repository.

`samplers.rs` implements F16.3: one **named** sampler per branch row, each
emitting cases at, around and exactly on the boundary, plus a low-discrepancy
background sweep. The sampler's name goes in the manifest beside the row count,
so a failing row can be traced to the hazard it was written for.

Tier 1 is `dynamics::derivative` at sampled states with a `WindForces` model.
Tier 2 is 1–5 s trajectories from fixed initial conditions driven by the cue
pattern of `crates/sailgym-physics/tests/golden/script.rs`, sampled per step;
**including a dedicated sheet-transient case**, because `k_sheet = 2e4 N/m` with
`I_b = 12 kg·m²` gives `ω ≈ 41 rad/s`, about 30 steps per period at
`dt = 0.005`. F11's R1 names it the stiffest mode and the likeliest blow-up, and
it is where a lower-precision port degrades first.

Two further behaviours, both copied from `gen_golden.rs`:

- **Refuses to run on a dirty physics tree** (`git status --porcelain -- crates/sailgym-physics/src`),
  with `--allow-dirty` as a loud override, and refusing equally if git cannot be
  consulted at all — "I could not check" is not "it is clean".
- Writes the measured `wave` vs `f64::cos` gap into the manifest as tier 0's
  wind tolerance, with its derivation string. This is F16.6's measured bound,
  and section 03 consumes it rather than choosing its own.

Acceptance:

- The command writes a bundle whose total size is **≤ 2 MB**; the size is
  printed and asserted by the binary itself.
- Every `.npy` round-trips through the task's own reader bit-for-bit.
- A file written here loads under `numpy.load` — asserted in section 03, listed
  here as the contract it is written against.
- Run twice on a clean tree, the bundle is byte-identical.
- `docs/v2/conformance.md` is written by the same command (`--write`), carries
  every tier's tolerance **and its derivation**, and names the sampler behind
  every branch row.

### 2.5 — The committed bundle

**Owns:** `conformance/`
**P-group: B**

The generated artifact, committed. Goldens are committed for the same reason and
at a similar cost (`tests/golden/*.json` are ~51 KB each); the 2 MB budget in 2.4
is what keeps this defensible.

Acceptance: `conformance/<digest>/manifest.json` exists, its `digest` field
equals the directory name, and `git status --porcelain conformance/` is empty
after a regeneration on a clean tree.

### 2.6 — The Rust runner

**Owns:** `crates/sailgym-physics/tests/conformance.rs`
**P-group: S**

The section's reason for existing. This is the `provenance.rs::docs_match_source`
pattern: a generated artifact is committed, and a test asserts the committed
copy still matches what the source produces.

For every tier-0, tier-1 and tier-2 file the runner reads the committed inputs,
recomputes the outputs from current source, and compares **bit-for-bit** — not
to a tolerance. The tolerances in the manifest are for *other* stacks; Rust
against Rust on the same build is F9's territory and must be exact.

Three further assertions:

1. `manifest.digest == digest::physics_digest(&BoatParameters::ilca7())`.
2. `manifest.toolchain` is compared against `ToolchainInfo::current()`, and on a
   mismatch the runner **skips with a message naming both**, exactly as
   `tests/regression.rs:84` does and for exactly R7's reason: a red suite on a
   different compiler falsely says "the physics changed", and people learn to
   ignore it. It must never skip *silently* — the message is the deliverable.
3. An anti-vacuity guard in the style of `regression.rs:150`: row counts per
   file exceed a stated minimum, and the recomputed outputs are not all zero,
   "or a match proves nothing".

Acceptance:

- `cargo test -p sailgym-physics --test conformance` passes.
- **Proven able to fail**, and recorded in the handoff the way
  `no_shortcuts.rs`'s module doc records it for each of its audits: flip one
  sign in `foil.rs`, watch tier 0 go red naming the function; change one
  parameter default, watch the digest assertion go red; hand-edit one byte of
  one `.npy`, watch that file's comparison go red. Revert all three.

### 2.7 — The throughput question

**Owns:** `crates/sailgym-bench/src/bin/vec_bench.rs`, `docs/v2/throughput.md`
**P-group: C**

`cargo run --release -p sailgym-bench --bin vec_bench -- --write docs/v2/throughput.md`.

N independent `Simulation`s, stepped under rayon, sweeping N ∈ {1, 8, 64, 512,
4096} and thread count ∈ {1, 2, 4, …, all}. Reports steps/s, × real time, and
the scaling efficiency against the one-thread column. Generated, in the house
style of `../../v1/performance.md` — every figure produced by a command in the
repository.

`rayon` enters **`sailgym-bench` only** (F16.5). The binary asserts that the
parallel run's final states are `to_bits()`-identical to the serial run's, for
every N — stepping independent boats in parallel shares no accumulator and so
does not touch F9.6, but that is an argument, and this is the test that turns it
into a fact.

Acceptance:

- `docs/v2/throughput.md` exists, carries the host block
  (`../../v1/performance.md`'s shape), and states aggregate steps/s at the
  largest N.
- The bit-identity assertion passes at every N and every thread count.
- The handoff states, in one sentence, whether §0's throughput problem exists on
  this machine.

### 2.8 — The gate, and the no-physics-changed guard

**Owns:** `scripts/check.sh`, `scripts/check.ps1`, `CLAUDE.md`,
`docs/v1/00-foundations.md`, `docs/v2/README.md`
**P-group: S**

D1: step 4's test list gains `--test conformance`, at **both** sites in
`check.sh` (the `step_names` array and the `run_step` case) and in `check.ps1`'s
`$Steps` array, plus the table in `CLAUDE.md`, the chain in `docs/v2/README.md`,
and F12 itself with the date and the approval — the same five-site edit section
01's D1 made. `[ValidateRange(1, 9)]` does **not** change in this section; step
count is still nine.

Also: record in the handoff the output of

```
git diff --stat crates/sailgym-physics/src/forces/ \
                crates/sailgym-physics/src/dynamics.rs \
                crates/sailgym-physics/src/integrator.rs \
                crates/sailgym-physics/src/parameters.rs
```

It must be empty. The section touches the physics crate in three files and adds
one; anything else is a defect.

## Section acceptance criteria

1. `scripts/check.sh` green, all nine steps.
2. `cargo test -p sailgym-physics --test conformance` passes, and each of the
   three failure modes in 2.6 was demonstrated and reverted.
3. `cargo run --release -p sailgym-bench --bin gen_conformance` on a clean tree
   leaves `git status --porcelain conformance/` empty.
4. Bundle total size ≤ 2 MB.
5. `git diff` over `forces/`, `dynamics.rs`, `integrator.rs`, `parameters.rs` is
   empty.
6. Every existing test that was green before this section is green after, with
   the same assertions — in particular `--test regression`, `--test wind`,
   `--test provenance`, `--test determinism`.
7. `docs/v2/throughput.md` and `docs/v2/conformance.md` are both generated by a
   named command, and the handoff states the §0 verdict.
8. `docs/v2/progress/02-handoff.md` written per F13.6, recording the `.npy`
   deviation from the note, the measured `wave` gap, and every `RV` that fired.

## Risks

| # | Risk | Mitigation | Fires when |
|---|---|---|---|
| **RV7** | The bundle goes stale and nobody notices, which is the exact failure the section exists to prevent. | 2.6 is a gate-step-4 test comparing committed data against recomputed source, plus the digest assertion. | `--test conformance` is ever made non-blocking, or the digest check is loosened |
| **RV8** | An "additive" accessor in 2.2 perturbs a bit of `sample`. | `--test regression` and `--test wind` unchanged; the mode-round-trip assertion at 1 000 points. | any golden's worst `\|Δ\|` moves |
| **RV9** | The committed bundle grows without bound as tiers are added. | Hard 2 MB budget asserted by the generator itself, not by review. | the generator's own size assertion fails |
| **RV10** | `no_stray_constants` fires on the SHA-256 round constants, and someone quiets it with an `EXEMPT` entry. | 2.1 forbids that explicitly: the constants are `u32`, the audit scans floats, and if that is wrong it is escalated, not exempted. | an `EXEMPT` row is added by this section |
| **RV11** | Tier 2's tolerances get retyped from `convergence.md` and then drift from it. | 2.4 reads the file; the manifest carries the derivation string, not just the number. | a float from that table appears as a literal in `crates/` |
| **RV12** | 2.7 measures the benchmark harness rather than the physics — the co-tenant-worker mistake `../../v1/performance.md` records three sections chasing. | Host block recorded; thread count swept explicitly; serial baseline in the same process; bit-identity asserted at every point. | efficiency exceeds 1.0, or the serial column disagrees with `bench` |

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| `.npy` rather than `.npz`, so the bundle is a directory of files rather than one archive | 2.4, to avoid a zip dependency | never, unless a stack appears that cannot read `.npy` |
| No tier-3 runner; the invariants exist only as `tests/invariants.rs` | scope | section 03 for the wind slice; a later section for the rest |
| The bundle covers one parameter catalogue (`ilca7`) and one wind config per mode | scope, to hold the 2 MB budget | when a second catalogue is ever shipped |
| `vec_bench` steps bare `Simulation`s, not episodes, so it cannot see the cost of observation or agent cadence | 2.7, because the answer is wanted before `sailgym-env` is designed | section 06, which re-measures on the real runner |
