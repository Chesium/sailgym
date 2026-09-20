import { describe, expect, it } from 'vitest'

import { boomDirection } from '../../src/render/geometry'
import type { Vec3 } from '../../src/render/model3d'
import { rollToH } from '../../src/render/project3d'
import { rigSolid } from '../../src/render/rigSolid'
import { visualDims } from '../../src/render/visualDims'
import { ilcaParams } from './ilca'

const params = ilcaParams()
const dims = visualDims(params)

const build = (beta: number, deltaR = 0, alpha = 0) =>
  rigSolid(params, params.hull, dims, { beta, deltaR, alpha })

/** The clew is the far end of the boom line — the element the E2E specs read. */
function clewOf(beta: number): Vec3 {
  const boom = build(beta).lines.find((l) => l.testId === 'boat-boom')
  if (boom === undefined) {
    throw new Error('rigSolid emitted no boat-boom line')
  }
  return boom.b
}

const gap = (p: Vec3, q: Vec3) => Math.hypot(p.x - q.x, p.y - q.y, p.z - q.z)

describe('rigSolid', () => {
  it('clew_follows_beta', () => {
    for (const beta of [-1.2, -0.3, 0, 0.3, 1.2]) {
      const b = boomDirection(beta)
      const expected: Vec3 = {
        x: params.sail.mast_pos_b.x + params.sail.boom_length * b.x,
        y: params.sail.boom_length * b.y,
        z: params.sheet.z_boom,
      }
      expect(gap(clewOf(beta), expected)).toBeLessThan(1e-12)
    }
    // F2.1: the boom points aft, so a positive (counter-clockwise, from above)
    // `β` swings the tip to **starboard**, which is `y < 0` because `+y` is
    // port. This is the R3 sign guard, in the model rather than the DOM.
    expect(clewOf(0.3).y).toBeLessThan(0)
    expect(clewOf(-0.3).y).toBeGreaterThan(0)
  })

  it('rudder_trailing_edge_follows_delta_r', () => {
    const blade = build(0, 0.3).tris.filter((t) => t.group === 'rudder')
    expect(blade.length).toBeGreaterThan(0)
    const ys = blade.flatMap((t) => [t.a.y, t.b.y, t.c.y])
    // F2.2: `δr > 0` deflects the trailing edge to starboard.
    expect(Math.min(...ys)).toBeLessThan(0)
    expect(Math.max(...ys)).toBe(params.rudder.pos_b.y)

    const mirrored = build(0, -0.3).tris.filter((t) => t.group === 'rudder')
    expect(Math.max(...mirrored.flatMap((t) => [t.a.y, t.b.y, t.c.y]))).toBeGreaterThan(0)
  })

  it('board_is_clipped_at_the_keel', () => {
    const board = build(0).tris.filter((t) => t.group === 'board')
    expect(board.length).toBe(6)
    for (const z of board.flatMap((t) => [t.a.z, t.b.z, t.c.z])) {
      expect(z).toBeLessThanOrEqual(dims.keelZ)
    }
    // And the clip is doing something: the unclipped top would be above the keel.
    expect(params.board.pos_b.z + dims.boardSpan / 2).toBeGreaterThan(dims.keelZ)
  })

  it('articulation_precedes_roll', () => {
    // The guard against building the model in `H` and articulating there: `β`
    // is a rotation about `+z_B`, so it has to be applied before `R_x(φ)`.
    let seed = 20260920
    const random = () => {
      seed = (seed * 1103515245 + 12345) % 2147483648
      return seed / 2147483648
    }
    for (let i = 0; i < 20; i += 1) {
      const beta = (random() - 0.5) * 2 * Math.PI
      const phi = (random() - 0.5) * 8
      const b = boomDirection(beta)
      const expected = rollToH(
        {
          x: params.sail.mast_pos_b.x + params.sail.boom_length * b.x,
          y: params.sail.boom_length * b.y,
          z: params.sheet.z_boom,
        },
        phi,
      )
      expect(gap(rollToH(clewOf(beta), phi), expected)).toBeLessThan(1e-12)
    }
  })

  it('triangle_budget', () => {
    const model = build(0.35, 0.2, 0.3)
    expect(model.tris.length).toBeLessThanOrEqual(26)
    expect(model.tris.filter((t) => t.group === 'sail').length).toBe(12)
    expect(model.tris.filter((t) => t.group === 'board').length).toBe(6)
    expect(model.tris.filter((t) => t.group === 'rudder').length).toBe(4)
    // Plates, never culled.
    expect(model.tris.every((t) => !t.closed)).toBe(true)
    expect(model.lines.map((l) => l.testId).sort()).toEqual(['boat-boom', 'boat-mast'])
  })
})
