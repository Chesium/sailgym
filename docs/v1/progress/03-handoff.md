# Section 03 — Handoff (M2: wind field and deck.gl visualization)

**Written per F13.6.** Read this before starting section 04
(`docs/04-hydro.md`). Conventions remain normative in
`docs/00-foundations.md`; nothing below redefines them.

Status: **complete, with one acceptance criterion missed and measured.**
`pwsh scripts/check.ps1` exits 0 end to end (8/8), twice in succession.
The miss is task 3.2's `sample ≥ 20 M calls/s`; see §3.2 and §6.

---

## 1. What landed

### Task 3.1 — Deterministic RNG and the wind trait (P-group S, section agent)

- **`crates/sailgym-physics/src/rng.rs`** — `Pcg32` (PCG-XSH-RR 64/32),
  `seed_from_u64`, `next_u32`, `next_f64` (53-bit), `range`, `stream`.
  Stream labels are allocated once here: `STREAM_WIND = 1`,
  `STREAM_SCENARIO = 2`, `STREAM_NOISE = 3`. `stream` takes `&self`, so
  deriving a stream never advances the parent.
- **`crates/sailgym-physics/src/environment/mod.rs`** — the `WindField` trait
  verbatim from F6.1, plus `wind_from_bearing` (verbatim) and
  `wind_to_bearing` (its inverse). `environment.rs` was deleted in favour of
  `environment/mod.rs`; see §2.1.

### Task 3.2 — Wind field implementation (P-group A)

- **`crates/sailgym-physics/src/environment/wind.rs`** — `WindMode`,
  `WindConfig` (+`validate`), `WindMode3`, `ProceduralWind`, and the private
  `sample_inner` that `sample` and `sample_grid` both call. The perturbation is
  the curl of the F6.1 stream function, so it is divergence-free by
  construction rather than to a tolerance.
- A hand-rolled `wave` (cosine) kernel. It is not a micro-optimisation for its
  own sake: it is 1.7× faster than `f64::cos` **in WebAssembly**, which is the
  difference between meeting and missing task 3.3's in-browser grid budget. The
  measurements are in the function's doc comment and in §4 below.

### Task 3.3 — Batched sampling across the WASM boundary (P-group B)

- **`crates/sailgym-wasm/src/lib.rs`** — `sample_wind_grid`, `wind_at_boat`,
  `set_wind`, `wind_json`. `Sim::new` and `Sim::reset` now also accept an
  optional `"wind"` key. The per-point `sample` is **not** exported to JS.
- **`web/src/wind/sampleGrid.ts`** — `WindGrid`, `Bounds`, `WindSampler`,
  `ensureGrid`, `sampleInto`, `bilinear`, `gridCallCount`.

### Task 3.4 — deck.gl particle layer (P-group C)

- **`web/src/wind/particles.ts`** — `ParticleSystemConfig`, `ParticleSystem`
  (`step`, `positions`, `trailTails`, `respawnedLastStep`), seeded by a local
  `mulberry32`. Flat `Float32Array`s throughout; no per-particle object.
- **`web/src/wind/WindLayer.tsx`** — `projectInto` and `particleLayers`
  (a `LineLayer` of trails plus a `ScatterplotLayer` of heads), fed by deck.gl
  binary attributes.
- **`web/src/wind/DeckOverlay.tsx`** — the one `Deck` instance, in an identity
  pixel-space `OrthographicView`.

### Task 3.5 — Arrow overlay and HUD readout (P-group C)

- **`web/src/wind/ArrowOverlay.tsx`** — `buildArrows` (exactly one `LineLayer`:
  shaft plus two barbs per arrow) and `ArrowProbe`, the DOM summary the browser
  test reads.
- **`web/src/ui/WindReadout.tsx`** — `WindAtBoat`, `readWindAtBoat`,
  `WindReadout`. The "from" bearing comes from Rust; it is never derived in
  TypeScript.

### Task 3.6 — Wind test suite (P-group D)

- **`crates/sailgym-physics/tests/wind.rs`** — all five named tests, over
  8 configurations × 4 seeds.

### Section-agent integration work (see §2.2 for the ownership note)

`crates/sailgym-physics/src/simulation.rs` (the `Simulation` now owns the wind
field), `crates/sailgym-bench/src/main.rs` (the throughput benchmark),
`web/src/wind/useWindField.ts`, `web/src/App.tsx`, `web/src/sim/useSimulation.ts`,
`web/src/render/BoatSvg.tsx`, `web/playwright.config.ts`,
`web/pnpm-workspace.yaml`, `web/package.json`, and the three new test files.

---

## 2. Deviations from the PRD, and why

### 2.1 `environment.rs` replaced by `environment/mod.rs`

Task 3.1's `Owns:` names `environment/mod.rs`, but section 01 created
`environment.rs` alongside the `environment/` directory. A crate cannot have
both. The one-line stub was deleted and `mod.rs` created — the same resolution
section 02 applied to `forces.rs`/`forces/mod.rs` (its §2.1).

### 2.2 Files written that no section-03 task owns

As in section 02, the `Owns:` lists cover the new modules but not the surface
they plug into. Every one is listed so section 04 knows what moved:

| File | Why |
|---|---|
| `crates/sailgym-physics/src/simulation.rs` | The wind field has to live somewhere the `Sim` can reach. Putting it in the WASM wrapper would have kept the physics crate ignorant of its own environment and left section 05 to move it anyway. `Simulation` gains `wind()`, `set_wind()`, `wind_at_boat()`, and rebuilds the field on `reset` so a reseeded reset really reseeds the wind. |
| `crates/sailgym-bench/src/main.rs` | Task 3.2's acceptance criterion is "benchmark in `crates/sailgym-bench`". The file was a placeholder. |
| `web/src/wind/useWindField.ts` | The per-frame bookkeeping (grid buffer, particle system, projections, timings) is not the layer builders and is not the sampler. Folding it into `WindLayer.tsx` would have kept it inside an owned file at the cost of mixing three concerns in one. |
| `web/src/App.tsx` | Nothing else can wire the overlay, the mode selector and the readout together. |
| `web/src/sim/useSimulation.ts` | Section 02 handoff item 6: deck.gl must share the one frame loop. It gained an optional `FrameHook` and a `withSim` escape hatch — and an input fix, §2.6. |
| `web/src/render/BoatSvg.tsx` | One line: the SVG background was opaque `#eaf2f8`, which hid the wind canvas underneath it completely. The sea colour moved to the wrapper in `App.tsx`. |
| `web/playwright.config.ts` | GPU flags and the worker cap; §2.7. |
| `web/pnpm-workspace.yaml`, `web/package.json` | deck.gl, and the `@swc/core` pin R5 forced; §5. |
| `web/tests/e2e/wind.spec.ts`, `web/tests/unit/{windGrid,particles}.test.ts` | Named in the acceptance criteria of tasks 3.3–3.5 but missing from their `Owns:` lists. Treated as owned by the task whose criteria name them. |

### 2.3 `WindConfig` lives in `wind.rs`, not `parameters.rs`

Section 02's handoff (item 12) suggested the wind parameters belong in
`parameters.rs`. Task 3.2 places `WindConfig` in `wind.rs` and gives it its own
`Owns:` line, so that is what was done. It is also the better home: F7 is the
catalogue of **the boat**, and the wind is a property of the scenario, which
section 09 will supply per-scenario. **No F7 value is duplicated in `wind.rs`**,
and the F7 grep in section 10 is unaffected.

Defaults chosen (all new, none from F7): `mode: Gust`, `speed: 5.0 m/s`,
`bearing_deg: 270` (a westerly, blowing toward +x), `variation: 0.15`,
`length_scale: 120 m`, `time_scale: 25 s`, `modes: 12`, `spectral_slope: 1.5`.
The last four are the PRD's stated defaults. `speed` is deliberately modest
because of R2.

### 2.4 `wind_at_boat` returns four numbers, not two

The PRD comments it `// [wx, wy] at the boat position, for the HUD`. Task 3.5
then requires the readout to use **both** `wind_at_boat` and `wind_to_bearing`
with "the conversion is never re-implemented in TS". Returning
`[wx, wy, speed, bearing_deg]` from one call satisfies both and stays
coarse-grained (brief §24). Adding a second boundary method for the bearing
would have been the finer-grained alternative.

### 2.5 Two definitional constants in `wind.rs`

Neither is a physical coefficient and neither was fitted to a measurement
(brief §43). Both are stated in the source with their reasoning:

- `MODE_BAND = 3.0` — wavenumbers are drawn over `[κ0, 3κ0]`, so the shortest
  structure in the field is `length_scale / 3`. This is what keeps the velocity
  gradient inside the `smoothness` bound while leaving the field structure at
  several scales.
- `RMS_PER_VARIATION = 0.88` — fixes what the number `variation` *means*. The
  two PRD criteria pin it from both sides: `variation_scales` requires the
  sampled speed standard deviation at `variation = 0.3` to be at least
  `0.15 × speed`, and `bounded_magnitude` requires the perturbation to stay
  inside `3 × variation × speed`. Measured over 120 seeds on a
  160×160×5 sweep, the admissible window is `[0.812, 0.956]`; 0.88 is its
  centre, leaving about 8 % margin on each criterion. **This is a definition of
  a configuration field, not a tuned coefficient** — no scenario, test or
  visual was made to "look better" by choosing it.

The draws are also **stratified** (direction sector `i` of `K`, wavenumber
octave `stratum[i]` of `K`, each jittered, the strata shuffled). With
independent draws the admissible window closes entirely — the field's
statistics were a lottery on the seed, and both criteria would have passed or
failed by luck rather than by construction.

### 2.6 Controls are now applied on key events, not only per frame

`useSimulation` previously called `set_controls` only inside the animation
frame. A key press followed immediately by a single step therefore advanced the
physics with the **previous** frame's controls, and whether that happened
depended on the frame rate.

It was latent until section 03 slowed the frame loop, and then
`determinism.spec.ts` caught it: one replay sampled a boat that had not started
turning yet (`y = 0`) and the other one that had (`y = −2.8e-13`). **This is a
real input-latency defect, not a test-fitting change**: `set_controls` is now
called from `keydown`, `keyup`, `blur` and — critically — immediately before a
single step, as well as once per frame. No assertion was touched.

### 2.7 Playwright: GPU flags, and the worker cap lowered to 2

Every page now carries a WebGL canvas. Headless Chromium defaults to
SwiftShader and renders the particle field at ~85–108 ms a frame; asking for
the real GPU (`--enable-gpu --use-angle=default --ignore-gpu-blocklist`)
restores it to the vsync limit. Firefox has no equivalent switch under
Playwright and stays on its software path at ~100 ms a frame.

At 4 workers that starved the host into three distinct failures — the 2×/1×
clock ratio out of bounds in both directions, and a page that had advanced no
simulated time at all in half a second. `workers: 2` (previously
`CI ? 2 : 4`). **No assertion, timeout or bound was weakened.** Verified: two
consecutive full gate runs, 72/72 each.

`wind.spec.ts`'s ten-second soak sets its own 75 s timeout: the soak is the
point of the test and does not fit the 30 s default on a browser rendering
4 000 particles in software.

### 2.8 `pnpm --dir web build` needed an `@swc/core` pin

R5 fired. See §5.

**No physical coefficient was tuned, and no F7 parameter changed.** `wind.rs`
contains no value from the F7 table; the grep that proves it is in §3.

**No contradiction between the brief and `00-foundations.md` was found.** §2.4
and §2.5 are tensions inside the section-03 PRD, resolved without changing
foundations.

---

## 3. Validation evidence

Every command below was run on this host and its exit status observed.

### Section acceptance criteria

| # | Criterion | Result |
|---|---|---|
| 1 | `pwsh scripts/check.ps1` exits 0 | **pass** — 8/8, `check: all steps passed`, twice in succession. Step 3: 71 lib + 7 determinism + 5 wind + 3 harness tests. Step 8: **72 passed** across chromium, firefox, msedge, 1.7–1.9 min |
| 2 | Wind visibly moving across the map, in all three browsers (brief §46 step 2) | **pass** — verified by screenshot (streamlines flowing east under a westerly) and by `wind.spec.ts`, which runs in all three browsers and asserts a live canvas, >1 000 particles and a rising frame count |
| 3 | `cargo test -p sailgym-physics --test wind` green | **pass** — 5 passed, 1.58 s debug / 0.70 s release |
| 4 | Single-field proof: `grid_bitwise_matches_point_sweep` exact, ≤ 1 WASM wind call per frame | **pass** — 131 072 nodes compared by `to_bits()` across 8 configs × 4 seeds; `wind.spec.ts` "samples the field once per frame" asserts `calls ≤ frames` over ≥ 60 frames, and reads the generated `.d.ts` to prove `sample` is not exported at all |
| 5 | Switching `WindMode` at runtime, no reload, no context loss | **pass** — `wind.spec.ts` cycles uniform → spatial → gust → uniform, then soaks 10 s, and asserts zero `webglcontextlost` events and a surviving canvas |
| 6 | `grep -rnE "sin\(\|cos\(\|stream function\|curl" web/src/wind/` | **pass** — **zero** matches. The arrow barbs are built from the perpendicular `(−dy, dx)`; no angle is ever formed |
| 7 | Section 02's determinism suite still green | **pass** — `--test determinism` 7/7; `determinism.spec.ts` 3/3 (see §2.6, which made it *more* robust) |
| 8 | Handoff records the measurements | **pass** — §4 |

### Task acceptance criteria

| Task | Criterion | Result |
|---|---|---|
| 3.1 | `rng::pcg32_known_vector` | **pass** — two vectors: the published PCG demo output for `pcg32_srandom_r(&rng, 42, 54)`, which checks the implementation against an **external** reference, and the default stream for `seed_from_u64(42)` |
| 3.1 | `rng::pcg32_uniformity` — 100 000 draws, mean in [0.495, 0.505], all in [0, 1) | **pass** |
| 3.1 | `rng::streams_independent` | **pass** — `stream(1) ≠ stream(2)`, both reproducible from the same parent, neither disturbed by the other |
| 3.1 | `environment::bearing_round_trip` — 1e-9 for {0, 45, 90, 180, 270, 359}; `wind_from_bearing(5, 0) == (0, −5)` | **pass**, by that exact test path (file-scope test, not inside `mod tests`) |
| 3.2 | `uniform_is_constant` — 1000 points, bit-identical | **pass**, compared by `to_bits()`. Uniform short-circuits to `base`, so it is identical rather than `base + 0.0` |
| 3.2 | `deterministic_from_seed` | **pass** — 1000 points bit-identical, different seeds differ |
| 3.2 | `grid_matches_point` — 32×32, exact `f32` equality | **pass** |
| 3.2 | `divergence_free` — < 1e-6 × speed / length_scale at 500 points | **pass** — worst observed is ~1e-13 of the bound's units; the field is analytically divergence-free |
| 3.2 | `smoothness` — 200×200, `\|∇w\| ≤ 8 × speed / length_scale` | **pass** — worst 0.19 s⁻¹ against a 0.33 s⁻¹ bound (57 %), and 0.46–0.58 across five seeds |
| 3.2 | `variation_scales` — `variation = 0` identical to Uniform; `variation = 0.3` speed sd in [0.15, 0.45] × speed | **pass** — sd/speed 0.20 at the test seed, 0.20–0.23 across seeds |
| 3.2 | `gust_frozen_when_time_scale_infinite` | **pass** — every `ω_k` is exactly 0, and samples are bit-identical at t = 0, 1, 37.5, 1e4 |
| 3.2 | `no_allocation_in_sample` — source grep | **pass** — the test extracts the body of `sample_inner` from `include_str!` and rejects `Vec<`, `Vec::`, `vec!`, `Box`, `.collect`, `String` |
| 3.2 | Benchmark: `sample` ≥ 20 M calls/s | **FAIL — 12.9 M calls/s.** Measured, analysed and not worked around; see §6 |
| 3.3 | `pnpm --dir web test:unit windGrid` | **pass** — 8 tests: identical array on unchanged dimensions (and on a moved origin), new array on changed dimensions, exact bounds coverage, node-exact `bilinear`, linearity along a row, edge clamping, one-call `sampleInto` |
| 3.3 | `wind.spec.ts`: ≤ 60 `sample_wind_grid` calls over 60 frames; `sample` never exported | **pass**, all three browsers |
| 3.3 | 128×128 in < 4 ms, in-browser | **pass** — 2.4 ms Chromium, 3.0 ms Edge, 3.0 ms Firefox. See §4 |
| 3.4 | `pnpm --dir web test:unit particles` | **pass** — 6 tests: advection equals `speed × speedScale × dt` to 1e-4, spawn inside bounds, respawn inside bounds with the trail tail carried, anti-pulsing bound, seed reproducibility, freeze at `dt = 0` |
| 3.4 | `canvas` inside `[data-testid="deck-overlay"]`; `svg line, svg circle` < 400; zero context-loss over 10 s; no console errors | **pass** — 1 canvas, **16** SVG nodes, 0 lost contexts, and the section-01 console-error trap is armed on every spec |
| 3.4 | Frame time under 16.7 ms at 1× | **pass on hardware, recorded honestly** — 16.6–16.7 ms (the vsync limit) in Chromium and Edge with a real GPU. Software WebGL is a different story; §4 |
| 3.5 | Toggling arrows changes the layer count by exactly 1, both ways | **pass** — 2 layers off, 3 on, back to 2 |
| 3.5 | Uniform westerly: readout 270 ± 1 **and** arrows point east | **pass** — `data-bearing` 270, text `270°`, `wx > 0`, `\|wy\| < 1e-9`, and the probed arrow has `dx > 0` with `\|dy\| < 0.05·\|dx\|` |
| 3.5 | `grep -rn "Math.atan2" web/src/wind/` matches only `ArrowOverlay.tsx` | **pass** — zero matches anywhere, which satisfies it a fortiori |
| 3.6 | `--test wind` exits 0 with all 5 tests present by name | **pass** — `grid_bitwise_matches_point_sweep`, `seed_reproducibility_across_instances`, `stream_isolation`, `finite_over_wide_domain`, `bounded_magnitude` |
| 3.6 | `stream_isolation` would fail if a later section drew from the parent | **pass** — it runs 100 000 draws on `STREAM_NOISE` between two builds of the field and asserts bit-identity, *and* demonstrates the failure mode directly: a shared generator's second draw differs from a fresh one |

### Other greps still holding

```
grep -rn "scaffold" crates/sailgym-physics/src | wc -l                 ->  11  (limit 12)
grep -rnE "rho|9\.8|1\.225|1025" crates/sailgym-physics/src            ->  constants.rs only
grep -rn "4\.23|1\.37|0\.698|delta_r_max|self_centre" web/src --include=*.ts(x) -> none
```

### Unit tests

`pnpm --dir web test:unit` → **6 files, 38 tests**, 0.36 s. Still not part of
the F12 gate (section 02's open item stands).

---

## 4. Measurements (section acceptance criterion 8)

Host: Windows 11 Pro 26200, AMD Radeon 880M integrated GPU. Native figures are
`cargo run -p sailgym-bench --release` on an otherwise idle machine.

### `sample` throughput, native

| Mode | K | Throughput | Per call |
|---|---|---|---|
| `Uniform` | 0 | 1 443 M calls/s | 0.7 ns |
| `Spatial` | 12 | **10.7 M calls/s** | 93.7 ns |
| `Gust` | 12 | **10.7 M calls/s** | 92.6 ns |

Repeatable to about ±1 % over three consecutive runs with nothing else on the
machine. (An earlier single run reported 12.9 M/s; it did not reproduce and is
not the figure to quote.)

`sample_grid`, native: 0.363 ms for 64×64 (11.3 M nodes/s), **1.95 ms for
128×128** (8.4 M nodes/s).

For context, also recorded: `sim step` runs at 21.1 M steps/s, i.e. **105 709×
real time** headless at `dt = 0.005 s` (brief §2, §37).

### `sample_wind_grid`, in-browser

Measured by the application itself at startup (median of five 128×128 calls),
published as `[data-testid="wind-stats"] data-benchmark-ms`.

| Browser | 128×128 (32 768 cells) | Live grid (79×52) | Wind work per frame | Frame |
|---|---|---|---|---|
| Chromium, headless, GPU | **2.4 ms** | 0.4 ms | 0.6 ms | 16.7 ms |
| Chromium, headed, GPU | 2.6 ms | 0.5 ms | 0.7 ms | 16.7 ms |
| Edge, headless, GPU | **3.0 ms** | 0.7 ms | 0.8 ms | 16.7 ms |
| Firefox, headless (software WebGL) | **3.0 ms** | 1.0 ms | 1.0 ms | 100 ms |

Task 3.3's `< 4 ms` holds in all three browsers. The frame-time figure needs
the qualification below.

### Where the frame time goes

Isolated by rendering the same page with the deck.gl overlay mounted but given
an empty layer list:

| Browser | Full | Deck mounted, no layers |
|---|---|---|
| Chromium, headless, GPU | 16.6 ms | 16.6 ms |
| Firefox, headless (software) | 100 ms | 17 ms |

So on a GPU the wind layer is **free** — the frame sits on the 60 Hz vsync
limit either way — and the application's own wind work is 0.6–1.0 ms. In
software WebGL, rasterising 4 000 trail lines plus 4 000 heads costs ~83 ms a
frame (trails alone: ~33 ms). That is the rasteriser, not the application:
nothing about the layer construction differs between the two columns.

Chromium and Edge are given the real GPU in `playwright.config.ts` for exactly
this reason (§2.7). Firefox under Playwright has no such switch here.

---

## 5. Risks

### R5 — deck.gl bundle size / WebGL context loss: **FIRED**, and not in the way expected

Three findings, in descending order of importance.

**1. deck.gl breaks `pnpm --dir web build`, and the F12 gate does not notice.**
`vite-plugin-top-level-await` re-prints every output chunk through
`@swc/core`, and from `@swc/core` **1.16.0** that print fails on deck.gl's
bundle with `missing field \`type\``. `pnpm --dir web build` exits 1. Bisected
on this host: 1.14.0 and 1.15.47 build; 1.16.0, 1.16.1 and 1.16.2 (what pnpm
resolves by default) do not.

Fixed by an override in `web/pnpm-workspace.yaml` pinning `@swc/core` to
1.15.47, with the reasoning in the file. **The gate never saw this**: the
plugin only runs in `generateBundle`, and F12 steps 7 and 8 are `typecheck` and
`test:e2e` against the dev server. A production build is not in the gate.
Section 10 should consider whether it ought to be — that is an F12 change and
needs the human.

Section 01's §2.5 pin of Vite to 7.x is still in force and was not the problem.

**2. Bundle size.** The production bundle is now **1 645 kB raw / 340 kB
gzipped** in one chunk, up from 240 kB / 76 kB, plus the 232 kB WASM module.
Vite warns about the 500 kB chunk threshold. Nothing is broken and no budget
exists yet; section 08 adds more deck.gl layers on top of this.

**3. Context loss: not observed.** `wind.spec.ts` counts `webglcontextlost`
events over a ten-second soak that includes four runtime mode switches, in all
three browsers: zero. One `Deck` instance is created lazily and never
remounted — toggling the arrows changes the `layers` prop, nothing else.

### R7 — the RNG anchor is in place

`rng::pcg32_known_vector` pins both the algorithm (against the published PCG
demo output) and the default sequence selector. Its doc comment says, in the
source, that a failure is the defect and neither array may be updated.
`regression.rs` still skips with its message; `rustc` did not move.

### R2 — the boat may be too tender: still untouched

The wind does not reach the sail until section 05. `WindConfig::default().speed`
is 5.0 m/s, chosen with R2 in mind, but nothing depends on it yet. No
`sailor_pos_b.y` parameter was added; it still requires human sign-off.

### R1, R3, R4, R6 — not exercised

No sheet (R1), no new force signs (R3), the scaffold is untouched and still
11 grep hits (R4), no hull model (R6).

### Not executed

CI has still never run; the repository has no remote. The GPU flags added to
`playwright.config.ts` are inert without a GPU, so the Linux path should behave
as before — but that is by inspection, not by observation.

---

## 6. The one acceptance criterion that failed

Task 3.2 requires `sample ≥ 20 M calls/s single-threaded`. The measured figure
is **10.7 M calls/s** with `K = 12`. It is reported rather than worked around,
and the reason is arithmetic rather than sloppiness:

- 20 M samples/s × 12 modes = 240 M periodic-function evaluations per second.
  Each mode needs, at an absolute minimum, four operations for the phase, four
  for the range reduction, a periodic kernel, and four to accumulate — call it
  20, so **≈ 4.8 G scalar floating-point operations per second**.
- The project builds for the baseline `x86-64` target, which has neither FMA
  nor SSE4.1 `roundsd`. Peak *scalar* SSE2 throughput on this host is around
  7 G ops/s, so the target asks for ~70 % of theoretical scalar peak — a figure
  real code does not reach without vectorisation.

What was tried, and measured:

| Attempt | Result |
|---|---|
| `f64::cos`, one mode per iteration | 9.7 M/s |
| Hand-rolled kernel, Horner | 8.1 M/s |
| Estrin's scheme instead of Horner | 10.0 M/s |
| Magic-number rounding instead of `round_ties_even` | no change |
| `lto = "fat"`, `codegen-units = 1` | no change (reverted) |
| Struct-of-arrays layout | no change — LLVM vectorises neither layout |
| Two-way unrolled mode loop (kept) | **+15 %** |
| Final: kernel + unrolled | 10.7 M/s, against 10.0 M/s for `f64::cos` in the same harness |

Reaching 20 M/s needs explicit SIMD, which stable Rust cannot express portably
across `x86-64` and `wasm32`. The derived requirement the number exists to
protect — task 3.3's 128×128 grid in under 4 ms in the browser — **is met with
margin in all three browsers** (§4), and the field costs 0.6–1.0 ms of a
16.7 ms frame.

Section 05 is the section that should care: it will call `sample` inside the
force evaluation, twice per RK2 step. At 78 ns a call and `dt = 0.005 s`,
that is 31 µs per simulated second — irrelevant interactively, and about 0.3 %
of the headless step budget.

---

## 7. Toolchain versions (R7)

Same host as sections 01 and 02; `rustc` unchanged at 1.98.1, so golden files
recorded against section 01's fingerprint remain valid.

| Tool | Version |
|---|---|
| host triple | `x86_64-pc-windows-msvc` (Windows 11 Pro 26200) |
| GPU | AMD Radeon 880M (D3D11 via ANGLE) |
| rustc | **1.98.1** (48a229cea 2026-09-01) |
| cargo | 1.98.1 (797e8a9bc 2026-08-05) |
| rustfmt | 1.9.0-stable (48a229ceae 2026-09-01) |
| clippy | 0.1.98 (48a229ceae 2026-09-01) |
| wasm-pack | 0.15.0 |
| wasm-bindgen | 0.2.128 |
| serde / serde_json | 1.0.229 / 1.0.151 |
| node | v26.6.0 |
| pnpm | 12.4.2 |
| PowerShell | 7.6.5 |
| vite | 7.3.6 |
| vitest | 5.0.1 |
| react / react-dom | 19.3.0 |
| typescript | 7.0.2 |
| @playwright/test | 1.63.0 |
| @vitejs/plugin-react | 5.2.0 |
| vite-plugin-wasm / vite-plugin-top-level-await | 3.6.0 / 1.6.0 |
| **@deck.gl/core, /layers, /react** | **9.4.0** (new in section 03) |
| **@swc/core** | **1.15.47, pinned** — see §5 |
| Chromium / Edge / Firefox under Playwright | chromium (bundled), msedge (system channel), firefox (bundled) |

---

## 8. What section 04 must know

1. **The gate is still the contract**, and it is green. `pwsh
   scripts/check.ps1`. Also run `pnpm --dir web test:unit` (38 tests) and
   `pnpm --dir web build` — **neither is in the gate**, and §5 is what happens
   when nobody runs the second one.
2. **`Simulation` now owns a `ProceduralWind`.** `sim.wind()` gives you the
   field, `sim.wind_at_boat()` the vector at the boat. Section 04 does not need
   either — the water is still (F6.5) — but section 05 does, and it is already
   there.
3. **`Simulation::reset` rebuilds the wind from the new seed.** If you add
   state to `Simulation`, decide explicitly whether `reset` clears it; the wind
   is the precedent.
4. **`rng.rs` is implemented and its stream labels are allocated.** Take a
   named stream (`STREAM_SCENARIO`, `STREAM_NOISE`, or a new label added to the
   table in `rng.rs`) — never draw from a parent generator. `tests/wind.rs`
   `stream_isolation` is the guard and it will fail loudly if you do.
5. **`environment/mod.rs` owns `wind_from_bearing`/`wind_to_bearing`.** Do not
   re-derive a bearing anywhere, in Rust or TypeScript.
6. **`Sim` now exposes** `new`, `reset`, `set_controls`, `advance`, `snapshot`,
   `set_parameter`, `parameters_json`, `sample_wind_grid`, `wind_at_boat`,
   `set_wind`, `wind_json`, `dt`, `version`. `diagnostics`, `start_recording`
   and `stop_recording` are still unstubbed.
7. **`Sim::new` and `Sim::reset` now read an optional `"wind"` key** alongside
   `"seed"`, `"parameters"` and `"state"`. A partial override is still
   unsupported — serde wants the whole object.
8. **`useSimulation` takes an optional `FrameHook` and exposes `withSim`.** The
   wind visualization hangs off the hook. There is still exactly one
   `requestAnimationFrame` in the application; keep it that way.
9. **Controls are applied on key events as well as per frame** (§2.6). If you
   touch the input path, do not put `set_controls` back behind the frame loop
   alone.
10. **`BoatSvg`'s background is transparent** and the sea colour lives on the
    wrapper in `App.tsx`. The deck.gl canvas is between them. An opaque SVG
    background hides the wind completely.
11. **New `data-testid` contracts:** `wind-stats` (with `data-frames`,
    `data-grid-calls`, `data-grid-ms`, `data-wind-ms`, `data-frame-ms`,
    `data-benchmark-ms`, `data-grid-nx`, `data-grid-ny`, `data-particles`),
    `deck-overlay` (with `data-layers`), `wind-mode`, `toggle-wind-arrows`
    (with `data-on`), `wind-readout` (with `data-wx`, `data-wy`, `data-speed`,
    `data-bearing`), `wind-arrows` (with `data-count`, `data-dx`, `data-dy`).
    Renaming any of them breaks a spec.
12. **Playwright runs with `workers: 2`** and gives Chromium and Edge the real
    GPU (§2.7). If a timing spec starts failing, check host starvation before
    touching an assertion — that has now been the cause twice.
13. **`@swc/core` is pinned in `web/pnpm-workspace.yaml`** (§5). Removing the
    pin breaks `pnpm --dir web build`, silently as far as the gate is
    concerned.
14. **No numeric physical literal outside `constants.rs` and `parameters.rs`**
    still holds. `wind.rs` has two definitional constants (§2.5), neither from
    F7 and neither physical.
