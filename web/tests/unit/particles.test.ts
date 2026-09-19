import { describe, expect, it } from 'vitest'

import { DEFAULT_PARTICLES, ParticleSystem } from '../../src/wind/particles'
import { ensureGrid, type Bounds, type WindGrid } from '../../src/wind/sampleGrid'

const BOUNDS: Bounds = { minX: -100, minY: -100, maxX: 100, maxY: 100 }

/** A grid with the same `[wx, wy]` at every node. */
function uniformGrid(wx: number, wy: number, bounds = BOUNDS): WindGrid {
  const g = ensureGrid(null, bounds, 256)
  for (let i = 0; i < g.data.length; i += 2) {
    g.data[i] = wx
    g.data[i + 1] = wy
  }
  return g
}

describe('ParticleSystem.step', () => {
  it('advects every particle by speed × speedScale × dt', () => {
    const speed = 5
    const speedScale = 1.7
    const dt = 0.016
    // A lifetime no particle can reach, so advection is measured alone.
    const p = new ParticleSystem({ ...DEFAULT_PARTICLES, count: 500, maxAgeFrames: 1e6, speedScale })
    const grid = uniformGrid(speed, 0)

    p.step(grid, dt, BOUNDS)
    const before = Float32Array.from(p.positions())
    p.step(grid, dt, BOUNDS)
    const after = p.positions()

    expect(p.respawnedLastStep()).toBe(0)
    for (let i = 0; i < after.length; i += 2) {
      expect(after[i] - before[i]).toBeCloseTo(speed * speedScale * dt, 4)
      expect(after[i + 1] - before[i + 1]).toBeCloseTo(0, 4)
    }
  })

  it('starts every particle inside the bounds', () => {
    const p = new ParticleSystem({ ...DEFAULT_PARTICLES, count: 400 })
    p.step(uniformGrid(0, 0), 0.016, BOUNDS)
    const pos = p.positions()
    for (let i = 0; i < pos.length; i += 2) {
      expect(pos[i]).toBeGreaterThanOrEqual(BOUNDS.minX)
      expect(pos[i]).toBeLessThanOrEqual(BOUNDS.maxX)
      expect(pos[i + 1]).toBeGreaterThanOrEqual(BOUNDS.minY)
      expect(pos[i + 1]).toBeLessThanOrEqual(BOUNDS.maxY)
    }
  })

  it('respawns particles that leave the bounds, inside them', () => {
    // Fast enough that everything crosses the whole box in one step.
    const p = new ParticleSystem({
      ...DEFAULT_PARTICLES,
      count: 300,
      maxAgeFrames: 1e6,
      speedScale: 1,
    })
    const grid = uniformGrid(1e4, 0)
    p.step(grid, 1, BOUNDS)
    expect(p.respawnedLastStep()).toBe(p.count)

    const pos = p.positions()
    const prev = p.trailTails()
    for (let i = 0; i < pos.length; i += 2) {
      expect(pos[i]).toBeGreaterThanOrEqual(BOUNDS.minX)
      expect(pos[i]).toBeLessThanOrEqual(BOUNDS.maxX)
      // The trail tail moves with the particle; otherwise a respawn draws a
      // line right across the view.
      expect(prev[i]).toBe(pos[i])
      expect(prev[i + 1]).toBe(pos[i + 1])
    }
  })

  it('staggers ages so no single frame recycles the population', () => {
    const cfg = { ...DEFAULT_PARTICLES, count: 4000, maxAgeFrames: 120 }
    const p = new ParticleSystem(cfg)
    const grid = uniformGrid(0.5, 0.25)
    const limit = (cfg.count / cfg.maxAgeFrames) * 3

    let worst = 0
    // Three full lifetimes: long enough for any hidden synchronisation to
    // show up as a pulse.
    for (let frame = 0; frame < 3 * cfg.maxAgeFrames; frame += 1) {
      p.step(grid, 0.016, BOUNDS)
      worst = Math.max(worst, p.respawnedLastStep())
    }
    expect(worst).toBeLessThanOrEqual(limit)
    expect(worst).toBeGreaterThan(0)
  })

  it('is reproducible from its seed and differs between seeds', () => {
    const cfg = { ...DEFAULT_PARTICLES, count: 200 }
    const run = (seed: number) => {
      const p = new ParticleSystem(cfg, seed)
      const grid = uniformGrid(1, 0.5)
      for (let i = 0; i < 10; i += 1) {
        p.step(grid, 0.016, BOUNDS)
      }
      return Array.from(p.positions())
    }
    expect(run(11)).toEqual(run(11))
    expect(run(11)).not.toEqual(run(12))
  })

  it('freezes when no simulated time passes', () => {
    const p = new ParticleSystem({ ...DEFAULT_PARTICLES, count: 100, maxAgeFrames: 1e6 })
    const grid = uniformGrid(9, -4)
    p.step(grid, 0.016, BOUNDS)
    const before = Float32Array.from(p.positions())
    p.step(grid, 0, BOUNDS)
    expect(Array.from(p.positions())).toEqual(Array.from(before))
  })
})
