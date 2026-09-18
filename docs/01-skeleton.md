# Section 01 — Repository, Toolchain, WASM and Playwright Skeleton (M0)

**Prerequisite reading:** `docs/00-foundations.md` (all), `docs/brief.md` §40–43.
**Predecessor handoff:** none (first section).
**Brief coverage:** §23, §24, §38, §40, §41, §42, §43.

## Goal

A runnable, empty-but-real vertical slice: a Rust workspace whose physics crate
tests on the host, a WASM wrapper the browser loads, a React app that proves the
WASM is alive, a Playwright test that proves the browser proves it, and one
command that runs the whole gate chain.

No physics. No boat. Anything resembling a force equation in this section is out
of scope.

## Non-goals

State vector, integrator, rendering, wind, controls. All of those are section 02+.

---

## Tasks

### 1.1 — Rust workspace and crate skeleton
**P-group: S** (section agent executes this itself)
**Owns:** `Cargo.toml`, `rust-toolchain.toml`, `crates/sailgym-physics/**`, `crates/sailgym-wasm/**`, `crates/sailgym-bench/**`, `.gitignore`

Create the three-crate workspace of F8.1 with the module tree of F10 present as
empty-but-compiling modules (`//!` doc comment + nothing else is fine).

```rust
// crates/sailgym-physics/src/lib.rs
pub mod constants; pub mod vec; pub mod rng;
pub mod state; pub mod parameters; pub mod frames; pub mod foil;
pub mod dynamics; pub mod forces; pub mod integrator; pub mod diagnostics;
pub mod scenario; pub mod recording; pub mod simulation;
pub mod environment; pub mod aero; pub mod hydro; pub mod rigging; pub mod stability;
```

Implement only `constants.rs` (verbatim from F1) and `vec.rs`:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct Vec2 { pub x: f64, pub y: f64 }
#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct Vec3 { pub x: f64, pub y: f64, pub z: f64 }
impl Vec2 {
    pub fn new(x: f64, y: f64) -> Self;
    pub fn dot(self, o: Self) -> f64;
    pub fn cross(self, o: Self) -> f64;        // self.x*o.y − self.y*o.x
    pub fn length(self) -> f64;
    pub fn length_squared(self) -> f64;
    pub fn normalize(self) -> Self;            // Vec2::ZERO if length < EPS_FLOW
    pub const ZERO: Self;
}
// Vec3: new, dot, cross (Vec3), length, length_squared, normalize, ZERO, xy() -> Vec2
// Both: Add, Sub, Neg, Mul<f64>, Div<f64> via std::ops
```

`sailgym-physics` must have **no** `wasm-bindgen` dependency (F8.1).

**Acceptance criteria**
- `cargo build --workspace` exits 0.
- `cargo test -p sailgym-physics` exits 0 (may run 0 tests, but the `vec` tests below must exist).
- `cargo test -p sailgym-physics vec::` runs ≥ 6 tests covering: `cross` sign
  (`Vec2::new(1,0).cross(Vec2::new(0,1)) == 1.0`), `normalize` of a zero vector
  returning `Vec2::ZERO` exactly, `Vec3::cross` right-handedness
  (`x̂ × ŷ == ẑ`), and operator round-trips.
- `cargo tree -p sailgym-physics | grep -c wasm-bindgen` returns 0.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` exit 0.

---

### 1.2 — WASM wrapper and build script
**P-group: A**
**Owns:** `crates/sailgym-wasm/src/lib.rs`, `crates/sailgym-wasm/Cargo.toml`, `scripts/build-wasm.ps1`, `scripts/build-wasm.sh`
**Depends:** 1.1

A placeholder `Sim` exposing just enough surface to prove the boundary works.
Signatures must match F8.2 exactly where implemented; unimplemented methods are
not stubbed in this section.

```rust
#[wasm_bindgen]
pub struct Sim { built: String, counter: u32 }

#[wasm_bindgen]
impl Sim {
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<Sim, JsValue>;
    pub fn advance(&mut self, n: u32) -> u32;      // increments counter, returns n
    pub fn snapshot(&self) -> Box<[f64]>;          // [counter as f64]; real layout lands in 2.4
    pub fn version(&self) -> String;               // CARGO_PKG_VERSION
}
#[wasm_bindgen(start)] pub fn main() { console_error_panic_hook::set_once(); }
```

Both build scripts run the F12 step 6 command and must be equivalent. The
PowerShell script is the primary (Windows host); the `.sh` is for CI/Linux.

**Acceptance criteria**
- `pwsh scripts/build-wasm.ps1` exits 0 and produces
  `web/src/wasm/sailgym_wasm.js` and `web/src/wasm/sailgym_wasm_bg.wasm`.
- `web/src/wasm/` is listed in `.gitignore` (generated artefact).
- `Sim::new("{}")` returns `Ok`; `Sim::new("not json")` returns `Err` — asserted
  by a `wasm-bindgen-test` in `crates/sailgym-wasm/tests/boundary.rs`.
- `console_error_panic_hook` is wired so a Rust panic surfaces as a JS console
  error, not as `unreachable`.

---

### 1.3 — Vite + React + TypeScript app that loads the WASM
**P-group: A**
**Owns:** `web/package.json`, `web/tsconfig.json`, `web/vite.config.ts`, `web/index.html`, `web/src/main.tsx`, `web/src/App.tsx`, `web/src/sim/loadWasm.ts`
**Depends:** 1.1 (runs in parallel with 1.2 against the F8.2 contract)

Stack exactly as F12. `vite.config.ts` includes `vite-plugin-wasm` and
`vite-plugin-top-level-await`.

```ts
// web/src/sim/loadWasm.ts
export interface WasmModule { Sim: new (configJson: string) => SimHandle }
export interface SimHandle {
  advance(n: number): number
  snapshot(): Float64Array
  version(): string
  free(): void
}
/** Idempotent: repeated calls return the same initialised module. */
export async function loadWasm(): Promise<WasmModule>
```

`App.tsx` renders, once the module is ready, a status element:

```tsx
<div data-testid="wasm-status" data-ready="true">sailgym {version}</div>
```

`data-testid` and `data-ready` are the contract Playwright depends on. Do not
rename them in later sections.

**Acceptance criteria**
- `pnpm --dir web install` then `pnpm --dir web build` exit 0.
- `pnpm --dir web typecheck` (`tsc --noEmit`) exits 0 with `strict: true` in `tsconfig.json`.
- `pnpm --dir web dev` serves the app; `[data-testid="wasm-status"]` reaches
  `data-ready="true"` within 5 s of load.
- `loadWasm()` called twice returns the identical module object (asserted in 1.4).

---

### 1.4 — Playwright harness and first E2E test
**P-group: B**
**Owns:** `web/playwright.config.ts`, `web/tests/e2e/smoke.spec.ts`, `web/tests/e2e/fixtures.ts`
**Depends:** 1.2, 1.3

`playwright.config.ts` starts the Vite dev server via `webServer`, targets
`chromium`, `firefox` and `msedge` (brief §38), retries 0 locally.

`fixtures.ts` exports a shared helper used by every later spec:

```ts
/** Navigates, waits for WASM ready, and fails the test on any console error
 *  or pageerror. Every spec in this repo must use it. */
export async function gotoApp(page: Page, opts?: { scenario?: string }): Promise<void>
```

`smoke.spec.ts` asserts:
1. the app loads;
2. `[data-testid="wasm-status"]` becomes ready;
3. zero `console.error` and zero `pageerror` events (brief §42);
4. `loadWasm` idempotency via an in-page `evaluate`.

**Acceptance criteria**
- `pnpm --dir web test:e2e` exits 0 on chromium, firefox and msedge.
- Deliberately throwing inside `App.tsx` makes `smoke.spec.ts` **fail** — verify
  this once by hand, then revert. The console-error trap must be proven live,
  not assumed.
- Total E2E wall time < 60 s on a warm cache.

---

### 1.5 — Gate script, CI, and agent-facing docs
**P-group: C**
**Owns:** `scripts/check.ps1`, `scripts/check.sh`, `.github/workflows/ci.yml`, `CLAUDE.md`, `README.md`
**Depends:** 1.4

`scripts/check.*` implements the F12 chain verbatim, failing fast, printing a
clear `[n/8] <step>` banner per step.

`CLAUDE.md` is what every future agent reads on entry. It must contain:
- the gate command and what each step proves;
- a pointer to `docs/00-foundations.md` as normative;
- the file-ownership rule (F13.2);
- the "never tune a coefficient to make a scenario look better" rule (brief §43);
- the handoff-note requirement (F13.6).

`CLAUDE.md` must **not** duplicate conventions, equations or parameters — it
points at foundations. Duplication is how drift starts.

**Acceptance criteria**
- `pwsh scripts/check.ps1` exits 0 from a clean clone (after `pnpm install`).
- Each of the 8 steps is individually runnable and its failure aborts the rest.
- CI runs the same script on `ubuntu-latest`; the workflow does not re-list the
  steps (it calls `scripts/check.sh`).
- `CLAUDE.md` is < 100 lines and contains no numeric physical parameter.

---

## Section acceptance criteria

All must pass before section 02 is dispatched.

1. `pwsh scripts/check.ps1` exits 0 end to end.
2. `cargo test -p sailgym-physics` runs on the host with no WASM toolchain present.
3. `pnpm --dir web dev` serves an app that displays the WASM version string.
4. `pnpm --dir web test:e2e` green on Chrome, Edge and Firefox.
5. `cargo tree -p sailgym-physics` contains no `wasm-bindgen` and no JS-facing crate.
6. `grep -riE "rho|9\.8|1\.225|1025" crates/sailgym-physics/src --include=*.rs` matches
   only `constants.rs`.
7. `docs/progress/01-handoff.md` exists and follows F13.6.

## Risks touched

- **R5** (deck.gl / WebGL) — not yet exercised; note in the handoff whether
  `vite-plugin-wasm` and the future deck.gl bundle coexist cleanly.
- **R7** — record the exact `rustc`, `wasm-pack`, `node` and `pnpm` versions in
  the handoff note. Golden regression files in section 09 will reference them.
