# v2 Section 03 — Python, and `wind.sample` in JAX

Source discussion: `../discussions/cross-stack.md`, §§1.1, 2, 3, 4.1, 4.2, 4.3, 5.
Answers §8 point **5**.

Read first, in order: `../../v1/00-foundations.md` in full, `../README.md`,
`../brief.md`, `../00-foundations.md` (F12′, F16, F17), `../progress/02-handoff.md`,
then this PRD.

## Goal

Stand up the first Python in the repository, and port exactly one function —
`ProceduralWind::sample` — to JAX, tested against section 02's bundle.

The function is chosen because it is the easiest possible target: pure,
branch-free, a twelve-term sum, and — because section 02 exported the mode table
as data — carrying **no RNG dependency at all**. What is actually being built
here is the toolchain: bundle, loader, tolerance, digest check, gate steps. When
the hard port lands in a later section, the only new thing being tested will be
the physics.

## Why this shape

`cross-stack.md` §8: *"Steps 4 and 5 are the ones that make the rest cheap."*
Every mistake a porting effort can make about file formats, dtype promotion,
digest keying, tolerance derivation and gate plumbing is cheaper to make against
a twelve-term cosine sum than against a 13-state RK2 step with six force terms
and eight branch points.

## What this section does **not** do

- No `derivative`, no integrator, no forces, no trajectories. Tiers 1 and 2 of
  the bundle are consumed by a later section; this one consumes tier 0 and the
  wind slice of tier 3.
- No Warp. No f32 training path. No training loop, no policy, no Gymnasium — all
  of that is blocked on V-A and is sections 04–07.
- **No change to any Rust file.** `git diff --name-only crates/` must be empty at
  the end of the section; acceptance criterion 6 asserts it. If the port needs
  something from Rust that section 02 did not export, **stop and report it** —
  do not reach into a crate this section does not own.
- No `pip`, no `conda`, no `requirements.txt`.

## Normative deltas

### D1 — F12′ steps 10 and 11. **Open; blocking task 3.7.**

The gate grows from nine steps to eleven:

```
10.  uv run ruff check python && uv run ruff format --check python
11.  scripts/py-test.sh
```

Needs the same human approval, with a date, that section 01's D1 and section
02's D1 received. Recorded in `../00-foundations.md` F12′, which also states why
step 11 is a script: section 07 puts `maturin develop` in front of `pytest`
**without amending F12 again**, exactly as step 6 has delegated to
`build-wasm.sh` since v1.

### D2 — F17 (the Python boundary). **Open; blocking the section.**

`../00-foundations.md` F17 in full. F17.1 is the load-bearing clause here: the
F8 rule restated for Python, enforced by an audit in the three-tier shape of
`provenance.rs::no_stray_constants`.

### D3 — F16.6 (the wind kernel is not `libm`). **Open; blocking task 3.3.**

`environment/wind.rs`'s `wave` is a hand-rolled cosine and `sample_inner`
accumulates in a fixed two-slot pairwise order required by F9.4. F16.6 requires
a port to ship **two arms** and to report the divergence between them as a
number.

### D4 — `brief.md` S2. **Open; the human must sign.** Same reading as section 02's D3.

## The two arms, stated once

This is the section's one real design decision, so it is stated here and not
left to a task.

| Arm | Kernel | Reduction | Held to |
|---|---|---|---|
| `wave_exact` | the Cody–Waite three-way argument reduction and the polynomial of `wind.rs`, transcribed | the two-slot pairwise accumulation of `sample_inner`, transcribed | tier 0's 32 ulp |
| `wave_cos` | `jnp.cos` | whatever XLA's reduction does | the **measured** bound section 02 wrote into the manifest |

Both arms read the same mode table and the same parameters. Neither may be
called "the port" — the module exports both and the caller chooses.

Why both, rather than either alone:

- `wave_cos` alone can never catch a defect smaller than the kernel gap, which
  is about `1e-14` per mode. That is enough to catch every sign error, every
  unit slip and every stratification mistake — the R3 defect class — but it
  cannot distinguish a correct port from one that is wrong in the sixteenth
  digit, and "the port agrees to 1e-13" would then be an untestable claim.
- `wave_exact` alone is the arm nobody will use in production, because it is
  slower and will not fuse.
- Together they give the thing §5 asks for on f32 versus f64, one section early
  and on a much simpler function: **a measured number for how much the
  convenient choice costs**, rather than an assumption that it costs nothing.

The divergence between the two arms is a **reported number**, not a pass or a
fail. It goes in the handoff and in `docs/v2/conformance.md`.

## Tasks

### 3.1 — The Python toolchain

**Owns:** `pyproject.toml`, `uv.lock`, `.python-version`, `.gitignore`,
`python/sailgym_conformance/__init__.py`, `python/sailgym_jax/__init__.py`,
`python/tests/conftest.py`
**P-group: S**

`uv`, one workspace at the repository root, `requires-python` pinned to an exact
minor version. Dependencies: `jax`, `numpy`, `pytest`; dev: `ruff`. **`uv.lock`
is committed** — the same reason `web/pnpm-lock.yaml` and `Cargo.lock` are, and
the same reason F9.2 hand-rolls PCG32: a toolchain that can drift under you is
not a reference.

`.gitignore` gains `.venv/`, `__pycache__/`, `.pytest_cache/`, `.ruff_cache/`.

`conftest.py` enables `jax_enable_x64` **once, at import**, and asserts it took
effect. F9.5 forbids f32 intermediates in physics; a conformance run in f32 by
accident would produce a number that looks like a tolerance failure and is
actually a configuration failure, and that is a day lost.

Acceptance: `uv sync --frozen` succeeds from a clean checkout;
`uv run python -c "import jax; assert jax.config.jax_enable_x64"` under
`conftest`'s import; `uv run ruff check python` and
`uv run ruff format --check python` both clean.

### 3.2 — The bundle loader

**Owns:** `python/sailgym_conformance/bundle.py`,
`python/tests/test_bundle.py`
**P-group: A**

Shared by every stack's runner, now and later, so it belongs to no stack.

```python
bundle = Bundle.load(root)            # discovers conformance/<digest>/
bundle.digest                          # str
bundle.tolerance("tier0", "wind")      # (value, justification)
bundle.table("tier0_wind_sample")      # named columns, not positions
bundle.modes()                         # the wind mode table
```

Three behaviours are the point:

1. **Refuses on a digest mismatch** (F16.4.2) — the manifest's `digest` field
   must equal its directory name, and the loader raises rather than warning. A
   stale bundle that silently passes is worse than no bundle.
2. **Columns are addressed by name**, never by index. The manifest's column
   names are the contract; a port that reads column 3 is one insertion away from
   comparing the wrong quantity and never knowing.
3. A tolerance is returned **with its justification string**, and the test
   helper prints the justification on failure. A number without its derivation
   is how a tolerance gets widened by someone who does not know what it meant.

Acceptance: `uv run pytest python/tests/test_bundle.py`;
`numpy.load` opens every `.npy` in the committed bundle (this is section 02
task 2.4's contract, asserted here, on the other side); a hand-corrupted
`manifest.json` digest raises; an unknown column name raises with the available
names in the message.

### 3.3 — The port

**Owns:** `python/sailgym_jax/wind.py`
**P-group: A**

```python
def wave_exact(theta): ...      # transcribed kernel
def wave_cos(theta): ...        # jnp.cos
def sample(modes, base, x, y, t, *, wave=wave_exact): ...
def sample_grid(modes, base, x0, y0, dx, dy, nx, ny, t, *, wave=wave_exact): ...
```

Modes come from `wind_modes.json` via the loader and are **never regenerated**.
There is no PCG32 here, no stratification, no Fisher–Yates shuffle, and no
`seed` argument — F16.7. If a reviewer finds a seed in this file, the port is
wrong in a way no tolerance will catch.

`sample_grid` exists because F6.1 requires it to produce **bit-identical**
values to `sample` at the same points; the port inherits that requirement and
3.5 asserts it.

Transcription rules for `wave_exact`, because this is the file where a port most
easily becomes a paraphrase:

- The three-way Cody–Waite reduction constants, the fold onto `[0, π/2]`, the
  `clamp`, and the polynomial's Estrin grouping are transcribed **structurally**,
  not re-derived. A mathematically equivalent regrouping is not equivalent in
  floating point, and reproducing the reference bit pattern is the entire
  purpose of this arm.
- The two-slot pairwise accumulation of `sample_inner` is reproduced including
  the odd-`K` tail, and including the empty-modes early return that avoids
  `base + 0.0` negative-zero drift.
- Every constant is a **named module constant with a doc string naming its
  source line in `wind.rs`**. Task 3.6's audit enforces it, and F17.1 is why.

Acceptance: covered by 3.4 and 3.5. This task ships no test of its own, on
purpose — a port that grades its own homework is how two implementations agree
on the same mistake.

### 3.4 — Tier 0

**Owns:** `python/tests/test_wind_tier0.py`
**P-group: B**

Against `tier0_wave` and `tier0_wind_sample` from the bundle:

- `wave_exact` within **32 ulp** relative, per row.
- `wave_cos` within the **measured** bound from the manifest.
- The **divergence between the two arms** computed over the same inputs and
  reported: max absolute, max relative, and the ulp distribution. Printed, and
  written to the handoff. Not asserted against a threshold.
- Branch rows are reported **by their sampler's name** on failure, so a red test
  names the hazard rather than a row index. In particular the `wrap_pi`-adjacent
  rows, the `±π` boundary and the `EPS_FLOW` guard rows carry their names.

The failure demonstration this task must perform and record, in the style
`no_shortcuts.rs`'s module doc uses: negate one component of `amp` in `wind.py`,
watch tier 0 go red on **both** arms and name the sampler, revert. A port whose
test cannot catch a flipped sign is not testing the port.

Acceptance: `uv run pytest python/tests/test_wind_tier0.py` passes; the sign-flip
demonstration is recorded in the handoff with the observed error magnitude.

### 3.5 — The wind slice of tier 3

**Owns:** `python/tests/test_wind_tier3.py`
**P-group: B**

The invariants a wind-only port can actually satisfy, mirroring
`crates/sailgym-physics/tests/wind.rs`:

- **Finite over a wide domain** — brief §35's finite-number invariant. No NaN,
  no Inf, over a domain far wider than any scenario uses.
- **Bounded magnitude** — the perturbation stays within the bound `variation`
  and the normalisation imply.
- **Grid matches point** — `sample_grid` equals `sample` at the same points, to
  the tier-0 tolerance of whichever arm is in use. F6.1 requires bit-identity in
  Rust; across stacks it is a tolerance, and that difference is stated in the
  test's docstring so nobody later "fixes" it into an equality.
- **Uniform mode is exactly `base`** — the empty-modes path, asserted as exact
  equality including the sign of zero.

This is the slice that licenses "the same wind field" rather than "a function
that tracks a table for a few rows". The rest of brief §35 needs a full port and
belongs to a later section.

Acceptance: `uv run pytest python/tests/test_wind_tier3.py` passes; each test's
docstring names the brief §35 invariant or the `tests/wind.rs` test it mirrors.

### 3.6 — The Python constants audit

**Owns:** `python/tests/test_no_stray_constants.py`
**P-group: B**

F17.1, in the three-tier shape of `provenance.rs::no_stray_constants`. Scans
`python/sailgym_jax/` and `python/sailgym_conformance/`, sorted for deterministic
first-offender reporting (F9.3's reason), and permits a float literal in exactly
three ways:

1. it is structural — `0.0`, `1.0`, `0.5`, `2.0`;
2. it is the right-hand side of a **module-level named constant** that carries a
   doc string;
3. it is in an explicit exemption table **with a stated reason**, and — copying
   the comment `provenance.rs` puts on its own — *the list is meant to stay
   short; a new entry is a decision that a number belongs in the code rather
   than in the bundle, and it has to be argued here.*

A second pass asserts every named module constant that introduces a number
carries a doc string naming its source line in the Rust file it came from.

Two anti-vacuity self-checks, both copied from the Rust audit, because an audit
that silently scans nothing is worse than no audit:
`assert scanned > N` on literals and `assert files > N` on files, with the
current counts.

Acceptance: passes; **proven able to fail** by pasting `1.225` into `wind.py`,
observing the failure and the offending file and line, and reverting — recorded
in the handoff.

### 3.7 — The gate

**Owns:** `scripts/py-test.sh`, `scripts/py-test.ps1`, `scripts/check.sh`,
`scripts/check.ps1`, `CLAUDE.md`, `docs/v1/00-foundations.md`,
`docs/v2/README.md`
**P-group: S**

D1. `scripts/py-test.{sh,ps1}` mirrors `build-wasm.{sh,ps1}`: checks `uv` is on
the path and errors with an install hint, then runs `uv run --frozen pytest python`
from the repository root.

`check.sh` gains entries at **both** sites — the `step_names` array and the
`run_step` case. `check.ps1` gains two `$Steps` hashtables **and**
`[ValidateRange(1, 9)]` becomes `(1, 11)`; forgetting the range makes
`scripts/check.ps1 -Step 10` fail with a parameter error rather than running the
step, which is a confusing way to discover a typo. `CLAUDE.md`'s table becomes
eleven rows and its prose stops saying "Nine steps". `docs/v2/README.md`'s chain
and F12 itself both carry the new steps, the date and the approval.

`--fast` does not cover steps 10 and 11 in this section; the `--fast` subset
stays as it is. Recorded as a debt below.

Acceptance: `scripts/check.sh` runs eleven steps and is green;
`scripts/check.sh 10` and `scripts/check.sh 11` each run exactly one step;
`pwsh scripts/check.ps1 -Step 11` runs and is green; a deliberately broken
Python test makes `scripts/check.sh` exit non-zero **and** print
`[11/11] FAILED`.

## Section acceptance criteria

1. `scripts/check.sh` green, all eleven steps.
2. `uv sync --frozen` reproduces the environment from a clean checkout.
3. `git diff --name-only crates/` is empty — this section changes no Rust.
4. The sign-flip demonstration (3.4) and the stray-constant demonstration (3.6)
   were both performed, observed red, and reverted, with the observed magnitudes
   recorded.
5. The two-arm divergence is recorded as a number — max absolute, max relative
   and the ulp distribution — in the handoff and in `docs/v2/conformance.md`.
6. No file under `python/sailgym_jax/` contains a seed, an RNG, or the string
   `pcg`. Asserted by 3.6's audit as a fourth pass.
7. `docs/v2/progress/03-handoff.md` written per F13.6, recording which `RV`
   fired, the measured numbers, and the `--fast` debt.

## Risks

| # | Risk | Mitigation | Fires when |
|---|---|---|---|
| **RV13** | `wave_exact` is written as a paraphrase — mathematically equal, differently grouped — and the 32 ulp tolerance is then quietly widened to make it pass. | 3.3's transcription rules; the tolerance lives in the manifest, which this section does not own and cannot edit. | any tolerance in `manifest.json` changes in a commit owned by this section |
| **RV14** | f32 creeps in through a dtype promotion nobody notices, and a real disagreement reads as a tolerance failure. | `conftest.py` enables and **asserts** `jax_enable_x64` at import; the loader returns `float64` arrays explicitly. | any conformance array's dtype is not `float64` |
| **RV15** | Someone reimplements PCG32 to "check the modes", spending days to reproduce a kilobyte. | F16.7; 3.3 forbids a seed in the module; 3.6 asserts the absence. | the string `pcg`, `seed` or `stratum` appears under `python/sailgym_jax/` |
| **RV16** | The Python audit is vacuous — scans an empty directory, passes forever. | Two self-checks copied from `provenance.rs`, plus the proven-able-to-fail demonstration in 3.6. | `scanned` or `files` falls below its stated floor |
| **RV17** | Gate steps 10 and 11 are added at one of the five sites and not the others, so the gate lies about its own length. | 3.7 enumerates all five, including the PowerShell `ValidateRange` that has no bash counterpart. | `scripts/check.sh` and `scripts/check.ps1` disagree on `$total` |
| **RV18** | `uv` is not installed on the machine running the gate, and step 11 fails in a way that looks like a test failure. | `py-test.sh` checks for `uv` first and errors with an install hint, exactly as `build-wasm.sh` does for `wasm-pack`. | a gate failure at step 10 or 11 names a Python traceback rather than a missing tool |

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| `--fast` does not run steps 10 and 11 | 3.7, to keep the pre-commit subset fast | whenever the Python suite gets slow enough that skipping it matters |
| Only tier 0 and the wind slice of tier 3 are consumed; tiers 1 and 2 sit unused in the bundle | scope — one function is the point | the section that ports the full step |
| `wave_exact` will not fuse and is not the arm anyone trains with | by design, F16.6 | never; it is a conformance witness, not a backend |
| No Warp runner, though F16.4 says one runner per stack | scope | the Warp section |
| No CI runs any of this; `scripts/check.sh` is still invoked by hand | inherited from v1 — there is no `.github/` in the repository | whenever CI is set up; it is one call to one script |
