/**
 * The sail, as a 3-D patch (v2 section 01, task 1.3).
 *
 * v1 drew the sail as a single quadratic Bézier in the plane. This file keeps
 * that file's job — *what shape the sail is* — and changes the answer to a
 * surface in `B`: the triangle tack → head → clew, bulged along its own normal
 * by a cosmetic camber whose sign still follows the core's angle of attack.
 *
 * The corners come from the F7 catalogue and from `visualDims`: the tack and
 * the clew sit at `z_boom`, the head at the masthead, and the foot runs along
 * `b̂(β)` — `geometry.boomDirection`, the same expression the core uses (F2.1),
 * not a second copy of it. The area of that triangle is `sail.area` by the
 * construction of `mastHeight`; `sail_area_matches_the_parameter` checks it.
 *
 * **The camber is cosmetic**, exactly as it was in v1: nothing computed here
 * is fed back into the physics (F8), and no aerodynamic quantity is derived
 * from it.
 */

import { boomDirection } from './geometry'
import type { Tri, Vec3 } from './model3d'
import { SAIL_CAMBER_FRACTION, SAIL_CAMBER_POSITION } from './visualDims'

/** The three corners of the drawn sail, in `B`, metres. */
export interface SailCorners {
  /** Foot at the mast, at boom height. */
  tack: Vec3
  /** Head of the sail, on the mast axis. */
  head: Vec3
  /** Foot at the boom end. */
  clew: Vec3
  /**
   * Unit normal of the chord plane, `(sin β, −cos β, 0)`.
   *
   * A `−90°` rotation of `b̂(β)` about `+z`, so a positive camber bulges the
   * sail to the same side the v1 Bézier's control point did.
   */
  normal: Vec3
}

export interface SailRig {
  /** m, mast foot along `+x` from the CG. */
  mastX: number
  /** m, boom length. */
  boomLength: number
  /** m, boom height above the CG (`sheet.z_boom`). */
  zBoom: number
  /** m, height of the head above the CG (`visualDims.mastHeight`). */
  mastHeight: number
}

export function sailCorners(rig: SailRig, beta: number): SailCorners {
  const b = boomDirection(beta)
  return {
    tack: { x: rig.mastX, y: 0, z: rig.zBoom },
    head: { x: rig.mastX, y: 0, z: rig.mastHeight },
    clew: {
      x: rig.mastX + rig.boomLength * b.x,
      y: rig.boomLength * b.y,
      z: rig.zBoom,
    },
    normal: { x: -b.y, y: b.x, z: 0 },
  }
}

/** Chordwise and spanwise panel counts. 4 × 2, so at most 16 triangles. */
export const SAIL_CHORD_PANELS = 4
export const SAIL_SPAN_PANELS = 2

/**
 * Chordwise camber profile: 0 at the luff and the leech, 1 at
 * `SAIL_CAMBER_POSITION`.
 *
 * `sin(π · u^k)` with `k = ln ½ / ln p` puts the peak at `u = p` and stays
 * smooth and single-signed in between — no kink to show up as a crease in the
 * drawn surface.
 */
export function camberProfile(u: number): number {
  const k = Math.log(0.5) / Math.log(SAIL_CAMBER_POSITION)
  return Math.sin(Math.PI * u ** k)
}

/**
 * A point on the sail surface.
 *
 * `s` runs 0 at the foot to 1 at the head; `u` runs 0 at the luff to 1 at the
 * leech. The chord shortens linearly with `s`, so the surface is the corner
 * triangle, and the camber offset scales with the *local* chord.
 */
export function sailPoint(c: SailCorners, s: number, u: number, alpha: number): Vec3 {
  const lerp = (a: Vec3, b: Vec3, t: number): Vec3 => ({
    x: a.x + t * (b.x - a.x),
    y: a.y + t * (b.y - a.y),
    z: a.z + t * (b.z - a.z),
  })
  const luff = lerp(c.tack, c.head, s)
  const leech = lerp(c.clew, c.head, s)
  const base = lerp(luff, leech, u)
  const chord = Math.hypot(leech.x - luff.x, leech.y - luff.y, leech.z - luff.z)
  const bulge = SAIL_CAMBER_FRACTION * chord * camberProfile(u) * Math.sign(alpha)
  return {
    x: base.x + bulge * c.normal.x,
    y: base.y + bulge * c.normal.y,
    z: base.z + bulge * c.normal.z,
  }
}

/**
 * The sail patch: `SAIL_CHORD_PANELS × SAIL_SPAN_PANELS` panels, triangulated.
 *
 * The top strip degenerates at the head, so it contributes one triangle per
 * chordwise panel rather than two: 12 triangles in all, under the 16 the task
 * allows. `closed: false` — a sail is a plate and is drawn from both sides.
 */
export function sailPatch(c: SailCorners, alpha: number): Tri[] {
  const tris: Tri[] = []
  const at = (i: number, j: number) =>
    sailPoint(c, j / SAIL_SPAN_PANELS, i / SAIL_CHORD_PANELS, alpha)
  const face = (a: Vec3, b: Vec3, d: Vec3) =>
    tris.push({ a, b, c: d, material: 'sail', group: 'sail', closed: false })

  for (let j = 0; j < SAIL_SPAN_PANELS; j += 1) {
    const top = j + 1 === SAIL_SPAN_PANELS
    for (let i = 0; i < SAIL_CHORD_PANELS; i += 1) {
      const a = at(i, j)
      const b = at(i + 1, j)
      if (top) {
        // Every point of the upper edge is the head: one triangle, not two.
        face(a, b, c.head)
      } else {
        face(a, b, at(i + 1, j + 1))
        face(a, at(i + 1, j + 1), at(i, j + 1))
      }
    }
  }
  return tris
}

/**
 * Luff, leech and foot as polylines in `B`, for the stroked outline.
 *
 * The luff runs up the mast and is straight; the leech and the foot follow the
 * cambered surface, so the drawn edge is the edge of the drawn surface.
 */
export function sailEdges(c: SailCorners, alpha: number): Vec3[][] {
  const span = Array.from({ length: SAIL_SPAN_PANELS + 1 }, (_, j) => j / SAIL_SPAN_PANELS)
  const chordwise = Array.from(
    { length: SAIL_CHORD_PANELS + 1 },
    (_, i) => i / SAIL_CHORD_PANELS,
  )
  return [
    span.map((s) => sailPoint(c, s, 0, alpha)),
    span.map((s) => sailPoint(c, s, 1, alpha)),
    chordwise.map((u) => sailPoint(c, 0, u, alpha)),
  ]
}
