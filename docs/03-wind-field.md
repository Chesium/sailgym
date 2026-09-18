# Section 03 — Wind Field and deck.gl Visualization (M2)

**Prerequisite reading:** `docs/00-foundations.md` (F6.1, F8.2, F9),
`docs/progress/02-handoff.md`, `docs/brief.md` §18, §19, §20, §39.

## Goal

A deterministic, seeded, spatially varying wind field sampled cheaply by Rust,
plus an animated deck.gl particle visualization driven by **the same field**
through a single batched call per frame. The boat still uses the scaffold force
model; wind does not yet affect it. That coupling lands in section 05.

## The single-field rule

Brief §19 and §47 both require that the displayed wind and the physically sampled
wind are the same field. That is enforced structurally: `sample_grid` and
`sample` share one implementation, and task 3.6 asserts they agree **bit for
bit**, not approximately. There is no wind code in TypeScript beyond advecting
particles through the buffer Rust produced.

---

## Tasks

### 3.1 — Deterministic RNG and the wind trait (contracts)
**P-group: S**
**Owns:** `crates/sailgym-physics/src/rng.rs`, `crates/sailgym-physics/src/environment/mod.rs`

PCG32 implemented in-crate (F9.2) — no external RNG whose algorithm may change.

```rust
#[derive(Clone, Debug)]
pub struct Pcg32 { state: u64, inc: u64 }
impl Pcg32 {
    pub fn seed_from_u64(seed: u64) -> Self;
    pub fn next_u32(&mut self) -> u32;
    pub fn next_f64(&mut self) -> f64;              // [0, 1)
    pub fn range(&mut self, lo: f64, hi: f64) -> f64;
    /// Derive an independent stream. Used so adding a consumer does not shift
    /// every later draw and silently change existing scenarios.
    pub fn stream(&self, label: u64) -> Pcg32;
}
```

The `stream` method matters: without it, adding one RNG consumer in a later
section perturbs every subsequent draw and breaks the section 09 golden
trajectories for reasons unrelated to physics. Each consumer takes a named
stream (`wind` = 1, reserved: `scenario` = 2, `noise` = 3).

`environment/mod.rs` declares the `WindField` trait **verbatim from F6.1** plus:

```rust
pub fn wind_from_bearing(speed: f64, bearing_deg: f64) -> Vec2;   // F6.1, verbatim
pub fn wind_to_bearing(w: Vec2) -> (f64, f64);                    // inverse: (speed, bearing_deg)
```

**Acceptance criteria**
- `cargo test -p sailgym-physics rng::`:
  - `pcg32_known_vector`: the first 8 `next_u32()` values from seed `42` match a
    hard-coded reference array committed in the test. This pins the algorithm
    forever; if it ever changes, this test must fail loudly.
  - `pcg32_uniformity`: 100 000 `next_f64()` draws, mean in `[0.495, 0.505]`, all in `[0, 1)`.
  - `streams_independent`: `stream(1)` and `stream(2)` from the same parent
    produce different sequences; `stream(1)` is reproducible from the same parent.
- `cargo test -p sailgym-physics environment::bearing_round_trip`:
  `wind_to_bearing(wind_from_bearing(s, b))` returns `(s, b)` within 1e-9 for
  `b` in `{0, 45, 90, 180, 270, 359}`; and `wind_from_bearing(5, 0)` — a
  northerly, blowing *from* the north — returns `(0, −5)`, i.e. pointing south.
  That last assertion is the guard against the from/toward flip.

---

### 3.2 — Wind field implementation
**P-group: A**
**Owns:** `crates/sailgym-physics/src/environment/wind.rs`
**Depends:** 3.1

Implements F6.1 exactly.

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum WindMode { Uniform, Spatial, Gust }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WindConfig {
    pub mode: WindMode,
    pub speed: f64,               // m/s,  base speed
    pub bearing_deg: f64,         // deg,  meteorological FROM direction, CW from north
    pub variation: f64,           // 0..1, perturbation amplitude as a fraction of `speed`
    pub length_scale: f64,        // m,    dominant spatial wavelength, default 120.0
    pub time_scale: f64,          // s,    dominant gust period, default 25.0
    pub modes: usize,             // K,    default 12
    pub spectral_slope: f64,      // pow,  default 1.5
}

pub struct ProceduralWind { base: Vec2, modes: Vec<WindMode3>, cfg: WindConfig }
impl ProceduralWind { pub fn new(cfg: WindConfig, seed: u64) -> Self; }
impl WindField for ProceduralWind { /* sample, sample_grid */ }
```

The perturbation is the curl of the stream function in F6.1, so the field is
divergence-free by construction. `sample` allocates nothing and is `O(K)`.

`sample_grid` **must** call the same inner scalar routine `sample` calls —
literally the same function, not a re-derivation. Write it as:

```rust
fn sample_inner(&self, x: f64, y: f64, t: f64) -> Vec2;   // the only implementation
```

**Acceptance criteria**
- `cargo test -p sailgym-physics environment::wind::`, by name:
  - `uniform_is_constant`: `Uniform` mode returns the same vector at 1000 random
    `(x, y, t)`, bit-identical, and equal to `wind_from_bearing(speed, bearing)`.
  - `deterministic_from_seed`: two `ProceduralWind` built from the same config and
    seed return bit-identical values at 1000 random points; different seeds differ.
  - `grid_matches_point`: for a 32×32 grid, every entry equals
    `sample(x, y, t)` cast to `f32` — **exact equality**, no tolerance. This is
    the single-field guarantee.
  - `divergence_free`: central-difference divergence at 500 random points is
    < 1e-6 × `speed` / `length_scale`.
  - `smoothness`: the finite-difference second derivative is bounded; no point
    in a 200×200 sweep has `|∇w|` exceeding `8 × speed / length_scale`
    (brief §18 "smooth enough to avoid numerical artifacts").
  - `variation_scales`: with `variation = 0`, `Spatial` equals `Uniform` bit-for-bit;
    with `variation = 0.3`, the sampled speed standard deviation over a
    500 m × 500 m sweep is within `[0.15, 0.45] × speed`.
  - `gust_frozen_when_time_scale_infinite`: `time_scale = f64::INFINITY` ⇒ the
    field is time-invariant.
  - `no_allocation_in_sample`: `sample` is `#[inline]`-friendly and contains no
    `Vec`, `Box` or `collect` — asserted by a source grep.
- Benchmark in `crates/sailgym-bench`: `sample` ≥ 20 M calls/s single-threaded on
  the dev machine. Record the actual figure in the handoff note.

---

### 3.3 — Batched wind sampling across the WASM boundary
**P-group: B**
**Owns:** `crates/sailgym-wasm/src/lib.rs` (wind methods only), `web/src/wind/sampleGrid.ts`
**Depends:** 3.2

```rust
pub fn sample_wind_grid(&self, x0: f64, y0: f64, dx: f64, dy: f64,
                        nx: u32, ny: u32, t: f64, out: &mut [f32]);
pub fn wind_at_boat(&self) -> Box<[f64]>;   // [wx, wy] at the boat position, for the HUD
```

TS side maintains **one** reusable `Float32Array` sized `2·nx·ny`, reallocated
only when the grid dimensions change:

```ts
export interface WindGrid {
  x0: number; y0: number; dx: number; dy: number; nx: number; ny: number
  data: Float32Array          // [wx, wy] row-major
}
export function ensureGrid(cur: WindGrid | null, bounds: Bounds, targetCells: number): WindGrid
export function sampleInto(sim: SimHandle, grid: WindGrid, t: number): void
export function bilinear(grid: WindGrid, x: number, y: number): [number, number]
```

Brief §19 is explicit: **do not perform thousands of individual JS→WASM wind
queries per frame.** One `sample_wind_grid` call per frame, period.

**Acceptance criteria**
- `pnpm --dir web test:unit windGrid`:
  - `ensureGrid` returns the identical array object when dimensions are unchanged
    (no reallocation), and a new one when they change;
  - `bilinear` at an exact grid node returns that node's value within 1e-6;
  - `bilinear` is linear along a grid row (midpoint equals the mean of neighbours).
- `web/tests/e2e/wind.spec.ts` instruments the WASM call count over 60 frames
  and asserts `sample_wind_grid` was called **≤ 60 times** and
  `sample` was never exported to JS at all.
- Grid of 128×128 (32 768 cells) samples in < 4 ms per call, measured in-browser
  and recorded in the handoff note.

---

### 3.4 — deck.gl particle wind layer
**P-group: C**
**Owns:** `web/src/wind/WindLayer.tsx`, `web/src/wind/particles.ts`, `web/src/wind/DeckOverlay.tsx`
**Depends:** 3.3

Earth-Nullschool-style animated particles (brief §19), deck.gl/WebGL only. The
dense field must never be represented as SVG DOM elements (brief §39).

```ts
export interface ParticleSystemConfig {
  count: number            // default 4000
  maxAgeFrames: number     // default 120, randomised per particle to avoid pulsing
  speedScale: number       // world metres per second per unit wind
  trailAlpha: number       // fade factor, default 0.92
}
export class ParticleSystem {
  constructor(cfg: ParticleSystemConfig)
  /** Advect all particles one frame through the grid; respawn the expired. */
  step(grid: WindGrid, dtSeconds: number, bounds: Bounds): void
  positions(): Float32Array   // [x, y] pairs, for a LineLayer or ScatterplotLayer
}
```

One `Deck` instance, created lazily, sharing the camera from
`web/src/render/Camera.ts` (section 02) so the particle layer and the SVG boat
stay registered. Particle advection reads the cached grid via `bilinear`; it does
**not** call into WASM per particle.

**Acceptance criteria**
- `pnpm --dir web test:unit particles`:
  - `step` with a uniform eastward grid moves every particle in `+x` by
    `speed × speedScale × dt` within 1e-4;
  - particles leaving `bounds` respawn inside it;
  - ages are staggered — no frame respawns more than `count / maxAgeFrames × 3`
    particles (the anti-pulsing check).
- `web/tests/e2e/wind.spec.ts`:
  - a `canvas` element exists inside `[data-testid="deck-overlay"]`;
  - `document.querySelectorAll('svg line, svg circle').length < 400` while the
    wind layer is on — proof the dense field is not in the DOM (brief §39);
  - zero WebGL context-loss events over a 10 s run (R5);
  - no console errors.
- Frame time with the wind layer enabled stays under 16.7 ms at 1× on the dev
  machine; record the measured figure.

---

### 3.5 — Wind overlays and HUD readout
**P-group: C**
**Owns:** `web/src/wind/ArrowOverlay.tsx`, `web/src/ui/WindReadout.tsx`
**Depends:** 3.3

A toggleable coarse arrow/grid overlay (brief §19 "optional arrow/grid overlay")
drawn as a deck.gl layer, not SVG. A HUD readout showing wind speed and the
**"from" bearing in degrees** at the boat, using `wind_at_boat` and
`wind_to_bearing` — the conversion is never re-implemented in TS.

**Acceptance criteria**
- `web/tests/e2e/wind.spec.ts`: toggling `[data-testid="toggle-wind-arrows"]`
  changes the deck layer count by exactly 1 in both directions.
- With a uniform `Uniform` wind at `bearing_deg = 270` (a westerly), the readout
  shows `270°` ± 1 and the arrows point **east**. Assert both — this is the
  second guard against the from/toward flip.
- `grep -rn "Math.atan2" web/src/wind/` matches only `ArrowOverlay.tsx` (for
  rotating the arrow glyph), never for computing a bearing.

---

### 3.6 — Wind test suite
**P-group: D**
**Owns:** `crates/sailgym-physics/tests/wind.rs`
**Depends:** 3.2, 3.3

Integration-level tests that belong outside the module:

| Test | Assertion |
|---|---|
| `grid_bitwise_matches_point_sweep` | 8 configs × 4 seeds × 64×64 grid, exact `f32` equality with `sample` |
| `seed_reproducibility_across_instances` | Rebuilding `ProceduralWind` from the same seed after dropping the first reproduces bit-identically |
| `stream_isolation` | Adding a second RNG consumer on `stream(3)` leaves `stream(1)` wind output bit-identical |
| `finite_over_wide_domain` | No `NaN`/`Inf` for `x, y ∈ [−1e5, 1e5]`, `t ∈ [0, 1e4]` (brief §20 numerical stability) |
| `bounded_magnitude` | `\|w\| ≤ speed × (1 + 3 × variation)` everywhere sampled |

**Acceptance criteria**
- `cargo test -p sailgym-physics --test wind` exits 0 with all 5 tests present by name.
- `stream_isolation` is written so that it would fail if a later section took its
  RNG draws from the parent generator rather than a named stream.

---

## Section acceptance criteria

1. `pwsh scripts/check.ps1` exits 0.
2. The browser shows wind visibly moving across the map (brief §46 step 2),
   animated, in all three target browsers.
3. `cargo test -p sailgym-physics --test wind` green.
4. **Single-field proof**: `grid_bitwise_matches_point_sweep` passes with exact
   equality, and the E2E call-count test shows ≤ 1 WASM wind call per frame.
5. Switching `WindMode` between `Uniform`, `Spatial` and `Gust` at runtime works
   without a reload and without a WebGL context loss.
6. No wind mathematics exists in TypeScript:
   `grep -rnE "sin\(|cos\(|stream function|curl" web/src/wind/` matches only
   `particles.ts` advection and the arrow glyph rotation.
7. Section 02's determinism suite still green — the new RNG did not perturb it.
8. `docs/progress/03-handoff.md` records: measured `sample` throughput, measured
   grid-sample time, measured frame time with the wind layer, and whether R5
   fired.

## Risks touched

- **R5** — first real exercise of deck.gl. If bundle size or context loss is a
  problem, record it now; section 08 adds more layers on top.
- **R7** — `pcg32_known_vector` is the anchor that makes later golden
  trajectories meaningful. Do not weaken it.
