# v2 Section 03 — Handoff (Python, and `wind.sample` in JAX)

**Written per F13.6.** Read this, `docs/v2/00-foundations.md` F12′, **F16.6**
and the new **F17.6**, and [`docs/v2/conformance.md`](../conformance.md)
before starting any later port.

`docs/v1/00-foundations.md` remains normative and **nothing in this section
redefines any of it**. Nothing in `docs/v2/00-foundations.md` F16 was
redefined either: the tolerances this section is held to were measured by
section 02 and live in a manifest this section does not own and never edited.

**No Rust changed.** `git diff --name-only crates/` is empty, which is
acceptance criterion 3. Twice the port would have been easier with a change
on the Rust side; §7 records both, what was done instead, and the one that is
left as a gap rather than closed.

Status: **complete**. `scripts/check.sh` is green end to end at eleven steps.
The two demonstrations the PRD requires were performed, observed red, and
reverted. The two-arm divergence is a number, in §4 here and in
`conformance.md`.

---

## 1. The normative decisions this section needed, and where they are recorded

`docs/v2/brief.md` §5 requires, before a changed normative contract is
implemented: the selected S rows, the exact F deltas, the decision source and
date, and the required validation — "in the section handoff **or** this
table". This is the handoff.

| | |
|---|---|
| Selected scope row | **S2** — "JAX wind verification pilot. Follow-on 03; no full dynamics/integrator port or training claim." |
| Normative deltas implemented | **D1** — F12′ gains steps 10 and 11. **D2** — F17.1, the Python boundary, with the departures recorded in the new **F17.6**. **D3** — F16.6's two arms, consuming section 02's measurements rather than the clause's figure. **D4** — brief S2, recorded here. |
| Decision source | The human's dispatch of `docs/v2/prds/03-jax-wind.md` as the section PRD, **2026-09-21**, after 02 shipped. The PRD's task 3.7 instructs the five-site gate edit explicitly, which is the same shape of approval section 01's D1 and section 02's D1 received and recorded. |
| Prerequisites met | 02 is shipped: the bundle, its manifest, `wind_modes.json`, and F16.6's two measured numbers all exist and are consumed, not re-derived. |
| Validation performed | §3 (measured agreement), §4 (the two-arm divergence), §5 (the three demonstrated failure modes), §6 (the gate and RV17), §7 (no Rust changed). |

**One thing to confirm, not to assume.** The PRD marks D1 "Open; blocking task
3.7", D2 "Open; blocking the section", D3 "Open; blocking task 3.3" and D4
"Open". `docs/v2/README.md` said in as many words that **03 is not
"dispatchable now" while its own normative deltas remain open**. What resolved
them here is the dispatch itself, exactly as section 02's handoff §1 treated
its own. The edits are made and dated at all five gate sites and in F17.6; if
the intent was a separate sign-off, this is the paragraph to strike and the
`docs/v2/00-foundations.md` and `docs/v2/README.md` edits to revert — the
Python side stands on its own and would simply not be in the gate.

---

## 2. What landed, file by file

Every file written is in a task's `Owns:` list. No task was delegated: group A
holds two tasks and group B three, and the coupling between the loader's
column contract and the port's transcription made splitting them more
expensive than executing them, as in sections 02 §12.8 and 08 §6.6.

### Task 3.1 — the Python toolchain (P-group S)

- **`pyproject.toml`** — one `uv` workspace at the repository root.
  `requires-python = "==3.12.*"`, an exact minor version rather than a floor:
  a floor lets the interpreter move under a recorded measurement, and the
  divergence in §4 is a property of a numerical stack of which the
  interpreter is part. Dependencies `jax`, `numpy`, `pytest`; dev `ruff`.
  Two packages in one project via `hatchling`, because F17.1 audits two
  scopes and they are two scopes.
- **`uv.lock`** — **committed**, 23 066 bytes. Same reason as
  `web/pnpm-lock.yaml` and `Cargo.lock`, and the same reason F9.2 hand-rolls
  PCG32: a toolchain that can drift under you is not a reference.
- **`.python-version`** — `3.12`.
- **`.gitignore`** — `.venv/`, `__pycache__/`, `.pytest_cache/`,
  `.ruff_cache/`.
- **`python/sailgym_conformance/__init__.py`**,
  **`python/sailgym_jax/__init__.py`** — the two scopes, each stating which
  half of F17.1 governs it.
- **`python/tests/conftest.py`** — `jax_enable_x64` enabled at import and
  **asserted**; the `EXPECTED` bundle identity; the `bundle` and `repo_root`
  fixtures.

### Task 3.2 — the bundle loader (P-group A)

- **`python/sailgym_conformance/bundle.py`** — `Bundle.load(root, expect=…)`,
  `Table`, `Tolerance`, `WindKernelBounds`, `WindModes`, `ExpectedIdentity`,
  `BundleError`. Holds no equation and no parameter value.
- **`python/tests/test_bundle.py`** — 15 tests, including the x64 assertion
  task 3.1's acceptance names.

### Task 3.3 — the port (P-group A)

- **`python/sailgym_jax/wind.py`** — `wave_exact`, `wave_cos`, `sample`,
  `sample_grid`, `base_from_bearing`, and fourteen named constants each
  citing the `wind.rs` or `environment/mod.rs` line it was transcribed from.
  Ships no test of its own, on purpose.

### Task 3.4 — tier 0 (P-group B)

- **`python/tests/test_wind_tier0.py`** — 6 tests.
- **`docs/v2/conformance.md`** — one appended section, between
  `<!-- BEGIN section-03 two-arm divergence -->` markers. §8.2 records why
  this is an append and what that costs.

### Task 3.5 — the wind slice of tier 3 (P-group B)

- **`python/tests/test_wind_tier3.py`** — 5 tests, each naming the brief §35
  invariant or the `tests/wind.rs` test it mirrors.

### Task 3.6 — the Python constants audit (P-group B)

- **`python/tests/test_no_stray_constants.py`** — 3 tests: the literal scan,
  the citation pass, and the absence of the mode-table generator.

### Task 3.7 — the gate (P-group S)

- **`scripts/py-test.sh`**, **`scripts/py-test.ps1`** — new, mirroring
  `build-wasm.{sh,ps1}`.
- **`scripts/check.sh`** (`step_names` **and** the `run_step` case),
  **`scripts/check.ps1`** (`$Steps` **and** `[ValidateRange(1, 9)]` →
  `(1, 11)`), **`CLAUDE.md`** (eleven rows, "Eleven steps", and `python/` in
  the layout), **`docs/v2/00-foundations.md`** (F12′, the delta table, and
  the new F17.6), **`docs/v2/README.md`** (the gate paragraph, V-E, the
  delivery table, the status paragraph).
- **`docs/v2/progress/03-handoff.md`** — this file.

---

## 3. What was measured, and what it means

### 3.1 The transcribed arm reproduces Rust exactly, on every sampled row

Not "within tolerance" — **bit for bit**, which nobody promised and nobody may
rely on.

| Fixture / column | rows | max \|err\| | max ULP | bit-identical | manifest bound |
|---|---|---|---|---|---|
| `tier0_wave` / `wave` | 1905 | **0.0** | 0 | 1905 / 1905 | 7.105427357601002e-15 |
| `tier0_wind_sample` / `wx` | 540 | **0.0** | 0 | 540 / 540 | 4.4827529395215666e-14 |
| `tier0_wind_sample` / `wy` | 540 | **0.0** | 0 | 540 / 540 | 6.394884621840902e-14 |

This is a **measurement on one build of one backend, not a contract.** F16.1
forbids claiming cross-stack equality and this section claims none: the
assertions in `test_wind_tier0.py` are the manifest's tolerances, and they
are what a later JAX, XLA or CPU-target change is held to. That the margin is
currently infinite is worth knowing — it says XLA did not reassociate the
Estrin tree, did not fold the `+ROUND_MAGIC −ROUND_MAGIC` rounding trick away,
and did not contract the reduction into an FMA — and it is worth knowing that
none of those is guaranteed.

The rounding trick was the one real worry. `(θ·INV_TAU + ROUND_MAGIC) −
ROUND_MAGIC` is algebraically `θ·INV_TAU`, and a compiler permitted to
reassociate would delete it and leave the Cody–Waite reduction with an
unrounded `n` — which is not a subtle error, it is a wrong answer by whole
radians. It survived.

### 3.2 The host-cosine arm is within its measured bound

| Fixture / column | bound used | source | max \|err\| |
|---|---|---|---|
| `tier0_wave` / `wave` | 4.440892098500626e-16 | `kernel_vs_libm_absolute`, verbatim | 3.330669e-16 |
| `tier0_wind_sample` / `wx` | `summation_vs_compensated_absolute` + Σ\|amp\|·`kernel_vs_libm_absolute` | composed, §3.3 | 8.881784e-16 |
| `tier0_wind_sample` / `wy` | as above | composed, §3.3 | 8.881784e-16 |

### 3.3 One bound is composed, and the composition is stated rather than tuned

The manifest gives two numbers and the derivation string assigns them: a port
calling the host cosine is held to `kernel_vs_libm_absolute`, and a port that
reassociates the sum to `summation_vs_compensated_absolute`. `wave_cos` on
`tier0_wind_sample` does **both**, so it is held to the sum of the two
effects:

```
bound = summation_vs_compensated_absolute
      + (Σ_k |amp_k|) · kernel_vs_libm_absolute
```

The second term is the kernel gap propagated through the stored amplitudes:
mode `k` contributes `amp_k · cos(θ_k)`, so a per-kernel gap of `g` moves the
field by at most `Σ|amp_k|·g`. Both inputs come from `manifest.json`. **No
tolerance in the manifest was changed, and none could have been** — this
section does not own that file, which is RV13's designed mitigation working.

Applying `summation_vs_compensated_absolute` alone would also have passed, at
exactly `8.881784197001252e-16 ≤ 8.881784197001252e-16`: equality, with no
margin, on one backend. That is a bound that would break on the next XLA
release and teach nobody anything, which is why the composition is written
out instead.

### 3.4 Tier 3 — the four invariants a wind-only port can satisfy

All four hold for **both arms**, over all nine fields.

| Invariant | Mirrors | Result |
|---|---|---|
| Finite over a wide domain | `tests/wind.rs::finite_over_wide_domain`, brief §35 | no NaN, no Inf, over 294 corner points + 5 000 interior points per field per arm |
| Bounded magnitude | `tests/wind.rs::bounded_magnitude` | worst \|w\|/bound = **0.931** (`scenario_gybe`); every field inside |
| Grid matches point | `tests/wind.rs::grid_bitwise_matches_point_sweep` | within the tier-0 bound, **not** asserted as equality — see below |
| Uniform is exactly `base` | `tests/wind.rs::uniform_is_constant` | exact, sign of zero included |

Worst-case magnitudes, all nine fields, both arms:

```
synthetic_uniform                5.000000 of 7.250000      scenario_free_sail               5.000000 of  7.250000
synthetic_spatial                6.703020 of 7.250000      scenario_gybe                    6.333782 of  6.800000
synthetic_gust                   6.561662 of 7.250000      scenario_sheet_release_recovery  9.000000 of 13.050000
scenario_beam_reach_capsize      9.000000 of 13.050000     scenario_tack                    4.000000 of  5.800000
scenario_close_hauled            3.500000 of 5.075000
```

**"Grid matches point" is a tolerance here and an equality in Rust, and the
test's doc string says so at length** so that nobody later "fixes" it. F6.1
requires bit identity between `sample` and `sample_grid` because the
visualization and the physics must be the same field (brief §19, §47); across
stacks F16.1 forbids claiming it, because XLA may fuse a vectorised grid
evaluation differently from a point evaluation.

---

## 4. The two-arm divergence — F16.6's reported number

**Reported, not asserted.** No threshold is applied anywhere; the test
asserts only that the numbers are finite and that `conformance.md` records
them for this bundle.

| Fixture / column | max \|Δ\| | rows | near-zero rows | max relative | max ULP | identical |
|---|---|---|---|---|---|---|
| `tier0_wave` / `wave` | 3.330669e-16 | 1905 | 18 (\|v\| ≤ 7.105e-15) | 1.244400e-04 | 616 290 671 953 | 813 |
| `tier0_wind_sample` / `wx` | 8.881784e-16 | 540 | 80 (\|v\| ≤ 4.483e-14) | 1.286321e-13 | 1 024 | 434 |
| `tier0_wind_sample` / `wy` | 8.881784e-16 | 540 | 40 (\|v\| ≤ 6.395e-14) | 2.555074e-14 | 192 | 334 |

ULP distributions are in `conformance.md` and in the test's own output.

**Read the absolute column; the other two are cancellation.** The relative and
ULP figures are computed over the rows whose larger value exceeds the
manifest's own absolute bound for that column, and even so `tier0_wave`'s
relative figure is `1.2e-4` and its ULP figure is `6.2e11`. Both come from
rows where `cos θ` is itself of order `1e-12`: an absolute difference of
`3.3e-16` there is one and a half ULP of the kernel's own output range and a
relative difference of order `1e-4`. The ULP count is worse still, because an
ULP distance counts representable steps and there are `6e11` of them between
two numbers that close to zero. F16.2 says this in advance — "neither is meaningful near
cancellation, which is what the absolute bound is for" — and the honest
summary is the one-line version: **substituting the host cosine costs at most
3.3e-16 on the kernel and 8.9e-16 m/s on the field, while changing the last
bit of 57 % of the kernel's rows, 20 % of `wx` and 38 % of `wy`.**

That 8.881784e-16 m/s is the quantum this comparison can resolve: it is half
an ULP of a wind speed near 8 m/s. Against a base of 5–9 m/s it is a relative
`1.8e-16`. **The measured cost of the convenient choice, on this function, is
one bit.**

What that buys, and what it does not: `wave_cos` would have caught every sign
error, unit slip and stratification mistake in this port — §5's demonstration
puts the error at `2.59` m/s, fifteen orders above the gap. What it could not
have caught is a port wrong in the sixteenth digit, and "the port agrees to
1e-13" would then have been an untestable claim. That is the whole argument
for shipping two arms, and it is now a number rather than an argument.

---

## 5. Proven able to fail — the three demonstrations, and their revert

Each defect was injected, measured, and reverted from a copy taken beforehand;
`diff` against the copy was empty afterwards and the suite green.

### 5.1 The sign flip (task 3.4)

`sailgym_jax/wind.py`: `amp_y = jnp.asarray(modes.amp_y)` →
`amp_y = -jnp.asarray(modes.amp_y)`. One component of `amp`, negated — R3's
defect class, and the smallest version of it.

**Both arms went red, and named the hazard rather than a row index:**

```
tier0_wind_sample/wy/wave_exact: 420 of 540 rows exceed the recorded bound.
  hazards hit: ['even_mode_count']
  worst row 258 (sampler 'even_mode_count'): port 1.296039583845534,
    recorded -1.2960395838455323, |err| 2.592079e+00 > bound 6.394885e-14

tier0_wind_sample/wy/wave_cos: 420 of 540 rows exceed the recorded bound.
  hazards hit: ['even_mode_count']
  worst row 258 (sampler 'even_mode_count'): port 1.2960395838455343,
    recorded -1.2960395838455323, |err| 2.592079e+00 > bound 1.818404e-15
```

**Observed magnitude: 2.592079 m/s**, against bounds of `6.39e-14` and
`1.82e-15` — 13 and 15 orders of margin. `test_the_odd_mode_tail_is_not_covered_by_this_bundle`
went red as well. 3 failed, 26 passed; after the revert, 29 passed.

Worth noting: the 120 rows that did *not* fail are the uniform fields, which
have no modes and therefore no `amp` to negate. A defect in the perturbation
is invisible on the uniform path — which is the argument for `zero_modes`
being a named sampler rather than rows nobody counted.

### 5.2 The stray constant (task 3.6)

`RHO_AIR_PASTED = 1.225` pasted into `sailgym_jax/wind.py` — `RHO_AIR`, the
most physical number in the project.

```
AssertionError: physical or contract literals loose in Python — move each one
into the bundle, give it a module-level named constant with a doc string
citing its Rust source line, or argue for it in EXEMPT:
  python/sailgym_jax/wind.py:207: 1.225   |RHO_AIR_PASTED = 1.225

AssertionError: named constants that introduce a number and carry no doc string:
  python/sailgym_jax/wind.py: RHO_AIR_PASTED
```

**Two of the three audit passes went red, at the file and the line.** 2
failed, 1 passed; after the revert, 3 passed. The second failure is the
interesting one: naming the constant would not have saved it, because the
citation pass is what F17.1's "a named constant is not automatically valid
merely because it is named" asks for.

### 5.3 The gate (task 3.7)

A deliberately failing test appended to `test_wind_tier3.py`:

```
$ scripts/check.sh 11
FAILED python/tests/test_wind_tier3.py::test_deliberately_broken_for_the_gate_demonstration
1 failed, 29 passed in 3.79s
[11/11] FAILED (scripts/py-test.sh)
$ echo $?
1
```

`[11/11] FAILED` and a non-zero exit, which is the acceptance criterion
verbatim. Reverted.

---

## 6. The gate, and the five sites

### 6.1 The chain

`scripts/check.sh` from the working tree, nothing else running:
**all eleven steps green, exit 0.**

| step | | time |
|---|---|---|
| 1 | `cargo fmt --check` | 0 s |
| 2 | `cargo clippy --all-targets -- -D warnings` | 0 s |
| 3 | `cargo test -p sailgym-physics -p sailgym-task` | 64 s |
| 4 | `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance` | 43 s |
| 5 | `--test regression` | 0 s |
| 6 | `wasm-pack build` | 9 s |
| 7 | `pnpm --dir web typecheck` | 1 s |
| 8 | `pnpm --dir web test:unit` | 1 s — 20 files, 208 tests |
| 9 | `pnpm --dir web test:e2e` | 674 s — **349 passed, 0 failed** |
| 10 | `uv run ruff check python && uv run ruff format --check python` | 0 s — 9 files |
| 11 | `scripts/py-test.sh` | 4 s — **29 passed** |
| | **all steps passed** | **796 s** |

Step 11's 29 tests: 15 loader (`test_bundle.py`), 6 tier 0
(`test_wind_tier0.py`), 5 tier 3 (`test_wind_tier3.py`), 3 audit
(`test_no_stray_constants.py`). Steps 1–9 are unchanged from section 02's run
and are green with the same assertions; nothing was weakened anywhere.

The section-02 dirty-tree property (F16.9.8) did not bite: this section
touched no file under `crates/sailgym-physics/src`, so the compiled
`ModelIdentity` stayed clean and the two section-11 comparison tests passed
in all three browsers as part of the 349.

### 6.2 RV17 did not fire, and was measured rather than argued

The risk is that steps 10 and 11 land at some of the five sites and not the
others, so the gate lies about its own length. Both scripts were parsed and
their step-name lists diffed:

```
check.sh  total: 11
check.ps1 total: 11
RV17: both scripts have 11 steps with identical names
```

The five sites, all moved: `check.sh`'s `step_names` array; `check.sh`'s
`run_step` case; `check.ps1`'s `$Steps`; `check.ps1`'s `[ValidateRange(1, 9)]`
→ `(1, 11)`; `CLAUDE.md`'s table. F12′ itself and `docs/v2/README.md`'s chain
moved with them.

### 6.3 `check.ps1` was verified, unlike in sections 01, 02, 08 and 11

Those four recorded their `check.ps1` edits as **unverified** — no Windows
host and no `pwsh`. This section did not leave it there: PowerShell 7.4.6 was
unpacked into the session's scratch directory, used, and discarded. **Nothing
was installed on the machine and nothing outside the repository changed.**

```
$ pwsh scripts/check.ps1 -Step 11
[11/11] scripts/py-test.sh
[py-test] uv run --frozen pytest python
29 passed in 3.74s
[11/11] ok in 4s
check: all steps passed in 4s          exit 0

$ pwsh scripts/check.ps1 -Step 10      → [10/11] ok in 0s, exit 0

$ pwsh scripts/check.ps1 -Step 12
check.ps1: Cannot validate argument on parameter 'Step'. The 12 argument is
greater than the maximum allowed range of 11.
```

The last one is the point of moving `[ValidateRange]`: before the edit,
`-Step 10` produced that same parameter error, which is a confusing way to
discover a typo. The whole eleven-step `check.ps1` chain was **not** run —
only steps 10, 11 and the range check — because the Rust and browser steps
are the same commands `check.sh` already ran.

### 6.4 `--fast` still covers steps 1–9 only

The PRD says the subset stays as it is, so it does. Measured:

```
$ scripts/check.sh --fast
[1/11] ok in 1s   [4/11] ok in 42s   [7/11] ok in 1s
[2/11] ok in 0s   [5/11] ok in 0s    [8/11] ok in 1s
[3/11] ok in 64s  [6/11] ok in 9s    [9/11] ok in 131s
check: steps 1-9 passed (--fast subset; 10 and 11 not run) in 249s     exit 0
```

`check.ps1 -Fast` selects the same range. Selecting a step explicitly always
runs it, `--fast` or not. Tracked as a debt in §9.2 — and a cheap one: step
11 is 4 s against the subset's 249 s, so the case for the exclusion is
consistency with the PRD rather than time.

---

## 7. No Rust changed

```
$ git diff --name-only crates/
(empty)
$ git status --porcelain crates/
(empty)
```

Acceptance criterion 3, and the reason the section-02 dirty-tree property
(F16.9.8) never came near firing: the compiled `ModelIdentity` stayed clean
throughout, so the two section-11 browser tests that cannot pass against a
dirty identity passed normally.

Two things the port wanted from Rust and did not take:

1. **A bit-exact `base` vector.** `wind_from_bearing` calls `f64::sin`/`cos`;
   the port calls `jnp.sin`/`cos`. Rather than export the computed base,
   `sample` takes `base` as an argument and a caller may supply a bit-exact
   one; `test_the_uniform_base_agrees_with_the_recorded_field` checks the
   Python derivation against the recorded uniform samples to the tier-0
   bound, and it passes. In the event the derived base was good enough for
   every row of `tier0_wind_sample` to come out bit-identical anyway.
2. **A wind field with an odd `K`.** See §8.1.

---

## 8. Limitations, stated rather than papered over

### 8.1 The odd-`K` tail is transcribed but unverified

`sample_inner` has a tail loop for an odd mode count
(`wind.rs:516-519`) whose contribution lands in accumulator slots 0 and 1
rather than in a third pair. **Every field in the committed bundle has `K` of
0 or 12**, so no recorded row exercises that path and the port's version of it
is checked against nothing.

`test_the_odd_mode_tail_is_not_covered_by_this_bundle` records the gap as an
assertion rather than a comment: it fails if a field with an odd `K` ever
appears, at which point the test should become a real comparison. What it can
check without a reference is that the tail is not silently dropped —
truncating a twelve-mode field to eleven moves the answer by the eleventh
mode's contribution, to the tier-0 bound. Not to equality, and the reason is
itself the reason the tail must be transcribed: the tail lands *inside* slot 0
rather than being added to the finished total, so the two associations differ
in the last bit.

**What a later section should do:** add one odd-`K` field to
`Context::new` in `crates/sailgym-bench/src/conformance/mod.rs` and
regenerate. It is a few lines, it costs a few kilobytes of the 24 % of the
2 MB budget the bundle still has spare, and it renames the bundle — which is
why it belongs to a section that owns `crates/`, and not to this one.

### 8.2 `docs/v2/conformance.md` is appended to, and a Rust regeneration drops it

The file says "Do not edit by hand" and means it: `gen_conformance --write`
rewrites it whole. Task 3.4 owns it and section acceptance 5 requires the
divergence to be in it, and this section may not touch
`crates/sailgym-bench`, so the only honest route was an append between
explicit markers.

The mitigation is that the append is **also produced by a named command** —
`SAILGYM_WRITE_CONFORMANCE=1 scripts/py-test.sh` — and that
`test_wind_tier0.py` fails if the marked section is missing or names a
different bundle. So a Rust regeneration that drops it turns step 11 red with
a message saying which command restores it, rather than losing the numbers
quietly. The numbers themselves are deliberately **not** asserted against the
document: they are a property of the JAX build, and F16.6 makes them a report.

**The better fix belongs to whoever next owns `gen_conformance`:** have it
preserve marked foreign sections, or give the Python side its own generated
document.

### 8.3 The citation pass checks the file, not the line's content

F17.1 asks kernel constants to cite the Rust source, and
`test_every_named_constant_cites_its_source_line` requires a
`<file>.rs:<line>` in each doc string and asserts the cited file exists. It
does **not** check that the line still contains what the citation claims, so
the fourteen line numbers in `wind.py` will drift the first time `wind.rs` is
edited above them. Checking the content would couple the Python audit to Rust
source text; citing only the file would make the citation unfalsifiable. The
middle was chosen deliberately and is recorded here so the drift is expected
rather than discovered.

### 8.4 Agreement is scoped to what the samplers visited

`tier0_wind_sample` is 540 rows over nine fields at `x, y ∈ [−2000, 2000] m`
and `t ∈ [0, 600] s`; `tier0_wave` is 1 905 rows over `θ ∈ [−10⁵, 10⁵]`. The
port has said nothing about anything else, and in particular nothing about
`sample` under `jax.jit`, under `vmap` over fields, on a GPU or TPU backend,
or in f32. F16.8 stands: **f32 is a different environment until measured**,
and it was not measured here because nothing in this section runs in f32 —
`conftest.py` asserts that.

### 8.5 One host quirk, handled in the script

The development machine exports a large `PYTHONPATH` from unrelated
toolchains, which put foreign `site-packages` ahead of the locked environment
and made `pytest` autoload third-party plugins; the first bare run failed
inside someone else's import with `ModuleNotFoundError: No module named
'yaml'`. That is RV18's failure mode wearing a different hat — a gate failure
naming a traceback that has nothing to do with the gate. `py-test.{sh,ps1}`
therefore unset `PYTHONPATH`/`PYTHONHOME` and set `PYTHONNOUSERSITE=1` before
running, which is the same discipline as checking for `uv` first. A bare
`uv run pytest python` still works on a machine without that quirk; on this
one it needs `env -u PYTHONPATH`.

---

## 9. Risks and debts

### 9.1 Risks, one by one

**None of RV13–RV18 fired.**

| # | Risk | Outcome |
|---|---|---|
| **RV13** | `wave_exact` written as a paraphrase, tolerance then widened | **Did not fire**, and could not have: the tolerance lives in `manifest.json`, which this section does not own; `git diff conformance/` is empty. The transcription is structural and the evidence is §3.1 — a paraphrase would not have come out bit-identical. |
| **RV14** | f32 creeps in through a dtype promotion | **Did not fire.** `conftest.py` enables and asserts x64 at import; the loader refuses a fixture whose dtype is not `float64`; `test_x64_is_enabled_for_every_test` checks a literal, an array and a computed quotient. |
| **RV15** | Someone reimplements PCG32 to "check the modes" | **Did not fire.** No draw was reproduced. The fourth audit pass scans `python/sailgym_jax/` for the four words and passes over 2 files; the words appear nowhere in the package, not even in a comment saying they must not. |
| **RV16** | The Python audit is vacuous | **Did not fire.** `files > 3` and `scanned > 12`, currently 4 and 19, plus `cited >= 12` (currently 14) and `files >= 2` on the fourth pass — and §5.2 proves it can go red. |
| **RV17** | Steps 10–11 land at some sites and not others | **Did not fire**, and was measured rather than argued: §6.2. |
| **RV18** | `uv` missing, and step 11 fails like a test failure | **Did not fire.** `py-test.sh` checks for `uv` first and prints the install command. A near relative of it *did* occur and is §8.5. |

**R7 (v1 F11) — generated artifacts are build-sensitive. Fired, as designed,
in a new place.** The two-arm divergence is a property of the JAX and XLA
build that produced it; the appended section of `conformance.md` therefore
names its bundle and its numbers are reported, not asserted. The figures in §4
were measured with `jax 0.11.2` / `jaxlib 0.11.2` on the CPU backend under
CPython 3.12.3, and that is the only environment they describe.

### 9.2 Debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| `--fast` does not run steps 10 and 11 | 3.7, because the PRD says the subset stays as it is | whenever the Python suite gets slow enough that skipping it matters — the opposite of today: 4 s against the subset's 249 s. The repayment is one number (`fast_total` / `$FastTotal`) in two scripts |
| Only tier 0 and the wind slice of tier 3 are consumed; tiers 1 and 2 sit unused | scope — one function is the point | the section that ports the full step |
| `wave_exact` preserves source structure; its fusion and performance are unmeasured | by design, F16.6 | never; it is a conformance witness, not a backend |
| No Warp runner, though F16.4 says one runner per stack | scope | the Warp section |
| No CI runs any of this | inherited from v1; there is still no `.github/` | whenever CI is set up. It is now two calls to two scripts |
| The odd-`K` tail is unverified | the bundle ships no odd-`K` field | §8.1 — one field added to `conformance/mod.rs` |
| `conformance.md`'s appended section is dropped by a Rust regeneration | §8.2 | whoever next owns `gen_conformance` |

---

## 10. Section acceptance criteria, one by one

| # | Criterion | Verdict |
|---|---|---|
| 1 | `scripts/check.sh` green, all eleven steps | **Pass.** Exit 0, 796 s, §6.1. Step 9 at 349 passed / 0 failed; steps 10 and 11 new and green. |
| 2 | `uv sync --frozen` reproduces the environment from a clean checkout | **Pass.** Measured: `pyproject.toml`, `uv.lock`, `.python-version` and `python/` copied to an empty directory with no `.venv`; `uv sync --frozen` installed 13 packages and `import sailgym_jax, sailgym_conformance, jax` succeeded. |
| 3 | `git diff --name-only crates/` is empty | **Pass.** §7 — and `git status --porcelain crates/` is empty too, so nothing was added either. |
| 4 | The sign-flip (3.4) and stray-constant (3.6) demonstrations performed, observed red, reverted, magnitudes recorded | **Pass.** §5.1 (2.592079 m/s against bounds of 6.39e-14 and 1.82e-15, both arms red, sampler named) and §5.2 (two audit passes red at `wind.py:207`). Both reverted and `diff` clean; the gate demonstration of §5.3 was performed as well, which the criterion does not require. |
| 5 | The two-arm divergence recorded as max absolute, max relative and the ULP distribution, in the handoff **and** in `docs/v2/conformance.md` | **Pass.** §4 here; the appended section of `conformance.md` between the `section-03 two-arm divergence` markers. §8.2 records that a Rust regeneration of that file drops the section and that step 11 goes red when it does. |
| 6 | No file under `python/sailgym_jax/` contains a seed, an RNG or the string `pcg`; asserted by 3.6's audit as a fourth pass | **Pass.** `test_the_verification_implementation_has_no_generator_of_the_mode_table`, scanning 2 files for four words. |
| 7 | `docs/v2/progress/03-handoff.md` written per F13.6, recording which `RV` fired, the measured numbers and the `--fast` debt | **Pass.** This file: §9.1 (no RV fired, each one accounted for), §3–§4 (the numbers), §9.2 (the debts, `--fast` first). |

**Task-level acceptance, for completeness:**

| Task | Criterion | Verdict |
|---|---|---|
| 3.1 | `uv sync --frozen` from clean; x64 asserted in `test_bundle.py`; `ruff check` and `ruff format --check` clean | **Pass**, all four. |
| 3.2 | `test_bundle.py` passes; `numpy.load` opens every `.npy`; a corrupted manifest digest raises; an unknown column raises with the available names | **Pass.** 15 tests. The `numpy.load` test opens all 11 fixtures and checks dtype, rank and non-emptiness; two further tests cover a payload edited without the manifest, and a contract field that moved while the digest agreed. |
| 3.3 | Covered by 3.4 and 3.5; ships no test of its own | **Pass** — `wind.py` contains no test. |
| 3.4 | `test_wind_tier0.py` passes; the sign-flip demonstration recorded with its magnitude | **Pass.** 6 tests; §5.1. |
| 3.5 | `test_wind_tier3.py` passes; each doc string names its brief §35 invariant or its `tests/wind.rs` test | **Pass.** 5 tests, each naming one; §3.4 lists the mapping. |
| 3.6 | Passes; proven able to fail by pasting `1.225`, with the offending file and line | **Pass.** §5.2. |
| 3.7 | Eleven steps green; `check.sh 10` and `check.sh 11` each run exactly one step; `pwsh check.ps1 -Step 11` green; a broken Python test makes `check.sh` exit non-zero **and** print `[11/11] FAILED` | **Pass**, all four. §6.1, §6.3, §5.3. |

---

## 11. What the next section must know

1. **The bundle loader is stack-neutral and belongs to no stack.**
   `sailgym_conformance` is where a Warp or Torch runner reads the same
   bundle. It holds no equation, and F17.1's audit is what keeps it that way.
   Do not fork it.

2. **`Bundle.load` has no default expectation, deliberately.** A runner
   states the bundle it was written against; a newer bundle is a refusal, not
   an upgrade (F16.4.2). `python/tests/conftest.py` is where this suite's
   expectation lives, and it lives there rather than in the loader because
   F17.1 forbids wrapper scope from defining `dt`.

3. **Consume the manifest's measurements, never the clause's prose.** F16.6
   says the kernel agrees with `f64::cos` "to about 1e-14"; the measured
   figure is `4.44e-16`, two decades tighter. Section 02 said this and it is
   worth repeating: the clause is a bound, the manifest is the measurement.

4. **Two arms is the pattern, and the divergence is a number.** A later port
   of `derivative` or the integrator ships the same two arms for every
   library function it substitutes, and reports the gap. Not a pass, not a
   fail (F16.6).

5. **The transcription rules are what made §3.1 possible.** Structure, not
   algebra: the Estrin grouping, the magic-number rounding, the three-way
   reduction and the two-slot accumulation were copied rather than
   re-derived. A regrouping that is mathematically identical is a different
   floating-point number, and the whole point of a conformance witness is to
   isolate a kernel difference from a modelling difference.

6. **Gate steps 10 and 11 must stay, and `-p sailgym-task` and
   `--test conformance` must stay with them.** A section that rewrites the
   chain for a new step and drops one of the three takes a whole subsystem
   out of the gate silently. F12′ now says this about all three.

7. **`scripts/py-test.sh` is where `maturin develop` goes.** That is the
   entire reason step 11 is a script (F12′). Section 07 adds the line inside
   the script and does not amend F12 again.

8. **The repository root `README.md` still lies about the gate, now in four
   places.** No task in sections 11, 02 or 03 owns that file (F13.2). It
   needs, at `README.md:188` and in the table below it:

   ```
   188  Eleven steps, each of which proves something specific:

   194  | 3 | `cargo test -p sailgym-physics -p sailgym-task` | The physics core and
        the practice evaluator are correct **and build on the host with no WASM
        toolchain**. |

   195  | 4 | `--test invariants --test no_shortcuts --test convergence --test symmetry
        --test provenance --test conformance` | … and the committed conformance bundle
        still describes the compiled physics. |

   200+ | 10 | `uv run ruff check python && uv run ruff format --check python` | The
        Python is canonically formatted and lint-clean. |
        | 11 | `scripts/py-test.sh` | The Python suite: the conformance-bundle loader,
        the JAX wind port's two arms and the F17.1 constants audit. |
   ```

   (Lines 194 and 195 are the two section 11 and section 02 already reported;
   the other two are this section's.)

   Its "about 9½ minutes" figure also predates steps 10 and 11, which add
   about 5 s between them.

---

## 12. Deviations from the PRD, all of them

1. **`bundle.tolerance` is `(fixture, quantity)`, not `("tier0", "wind")`.**
   The PRD's sketch shows `bundle.tolerance("tier0", "wind")`. The manifest
   keys tolerances by fixture **and** column, and the fixture names already
   carry their tier, so the two-argument shape is preserved with the real
   keys: `bundle.tolerance("tier0_wave", "wave")`. Addressing by tier alone
   would have meant one bound for `wx` and `wy`, which have different
   measured scales.

2. **`Bundle.load` takes `expect=` rather than discovering the only
   directory.** The PRD's sketch is `Bundle.load(root)`; its own requirement
   1 is that the loader "takes that expected identity explicitly and raises
   rather than warning". The requirement won.

3. **`sample` takes a duck-typed mode table, not
   `sailgym_conformance.WindModes` by import.** `sailgym_jax` imports nothing
   from `sailgym_conformance`: a verification implementation that shares a
   data class with the loader it is checked through has shared one decision
   more than it should. `WindModes` satisfies the protocol structurally.

4. **`base_from_bearing` lives in `sailgym_jax/wind.py`.** The PRD's
   signature list does not mention it, but `sample` needs a `base` and F6.1's
   conversion is an equation — so it belongs in the scope F17.1 permits
   equations in, not in the loader.

5. **The `wave_cos` bound on `tier0_wind_sample` is composed from two
   manifest numbers** rather than being one of them. §3.3 states the
   composition and why the single number alone would have passed with zero
   margin.

6. **`docs/v2/conformance.md` is appended to rather than regenerated.** §8.2.

7. **The two-arm divergence report splits near-zero rows out of the relative
   and ULP figures**, using the manifest's own absolute bound as the
   boundary. Reporting one relative number over rows that include a sign
   crossing would have reported the cancellation, not the arms. §4.

8. **`py-test.{sh,ps1}` sanitise the import environment.** Not in the PRD;
   §8.5 records the host quirk that made it necessary and why it is the same
   discipline as `build-wasm.sh`'s `wasm-pack` check.

9. **No task was delegated.** Groups A and B were small and tightly coupled
   through the column contract, as in sections 02 §12.8 and 08 §6.6.

10. **`pwsh` was used, from the scratch directory.** Sections 01, 02, 08 and
    11 all recorded `check.ps1` as unverified for want of it. §6.3; nothing
    was installed and nothing outside the repository changed.

---

## 13. Commands

```
uv sync --frozen                                    # reproduce the environment
scripts/py-test.sh                                  # the suite  (gate step 11)
scripts/py-test.sh -s                               # …with the measured numbers printed
SAILGYM_WRITE_CONFORMANCE=1 scripts/py-test.sh      # refresh conformance.md's appended section
scripts/check.sh                                    # the gate, eleven steps
scripts/check.sh 10                                 # ruff
scripts/check.sh 11                                 # pytest
scripts/check.sh --fast                             # steps 1-9
pwsh scripts/check.ps1 -Step 11
```
