/**
 * Earth-Nullschool-style wind particles (brief section 19).
 *
 * Pure state and arithmetic: no DOM, no WebGL, no React, no WASM. The
 * rendering lives in `WindLayer.tsx`; this file is what the unit tests drive.
 *
 * Particles are advected through the **cached grid** via `bilinear`, never by
 * calling into WASM per particle (brief section 19). There is no wind model
 * here — a particle simply drifts with whatever vector the grid holds.
 *
 * Positions are kept interleaved in flat `Float32Array`s so they can be handed
 * to deck.gl as binary attributes without building four thousand objects a
 * frame.
 */

import { bilinear, type Bounds, type WindGrid } from './sampleGrid'

export interface ParticleSystemConfig {
  /** How many particles. */
  count: number
  /** Mean lifetime in frames. Randomised per particle to avoid pulsing. */
  maxAgeFrames: number
  /** World metres travelled per second per unit of wind speed. */
  speedScale: number
  /**
   * How much of the previous trail tail is kept each frame.
   *
   * The tail is an exponentially lagged copy of the head, so 0.92 leaves it
   * trailing by about `1 / (1 − 0.92) ≈ 12` frames. A one-frame trail is not
   * enough to read as wind: at 5 m/s and 20 px/m a particle moves 1.6 screen
   * pixels per frame, which draws as a stipple of dots rather than as
   * streamlines. Lagging the tail is what turns the dots into streaks —
   * without exaggerating the advection, which stays at the true wind speed.
   */
  trailAlpha: number
}

export const DEFAULT_PARTICLES: ParticleSystemConfig = {
  count: 4000,
  maxAgeFrames: 120,
  speedScale: 1,
  trailAlpha: 0.92,
}

/**
 * How densely and how strongly the field is drawn (v2 section 09, task 9.4).
 *
 * **Presentation only.** Neither number reaches `sample_wind_grid`, the
 * advection below or anything in Rust: the whole population is still stepped
 * through the same field at the same speed, and `density` decides only how
 * many of them a frame *draws*. The wind the boat feels is unchanged, and
 * `wind.spec.ts`'s once-per-frame grid call is unaffected.
 */
export interface WindVisual {
  /** Fraction of the population drawn, in `(0, 1]`. */
  density: number
  /** Multiplies the trails' and heads' alpha and the trail width, in `(0, 1]`. */
  contrast: number
}

/**
 * Sail Mode's setting.
 *
 * **Provenance.** Four thousand trails at full contrast across a phone-sized
 * view put roughly one mark every four CSS pixels, and at that spacing the
 * hull's 0.8 px stroke, the boom line and the heel of the deck read as texture
 * rather than as a boat — which is RV55 exactly. Halving the count doubles the
 * mean gap between trails, and taking the alpha down by the same factor keeps
 * the field readable as *motion* while letting the boat sit in front of it.
 *
 * `0.5` is also the largest reduction that leaves the drawn count in the
 * thousands (2000 of 4000), which is what `wind.spec.ts` asserts is really
 * being drawn rather than faked in the DOM.
 *
 * It is a visual constant and it is not tuned to make anything *look better*
 * in the sense brief §43 forbids — nothing physical moves with it, and the
 * same field is sampled either way.
 */
export const SAIL_MODE_WIND: WindVisual = { density: 0.5, contrast: 0.55 }

/**
 * Debug Mode's setting: everything, at full strength.
 *
 * Debug Mode exists to show the field and the force overlay together, and
 * someone reading a streamline pattern wants every streamline.
 */
export const DEBUG_MODE_WIND: WindVisual = { density: 1, contrast: 1 }

/**
 * How many particles a frame draws at a given density.
 *
 * The population is spawned in index order at independent random positions, so
 * the first `n` are a uniform random sample of the field rather than a patch
 * of it. At least one is always drawn, so a mistyped density cannot empty the
 * screen silently.
 */
export function visibleCount(total: number, density: number): number {
  if (!Number.isFinite(density) || density >= 1) {
    return total
  }
  return Math.max(Math.min(total, 1), Math.round(total * clamp01(density)))
}

/**
 * Per-particle lifetimes are drawn from `maxAgeFrames × [LIFETIME_LO,
 * LIFETIME_HI]`. Identical lifetimes would make the whole population expire
 * together every `maxAgeFrames` frames — a visible pulse, and the thing
 * `ages_are_staggered` exists to catch.
 */
const LIFETIME_LO = 0.6
const LIFETIME_HI = 1.4

export class ParticleSystem {
  readonly config: ParticleSystemConfig

  /** `[x, y]` pairs, world metres. */
  private readonly pos: Float32Array
  /** The lagging tail of each particle's trail, world metres. */
  private readonly tail: Float32Array
  private readonly age: Int32Array
  private readonly life: Int32Array
  private rng: number
  private spawned = false
  private lastRespawns = 0

  constructor(config: ParticleSystemConfig = DEFAULT_PARTICLES, seed = 0x5a1_1c0d) {
    this.config = config
    const n = Math.max(0, Math.floor(config.count))
    this.pos = new Float32Array(2 * n)
    this.tail = new Float32Array(2 * n)
    this.age = new Int32Array(n)
    this.life = new Int32Array(n)
    this.rng = seed >>> 0
  }

  /** Particle count, after flooring and clamping the configured value. */
  get count(): number {
    return this.age.length
  }

  /**
   * Advance every particle one frame.
   *
   * Order matters: advect first, then age, then respawn what expired or left
   * `bounds`. Advecting last would mean a particle spawned this frame is drawn
   * where it was put rather than where the wind takes it, and the very first
   * frame would move nothing.
   */
  step(grid: WindGrid, dtSeconds: number, bounds: Bounds): void {
    if (!this.spawned) {
      this.spawnAll(bounds)
      this.spawned = true
    }

    if (dtSeconds === 0) {
      // Paused: nothing advects, and the trails must not quietly collapse
      // into the heads while the clock is stopped.
      this.lastRespawns = 0
      return
    }

    const scale = this.config.speedScale * dtSeconds
    const keep = clamp01(this.config.trailAlpha)
    let respawns = 0

    for (let i = 0; i < this.count; i += 1) {
      const p = 2 * i
      const x = this.pos[p]
      const y = this.pos[p + 1]
      const [wx, wy] = bilinear(grid, x, y)

      const nx = x + wx * scale
      const ny = y + wy * scale
      this.pos[p] = nx
      this.pos[p + 1] = ny
      this.tail[p] += (nx - this.tail[p]) * (1 - keep)
      this.tail[p + 1] += (ny - this.tail[p + 1]) * (1 - keep)

      this.age[i] += 1
      const outside =
        nx < bounds.minX || nx > bounds.maxX || ny < bounds.minY || ny > bounds.maxY
      if (this.age[i] >= this.life[i] || outside) {
        this.respawn(i, bounds)
        respawns += 1
      }
    }
    this.lastRespawns = respawns
  }

  /** `[x, y]` pairs, world metres. The live buffer — do not retain it. */
  positions(): Float32Array {
    return this.pos
  }

  /** `[x, y]` pairs: the lagging tail end of each particle's trail. */
  trailTails(): Float32Array {
    return this.tail
  }

  /** How many particles the last {@link step} recycled. */
  respawnedLastStep(): number {
    return this.lastRespawns
  }

  private spawnAll(bounds: Bounds): void {
    for (let i = 0; i < this.count; i += 1) {
      this.respawn(i, bounds)
      // Stagger the initial ages across each particle's own lifetime, so the
      // population is already evenly spread through its cycle on frame one.
      this.age[i] = Math.floor(this.random() * this.life[i])
    }
  }

  /**
   * Put particle `i` somewhere new inside `bounds`, with a fresh lifetime.
   *
   * The trail tail is moved with it: leaving the tail behind would draw a line
   * from the old position clear across the view.
   */
  private respawn(i: number, bounds: Bounds): void {
    const p = 2 * i
    const x = bounds.minX + this.random() * (bounds.maxX - bounds.minX)
    const y = bounds.minY + this.random() * (bounds.maxY - bounds.minY)
    this.pos[p] = x
    this.pos[p + 1] = y
    this.tail[p] = x
    this.tail[p + 1] = y
    this.age[i] = 0
    this.life[i] = Math.max(
      1,
      Math.round(
        this.config.maxAgeFrames * (LIFETIME_LO + this.random() * (LIFETIME_HI - LIFETIME_LO)),
      ),
    )
  }

  /**
   * `mulberry32`, in `[0, 1)`.
   *
   * Seeded rather than `Math.random` so the unit tests are reproducible and a
   * recorded session replays identically (brief section 34). It decides where
   * dots appear and nothing else — no physical quantity depends on it, which
   * is why it is here and not in `rng.rs`.
   */
  private random(): number {
    this.rng = (this.rng + 0x6d2b79f5) >>> 0
    let t = this.rng
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v
}
