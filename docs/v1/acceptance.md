# Acceptance — brief §47's fourteen success criteria

One row per criterion, with the test, command or recorded demonstration that
evidences it and an honest verdict. Where a criterion is partly met, it says so
and says which part; brief §47 is the specification's own definition of done and
a generous reading of it would be worth nothing to whoever reads this next.

Verdicts used: **Met** — evidenced by something that runs. **Met, with a stated
limit** — the criterion holds, and a specific boundary on it is recorded.
**Partly met** — a material part is not evidenced.

Commands are run from the repository root. `pwsh scripts/check.ps1` is the gate
(F12); `scripts/check.sh` is its Linux equivalent and is what section 10 ran.

---

| # | brief §47 criterion | Evidence | Verdict |
|---|---|---|---|
| 1 | it is interactive and enjoyable enough to manually experiment with | The three brief §46 demonstrations are driven through the **real controls** in a real browser — a held mouse drag on the boat for the mainsheet, `A`/`D` for the tiller, `Space` for the release — in `web/tests/e2e/demonstrations.spec.ts`, on Chromium, Firefox and Edge. Interactivity is measured rather than asserted: 60 fps medians in all three browsers and 12–29 ms of input lag at the 95th percentile (`docs/performance.md`). Six scenarios, a scenario picker, recording and replay, four clock speeds and two camera modes all work (`scenarios.spec.ts`, `replay.spec.ts`, `clock.spec.ts`, `render.spec.ts`). | **Partly met.** Everything that can be measured about interactivity is measured and is good. *Enjoyable* has not been assessed: no sailor has sat down with it, and two recorded findings bound what there is to enjoy — `stability.gm` makes the boat impossible to sail past ≈ 86° of heel or to invert (section 07 §4), and `l_sheet_min` puts 2.8 kN of permanent pre-tension in a fully hauled sheet (section 06 §4). Both need a human decision, not more tests. |
| 2 | sail angle emerges from physics rather than direct command | `β` is a dynamic degree of freedom integrated from `I_b β̈ = ΣM_β` (F4.2, F6.9). `invariants::no_sail_angle_command` greps every non-test source file in the physics crate and fails on any assignment to `.beta` outside `integrator.rs` and `state.rs`; `rigging::mainsheet::no_boom_angle_assignment` does the same for the mainsheet module. The controls are **rates**, not angles (F3), and `forces::sheet_integrated::rate_command_not_angle_command` proves the same haul command produces a different boom angle at a different wind speed. | **Met.** |
| 3 | sheet release physically depowers the sail | `forces::sheet_integrated::release_depowers` and `forces::roll_integrated::easing_reduces_heel` assert the causal chain in order in Rust; `demonstrations.spec.ts` asserts it in the browser as five ordered events — tension → 0, boom swings out, heeling moment falls, sail force falls, heel decreases. `no_shortcuts::no_release_recovery_rule` is the standing proof that nothing in the source says release causes recovery. **Verified negatively once:** disabling the sheet's contribution to the boom torque makes demonstrations 1 and 3 fail (section 10 handoff). | **Met.** |
| 4 | tacking and gybing emerge without hard-coded manoeuvre states | `no_shortcuts::no_maneuver_state` greps for any tack/gybe state in the source. `invariants::tack_through_wind` asserts the boom crosses continuously — no step above 0.3 rad in a physics step — and `demonstrations.spec.ts` asserts the full tack chain (sail unloads below `|cl| = 0.1`, `psi` passes head to wind, `β` changes sign, sail fills above `|cl| = 0.5`) and the full gybe chain (boom torque reverses, boom crosses, `|β̇|` peaks above 2 rad/s, tension spikes) in three browsers. | **Met.** |
| 5 | roll responds plausibly to sail loading | `forces::roll_integrated::sail_force_heels_boat` and `couple_from_sail_and_board` assert the sign and the source of the heeling couple; `stability::roll::free_decay_period` matches `2π√(I_x/Δ·g·GM)` to **0.78 %**; `invariants::heel_reduces_drive` proves the heel correction is geometric (F6.4) rather than an empirical `cos φ`. | **Met, with a stated limit.** The `GZ` curve the F7 stability group produces regains positive stability past ≈ 82° and peaks *larger* there than upright, so the boat lies down at ≈ 86–91° and cannot be driven over at any wind speed (section 07 §4, measured up to 50 m/s). Below 45° of heel the response is the one the parameters describe. |
| 6 | capsize can occur dynamically | `stability::capsize::*` (7 tests), `invariants::capsize_finite` (100 × 60 s episodes at 8–22 m/s, more than half of which pass the threshold), and `demonstrations.spec.ts` demonstration 1, which reaches `capsized` at `t ≈ 10 s` from a boat that started upright and was driven over by the wind. The threshold wind was measured: hauled hard in on a beam reach, ≈ **6.93 m/s** (section 07 §6). | **Met, with the same limit as row 5.** The boat capsizes and is reported capsized; it cannot be inverted. |
| 7 | the same wind field powers visualization and physics | `wind::grid_bitwise_matches_point_sweep` compares `sample_grid` against `sample` at 131 072 nodes across 8 configurations × 4 seeds **by `to_bits()`**. The per-point `sample` is not exported to JavaScript at all, which `wind.spec.ts` asserts by reading the generated `.d.ts`; the visualization gets one batched `sample_wind_grid` call per animation frame and nothing else. | **Met.** |
| 8 | the simulator is deterministic | `tests/determinism.rs` (8 tests, including the JSON round trip that section 06 found and fixed), `invariants::deterministic_replay` (two scenarios, ~600 frames, all 38 scalars bit-identical), `tests/regression.rs` (six golden trajectories, worst \|Δ\| **0** at every sample, proven sensitive to a 0.1 % coefficient change), and `determinism.spec.ts` in the browser at 0 ULP. | **Met, with the scope F9 states.** Bit-identity is claimed for the same build on the same platform only; a golden file records its toolchain and the comparison **skips with a message** on any other. Cross-platform identity is not claimed and is not true. |
| 9 | debug tooling makes force/moment behaviour inspectable | `diagnostics::covers_brief_30` maps every one of brief §30's twenty items to a named field and checks it against both the struct declaration and the published JSON. `diagnostics::matches_step_forces` proves the displayed forces are **bit-identical** to the ones the integrator consumes. Sixteen force/moment overlays and eight charts, each toggled in both directions by `debug.spec.ts`; the whole F7 catalogue is live-editable and every brief §31 example is reachable (`params.spec.ts`). | **Met.** |
| 10 | invariant tests pass | `cargo test -p sailgym-physics --test invariants` — **23 tests**, every brief §35 item present, each with a tolerance justified from a measured margin in `docs/invariants.md`. Plus `--test convergence` (3), `--test symmetry` (96 cases), `--test no_shortcuts` (5), `--test provenance` (5) and `--test regression` (6), all in gate step 4 and 5. | **Met.** |
| 11 | rendering is cleanly separated from physics | `provenance::no_physics_in_typescript` scans all 45 hand-written TypeScript sources for a density, a `g` or a `½ρV²` and finds none. Every derived physical quantity crosses the boundary in the `Diagnostics` record, whose field list is compared in both directions by `web/tests/unit/diagnostics.test.ts`. The render rate is decoupled from the physics rate, **measured**: `t` advances within 0.56 % of the same rate at 20 fps as at 60 (`docs/performance.md`). | **Met.** |
| 12 | the Rust core can run without rendering | `cargo test -p sailgym-physics` builds and runs the whole physics crate with no WASM toolchain present — proven by removing the target and the toolchain in section 01. `crates/sailgym-bench` is a native headless binary; `cargo run --release -p sailgym-bench --bin bench -- --all` runs every shipped scenario for ten simulated minutes with nothing rendering. `cargo tree -p sailgym-physics` contains no `wasm-bindgen` and no JS-facing crate. | **Met.** |
| 13 | architecture is suitable for later RL/vectorization work | The core has no global mutable state, reads no wall clock (`determinism::no_wall_clock` greps for it), owns its seed explicitly, and `evaluate` is a pure function of `(state, controls, parameters, field, t)`. A native environment steps at **3 500× real time** single-threaded; the episode format of section 09 is versioned, has a `reward` hook (asserted zero), and round-trips bit-for-bit in both JSON and binary. `advance(n)` is exactly `n` calls to `advance(1)` (F9.7). | **Met as an architecture claim; unexercised.** Nothing has been vectorised, no batched API exists and no RL loop has been run against it. brief §37 asks only that the design not preclude these, and the properties above are why it does not — but "suitable" here is an argument from structure, not a demonstration. |
| 14 | the application and key interactions are covered by Playwright tests | **243 browser tests** across Chromium, Firefox and Edge, covering every item of brief §42's list: the app loads, WASM initialises, the default scenario appears, keyboard rudder control, mouse sheet control, pause/resume, reset determinism, scenario switching, debug mode, recording and replay, accelerated simulation — and no console or page errors, enforced automatically for every spec by the fixture in `web/tests/e2e/fixtures.ts`. | **Met.** |

> brief §47's closing line — "Quantitative real-world ILCA accuracy is
> explicitly **not** required for prototype completion" — is taken at its word,
> and `docs/parameters.md` states the corresponding disclaimer parameter by
> parameter.

---

## The gate

`scripts/check.sh` (and `scripts/check.ps1`, its mirror) runs the F12 chain,
failing fast, and prints each step's wall time.

Measured on the section 10 host (`docs/performance.md` has the machine spec),
on a warm `target/` tree:

| Step | What it proves | Wall time |
|---|---|---|
| 1 `cargo fmt --check` | source is canonically formatted | < 1 s |
| 2 `cargo clippy --all-targets -- -D warnings` | no lint regressions, tests and benches included | < 1 s warm, 8 s cold |
| 3 `cargo test -p sailgym-physics` | the physics core is correct, and builds on the host with no WASM toolchain | 28 s |
| 4 `--test invariants --test no_shortcuts --test convergence --test symmetry --test provenance` | the brief §35 invariants, the prohibited-shortcut audits, convergence, the 96-case symmetry sweep and the F7 provenance audit | 16 s |
| 5 `--test regression` | the six recorded scenarios still reproduce bit-for-bit | < 1 s |
| 6 `wasm-pack build …` | the core still compiles to WASM and the glue regenerates | 7 s |
| 7 `pnpm --dir web typecheck` | TypeScript still matches the WASM surface | 1 s |
| 8 `pnpm --dir web test:e2e` | **243 tests** in Chrome, Edge and Firefox, with no console or page errors | **516 s** (8.6 min) |
| | **Total** | **568 s — 9 min 28 s** |

Step 4 gained three targets in section 10 — `convergence`, `symmetry` and
`provenance` — as task 10.8 requires; `no_shortcuts` was added by section 07.

### The fast subset

At 9 min 28 s the chain does not *exceed* task 10.8's ten minutes, but it is
close enough — and 91 % of it is step 8 — that the split was built anyway:

```
scripts/check.sh --fast        # pwsh scripts/check.ps1 -Fast
```

`--fast` runs the same eight steps with step 8 restricted to **Chromium** and to
the specs not tagged `@slow` — the browser performance measurement and the three
brief §46 demonstrations, which between them are about half the browser suite's
wall time and neither of which can be made quick without making it mean less.
It runs **74 tests in 149 s — 2 min 29 s**, measured.

**`--fast` is not the gate.** It is what to run while working. F13.7 is
unchanged: the full chain is what must be green before a section is finished,
and it is what CI runs.
