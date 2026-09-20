/**
 * The low-poly boat model: types only (v2 section 01, task 1.1).
 *
 * Every vertex in a {@link BoatModel} lives in the **boat-fixed frame `B`**
 * (F2: `+x` forward, `+y` to port, `+z` up), in metres, with the rig's
 * articulation — the boom's `β` and the rudder's `δr` — already baked in. The
 * model is therefore *constant for a given pose of the rig*; roll is applied
 * per frame by `project3d.rollToH`, and heading and the camera stay where they
 * have always been, in `Camera.boatTransform`.
 *
 * There is no lighting term anywhere in this file or in anything that consumes
 * it. Materials are flat colours, resolved in `BoatSvg.tsx`.
 *
 * This module contains no physics and no frame conversion: the only arithmetic
 * here is {@link normal}, a cross product of two edge vectors, which is a
 * property of a triangle and not a property of any frame.
 */

export interface Vec3 {
  x: number
  y: number
  z: number
}

export interface Vec2 {
  x: number
  y: number
}

/** Flat material colours. No lighting term exists anywhere in this section. */
export type Material = 'deck' | 'topsides' | 'bilge' | 'underside' | 'sail' | 'foil'

/** Which `data-testid` group the primitive is rendered into. */
export type BoatGroup = 'hull' | 'sail' | 'board' | 'rudder'

export interface Tri {
  /** Vertices in B, metres, wound counter-clockwise seen from outside. */
  a: Vec3
  b: Vec3
  c: Vec3
  material: Material
  group: BoatGroup
  /** True only for triangles of the closed hull solid — the only ones culled. */
  closed: boolean
}

/** The mast and the boom stay line elements; see task 1.5 on why. */
export interface Line3 {
  a: Vec3
  b: Vec3
  testId: string
  widthPx: number
  colour: string
}

export interface BoatModel {
  tris: readonly Tri[]
  lines: readonly Line3[]
}

/**
 * Outward normal of a triangle, in whatever frame its vertices are in.
 *
 * `(b − a) × (c − a)`, unnormalised: the length is twice the triangle's area,
 * which is what the projected-area checks in `tests/unit/project3d.test.ts`
 * want, and the direction is all the visibility cull needs.
 */
export function normal(t: { a: Vec3; b: Vec3; c: Vec3 }): Vec3 {
  const ux = t.b.x - t.a.x
  const uy = t.b.y - t.a.y
  const uz = t.b.z - t.a.z
  const vx = t.c.x - t.a.x
  const vy = t.c.y - t.a.y
  const vz = t.c.z - t.a.z
  return {
    x: uy * vz - uz * vy,
    y: uz * vx - ux * vz,
    z: ux * vy - uy * vx,
  }
}

/** Centroid of a triangle, in whatever frame its vertices are in. */
export function centroid(t: { a: Vec3; b: Vec3; c: Vec3 }): Vec3 {
  return {
    x: (t.a.x + t.b.x + t.c.x) / 3,
    y: (t.a.y + t.b.y + t.c.y) / 3,
    z: (t.a.z + t.b.z + t.c.z) / 3,
  }
}
