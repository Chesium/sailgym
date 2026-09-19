import { describe, expect, it } from 'vitest'

import {
  bilinear,
  ensureGrid,
  sampleInto,
  type Bounds,
  type WindGrid,
  type WindSampler,
} from '../../src/wind/sampleGrid'

const BOUNDS: Bounds = { minX: -100, minY: -50, maxX: 100, maxY: 50 }

/** Fills the grid with a ramp, so every node carries a distinguishable value. */
function ramp(grid: WindGrid): void {
  for (let j = 0; j < grid.ny; j += 1) {
    for (let i = 0; i < grid.nx; i += 1) {
      const at = 2 * (j * grid.nx + i)
      grid.data[at] = i
      grid.data[at + 1] = j
    }
  }
}

describe('ensureGrid', () => {
  it('returns the identical array object when the dimensions are unchanged', () => {
    const first = ensureGrid(null, BOUNDS, 1024)
    const data = first.data
    const second = ensureGrid(first, BOUNDS, 1024)
    expect(second).toBe(first)
    expect(second.data).toBe(data)

    // …and still identical when only the origin moves, which is what happens
    // every frame as the camera tracks the boat.
    const moved = ensureGrid(first, { minX: 0, minY: 0, maxX: 200, maxY: 100 }, 1024)
    expect(moved.data).toBe(data)
    expect(moved.x0).toBe(0)
    expect(moved.y0).toBe(0)
  })

  it('allocates a new array when the dimensions change', () => {
    const first = ensureGrid(null, BOUNDS, 1024)
    const second = ensureGrid(first, BOUNDS, 4096)
    expect(second.nx * second.ny).toBeGreaterThan(first.nx * first.ny)
    expect(second.data).not.toBe(first.data)
    expect(second.data.length).toBe(2 * second.nx * second.ny)
  })

  it('covers the bounds exactly and roughly honours the cell target', () => {
    const g = ensureGrid(null, BOUNDS, 4096)
    expect(g.x0).toBe(BOUNDS.minX)
    expect(g.y0).toBe(BOUNDS.minY)
    expect(g.x0 + (g.nx - 1) * g.dx).toBeCloseTo(BOUNDS.maxX, 9)
    expect(g.y0 + (g.ny - 1) * g.dy).toBeCloseTo(BOUNDS.maxY, 9)
    expect(g.nx * g.ny).toBeGreaterThan(4096 * 0.8)
    expect(g.nx * g.ny).toBeLessThan(4096 * 1.25)
    // Wider than tall, so it gets more columns than rows.
    expect(g.nx).toBeGreaterThan(g.ny)
  })

  it('never produces a grid too small to interpolate on', () => {
    const g = ensureGrid(null, BOUNDS, 1)
    expect(g.nx).toBeGreaterThanOrEqual(2)
    expect(g.ny).toBeGreaterThanOrEqual(2)
  })
})

describe('bilinear', () => {
  it('returns the node value exactly at a node', () => {
    const g = ensureGrid(null, BOUNDS, 1024)
    ramp(g)
    for (const [i, j] of [
      [0, 0],
      [1, 3],
      [g.nx - 1, g.ny - 1],
      [g.nx - 2, 2],
    ]) {
      const [wx, wy] = bilinear(g, g.x0 + i * g.dx, g.y0 + j * g.dy)
      expect(wx).toBeCloseTo(i, 6)
      expect(wy).toBeCloseTo(j, 6)
    }
  })

  it('is linear along a row: the midpoint is the mean of its neighbours', () => {
    const g = ensureGrid(null, BOUNDS, 1024)
    ramp(g)
    // Deliberately non-uniform values, so "linear" is a real claim.
    for (let i = 0; i < g.nx; i += 1) {
      g.data[2 * i] = i * i
    }
    for (let i = 0; i + 1 < g.nx; i += 1) {
      const left = bilinear(g, g.x0 + i * g.dx, g.y0)[0]
      const right = bilinear(g, g.x0 + (i + 1) * g.dx, g.y0)[0]
      const middle = bilinear(g, g.x0 + (i + 0.5) * g.dx, g.y0)[0]
      expect(middle).toBeCloseTo((left + right) / 2, 6)
    }
  })

  it('clamps outside the grid instead of reading out of bounds', () => {
    const g = ensureGrid(null, BOUNDS, 1024)
    ramp(g)
    const corner = bilinear(g, BOUNDS.maxX + 1e6, BOUNDS.maxY + 1e6)
    expect(corner[0]).toBeCloseTo(g.nx - 1, 6)
    expect(corner[1]).toBeCloseTo(g.ny - 1, 6)
    const far = bilinear(g, -1e9, -1e9)
    expect(far).toEqual([0, 0])
  })
})

describe('sampleInto', () => {
  it('makes exactly one boundary call, passing the grid geometry through', () => {
    const g = ensureGrid(null, BOUNDS, 256)
    const seen: unknown[][] = []
    const sim: WindSampler = {
      sample_wind_grid: (...args) => {
        seen.push(args)
        const out = args[7] as Float32Array
        out[0] = 7
      },
    }
    sampleInto(sim, g, 1.5)
    expect(seen).toHaveLength(1)
    expect(seen[0].slice(0, 7)).toEqual([g.x0, g.y0, g.dx, g.dy, g.nx, g.ny, 1.5])
    // The buffer handed over is the grid's own, so the write lands in it.
    expect(g.data[0]).toBe(7)
  })
})
