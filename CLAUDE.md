# sailgym — agent entry point

A browser sailing simulator: physics in Rust, compiled to WASM, driven by a
React/TypeScript front end. Read this file first, then the normative docs below.

## Normative documents — read before editing

| File | Authority |
|---|---|
| `docs/v1/brief.md` | Authoritative on **scope**. |
| `docs/v1/00-foundations.md` | **Normative** on conventions, frames, sign conventions, equations, parameters and the WASM surface. |
| `docs/v1/NN-<name>.md` | The executable PRD for one section (milestone). |
| `docs/v1/progress/NN-handoff.md` | Written on completing section NN; read by section NN+1. |
| `docs/v2/README.md`, `docs/v2/00-foundations.md`, `docs/v2/prds/NN-*.md` | The v2 milestone: its index, its **normative deltas** to the table above, and its section PRDs. A v2 delta amends v1 only where it says so, in writing. |
| `conformance/<key>/manifest.json` | The committed conformance bundle's own record: what a second implementation is held to, and to what tolerance (v2 F16). Generated, never edited. |

`00-foundations.md` outranks the section PRDs. The brief outranks everything on
scope. **No agent may redefine anything in `00-foundations.md`.** If you find a
contradiction, stop, record the exact passages in your handoff note, and
escalate to the human — never silently pick a convention.

This file deliberately contains **no** conventions, equations or parameters.
They live in `00-foundations.md` and are not duplicated here; duplication is how
drift starts.

## The gate

```
pwsh scripts/check.ps1        # Windows (primary)
scripts/check.sh              # Linux / CI
pwsh scripts/check.ps1 -Step 3
scripts/check.sh 3            # a single step
```

Eleven steps, in order, failing fast. What each one proves:

| # | Step | Proves |
|---|---|---|
| 1 | `cargo fmt --check` | Source is canonically formatted, so diffs are semantic. |
| 2 | `cargo clippy --all-targets -- -D warnings` | No lint regressions, tests and benches included. |
| 3 | `cargo test -p sailgym-physics -p sailgym-task` | The physics core and the practice evaluator are correct **and build on the host with no WASM toolchain**. |
| 4 | `cargo test -p sailgym-physics --test invariants --test no_shortcuts --test convergence --test symmetry --test provenance --test conformance` | The physical invariants still hold (brief section 35), no prohibited shortcut has crept in, the integrator converges, port/starboard symmetry holds, no coefficient has drifted from the F7 catalogue, and the committed conformance bundle still describes the compiled physics (v2 F16, section 02). |
| 5 | `cargo test -p sailgym-physics --test regression` | Recorded scenarios still reproduce bit-for-bit (determinism). |
| 6 | `wasm-pack build …` | The Rust core still compiles to WASM and the JS glue regenerates. |
| 7 | `pnpm --dir web typecheck` | The TypeScript side still matches the WASM surface. |
| 8 | `pnpm --dir web test:unit` | The pure TypeScript — projection, camera, clock, controls, schemas — is correct without a browser. |
| 9 | `pnpm --dir web test:e2e` | The app actually runs in Chrome, Edge and Firefox, with no console or page errors. |
| 10 | `uv run ruff check python && uv run ruff format --check python` | The Python is canonically formatted and lint-clean — steps 1 and 2 for the other language. |
| 11 | `scripts/py-test.sh` | The Python suite: the conformance-bundle loader, the JAX wind port's two arms and the F17.1 constants audit (v2 F12′, section 03). A script, not a command line, so a later section can put `maturin develop` in front of `pytest` without amending F12 again. |

Every section must leave the app runnable and the gate green. No exceptions and
no "will fix next section".

## Rules

**File ownership is exclusive** (F13.2). Each task in a section PRD lists
`Owns:`. You may *read* any file; you may *write* only the files your task owns.
If you need a change in a file you do not own, **stop and report it** — do not
edit it.

**Parallel groups** (F13.3). Each task carries `P-group: <letter>` or
`P-group: S`. Tasks sharing a letter may run in parallel; groups run in
alphabetical order. `S` means solo — the section agent executes it itself and
never delegates or parallelises it.

**Acceptance criteria are commands or numeric assertions** (F13.4). A task is
done when its named tests exist and pass. "Looks right" is never sufficient.
Never weaken a test to obtain a green result.

**Never tune a physical coefficient to make a scenario look better**
(brief section 43). If a coefficient must change, record the reason, the source
and the assumption — both in the `parameters.rs` doc comment and in the section
handoff note.

**No physical equation is implemented in TypeScript** (F8). The physics lives in
`crates/sailgym-physics`, which must stay free of `wasm-bindgen` and any
JS-facing crate.

**Write a handoff note** (F13.6). On completing a section, write
`docs/v1/progress/NN-handoff.md` recording: what landed; what deviated from the PRD
and why; any risk from the F11 register that fired; any parameter changed and
why; and anything the next section must know.

## Layout

```
crates/sailgym-physics/   pure Rust core; all physics tests live here
crates/sailgym-task/      pure practice-task evaluation; depends on physics, never the reverse
crates/sailgym-wasm/      thin wasm_bindgen wrapper
crates/sailgym-bench/     native headless benchmark / golden-trajectory generator
web/                      Vite + React + TypeScript app and Playwright specs
python/sailgym_conformance/  stack-neutral conformance-bundle loader; no equations (F17.1)
python/sailgym_jax/       the JAX verification implementation; the one F17.1 exception
python/tests/             pytest: the loader, the two arms, the constants audit
scenarios/                scenario JSON
scripts/                  build-wasm.(ps1|sh), check.(ps1|sh)
docs/v1/                     brief, foundations, section PRDs, progress notes
```
