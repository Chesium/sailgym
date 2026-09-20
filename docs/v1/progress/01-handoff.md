# Section 01 — Handoff (M0: repository, toolchain, WASM, Playwright)

**Written per F13.6.** Read this before starting section 02
(`docs/v1/02-kinematics-svg.md`). Conventions remain normative in
`docs/v1/00-foundations.md`; nothing below redefines them.

Status: **complete.** `pwsh scripts/check.ps1` exits 0 end to end.

---

## 1. What landed

### Task 1.1 — Rust workspace and crate skeleton (P-group S, section agent)

Three-crate workspace per F8.1, module tree per F10, all modules present as
`//!`-only stubs that compile.

- `Cargo.toml` (workspace, resolver 2), `rust-toolchain.toml` (channel
  `stable`, components `rustfmt`/`clippy`, target `wasm32-unknown-unknown`),
  `.gitignore`.
- `crates/sailgym-physics/` — `constants.rs` verbatim from F1; `vec.rs`
  implemented in full (`Vec2`, `Vec3`, `Add`/`Sub`/`Neg`/`Mul<f64>`/`Div<f64>`)
  with 8 unit tests; every other module a stub naming the F-section that
  specifies it and the section that will implement it.
- `crates/sailgym-physics/tests/` — `invariants.rs`, `convergence.rs`,
  `regression.rs` exist from M0 so gate steps 4 and 5 are runnable. Each holds a
  harness-wiring test only; the real content arrives with the physics it guards.
- `crates/sailgym-bench/src/main.rs` — placeholder binary.

### Task 1.2 — WASM wrapper and build scripts (P-group A, delegated)

- `crates/sailgym-wasm/src/lib.rs` — placeholder `Sim { built, counter }` with
  `new` / `advance` / `snapshot` / `version`, signatures matching F8.2. F8.2
  methods that section 01 does not need are deliberately **not** stubbed.
- `crates/sailgym-wasm/tests/boundary.rs` — 3 `wasm-bindgen-test`s,
  `run_in_browser`, whole file gated `#![cfg(target_arch = "wasm32")]`.
- `scripts/build-wasm.ps1` (primary, Windows) and `scripts/build-wasm.sh`
  (CI/Linux), both running F12 step 6 and both failing loudly.

### Task 1.3 — Vite + React + TypeScript app (P-group A, delegated)

- `web/{package.json,tsconfig.json,vite.config.ts,index.html}`,
  `web/src/{main.tsx,App.tsx,vite-env.d.ts}`, `web/src/sim/loadWasm.ts`.
- `loadWasm()` caches the **in-flight promise**, so sequential *and* concurrent
  callers all receive the identical module object.
- `App.tsx` renders `<div data-testid="wasm-status" data-ready="true">sailgym
  {version}</div>`. Before ready the element is present with
  `data-ready="false"`; the failure path renders a message rather than throwing.

### Task 1.4 — Playwright harness (P-group B, section agent)

- `web/playwright.config.ts` — `webServer` starts Vite on a fixed port 5173;
  projects `chromium`, `firefox`, `msedge` (brief §38); `retries: 0` locally.
- `web/tests/e2e/fixtures.ts` — `gotoApp(page, opts?)` with the mandated
  signature, **plus** an exported `test` extended with an auto-fixture that
  asserts an empty error log at teardown. `gotoApp` fails on any `console.error`
  or `pageerror` seen up to readiness; the auto-fixture catches anything raised
  later, during interaction. Specs import `test`/`expect` from `fixtures.ts`.
- `web/tests/e2e/smoke.spec.ts` — app loads; `wasm-status` reaches ready;
  zero console/page errors; `loadWasm` idempotency proven by an in-page
  `evaluate` (sequential, concurrent, and `Sim` constructor identity).

### Task 1.5 — Gate, CI, agent docs (P-group C, section agent)

- `scripts/check.ps1` / `scripts/check.sh` — the F12 chain verbatim, fail-fast,
  `[n/8] <step>` banner per step, and each step individually runnable
  (`-Step N` / `check.sh N`).
- `.github/workflows/ci.yml` — `ubuntu-latest`, installs the toolchain and calls
  `scripts/check.sh`. It does not re-list the steps.
- `CLAUDE.md` (89 lines) — gate command and what each step proves, foundations
  as normative, F13.2 ownership, F13.3 groups, F13.4 criteria, brief §43
  no-tuning, F8 no-physics-in-TS, F13.6 handoff. No conventions, equations or
  parameters are duplicated; no numeric physical parameter appears.
- `README.md` — human-facing setup and gate documentation.

---

## 2. Deviations from the PRD, and why

### 2.1 `EPS_FLOW` is declared in `foil.rs`, not `vec.rs` (task 1.1)

Task 1.1 says "implement only `constants.rs` and `vec.rs`", and specifies
`Vec2::normalize` as returning `Vec2::ZERO` when `length < EPS_FLOW`. F5.1 pins
`EPS_FLOW` to `physics/foil.rs`. Declaring a second epsilon in `vec.rs` would be
exactly the constant drift R3 warns about, so `foil.rs` carries the one
declaration, verbatim from F5.1, and `vec.rs` imports it. `foil.rs` is otherwise
still an empty stub.

**Action for section 04:** `foil.rs` already has `pub const EPS_FLOW: f64 = 1e-9;`.
Do not redeclare it.

### 2.2 The WASM start function carries `js_name` (task 1.2)

The PRD writes `#[wasm_bindgen(start)] pub fn main() { ... }`. As written, the
build fails:

```
the name `main` is exported by multiple crates in this build;
rename one side with `js_name`/`js_namespace`
```

`wasm-bindgen-test` emits its own `main` export into the integration-test
module, so the PRD's literal text and its own acceptance criterion ("asserted by
a `wasm-bindgen-test` in `crates/sailgym-wasm/tests/boundary.rs`") cannot both
hold. **I verified this rather than accepting it**: I reverted the attribute to
the literal PRD text and re-ran — `wasm-pack test` exited 1 with the error
above; I restored the fix and 3/3 tests passed again.

Resolution: `#[wasm_bindgen(start, js_name = sailgymStart)] pub fn main()`. The
Rust signature is unchanged; only the JS symbol is renamed. The glue invokes it
via `wasm.__wbindgen_start()` during init, never by name, so `loadWasm.ts` is
unaffected. The reason is recorded in a doc comment above the function.

### 2.3 `web/pnpm-workspace.yaml` exists, outside any task's `Owns` list

pnpm 12 fails installation outright (`ERR_PNPM_IGNORED_BUILDS`, exit 1) until
every dependency with an install script is explicitly allowed or denied, and it
no longer reads those settings from `package.json`. The two packages are
`esbuild` and `@swc/core`, both transitive dependencies of the F12-pinned Vite
plugin stack rather than choices of ours. **Verified:** moving the file aside
makes `pnpm --dir web install` exit 1.

The task-1.3 subagent correctly stopped and reported this rather than silently
widening its scope; I accepted the file as section agent. It is minimal and
commented. Gate steps 7 and 8 — and therefore CI on `ubuntu-latest` — depend on
it.

### 2.4 A favicon was added (`web/public/favicon.svg`, one line in `index.html`)

The first full E2E run failed **only on msedge**: Edge reports the browser's
implicit `GET /favicon.ico` 404 as a `console.error`, which the task-1.4 trap
correctly refuses to tolerate (brief §42). Confirmed the root cause directly —
`curl http://localhost:5173/favicon.ico` returned 404. The fix is an SVG favicon
and a `<link rel="icon">`; it is not a test weakening. This touched
`web/index.html`, which task 1.3 owns, after that task had completed.

### 2.5 Vite is pinned to 7.x, not the current 8.x

`vite-plugin-top-level-await` `require()`s `rollup` and `esbuild` at module
load. Vite 8 is rolldown-based and ships neither, so `pnpm --dir web build`
fails with `Cannot find module 'rollup'`. Vite was pinned to `^7.3.6`, which in
turn forced `@vitejs/plugin-react` to `^5` (its 6.x peer-requires Vite 8). F12's
plugin list is honoured verbatim; only the version is chosen.

**Carry forward:** the project is pinned to Vite 7 until
`vite-plugin-top-level-await` supports rolldown. Section 03 should re-check this
when it adds deck.gl.

### 2.6 `cargo fmt` reorders the `lib.rs` module list

Task 1.1 shows `pub mod` declarations grouped by concern. `rustfmt` sorts them
alphabetically, and gate step 1 is `cargo fmt --check`. The module *set* is
exactly as specified; only the order differs. No escalation: this is formatting,
not a convention.

**No physical coefficient was changed or tuned.** `parameters.rs` is still a
stub; the only numeric literals in the physics crate are the three constants of
F1, in `constants.rs`.

**No contradiction between the brief, `00-foundations.md` and `01-skeleton.md`
was found.** §2.2 above is a PRD-vs-toolchain conflict, not a documentation one.

---

## 3. Validation evidence

Every command below was run on this host and the exit code observed.

### Task acceptance criteria

| Task | Criterion | Result |
|---|---|---|
| 1.1 | `cargo build --workspace` | **pass**, exit 0 |
| 1.1 | `cargo test -p sailgym-physics` | **pass**, exit 0, 11 tests |
| 1.1 | `cargo test -p sailgym-physics vec::` >= 6 tests | **pass**, **8** tests: `cross` sign (`Vec2::new(1,0).cross(Vec2::new(0,1)) == 1.0`), zero-vector `normalize` to exactly `ZERO` (and sub-`EPS_FLOW`, never `NaN`), `Vec3::cross` right-handedness (x×y=z, y×z=x, z×x=y, y×x=−z), cross perpendicular to both operands, dot/length, unit length, and `Vec2`/`Vec3` operator round-trips |
| 1.1 | `cargo tree -p sailgym-physics \| grep -c wasm-bindgen` = 0 | **pass**, prints `0` |
| 1.1 | `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` | **pass**, exit 0 |
| 1.2 | `pwsh scripts/build-wasm.ps1` produces `sailgym_wasm.js` + `sailgym_wasm_bg.wasm` | **pass**; `.sh` equivalent also verified |
| 1.2 | `web/src/wasm/` gitignored | **pass**, `git check-ignore -v` reports `.gitignore:6` |
| 1.2 | `Sim::new("{}")` Ok, `Sim::new("not json")` Err, via `wasm-bindgen-test` | **pass, in a real browser**: `wasm-pack test --headless --chrome` gives 3 passed, 0 failed |
| 1.2 | `console_error_panic_hook` wired | **pass**; generated glue contains the hook's `console.error` import and calls `wasm.__wbindgen_start()` at init |
| 1.3 | `pnpm --dir web install`, `pnpm --dir web build` | **pass**, exit 0 (install requires §2.3) |
| 1.3 | `pnpm --dir web typecheck` with `strict: true` | **pass**, exit 0; `"strict": true` present |
| 1.3 | dev server reaches `data-ready="true"` within 5 s | **pass**, measured **289 ms**, zero console/page errors, text `sailgym 0.1.0` |
| 1.3 | `loadWasm()` twice returns the identical module | **pass**, asserted in-page in 1.4 |
| 1.4 | `pnpm --dir web test:e2e` green on chromium, firefox, msedge | **pass**, 6/6 |
| 1.4 | deliberate throw in `App.tsx` makes `smoke.spec.ts` fail | **pass — proven live**, see below |
| 1.4 | total E2E wall time < 60 s warm | **pass**, **4–5 s** wall, 3.7–4.4 s reported by Playwright |
| 1.5 | `pwsh scripts/check.ps1` exits 0 | **pass**, 8/8 |
| 1.5 | each step individually runnable; failure aborts the rest | **pass**, see below |
| 1.5 | CI calls `scripts/check.sh` and does not re-list steps | **pass** by inspection; **not executed** (see §5) |
| 1.5 | `CLAUDE.md` < 100 lines, no numeric physical parameter | **pass**, 89 lines; the only digits are F-section references |

### The console/page-error trap, proven live (task 1.4)

Required by the PRD to be verified by hand, not assumed.

1. Baseline: `pnpm --dir web test:e2e` gives **6 passed**, exit 0.
2. Inserted `throw new Error('deliberate failure: proving the console-error
   trap')` as the first statement of `App()` in `web/src/App.tsx`.
3. Re-ran: **6 failed** — both specs on **all three** browsers — exit 1. The
   failures came from the trap itself (`fixtures.ts:47`) and from `gotoApp`'s
   readiness assertion.
4. Reverted the file (`grep -c "deliberate failure"` returns 0).
5. Re-ran: **6 passed**, exit 0, 3.8 s.

An earlier, *unplanned* proof: the very first full run failed on msedge only,
catching the real `/favicon.ico` 404 described in §2.4. The trap found a genuine
defect before it was ever deliberately tested.

### Fail-fast, proven (task 1.5)

Appended deliberately misformatted code to `constants.rs` and ran the gate:
output was `[1/8] cargo fmt --check` then `[1/8] FAILED (cargo fmt --check)`,
exit 1. **Steps 2–8 never ran.** Reverted; `cargo fmt --check` clean.
Single-step execution verified both ways: `pwsh scripts/check.ps1 -Step 1` and
`bash scripts/check.sh 3`, both exit 0.

### Section acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | `pwsh scripts/check.ps1` exits 0 end to end | **pass** — 8/8, exit 0, 39 s cold / 7 s warm |
| 2 | `cargo test -p sailgym-physics` runs on the host with no WASM toolchain present | **pass — proven, not inferred.** Removed the `wasm32-unknown-unknown` target *and* moved `wasm-pack.exe` aside (`command -v wasm-pack` reported not found; installed targets reduced to `x86_64-pc-windows-msvc` only), then ran the tests: 11 passed, exit 0. Environment restored afterwards. |
| 3 | `pnpm --dir web dev` displays the WASM version string | **pass** — `sailgym 0.1.0` in 289 ms |
| 4 | `pnpm --dir web test:e2e` green on Chrome, Edge and Firefox | **pass** — 6/6 across `chromium`, `msedge` (system Edge 153.0.4234.32), `firefox` (Playwright Firefox 155.0) |
| 5 | `cargo tree -p sailgym-physics` has no `wasm-bindgen` and no JS-facing crate | **pass** — tree is `serde` + `serde_json` only; a case-insensitive grep for `wasm`/`js-sys`/`web-sys` matches 0 lines |
| 6 | `grep -riE "rho\|9\.8\|1\.225\|1025" crates/sailgym-physics/src --include=*.rs` matches only `constants.rs` | **pass** — exactly 3 matches, all in `constants.rs` |
| 7 | `docs/v1/progress/01-handoff.md` exists per F13.6 | **pass** — this file |

---

## 4. Toolchain versions (R7)

Golden regression files in section 09 must reference these. Per F9, bit-identity
is claimed only for the same build on the same platform.

| Tool | Version |
|---|---|
| host triple | `x86_64-pc-windows-msvc` (Windows 11 Pro 26200) |
| rustc | **1.98.1** (48a229cea 2026-09-01) |
| cargo | 1.98.1 (797e8a9bc 2026-08-05) |
| rustfmt | 1.9.0-stable (48a229ceae 2026-09-01) |
| clippy | 0.1.98 (48a229ceae 2026-09-01) |
| wasm-pack | 0.15.0 |
| wasm-bindgen | 0.2.128 |
| wasm-bindgen-test | 0.3.78 |
| console_error_panic_hook | 0.1.7 |
| serde / serde_json | 1.0.229 / 1.0.151 |
| node | v26.6.0 |
| pnpm | 12.4.2 (lockfile 9.0) |
| PowerShell | 7.6.5 |
| vite | 7.3.6 |
| react / react-dom | 19.3.0 |
| typescript | 7.0.2 |
| @playwright/test | 1.63.0 |
| @vitejs/plugin-react | 5.2.0 |
| vite-plugin-wasm | 3.6.0 |
| vite-plugin-top-level-await | 1.6.0 |
| Chrome (wasm-pack test) | 153.0.8010.50 |
| Edge (`msedge` project) | 153.0.4234.32 |
| Playwright Firefox | 155.0 (build 1543) |

> **Note.** Work began on rustc 1.97.1; `rustup target add` upgraded stable to
> **1.98.1** mid-section. The full gate and the browser boundary test were
> re-run on 1.98.1 and are green. 1.98.1 is the fingerprint to record.

---

## 5. Risks and open items

**R5 — deck.gl bundle size / WebGL context loss: OPEN, and deliberately not
exercised.** deck.gl is **not installed** and **no deck.gl code was written or
run** in this section. Nothing here supports any claim that deck.gl and
`vite-plugin-wasm` coexist cleanly — that remains unverified and must be
established by section 03, not assumed from this note. What *is* verified:
`vite-plugin-wasm` and `vite-plugin-top-level-await` coexist with React under
Vite 7, in both dev and production builds, and the WASM asset is emitted
correctly (`dist/assets/sailgym_wasm_bg-*.wasm`, 50.51 kB). The Vite-7 pin in
§2.5 is the one concrete constraint section 03 inherits: if deck.gl requires
Vite 8, that conflict surfaces there.

**R7 — golden regression files are build-sensitive: handled for now.** Versions
are recorded above. `crates/sailgym-physics/tests/regression.rs` exists, runs in
gate step 5, and establishes the convention: a missing or mismatched toolchain
fingerprint **skips with a clear message**, it does not fail confusingly. It
currently has no golden files to compare, and says so on stderr. Section 09 must
write the fingerprint into each golden file and implement the comparison.

**No other F11 risk fired.** R1, R2, R3, R4 and R6 are all about physics, and
there is none yet.

**Not executed:** the GitHub Actions workflow has never run — this repository has
no remote and no CI run exists. `ci.yml` is correct by inspection and calls
`scripts/check.sh` (itself verified locally under bash), but the Linux path,
`playwright install --with-deps msedge` on `ubuntu-latest`, and the pinned
action versions are **unproven**. Expect to debug the first CI run.

---

## 6. What section 02 must know

1. **The gate is the contract.** `pwsh scripts/check.ps1` must be green when you
   finish. Step 3 runs the whole physics crate; steps 4 and 5 run the
   `invariants` and `regression` targets, which already exist — add to them, do
   not recreate them.
2. **`data-testid="wasm-status"` and `data-ready` are frozen.** Every spec
   depends on them; do not rename them.
3. **Use `gotoApp` from `web/tests/e2e/fixtures.ts` in every new spec**, and
   import `test`/`expect` from there rather than from `@playwright/test`, or you
   lose the console-error trap. A spec that triggers a `console.error` will fail,
   by design.
4. **`snapshot()` currently returns `[counter]`.** Task 2.4 replaces it with the
   F8.3 layout. `web/src/sim/snapshot.ts` does not exist yet; F8.3 requires the
   TS accessor and the Rust layout to come from one shared constant list, with a
   test asserting they agree in length and order.
5. **`Sim` exposes only `new`/`advance`/`snapshot`/`version`.** The rest of F8.2
   is intentionally unstubbed; add methods as the sections that specify them
   arrive.
6. **`state.rs`, `parameters.rs`, `frames.rs`, `dynamics.rs`, `forces.rs`,
   `integrator.rs`, `diagnostics.rs`, `simulation.rs` are empty stubs** whose doc
   comments name their F-section. Fill them; do not move them.
7. **`foil.rs` already declares `EPS_FLOW`** (§2.1). Do not redeclare it.
8. **No numeric physical literal outside `constants.rs` and `parameters.rs`**
   (F7). Section 01 satisfies this; section 10 greps for it.
9. **The M1 scaffold force model you are about to write is debt (R4).** It is
   deleted by task 4.5 and by nothing else. Make it easy to grep for.
