import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { sailCorners, sailPatch } from '../../src/render/SailShape'
import type { Vec3 } from '../../src/render/model3d'
import { visualDims } from '../../src/render/visualDims'
import { apparentWindDegrees } from '../../src/sim/units'
import { ilcaParams } from './ilca'

const params = ilcaParams()
const dims = visualDims(params)
const rig = {
  mastX: params.sail.mast_pos_b.x,
  boomLength: params.sail.boom_length,
  zBoom: params.sheet.z_boom,
  mastHeight: dims.mastHeight,
}

const triangleArea = (a: Vec3, b: Vec3, c: Vec3) => {
  const ux = b.x - a.x
  const uy = b.y - a.y
  const uz = b.z - a.z
  const vx = c.x - a.x
  const vy = c.y - a.y
  const vz = c.z - a.z
  return Math.hypot(uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx) / 2
}

describe('sail display', () => {
  it('camber_mirrors_with_alpha', () => {
    // The camber is cosmetic, and its sign follows the core's angle of attack
    // (F8: nothing here is fed back). What must hold is that the two signs are
    // exact reflections of each other in the chord plane, so a boat on one
    // tack and the mirror-image boat on the other are drawn the same way.
    const beta = 0.35
    const c = sailCorners(rig, beta)
    const plus = sailPatch(c, 0.2)
    const minus = sailPatch(c, -0.2)
    expect(plus.length).toBe(minus.length)

    // Reflection in the chord plane through the tack with normal `c.normal`.
    const reflect = (p: Vec3): Vec3 => {
      const d =
        (p.x - c.tack.x) * c.normal.x +
        (p.y - c.tack.y) * c.normal.y +
        (p.z - c.tack.z) * c.normal.z
      return {
        x: p.x - 2 * d * c.normal.x,
        y: p.y - 2 * d * c.normal.y,
        z: p.z - 2 * d * c.normal.z,
      }
    }
    plus.forEach((t, i) => {
      for (const k of ['a', 'b', 'c'] as const) {
        const r = reflect(t[k])
        const m = minus[i][k]
        expect(Math.hypot(r.x - m.x, r.y - m.y, r.z - m.z)).toBeLessThan(1e-12)
      }
    })

    // At `alpha = 0` the patch is planar: every vertex lies in the chord plane.
    for (const t of sailPatch(c, 0)) {
      for (const k of ['a', 'b', 'c'] as const) {
        const d =
          (t[k].x - c.tack.x) * c.normal.x +
          (t[k].y - c.tack.y) * c.normal.y +
          (t[k].z - c.tack.z) * c.normal.z
        expect(Math.abs(d)).toBeLessThan(1e-12)
      }
    }
  })

  it('sail_area_matches_the_parameter', () => {
    // `mastHeight` is defined so the drawn triangle carries exactly
    // `sail.area`; the triangulation has to preserve it.
    for (const beta of [0, 0.35, -0.9]) {
      const patch = sailPatch(sailCorners(rig, beta), 0)
      const area = patch.reduce((s, t) => s + triangleArea(t.a, t.b, t.c), 0)
      expect(Math.abs(area - params.sail.area) / params.sail.area).toBeLessThan(0.005)
    }
  })

  it('preserves the FROM-angle sign at the degree boundary', () => {
    expect(apparentWindDegrees(Math.PI / 2)).toBe(90)
    expect(apparentWindDegrees(-Math.PI / 2)).toBe(-90)
  })
  it('keeps the M4 diagnostic field names identical across the boundary', () => {
    const rust = readFileSync('../crates/sailgym-physics/src/diagnostics.rs', 'utf8')
    const ts = readFileSync('src/sim/diagnostics.ts', 'utf8')
    const fields = [...rust.matchAll(/pub (\w+):/g)].map((m) => m[1]).sort()
    const mirror = [...ts.matchAll(/^  (\w+):/gm)].map((m) => m[1]).sort()
    expect(mirror).toEqual(fields)
  })
})
