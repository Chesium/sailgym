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

Eight steps, in order, failing fast. What each one proves:

| # | Step | Proves |
|---|---|---|
| 1 | `cargo fmt --check` | Source is canonically formatted, so diffs are semantic. |
| 2 | `cargo clippy --all-targets -- -D warnings` | No lint regressions, tests and benches included. |
| 3 | `cargo test -p sailgym-physics` | The physics core is correct **and builds on the host with no WASM toolchain**. |
| 4 | `cargo test -p sailgym-physics --test invariants` | The physical invariants still hold (brief section 35). |
| 5 | `cargo test -p sailgym-physics --test regression` | Recorded scenarios still reproduce bit-for-bit (determinism). |
| 6 | `wasm-pack build …` | The Rust core still compiles to WASM and the JS glue regenerates. |
| 7 | `pnpm --dir web typecheck` | The TypeScript side still matches the WASM surface. |
| 8 | `pnpm --dir web test:e2e` | The app actually runs in Chrome, Edge and Firefox, with no console or page errors. |

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
crates/sailgym-wasm/      thin wasm_bindgen wrapper
crates/sailgym-bench/     native headless benchmark / golden-trajectory generator
web/                      Vite + React + TypeScript app and Playwright specs
scenarios/                scenario JSON
scripts/                  build-wasm.(ps1|sh), check.(ps1|sh)
docs/v1/                     brief, foundations, section PRDs, progress notes
```
