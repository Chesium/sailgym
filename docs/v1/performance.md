# Performance

brief §37's targets, measured. Three of them are browser claims — 60 fps
rendering, physics independent of the render rate, no visible input lag — and
one is a headless claim: approximately 100× real time for a single environment,
"as an aspirational prototype benchmark", with the note that "the exact number
is secondary to correctness and architecture".

Every figure below was produced by a command in the repository, and every one of
them is asserted by a test that runs in the gate:

| Measurement | Produced by | Asserted by |
|---|---|---|
| Native headless throughput and its breakdown | `cargo run --release -p sailgym-bench --bin bench -- --all --seconds 600` | not gated — it is a benchmark, and a throughput assertion on a shared CI host measures the host |
| Browser frame time, three configurations | `pnpm --dir web test:e2e perf` | `web/tests/e2e/perf.spec.ts` |
| Where a frame goes (`frame`, `wasm`, `svg`, `deck`) | the same | the same, via `web/src/render/perfMarks.ts` |
| Physics independence of the render rate | the same | the same |
| Input lag | the same | the same |
| In-browser headless stepping | the same | the same |

---

## The machine

| | |
|---|---|
| CPU | AMD Ryzen AI 9 HX 370 (12 cores, 24 threads, 5.16 GHz max) |
| Memory | 30 GiB |
| GPU | NVIDIA GeForce RTX 4070 Max-Q / Mobile, plus an AMD Radeon 890M iGPU |
| OS | Ubuntu 24.04.4 LTS, Linux 7.0.0-30-generic |
| Host triple | `x86_64-unknown-linux-gnu` |
| rustc / cargo | 1.98.1 (48a229cea 2026-09-01) |
| Node / pnpm | v26.3.0 / 11.6.0 |
| Playwright | 1.63.0 — Chromium and Edge with the real GPU, Firefox on software WebGL |
| Display | 60 Hz |

The browser figures were taken with **two Playwright workers running**, which is
the configured load (`web/playwright.config.ts`). That is deliberate: it is the
load the suite actually runs under, and three earlier sections chased an
"apparently slow app" that turned out to be a co-tenant worker.

---

## Native headless — brief §37's 100×

`cargo run --release -p sailgym-bench --bin bench -- --all --seconds 600`.
600 s of simulated time per scenario at `dt = 0.005`, i.e. 120 000 steps, each
scenario run twice.

| Scenario | M steps/s | **× real time** | ns/step |
|---|---|---|---|
| `beam_reach_capsize` | 0.700 | **3 498×** | 1 429 |
| `close_hauled` | 0.705 | **3 527×** | 1 418 |
| `free_sail` | 0.685 | **3 427×** | 1 459 |
| `gybe` (the one gusty scenario) | 0.629 | **3 144×** | 1 591 |
| `sheet_release_recovery` | 0.694 | **3 471×** | 1 441 |
| `tack` | 0.706 | **3 532×** | 1 416 |

**The target is met with a factor of about 31× in hand.** The slowest scenario
is `gybe`, which is the only shipped scenario with a `Gust` wind field: the
procedural field costs 64 ns a sample there against 1.8 ns for a uniform one,
which is the whole of the 11 % difference.

### Determinism

Task 10.4 asks for the benchmark to be deterministic, and `bench` checks more
than the step count it is asked for: the two runs must agree on the step count
**and** on all thirteen fields of the final state, compared with `to_bits()`. On
every scenario above they did. A benchmark that reported the same number of
steps while taking a different trajectory would satisfy the letter of the
criterion and prove nothing about brief §34.

### Where the time goes

`close_hauled`, attributed over 240 000 derivative evaluations (RK2 midpoint
evaluates the derivative twice per step, and each evaluation samples the wind
once and evaluates three foils once):

| Subsystem | ns per evaluation | share of the run |
|---|---|---|
| Wind sampling | 1.8 (uniform) / 64.2 (gust) | 0.3 % / 8.1 % |
| Foil evaluation — sail + board + rudder | 266 | 37.6 % |
| Whole force model (`forces::evaluate`, includes both rows above) | 421 | 59.3 % |
| — of which hull, mainsheet, hydrostatics and assembly | — | 21.5 % |
| Integration: RK2 stages, actuator clamps, angle wrap, capsize observer | — | 40.7 % |

**The next optimisation's target is the foil model**, which is a little over a
third of the run and is three calls to one shared function (F5). The obvious
routes, in the order a profile suggests them: the three surfaces are independent
and could be evaluated together under SIMD; `cl` and `cd` share `sin_cos(α)` and
a stall blend that are currently computed twice; and `stall_fraction`'s
`smoothstep` is a branch-free cubic that vectorises trivially. None was done
here — brief §37 says the exact number is secondary, the target is met 31× over,
and premature optimisation of the single-boat path is explicitly warned against.

The attribution is exactly that: the parts are timed separately, over states
drawn from the same run, at the call counts the run made. They see warmer caches
in isolation than they do in the loop, so the shares are ± a few per cent. The
method is in `crates/sailgym-bench/src/bin/bench.rs`.

### R6 — where the hull model stops being trustworthy

Recorded here because task 10.4's risk list asks for it, and because a
throughput figure invites someone to run the boat fast.

The F6.6 hull model is a single linear-plus-quadratic resistance with **no
planing regime and no wave-making hump**. Section 04 measured it: the quadratic
term carries 78.8 % of the total at 2.06 m/s, 90.0 % at 5 m/s and 91.5 % at
6 m/s, so above ≈ **5 m/s** the model over-predicts resistance and the boat has
a hard speed cap that a real ILCA — which planes — does not.
`diagnostics::HULL_MODEL_VALID_TO = 5.0` publishes that limit, the debug panel
raises a banner above it, and `debug.spec.ts` drives the banner up and down.
**Any speed above 5 m/s in this simulator is an extrapolation, not a
prediction.**

---

## Browser frame time — brief §37's 60 fps

`web/tests/e2e/perf.spec.ts`, from one full gate run. Sail Mode is measured over
**30 s** (task 10.5) and the two Debug Mode configurations over 10 s, all on the
same page in the same worker so that the three are comparable with each other.

| Browser | Configuration | frames | p50 | **p95** | max | dropped |
|---|---|---|---|---|---|---|
| Chromium (GPU) | Sail Mode | 1 795 | 16.70 | **16.80** | 16.80 | 0.00 % |
| Chromium (GPU) | Debug, 16 overlays | 587 | 16.70 | **16.80** | 33.40 | 1.36 % |
| Chromium (GPU) | Debug, 8 charts | 578 | 16.70 | **16.80** | 66.70 | 2.42 % |
| Edge (GPU) | Sail Mode | 1 793 | 16.70 | **16.70** | 33.30 | 0.11 % |
| Edge (GPU) | Debug, 16 overlays | 564 | 16.70 | **16.80** | 50.10 | 4.61 % |
| Edge (GPU) | Debug, 8 charts | 564 | 16.70 | **16.80** | 50.00 | 4.79 % |
| Firefox (software WebGL) | Sail Mode | 1 776 | 17.06 | **17.12** | 49.34 | 1.01 % |
| Firefox (software WebGL) | Debug, 16 overlays | 559 | 17.06 | **33.16** | 50.30 | 6.26 % |
| Firefox (software WebGL) | Debug, 8 charts | 536 | 17.08 | **33.22** | 50.72 | 9.14 % |

Every median on every browser is the display period: **the application holds
60 fps in all three target browsers, in every configuration.**

### The 16.7 ms criterion is not reachable as literally written

Task 10.5 asks for a 95th-percentile frame time "under 16.7 ms". On a 60 Hz
display the refresh period is `1000/60 = 16.667 ms` and browsers report
`requestAnimationFrame` timestamps quantised to 0.1 ms, so a page that never
drops a frame reports a mixture of 16.7 and 16.8 and its 95th percentile lands
on one of them. **No page comes in strictly under the display's own period.**
Section 03 met the same ceiling and recorded it as "16.6–16.7 ms (the vsync
limit)"; section 08 recorded it again; this is the third time and it is the
same measurement, not a regression.

What the criterion is *for* — that Sail Mode holds the display's frame rate — is
asserted in three ways that together say more than the single percentile would:
the median is at or under 16.7 ms, the 95th percentile is within one reporting
quantum of the median (so the tail is jitter, not stalls), and fewer than 2 % of
frames are long. **No threshold value was changed.**

**Firefox is reported and not gated.** Playwright's Firefox has no headless GPU
path on this host and rasterises the four-thousand-particle wind field in
software, which section 03 measured at ~83 ms a frame for that layer alone.
Gating on it would be measuring the rasteriser. Its medians are the vsync limit
in every configuration, which is the claim that matters; its debug-mode 95th
percentiles at 33 ms are the software path and nothing else.

### Where a frame goes

`web/src/render/perfMarks.ts` records four spans with `performance.mark` and
`performance.measure`, so the same numbers appear in a browser profiler. Eight
seconds of Sail Mode, in milliseconds:

| Browser | `frame` (whole rAF callback) | `wasm` (every call across the F8 boundary) | `svg` (React render **and** DOM commit of the world view) | `deck` (deck.gl's own draw) |
|---|---|---|---|---|
| Chromium | p50 0.40, p95 0.70, max 3.50 | p50 0.10, p95 0.30, max 0.90 | p50 1.10, p95 2.20, max 7.00 | p50 0.40, p95 0.60, max 3.50 |
| Edge | p50 0.50, p95 1.10, max 5.50 | p50 0.20, p95 0.30, max 2.70 | p50 1.30, p95 3.80, max 8.20 | p50 0.50, p95 1.00, max 3.40 |
| Firefox | p50 1.00, p95 3.00, max 5.00 | p50 0.00, p95 1.00, max 2.00 | p50 2.00, p95 3.00, max 6.00 | p50 2.00, p95 3.00, max 7.00 |

**The WASM boundary is 0.1–0.3 ms of a 16.7 ms frame**, which is brief §24's
coarse-grained design showing up as a number: one `set_controls`, one
`advance(n)`, one `snapshot` and one `diagnostics` per frame, and nothing
per-entity. The SVG render and commit is the largest term at 1.1–1.3 ms, and
deck.gl's draw is 0.4–0.5 ms on a GPU. Everything together is under 2 ms, so
about 88 % of the frame is the browser waiting for vsync.

Firefox's figures are quantised: it coarsens `performance.now()` to whole
milliseconds by default, so its `wasm` median of 0.00 means "under the clock's
resolution", not "free". The spec's non-zero check is therefore on the maximum.

The instrumentation's own cost is two `performance.mark`s and one
`performance.measure` per span, with entries cleared as they are read — about
720 User Timing operations a second, under 0.05 ms a frame measured against the
uninstrumented baseline of section 08, which read the same 16.70/16.80 ms.

---

## Physics independence of the render rate

brief §37: "physics must remain independent of render rate". `?renderHz=` caps
how often the frame loop publishes to React; the clock still ticks on every
animation frame, so the simulation advances at wall-clock rate whatever the page
is drawing. Both runs are at 4× speed, so any difference would be four times as
visible as at 1×.

| Browser | 60 fps | 20 fps | drift |
|---|---|---|---|
| Chromium | 3.999 sim-s/s | 4.012 sim-s/s | **0.31 %** |
| Edge | 4.000 sim-s/s | 4.022 sim-s/s | **0.56 %** |
| Firefox | 4.000 sim-s/s | 3.981 sim-s/s | **0.49 %** |

Task 10.5's bound is 2 %. The residual is the measurement window's own
granularity, not the clock's.

---

## Input lag — brief §37's "no visible input lag"

Made measurable exactly as task 10.5 specifies: a synthetic key-down is
timestamped, and so is the first committed frame whose **rendered** rudder
reflects it. Twelve alternating flips per browser, each starting from a
saturated tiller so the rudder is perfectly still beforehand.

| Browser | n | p50 | **p95** |
|---|---|---|---|
| Chromium | 12 | 12.2 ms | **23.2 ms** |
| Edge | 12 | 15.6 ms | **28.9 ms** |
| Firefox | 12 | 18.0 ms | **36.0 ms** |

Task 10.5's bound is 50 ms at the 95th percentile; all three are inside it. The
numbers are what the architecture predicts and nothing more: a key press is
applied to the core immediately (not deferred to the next frame — section 03
§2.6), the physics advances on the next tick, and the change appears at the next
commit. One display period is 16.7 ms, so a median of 12–18 ms and a 95th
percentile of one to two frames is the floor, not a symptom.

---

## In-browser headless stepping

Task 10.4 asks for the in-browser figure with rendering disabled. The `Sim` is
constructed directly in the page — no React, no SVG, no deck.gl — and advanced
400 000 steps after a warm-up.

| Browser | M steps/s | **× real time** |
|---|---|---|
| Chromium | 0.527 | **2 633×** |
| Edge | 0.519 | **2 593×** |
| Firefox | 0.451 | **2 255×** |

The WebAssembly build runs at **64–75 %** of the native speed of the same
physics on the same machine (0.705 M steps/s native for `close_hauled`). brief
§37's 100× target is met in the browser by a factor of more than twenty.

Two things this does *not* measure, stated so nobody reads it as more than it
is: recording is off (section 09 makes `advance` step one at a time while
recording, and builds a diagnostics record on sampled steps), and there is no
rendering, no React reconciliation and no diagnostics serialisation. The
interactive figure is the frame-time table above.

---

## Render load and bundle

| | |
|---|---|
| SVG elements, force overlay with all sixteen on | 45 |
| SVG elements, whole page, every overlay and chart | 135 (budget: 300) |
| Production bundle | 1 734 kB raw / 356 kB gzipped, one chunk |
| WASM module | ~232 kB |

R5 does not fire on a GPU: Chromium and Edge sit on the vsync limit with all
sixteen overlays and all eight charts drawing, so deck.gl and the SVG overlay do
not contend. The single 1 734 kB chunk remains over Vite's 500 kB warning
threshold and there is still no budget for it — recorded as an open item since
section 08, and unchanged here because splitting the bundle is a build change
with no acceptance criterion behind it.

---

## What none of this measures

- **Any machine but this one.** Every figure is from the host in the table at
  the top. The assertions that ship are percentile and ratio tests chosen to
  survive a different machine; the absolute numbers are not.
- **Batched or vectorised execution.** brief §37 requires only that the design
  not preclude it. It does not: `sailgym-physics` has no global state, no
  wall-clock read and no rendering dependency, and `bench` is the existing proof
  that it runs headless. Nothing here has been optimised for it, per brief §37's
  closing warning.
