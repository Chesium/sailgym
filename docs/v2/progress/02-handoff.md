# v2 Section 02 — Handoff (the conformance bundle, its runner, and the throughput question)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F16 (and **F16.9**,
which records what this section actually implemented) and
[`docs/v2/conformance.md`](../conformance.md) before starting section 03.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. F1–F7 are untouched: no equation, no coefficient, no
frame convention and no summation order moved, `STATE_LEN` is still 13, the
F8.3 snapshot layout is unchanged, `parameters.rs` is byte identical, the F8.2
WASM surface did not grow by a single method, and F9 is unchanged.

Status: **complete, and the gate is green end to end** from the committed
tree (`39068ba feat: v2-02`, physics src tree
`750c6d6c0159e3cf3adbd0cd1d661eb6b7fc5064`). §7 has the run. Getting there
took one thing the section could not do to itself — two section-11 browser
tests cannot pass while `crates/sailgym-physics/src` has uncommitted edits,
because F18.1d makes a dirty identity comparable with nothing — and §7 records
that property, which is a standing one for every later section that touches
physics source. Nothing was weakened, no assertion was touched, and no file
outside a task's `Owns:` list was written.

---

## 1. The implementation decision this section needed, and where it is recorded

`docs/v2/brief.md` §5 requires, before a changed normative contract is
implemented: the selected S rows, the exact F deltas, the decision source and
date, and the required validation — "in the section handoff **or** this
table". This is the handoff, so here it is.

| | |
|---|---|
| Selected scope row | **S1** — "Conformance bundle and measured native throughput. Follow-on 02; generated from identified corrected source, not physical certification." |
| Normative deltas implemented | **D1** — F12′ step 4 gains `--test conformance`. **D2** — F16 in full (F16.1–F16.8), with the departures recorded in the new **F16.9**. **D3** — brief S1, recorded here. |
| Decision source | The human's dispatch of `docs/v2/prds/02-conformance-bundle.md` as the section PRD, **2026-09-21**, after 08–11 shipped. The PRD's task 2.8 instructs the five-site gate edit explicitly, which is the same shape of approval section 01's D1 received and recorded in `scripts/check.sh` as "Added 2026-09-20 by human approval". |
| Prerequisites met | 08 (corrected model and `ModelIdentity`, F18.1) and 10 (canonical identity, F18.3) are shipped; this section reuses both rather than inventing a second vocabulary. |
| Validation performed | §4 (measured values), §5 (the three demonstrated failure modes), §6 (the no-physics-changed guard), §7 (the gate). |

**One thing to confirm, not to assume.** The PRD marks D1 "Open; blocking task
2.8" and asks for "the same human approval, **with a date**, that section 01's
D1 received". What that approval consists of here is the dispatch itself. The
edit is made and dated, at all five sites the PRD names; if the intent was a
separate sign-off, this is the paragraph to strike and the edit to revert. The
same applies to D2/D3: F16 and brief S1 were "proposed", and dispatching the
section that implements them is what this handoff treats as resolving them.

---

## 2. What landed, file by file

### Task 2.1 — the bundle key (P-group S, section agent)

- **`crates/sailgym-physics/src/digest.rs`** — new. `BundleIdentity`
  (the canonical record), `FixtureId`, `WindFixtureId`, `sha256_hex`,
  `BUNDLE_SCHEMA_VERSION`, `TOLERANCE_CONTRACT_VERSION`. Eight tests,
  including the FIPS 180-4 / NIST CAVP known-answer vectors and the RV10 case
  (an equation change with identical parameters).
- **`crates/sailgym-physics/src/lib.rs`** — `pub mod digest`, gated
  `#[cfg(any(test, feature = "testkit"))]` like `testkit` itself.
- **`crates/sailgym-physics/Cargo.toml`**, **`Cargo.lock`** — `sha2 = { version = "0.10", optional = true, default-features = false }`,
  enabled only by `testkit`. §3.1 records the dependency decision.

### Task 2.2 — the wind modes become readable data (P-group A, section agent)

- **`crates/sailgym-physics/src/environment/wind.rs`** — additive only.
  `WindMode3` gains `Serialize` and `k()`, `amp()`, `omega()`, `phase()`, plus
  a `new` constructor; `ProceduralWind::modes()` and
  `ProceduralWind::from_modes()`; `wave` is `pub` under
  `#[cfg(any(test, feature = "testkit"))]`, with F16.6 stated at its doc
  comment. Two new tests: `modes_are_the_whole_of_the_randomness` (a field
  rebuilt from its **JSON** round trip reproduces `sample` bit for bit at
  1 000 points, for all three F6.1 modes) and
  `the_accessors_expose_the_stored_table` (which also re-checks that the
  stored amplitude is perpendicular to the wavenumber — the property that
  makes the field divergence-free).

### Task 2.3 — the GZ curve becomes readable data (P-group A, section agent)

- **`crates/sailgym-physics/src/stability/hydrostatics.rs`** — additive only.
  `GzRepresentation { version, form, coefficients }`, `GZ_FORM`,
  `GZ_REPRESENTATION_VERSION`, `GzCurve::representation()` and
  `GzCurve::from_representation()`. **The coefficient count is data**: nothing
  outside the module assumes four, and a record with the wrong count is
  refused rather than padded. Two new tests, one of which compares `gz`,
  `dgz`, `gz_integral` and `righting_moment` **bit for bit** at 20 400 heel
  angles covering `[−π, π]`, both peaks, both vanishing angles, inversion and
  four turns beyond the supported domain.

### Task 2.4 — the generator (P-group B, section agent)

- **`crates/sailgym-physics/src/testkit/npy.rs`** — new. A bounded `.npy` v1.0
  codec for `f64` matrices: every dtype, ordering, rank, length and size
  rejection happens **before** a buffer is allocated. Seven tests, including
  signed zero and subnormals surviving the round trip, and an absurd shape
  being refused rather than allocated for.
- **`crates/sailgym-physics/src/testkit.rs`** — `pub mod npy;` and the note on
  why the codec lives here and not in the generator.
- **`crates/sailgym-bench/src/conformance/mod.rs`** — new, and the single
  definition of what a bundle is: the eleven fixtures, the tier-2 trajectory
  cases, the tolerance derivation, the manifest, and `build`. It is
  `#[path]`-included by both `gen_conformance.rs` and
  `crates/sailgym-physics/tests/conformance.rs`, which is the arrangement
  `tests/golden/script.rs` already has with `gen_golden.rs`.
- **`crates/sailgym-bench/src/conformance/samplers.rs`** — new. One **named**
  sampler per F16.3 hazard, each emitting cases at, around and **exactly on**
  its boundary, plus a Halton background sweep. `nextafter`, not epsilon: a
  strict `>` and a `>=` differ on exactly one value.
- **`crates/sailgym-bench/src/bin/gen_conformance.rs`** — new.
- **`crates/sailgym-bench/Cargo.toml`** — `features = ["testkit"]` on the
  physics dependency, and `rayon` for task 2.7.
- **`docs/v2/conformance.md`** — new, generated.

### Task 2.5 — the committed bundle (P-group C, section agent)

- **`conformance/2323a34073ebdbd6adfe7e1fbfa22d41ff9874fd92df1fbdcb6f6e9ec9884a63/`**
  — 14 files, 1 522 949 bytes, **76.1 %** of the 2 MB budget.

### Task 2.6 — the Rust runner (P-group S, section agent)

- **`crates/sailgym-physics/tests/conformance.rs`** — new, 7 tests. §5 records
  the three demonstrated failure modes.

### Task 2.7 — the throughput question (P-group C, section agent)

- **`crates/sailgym-bench/src/bin/vec_bench.rs`** — new.
- **`docs/v2/throughput.md`** — new, generated.

### Task 2.8 — the gate and the guards (P-group S, section agent)

- **`scripts/check.sh`** (both sites), **`scripts/check.ps1`** (`$Steps`;
  `[ValidateRange(1, 9)]` unchanged), **`CLAUDE.md`** (the step-4 row, and a
  pointer to the bundle manifest), **`docs/v2/00-foundations.md`** (F12′, the
  delta table, and the new **F16.9**), **`docs/v2/README.md`** (the gate
  sentence, V-E, the delivery table, the status paragraph).
- **`docs/v2/progress/02-handoff.md`** — this file.

---

## 3. The four decisions worth arguing with

### 3.1 `sha2`, optional, behind `testkit`

F16.4 permits a compact key and requires "an established SHA-256
implementation, not handwritten cryptography". A bundle directory needs a
name, so the key is useful and `sha2` is the dependency.

It is **optional and enabled only by `testkit`**, so `cargo build` and
`wasm-pack build crates/sailgym-wasm` keep exactly the dependency graph they
had — `serde` and `serde_json`. The browser bundle is unchanged. `sha2` pulls
`digest`, `block-buffer`, `crypto-common`, `generic-array`, `typenum`,
`cpufeatures`, `libc` and `version_check`, all of them RustCrypto or
build-time, and all of them only into `cargo test` and `sailgym-bench`.
`digest::tests::sha256_known_answers` pins the FIPS 180-4 and NIST CAVP
vectors including the million-`a` long-message case, so a version bump that
changed the hash would fail loudly rather than silently rename every bundle.

### 3.2 The key covers the *contract*, and excludes `model.source`

This is the one place the implementation reads F16.4 rather than following it
literally, and F16.9 point 3 records it in the foundations.

F16.4 asks the key to cover "the model/source identity from 08". A source tree
id changes on **every** commit that touches `crates/sailgym-physics/src`,
including the commit that adds the bundle — so a directory keyed on it is
stale the instant it is committed, which is the chicken-and-egg `gen_golden`
already lives with (`08-handoff.md` §8.6). Worse, it would rename the
directory for a comment change.

So the key is SHA-256 over: the bundle schema, generator and
tolerance-contract versions, the declared `model_version`, the resolved F7
catalogue, the integrator and `dt`, every wind field's mode table, and **every
fixture's column names and `data_key`** — the digest of the numbers the
fixture actually holds. A changed equation changes the numbers, which changes
the digest, which changes the key; a source edit that moves no sampled bit
moves no key, which is the honest answer rather than a miss.
`digest::tests::an_equation_change_with_identical_parameters_invalidates_the_bundle`
measures the first half and
`a_different_clean_source_tree_leaves_the_key_alone` the second, and §5's
first demonstration measures it end to end on the real bundle.

The full `ModelIdentity`, `state` included, is still in the canonical record,
still printed by the runner on every run, and
`BundleIdentity::is_release_baseline()` is `false` for a dirty or unknown
source — so such a bundle can be generated and inspected but never presented
as a baseline. Both halves of F16.4's "both are required" travel.

### 3.3 The build profile is recorded but is not a toolchain mismatch

`tests/regression.rs` compares the whole `ToolchainInfo`, profile included,
and its goldens are generated by a debug `cargo run`. The conformance bundle
is generated by `cargo run --release` (the PRD's command; the tier-2 reference
study runs every case three times) and the runner executes under `cargo test`,
which is debug. Comparing the profile would make step 4's conformance target
**skip on every single run**, which is RV7 with extra steps.

So the runner compares `rustc` and `target` strictly — a mismatch skips with a
message naming both, exactly as `regression.rs:84` does and for R7's reason —
and prints, rather than acts on, a profile difference. The argument that this
is safe is that Rust enables no fast-math at any optimisation level and never
reassociates floating point. It was **measured**, not asserted: the same
bundle was generated in both profiles and every fixture's payload digest is
identical.

```
tier0_wrap_pi      37685b3ae6af4219a30200791cf668954354130d49acb89d1759c0808328cc87
tier0_wave         c2345064c44d95dff8900d4e344cb937fab16eb45512966d6604c0ac084319ca
…  (all eleven identical between `debug` and `release`; the whole key was
   403952e7…a48d in both, before the `eval_t` column rename of §3.4)
```

### 3.4 Column names are a key, and one of them was not

Found while reading the generated manifest: `tier1_derivative` had **two**
input columns called `t` — `BoatState::t` and `derivative`'s time argument,
which are equal in a running simulation and sampled independently in a pure
function fixture. The second is now `eval_t`, and both the generator and the
runner refuse a fixture whose column names are not unique. "The column names
are the contract" is the PRD's phrase; a contract with a duplicate key is not
one. This is why the committed key is `2323a340…` and not `403952e7…`.

### 3.5 The manifest's key is spelled `digest`

PRD 2.5's acceptance names the field: "`conformance/<digest>/manifest.json`
exists, its **`digest`** field equals the directory name", and F16.4's layout
sketch says `digest` too. The field is therefore `digest`.

The Rust method that computes it is still `BundleIdentity::key()`, and that
asymmetry is deliberate rather than sloppy: renaming the method would have
edited `crates/sailgym-physics/src`, which would have made the tree dirty,
which would have forced the bundle back through a `--allow-dirty` generation
and undone the release baseline of §7 — for a name. The manifest field's doc
comment says which method produces it, so there is no guessing.

---

## 4. What the bundle is, and every measured number in it

```
conformance/2323a34073ebdbd6adfe7e1fbfa22d41ff9874fd92df1fbdcb6f6e9ec9884a63/
```

| | |
|---|---|
| `digest` (the directory name, and `manifest.digest`) | `2323a34073ebdbd6adfe7e1fbfa22d41ff9874fd92df1fbdcb6f6e9ec9884a63` |
| Files | 11 × `.npy`, `parameters.json`, `wind_modes.json`, `manifest.json` |
| Size | **1 522 949 bytes**, 76.1 % of the 2 MB budget, asserted by the generator |
| Rows | 10 255 across the eleven fixtures; **98 401** recorded output values |
| Tolerance entries | 80 tier-0, 13 tier-1, 91 tier-2 |
| Wind fields shipped as data | 9 (three synthetic, one per F6.1 mode, and one per shipped scenario) |
| Model | `model v2`, physics src tree `750c6d6c0159e3cf3adbd0cd1d661eb6b7fc5064`, **clean** — a release baseline (`BundleIdentity::is_release_baseline()` is true, `declared_changes` is empty) |
| Toolchain | `rustc 1.98.1 (48a229cea 2026-09-01) / x86_64-unknown-linux-gnu / release` |

### 4.1 The measured `wave` gap — F16.6 was pessimistic by two decades

F16.6 says the hand-rolled kernel agrees with `f64::cos` "to about `1e-14`".
Measured over `θ ∈ [−10⁵, 10⁵]` at 700 000 points:

| | measured | F16.6's figure |
|---|---|---|
| max \|`wave(θ)` − `θ.cos()`\| | **4.440892098500626e-16** (at θ = −29.2053…) | ≈ 1e-14 |
| max \|fixed pairwise sum − Kahan sum\| | **8.881784197001252e-16 m/s** (1.78e-16 of base speed) | not previously measured |

4.44e-16 is two ULP of a result bounded by 1. The clause's number was a bound
and remains a safe one; **section 03 should consume the manifest's
measurement, not the clause**, which is what F16.6 asks for in the first
place. The Kahan comparison is the other arm F16.6 requires: it is what a port
that reassociates the mode sum is held to.

### 4.2 The tolerance contract, and what "derived" means here

Every number is measured on the generating build. **Nothing is read from
`docs/v1/convergence.md`**, and no float from it appears in `crates/`.

- **Tier 0 and 1**: relative bound `32 ULP = 7.105427357601002e-15`
  (F16.2's proposal for well-scaled values, recorded as a proposal), with an
  absolute bound of that budget against **each column's own measured scale**,
  because a relative bound means nothing near cancellation. Tier 1 is derived
  in each component's own units — `d_u` in m/s², `d_psi` in rad/s — never
  borrowed from a state-error tolerance, which would be dimensionally invalid.
  A column that is *structurally* zero over the whole domain (`f_z` of a sail
  load, F6.3 having dropped the spanwise flow) carries a **zero** bound, and
  the runner checks that such a column really is all zeros rather than merely
  unsampled.
- **Tier 2**: a **fresh** dt / dt2 / dt4 study on section 08's model, per case
  and per state quantity, compared at the same simulated times. The bound is
  10 % of the measured `|x(dt) − x(dt/4)|`, floored at
  `8 ULP/step × steps × DBL_EPSILON × max(scale, 1)`. Examples from the
  `sheet_transient` case:

| quantity | `\|x(dt) − x(dt/4)\|` | tolerance |
|---|---|---|
| `x` (m) | 5.3785e-2 | 5.3785e-3 |
| `psi` (rad) | 4.7204e-3 | 4.7204e-4 |
| `beta` (rad) | 3.0890e-2 | 3.0890e-3 |

  **No order is asserted anywhere**, and the `dt/2` column is printed so a
  refinement that failed to shrink would be visible — not so a slope can be
  read off it. F18.1b's tension law is discontinuous at take-up and RK2 has no
  order across an event (`08-handoff.md` §3); claiming one here would be
  claiming something the model does not have.

### 4.3 The fixtures, and the F16.3 samplers behind them

| File | Tier | Rows | In | Out | Samplers |
|---|---|---|---|---|---|
| `tier0_wrap_pi` | 0 | 336 | 1 | 1 | `wrap_pi_edge`, `halton_background` |
| `tier0_wave` | 0 | 1905 | 1 | 1 | `halton_background` |
| `tier0_wind_sample` | 0 | 540 | 4 | 2 | `halton_background` |
| `tier0_apparent` | 0 | 412 | 18 | 8 | `halton_background`, `phi_unwrapped` |
| `tier0_foil` | 0 | 996 | 18 | 6 | `stall_blend_edge`, `zero_flow_eps`, `halton_background` |
| `tier0_sail` | 0 | 417 | 15 | 17 | `halton_background`, `phi_unwrapped` |
| `tier0_sheet` | 0 | 850 | 14 | 21 | `sheet_slack_boundary`, `halton_background` |
| `tier0_gz` | 0 | 514 | 7 | 9 | `phi_unwrapped`, `halton_background` |
| `tier0_hydro` | 0 | 269 | 27 | 15 | `halton_background`, `phi_unwrapped`, `zero_flow_eps` |
| `tier1_derivative` | 1 | 609 | 18 | 13 | `limit_rate_boundary`, `rudder_self_centre_zero`, `sheet_release_precedence`, `sheet_slack_boundary`, `halton_background` |
| `tier2_trajectories` | 2 | 3407 | 2 | 13 | `golden_script_cues`, `sheet_transient` |

The runner asserts that **every** named F16.3 sampler appears somewhere, so a
bundle that kept its row counts and lost its branch-point rows — which would
agree with itself perfectly and miss exactly the places a port differs — fails.

`tier2_trajectories` is the six shipped scenarios at 2.0 s plus a dedicated
`sheet_transient` at 5.0 s, every physics step sampled. 2.0 s is a **budget**
decision and is recorded as one; the cues are `tests/golden/script.rs`'s
pattern with the steering cue moved inside the horizon, because a 2 s fixture
in which nothing is commanded tests only the initial transient.

### 4.4 `numpy.load` opens every file

The PRD lists this as the contract the format is written against and defers
the assertion to section 03. It was checked here anyway:

```
tier0_apparent.npy      (412, 26) float64 C
tier0_foil.npy          (996, 24) float64 C
…
tier2_trajectories.npy (3407, 15) float64 C
total elements 174938
```

---

## 5. Proven able to fail — the three demonstrations, and their revert

Task 2.6 requires this, and `no_shortcuts.rs`'s module doc is the house form.
Each defect was injected on a **copy** of the file (`cp` to a scratch
directory, edit, measure, `cp` back), never by editing and blindly reverting,
and `git diff --stat` was empty after each.

| # | Injected | What went red, verbatim |
|---|---|---|
| 1 | `foil.rs`: `l_hat = Vec2::new(f_hat.y, -f_hat.x)` → `(-f_hat.y, f_hat.x)` — the F5.3 lift-direction sign, R3's "single most likely defect" | `every_fixture_matches_current_source`: *tier0_foil: row 0, column `fx`: this build produces 31.483345123851972 where the committed bundle records 6.265020218184649* |
| 2 | `parameters.rs`: `sail.section.area` 7.06 → 7.07 | `the_manifest_identity_is_the_current_contract`: *the committed bundle does not describe this build's contract: ["parameters", "fixtures[tier0_foil]", "fixtures[tier0_sail]", "fixtures[tier1_derivative]", "fixtures[tier2_trajectories]"]* |
| 3 | one byte of `tier0_gz.npy` XOR 0x01 (byte 65 911, the low byte of the last `gz_integral`) | `every_fixture_matches_current_source`: *tier0_gz: row 513, column `gz_integral`: this build produces 0 where the committed bundle records 7.29e-309* |

Demonstration 2 is the interesting one: **a single parameter change moved four
fixture digests as well as the `parameters` field**, which is the RV10
property from the other side — the data digests are not a weaker substitute
for a parameter comparison, they are a second, independent detector.

All three were reverted; `cargo test -p sailgym-physics --test conformance`
passes 7/7 afterwards.

---

## 6. No physics changed

Task 2.8's guard, run on the finished section:

```
$ git diff --stat crates/sailgym-physics/src/forces/ \
                  crates/sailgym-physics/src/dynamics.rs \
                  crates/sailgym-physics/src/integrator.rs \
                  crates/sailgym-physics/src/parameters.rs
(empty)
```

The whole physics-source diff, for the record — four files, all additive:

```
 crates/sailgym-physics/src/environment/wind.rs     | 199 +++++++++++++++++++-
 crates/sailgym-physics/src/lib.rs                  |   7 +
 .../sailgym-physics/src/stability/hydrostatics.rs  | 208 +++++++++++++++++++++
 crates/sailgym-physics/src/testkit.rs              |  12 ++
 4 files changed, 425 insertions(+), 1 deletion(-)
```

The single deletion is `fn wave` becoming `pub fn wave` plus a private
`wave_kernel` holding the same arithmetic, so that the visibility can be
`cfg`-gated without a second copy of the polynomial.

**RV8 did not fire.** `--test regression` reports a worst `|Δ|` of **exactly
`0.0`** on all six goldens, and `--test wind`, `--test provenance`,
`--test determinism`, `--test symmetry`, `--test convergence`,
`--test invariants` and `--test no_shortcuts` are all green with the same
assertions they had. No `Owns:` list was exceeded: every file written is in a
task's list, and `Cargo.lock` is named in 2.1's.

---

## 7. The gate, and the dirty-tree property every later section inherits

### The gate, from the committed tree

`scripts/check.sh` from the committed tree, after the regeneration described
below, nothing else running: **all nine steps green, exit 0, 751 s.**

| step | | time |
|---|---|---|
| 1 | `cargo fmt --check` | 1 s |
| 2 | `cargo clippy --all-targets -- -D warnings` | 0 s |
| 3 | `cargo test -p sailgym-physics -p sailgym-task` | 63 s |
| 4 | `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance` | 42 s |
| 5 | `--test regression` | 0 s |
| 6 | `wasm-pack build` | 12 s |
| 7 | `pnpm --dir web typecheck` | 1 s |
| 8 | `pnpm --dir web test:unit` | 1 s — 20 files, 208 tests |
| 9 | `pnpm --dir web test:e2e` | 630 s — **349 passed, 0 failed** |
| | **all steps passed** | **751 s** |

`--test conformance` is 7/7 inside step 4. Rust test counts in step 4:
7 conformance, 4 convergence, 27 invariants, 5 no_shortcuts, 6 provenance,
1 symmetry.

The rest of this section records how the section got there, because the
property it ran into is a standing one for every later section that touches
physics source.

### What a *dirty* tree measures, and why

`scripts/check.sh` on the working tree **before** the commit:

| step | | result |
|---|---|---|
| 1 | `cargo fmt --check` | ok |
| 2 | `cargo clippy --all-targets -- -D warnings` | ok |
| 3 | `cargo test -p sailgym-physics -p sailgym-task` | ok, 64 s |
| 4 | `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance` | ok, 42 s (7 + 4 + 27 + 5 + 6 + 1 tests) |
| 5 | `--test regression` | ok |
| 6 | `wasm-pack build` | ok, 13 s |
| 7 | `pnpm --dir web typecheck` | ok |
| 8 | `pnpm --dir web test:unit` | ok, 20 files, 208 tests |
| 9 | `pnpm --dir web test:e2e` | **343 passed, 6 failed** |

The six are **two** section-11 tests in each of the three desktop browsers:

```
practice.spec.ts:353 › retry restores the exact conditions › the same initial snapshot, and the same canonical identity
practice.spec.ts:425 › comparison refuses what it cannot compare › two attempts at the same challenge compare, and show measured deltas
```

both failing identically:

```
locator resolved to <div data-count="2" data-comparable="false" data-verdict="indeterminate" …>
Expected: "true"
```

### The cause, which is the F18.1d contract working exactly as written

`crates/sailgym-physics/build.rs` runs `git status --porcelain -- src` and
marks the compiled identity **`dirty`** whenever the physics source has
uncommitted edits. `ModelIdentity::is_comparable_with` is `false` for a dirty
identity **including against itself** (F18.1d, and
`identity::tests::dirty_and_unknown_are_never_comparable` asserts it), so
`ExperimentIdentity::compare` returns `Indeterminate(["model"])` and the
panel — correctly — refuses to call two attempts the same experiment.

So those two tests are red for **any** uncommitted edit under
`crates/sailgym-physics/src`, in any section. It is not a defect and it is not
a flake: the same six failed in two independent full runs, one of them with
nothing else on the machine.

### Measured, not argued

The same source was committed in a throwaway `git worktree`, the WASM was
built there, and that build was run against the suite:

```
git worktree add --detach <tmp> HEAD
rsync the working tree in; git add -A; git commit      # in the worktree only
scripts/build-wasm.sh                                   # -> clean identity
# then, against that WASM:
pnpm --dir web test:e2e practice.spec.ts  → 36 passed (1.3m), all 3 browsers
pnpm --dir web test:e2e                   → 349 passed, 0 failed, exit 0
```

`git status --porcelain -- crates/sailgym-physics/src` was empty in the
worktree and `git rev-parse HEAD:crates/sailgym-physics/src` read
`750c6d6c0159e3cf3adbd0cd1d661eb6b7fc5064`. **With this exact source
committed, the whole of step 9 is green** — not just the two tests, the
suite: 349 passed against the 343 + 6 of the dirty run, which is the same 349
tests with the six red ones green. The worktree was removed, the working
tree's own WASM was rebuilt with `scripts/check.sh 6`, and **nothing in the
repository was committed**.

### The regeneration after the commit, and what it proved

The bundle's manifest records the identity of the build that generated it, so
the copy written before the commit said `state: "dirty"`. Regenerating once
from the committed tree fixed that — and, because `build.rs` keys its re-run
on source **mtimes**, which a commit does not change, the compiled identity
had to be refreshed first:

```
touch crates/sailgym-physics/src/lib.rs
cargo run --release -p sailgym-bench --bin gen_conformance -- --write docs/v2/conformance.md
```

No `--allow-dirty` this time; the generator ran on the clean-tree path and
printed `identity model v2 (physics src tree 750c6d6c…)` with no warning.
**This is the prediction §3.2 made, and it held exactly:**

| | |
|---|---|
| `digest` | `2323a340…a63` — **unchanged** |
| Directory | unchanged |
| The eleven `.npy` files | **byte identical**, all of them |
| `parameters.json`, `wind_modes.json` | byte identical |
| `manifest.json` | three lines: `tree` `55f3a73f…` → `750c6d6c…`, `state` `dirty` → `clean`, `declared_changes` → `""` (plus, in a later pass, the field rename of §3.5) |

That is the whole diff, and it is the point of excluding `model.source` from
the key: a commit that changed no sampled number did not rename the bundle.
`08-handoff.md` §8.6 describes the same one-time marker replacement for the
goldens.

**Section 08 hit the dirty-tree property too** and resolved it the same way —
its §11 says the gate was run "from a clean working tree of this section's
work". This handoff states it explicitly because it is a standing property of
the repository that no document recorded, and F16.9 point 8 now records it in
the foundations.

---

## 8. The throughput verdict — `cross-stack.md` §0, answered

**There is no throughput problem on this machine.** 4 096 independent boats
step at **5.251 M steps/s on 24 threads — 26 253× real time in aggregate**,
against 3 017× for a single environment; a port undertaken for throughput
alone would be buying a factor nobody has shown they need.

The full sweep is [`docs/v2/throughput.md`](../throughput.md), generated by
`cargo run --release -p sailgym-bench --bin vec_bench -- --write docs/v2/throughput.md`.
Highlights:

| N | 1 thread | 24 threads | efficiency | serial baseline |
|---|---|---|---|---|
| 1 | 0.601 M/s | 0.605 M/s | 0.04 | 0.603 M/s |
| 8 | 0.601 M/s | 3.430 M/s | 0.24 | 0.599 M/s |
| 64 | 0.596 M/s | 4.837 M/s | 0.34 | 0.596 M/s |
| 512 | 0.573 M/s | 5.116 M/s | 0.37 | 0.564 M/s |
| 4096 | 0.578 M/s | **5.251 M/s** | 0.38 | 0.575 M/s |

**F16.5's argument is now a fact.** At every N and every thread count the
parallel run's final states are `to_bits()`-identical to the serial run's; the
binary asserts it before it prints a figure, and every point passed. `rayon`
entered `sailgym-bench` only.

**RV12 did not fire.** The serial baseline runs in the same process and is
within 0.5 % of the one-thread rayon pool at every N, so the harness is not
what is being measured; no efficiency exceeds 1.0; the host block is recorded;
and the scenario is `gybe`, the *slowest* of the six in
`docs/v1/performance.md` and the only one with a `Gust` field — which is what
makes the per-boat seeds produce genuinely different trajectories and the
bit-identity assertion a test rather than a symmetry.

Scaling flattens at 38 % on 24 logical cores, which is what 12 physical cores
with SMT gives a memory-light, branch-heavy inner loop. Nothing here measures
observation building, agent cadence or the Python boundary; section 06
re-measures on the real runner.

---

## 9. Section acceptance criteria, one by one

| # | Criterion | Verdict |
|---|---|---|
| 1 | `scripts/check.sh` green, all nine steps | **Pass** — all nine, exit 0, 751 s, step 9 at 349 passed / 0 failed, from the committed tree. Before the commit it was steps 1–8 green and step 9 at 343/6, all six being the two section-11 comparison tests that F18.1d makes impossible to pass with a dirty identity; §7 records that property, which is standing. Nothing was weakened. |
| 2 | `--test conformance` passes, and the three failure modes were demonstrated and reverted | **Pass.** 7/7; §5 has all three, verbatim. |
| 3 | Regeneration on a clean tree leaves `git status --porcelain conformance/` empty | **Pass.** Measured on the clean tree: `diff -r` between two consecutive regenerations reports no difference, so the second run leaves `conformance/` untouched. §7 records the one-time post-commit manifest update that preceded it — and the proof that the digest, the directory name and every `.npy` survived it byte for byte. |
| 4 | Bundle total ≤ 2 MB | **Pass.** 1 522 949 bytes, 76.1 %, asserted by the generator itself rather than by review. |
| 5 | `git diff` over `forces/`, `dynamics.rs`, `integrator.rs`, `parameters.rs` empty | **Pass.** §6. |
| 6 | Every previously green test still green, same assertions — `regression`, `wind`, `provenance`, `determinism` | **Pass.** Worst regression `\|Δ\|` exactly `0.0`; no assertion touched. |
| 7 | `throughput.md` and `conformance.md` both generated by a named command; the handoff states the §0 verdict | **Pass.** §8. |
| 8 | This handoff, per F13.6, recording the `.npy` deviation, the measured `wave` gap and every RV that fired | **Pass.** §3, §4.1, §10. |

---

## 10. Risks that fired

**RV7 — the bundle goes stale and nobody notices. Did not fire, and the guard
is in the gate.** `--test conformance` is step 4 and compares committed data
against recomputed source bit for bit. §5's three demonstrations are the proof
that it can go red.

**RV8 — an "additive" accessor in 2.2 perturbs a bit of `sample`. Did not
fire, measured rather than assumed.** `--test regression` reports a worst
`|Δ|` of exactly `0.0` on all six goldens, `--test wind` is unchanged and
green, and `modes_are_the_whole_of_the_randomness` reproduces `sample` bit for
bit from a JSON round trip at 1 000 points in all three F6.1 modes.

**RV9 — the committed bundle grows without bound. Did not fire, and the budget
is asserted by the generator.** 76.1 % used. It was at **97.2 %** on the first
generation; the manifest was restructured so that F16.2's derivation is stated
**once per tier** rather than repeated per column (which is what F16.2 asks
for anyway), `tier0_sheet`'s slack sweep dropped a redundant rate-and-rate
cross product, and the tier-2 scenario horizon went from 2.5 s to 2.0 s. No
fixture lost a branch-point row.

**RV10 — the artifact key ignores equation changes. Did not fire, and was
tested from both sides.** §3.2 and §5's demonstrations 1 and 2.

**RV11 — tier-2 tolerances get retyped from `convergence.md` and drift.** The
PRD's stated mitigation is "2.4 reads the file". **This section did the
stronger thing and did not read it**: the same PRD's tolerance section says
"Fresh 1–5 s dt/dt2/dt4 study on section 08's model" and "Historical v1
convergence values are context only, not the corrected model's acceptance
values", and `docs/v1/convergence.md` describes a study of a model that
section 08 superseded. Every tier-2 number is measured by the generator on the
build that writes the bundle, and the manifest carries the derivation. The
risk's trigger — "a float from that table appears as a literal in `crates/`" —
is not armed, because no such float exists there. Flagged as a deliberate
deviation rather than claimed as satisfied.

**RV12 — 2.7 measures the harness rather than the physics. Did not fire.** §8.

**R1 (v1 F11) — mainsheet stiffness vs. timestep. Did not fire, and is now a
fixture.** `tier2_trajectories`'s `sheet_transient` case drives the rope
through take-up under load twice in five seconds; its measured reference
discretization error is the largest in the bundle (5.4 cm in `x` over 5 s
between `dt` and `dt/4`), which is the number a lower-precision port will
degrade against first. `k_sheet` and `c_sheet` are untouched.

**R7 (v1 F11) — generated artifacts are build-sensitive. Fired, as designed.**
The bundle records its toolchain and the runner skips with a message naming
both on a `rustc` or target mismatch. §3.3 records the one departure.

---

## 11. What section 03 must know

1. **The bundle is the artifact, `manifest.json` is its contract, and the
   column names are the API.** Read columns by name. `input_columns` and
   `output_columns` are in the manifest per fixture, they are unique within a
   fixture, and both the generator and the runner refuse a duplicate.

2. **Do not implement PCG32.** `wind_modes.json` carries `κ_k`, `ϕ_k`, `ω_k`
   for all nine fields; `ProceduralWind::from_modes` is the Rust side of the
   same contract and reproduces `sample` bit for bit. F16.7 is not a
   suggestion.

3. **F16.6's two arms are both in the manifest, measured.** `4.44e-16` for the
   kernel against `f64::cos` and `8.88e-16 m/s` for the fixed-order reduction
   against a compensated one. Consume those, not the "about 1e-14" in the
   clause — and report the divergence between a port's two arms as a **number**,
   never as a pass or a fail.

4. **No tier 3, and agreement is scoped to what the samplers visited.** The
   brief §35 invariants are Rust-only, in `tests/invariants.rs`. Each
   fixture's sampled domain is stated per tier in the manifest; a port that
   agrees on `tier0_foil` has said nothing about `α` outside
   `[−π, π]` at speeds outside `[0.05, 12] m/s`.

5. **`--test conformance` must stay in step 4.** A section that rewrites the
   chain for the Python steps 10–11 and drops it takes the whole freshness
   check out of the gate silently — the same warning F12′ already carries
   about `-p sailgym-task` in step 3.

6. **Regenerate deliberately, and commit what you regenerate.** Any physics
   change that moves a sampled number turns step 4 red until the bundle is
   regenerated. That is the point. `--allow-dirty` requires
   `--declare "<text>"`, the declaration goes into the manifest, and the
   runner refuses a non-baseline bundle that declares nothing.

7. **f32 is a different environment until measured** (F16.8). If the JAX arm
   runs in f32, measure the f32↔f64 divergence on this same bundle and report
   it as a number. If it exceeds the cross-stack tolerance, the escalation is
   **not** to raise `c_sheet` or lower `k_sheet` — brief §43 forbids it and
   R1's mitigation order is explicit. Sub-step the rigging DOF in the port, or
   run that DOF in f64.

8. **The repository root `README.md` still lies about the gate, twice.** Its
   nine-step table spells step 3 as `cargo test -p sailgym-physics` (section
   11's change) and step 4 without `--test conformance` (this section's). No
   task in either section owns that file (F13.2). The two one-line changes:

   | line | should read |
   |---|---|
   | 194 | `| 3 | \`cargo test -p sailgym-physics -p sailgym-task\` | The physics core and the practice evaluator are correct **and build on the host with no WASM toolchain**. |` |
   | 195 | `| 4 | \`--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance\` | … and the committed conformance bundle still describes the compiled physics. |` |

---

## 12. Deviations from the PRD, all of them

1. **`.npy`, not `.npz`** — the PRD's own instruction; recorded here as the
   deviation from `discussions/cross-stack.md` that it asks for. A zip
   dependency in a workspace whose graph is `serde` and `serde_json` buys
   nothing a directory does not already give.
2. **The key excludes `model.source`** — §3.2, and F16.9 point 3.
3. **A profile difference does not skip the runner** — §3.3.
4. **RV11's mitigation was superseded by the PRD's own tolerance section** —
   §10.
5. **`tier1_derivative`'s time column is `eval_t`** — §3.4. Not a deviation so
   much as a defect found and fixed inside the section, but it changed the
   committed key and so is recorded.
6. **`tier2_trajectories` is 2.0 s per shipped scenario, not the full 5** —
   a budget decision (RV9), stated at the constant and in the generated
   document. The `sheet_transient` case, which is the one R1 names, gets the
   full 5 s.
7. **`conformance/mod.rs` is `#[path]`-included by a physics test.** The PRD
   forbids making "physics tests depend on the bench crate"; a textual include
   is not a crate dependency (that would be a cycle), and it is the same
   arrangement `tests/regression.rs` already has with `tests/golden/script.rs`.
   The alternative was a second definition of every fixture, which is the
   thing the runner exists to prevent.
8. **No task was delegated.** Group A holds two tasks with disjoint `Owns:`
   lists and group C two more; all four were small enough that the section
   agent executed them, as in section 08 §6.6.
9. **`pwsh scripts/check.ps1` was not run** — no Windows host and no `pwsh`
   here, as in sections 01, 08 and 11. `check.ps1` was edited by this section
   (task 2.8 owns it), so the edit is **unverified**: it is the same one-entry
   addition made to `check.sh`, in the same two places, and
   `[ValidateRange(1, 9)]` is deliberately unchanged.

---

## 13. Commands

```
cargo run --release -p sailgym-bench --bin gen_conformance -- --write docs/v2/conformance.md
cargo run --release -p sailgym-bench --bin vec_bench       -- --write docs/v2/throughput.md
cargo test -p sailgym-physics --test conformance -- --nocapture
scripts/check.sh
scripts/check.sh 4
```

The generator refuses a dirty physics tree; this section's own runs used
`--allow-dirty --declare "…"` because the source, the bundle and the runner
land in one commit, which is the situation that override exists for.
