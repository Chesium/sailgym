import { describe, expect, it } from 'vitest'

import { hullPath } from '../../src/render/geometry'
import { hullRings, hullSolid, parsePath } from '../../src/render/hullSolid'
import { centroid, normal, type Tri, type Vec3 } from '../../src/render/model3d'
import { visualDims } from '../../src/render/visualDims'
import { ilcaParams } from './ilca'

const params = ilcaParams()
const dims = visualDims(params)
const tris = hullSolid(params.hull, dims)

const sub = (p: Vec3, q: Vec3): Vec3 => ({ x: p.x - q.x, y: p.y - q.y, z: p.z - q.z })
const dot = (p: Vec3, q: Vec3) => p.x * q.x + p.y * q.y + p.z * q.z
const cross = (a: Vec3, b: Vec3): Vec3 => ({
  x: a.y * b.z - a.z * b.y,
  y: a.z * b.x - a.x * b.z,
  z: a.x * b.y - a.y * b.x,
})

/** Divergence-theorem volume and volumetric centroid of a closed mesh. */
function solidVolume(mesh: readonly Tri[]): { volume: number; centre: Vec3 } {
  let volume = 0
  const centre = { x: 0, y: 0, z: 0 }
  for (const t of mesh) {
    const v = dot(t.a, cross(t.b, t.c)) / 6
    volume += v
    centre.x += (v * (t.a.x + t.b.x + t.c.x)) / 4
    centre.y += (v * (t.a.y + t.b.y + t.c.y)) / 4
    centre.z += (v * (t.a.z + t.b.z + t.c.z)) / 4
  }
  return {
    volume,
    centre: { x: centre.x / volume, y: centre.y / volume, z: centre.z / volume },
  }
}

const key = (p: Vec3) => `${p.x},${p.y},${p.z}`

describe('hullSolid', () => {
  it('outline_is_the_v1_outline', () => {
    // The boat at φ = 0 is the boat v1 drew: the sheer ring's plan-form is
    // `geometry.hullPath`, point for point.
    const drawn = parsePath(hullPath(params.hull))
    const { sheer } = hullRings(params.hull, dims)
    expect(sheer.length).toBe(drawn.length)
    sheer.forEach((p, i) => {
      expect(Math.abs(p.x - drawn[i][0])).toBeLessThan(1e-12)
      expect(Math.abs(p.y - drawn[i][1])).toBeLessThan(1e-12)
    })
  })

  it('is_a_closed_manifold', () => {
    const seen = new Map<string, number>()
    for (const t of tris) {
      for (const [u, v] of [
        [t.a, t.b],
        [t.b, t.c],
        [t.c, t.a],
      ] as const) {
        const k = `${key(u)}|${key(v)}`
        seen.set(k, (seen.get(k) ?? 0) + 1)
      }
    }
    // Every directed edge occurs once, and its reverse occurs once: the mesh
    // is closed and consistently wound.
    for (const [k, count] of seen) {
      expect(count, `directed edge ${k}`).toBe(1)
      const [u, v] = k.split('|')
      expect(seen.get(`${v}|${u}`) ?? 0, `reverse of ${k}`).toBe(1)
    }
    expect(seen.size).toBe(3 * tris.length)
  })

  it('normals_point_outward', () => {
    const { centre } = solidVolume(tris)
    for (const t of tris) {
      expect(dot(normal(t), sub(centroid(t), centre)), `${t.material}`).toBeGreaterThan(0)
    }
  })

  it('signed_volume_positive', () => {
    // A real ILCA canoe body is ≈ 0.9 m³ to the sheer; the band is wide on
    // purpose — it is a sanity check on winding and scale, not a hull-form
    // claim.
    const { volume } = solidVolume(tris)
    expect(volume).toBeGreaterThan(0.25)
    expect(volume).toBeLessThan(2.5)
  })

  it('triangle_count', () => {
    // 13 deck + 26 topsides + 26 bilge + 11 keel cap. The task 1.2 table says
    // 78, with a 13-triangle fan for the keel cap; a fan over the rockered
    // keel ring cannot have `n_y = 0`, and the Visibility rule the section
    // rests on needs it to — see the header of `hullSolid.ts`.
    expect(tris.length).toBe(76)
    const count = (m: string) => tris.filter((t) => t.material === m).length
    expect(count('deck')).toBe(13)
    expect(count('topsides')).toBe(26)
    expect(count('bilge')).toBe(26)
    expect(count('underside')).toBe(11)
    expect(tris.every((t) => t.closed && t.group === 'hull')).toBe(true)
  })

  it('the_caps_have_no_lateral_normal_component', () => {
    // The exactness the Visibility rule is built on: a deck or keel triangle's
    // outward normal lies in the `x–z` plane, so after `R_x(φ)` its `z`
    // component is `n_z · cos φ` and the sign of `cos φ` alone decides whether
    // it is drawn. The topsides are the mirror case: their normals are exactly
    // horizontal, so `sin φ` alone decides.
    for (const t of tris) {
      const n = normal(t)
      const scale = Math.hypot(n.x, n.y, n.z)
      if (t.material === 'deck' || t.material === 'underside') {
        expect(Math.abs(n.y) / scale, t.material).toBeLessThan(1e-15)
      }
      if (t.material === 'topsides') {
        expect(Math.abs(n.z) / scale).toBe(0)
      }
    }
  })
})
