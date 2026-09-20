/**
 * Roll, cull and depth-sort the boat model (v2 section 01, task 1.4).
 *
 * **This is the one file in the render layer that contains `R_x(φ)`**, and it
 * is a *view* transform, not a frame conversion (normative delta **D3**):
 * `web/src/render/Camera.ts` has held a 2-D view rotation since M1 on the same
 * reading, that F2's "frame conversions live in `physics/frames.rs` and
 * nowhere else" governs the physics. Nothing computed here is ever handed back
 * across the F8 boundary, and `render_rotation_is_view_only` in
 * `tests/unit/project3d.test.ts` enforces that mechanically.
 *
 * The pipeline, per frame:
 *
 * 1. roll every vertex by `φ` — F2's matrix, verbatim;
 * 2. cull the `closed` triangles whose rolled outward normal has `n_z ≤ 0`;
 * 3. sort ascending by depth, so higher faces paint last.
 *
 * The view is orthographic from `+z_H`: the SVG draws `(x_H, y_H)` and `z_H`
 * is depth. Yaw and the camera are still `Camera.boatTransform`'s job.
 *
 * `φ` is never wrapped (F3), so everything here must hold for `φ` of ±4 rad,
 * not just ±π. It does: there is no branch on the sign or the magnitude of
 * `φ` anywhere below.
 */

import type { BoatGroup, BoatModel, Material, Vec2, Vec3 } from './model3d'
import { normal } from './model3d'

/**
 * F2's `R_x(φ)`, verbatim.
 *
 * ```
 * x_H = x_B
 * y_H = y_B·cos φ − z_B·sin φ
 * z_H = y_B·sin φ + z_B·cos φ
 * ```
 *
 * At `φ = 0` this is the identity on all three components, bit for bit:
 * `cos 0` is exactly 1 and `sin 0` is exactly 0, so no rounding enters.
 */
export function rollToH(v: Vec3, phi: number): Vec3 {
  const c = Math.cos(phi)
  const s = Math.sin(phi)
  return {
    x: v.x,
    y: v.y * c - v.z * s,
    z: v.y * s + v.z * c,
  }
}

export interface DrawTri {
  /** `(x_H, y_H)`, metres. */
  points: readonly [Vec2, Vec2, Vec2]
  /** Centroid `z_H` — what the painter's algorithm sorts on. */
  depth: number
  /**
   * The three vertices' `z_H`, in the same order as `points`.
   *
   * Not in the task's listing, but {@link depthAt} needs the triangle's
   * *plane* and not merely its centroid, and the plane cannot be recovered
   * from a single scalar. The ray reference in `tests/unit/boat3d.test.ts` is
   * built on it.
   */
  depths: readonly [number, number, number]
  material: Material
  group: BoatGroup
}

export interface DrawLine {
  a: Vec2
  b: Vec2
  depth: number
  testId: string
  widthPx: number
  colour: string
}

export type DrawItem = DrawTri | DrawLine

export function isTri(item: DrawItem): item is DrawTri {
  return 'points' in item
}

/**
 * Rolled, culled, sorted back to front. The whole per-frame pipeline.
 *
 * The sort is **stable and total**: ties break on the item's index in the
 * model, so the output is a pure function of `(model, φ)` and two renders of
 * the same frame produce byte-identical SVG.
 */
export function projectModel(model: BoatModel, phi: number): readonly DrawItem[] {
  const ordered: { item: DrawItem; index: number }[] = []

  model.tris.forEach((t, i) => {
    const a = rollToH(t.a, phi)
    const b = rollToH(t.b, phi)
    const c = rollToH(t.c, phi)
    // A closed-solid face is drawn only when it faces the viewer. A plate —
    // sail, centreboard, rudder — is drawn from both sides.
    if (t.closed && normal({ a, b, c }).z <= 0) {
      return
    }
    ordered.push({
      index: i,
      item: {
        points: [
          { x: a.x, y: a.y },
          { x: b.x, y: b.y },
          { x: c.x, y: c.y },
        ],
        depths: [a.z, b.z, c.z],
        depth: (a.z + b.z + c.z) / 3,
        material: t.material,
        group: t.group,
      },
    })
  })

  model.lines.forEach((l, i) => {
    const a = rollToH(l.a, phi)
    const b = rollToH(l.b, phi)
    ordered.push({
      index: model.tris.length + i,
      item: {
        a: { x: a.x, y: a.y },
        b: { x: b.x, y: b.y },
        // A line takes the depth of its **higher** endpoint, which puts a
        // tipped-up mast in front of the sail rather than behind it. A
        // deliberate simplification: see the debt table in the section PRD.
        depth: Math.max(a.z, b.z),
        testId: l.testId,
        widthPx: l.widthPx,
        colour: l.colour,
      },
    })
  })

  ordered.sort((p, q) => (p.item.depth - q.item.depth) || (p.index - q.index))
  return ordered.map((o) => o.item)
}

/** True if the projected triangle contains `p`, edges included. */
export function containsPoint(t: DrawTri, p: Vec2): boolean {
  const [a, b, c] = t.points
  const side = (u: Vec2, v: Vec2) => (v.x - u.x) * (p.y - u.y) - (v.y - u.y) * (p.x - u.x)
  const s1 = side(a, b)
  const s2 = side(b, c)
  const s3 = side(c, a)
  const negative = s1 < 0 || s2 < 0 || s3 < 0
  const positive = s1 > 0 || s2 > 0 || s3 > 0
  return !(negative && positive)
}

/**
 * Depth of the triangle's plane above a projected point.
 *
 * Barycentric interpolation of the vertices' `z_H`: exact for a plane, which
 * is what the ray reference in task 1.6 needs in order to say which surface is
 * really topmost along a given ray. Returns `NaN` for a triangle that projects
 * to a line, which has no plane over the viewing direction.
 */
export function depthAt(t: DrawTri, p: Vec2): number {
  const [a, b, c] = t.points
  const [za, zb, zc] = t.depths
  const area = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)
  if (area === 0) {
    return NaN
  }
  const wa = ((b.x - p.x) * (c.y - p.y) - (c.x - p.x) * (b.y - p.y)) / area
  const wb = ((c.x - p.x) * (a.y - p.y) - (a.x - p.x) * (c.y - p.y)) / area
  const wc = 1 - wa - wb
  return wa * za + wb * zb + wc * zc
}

/** Projected (screen-plane) area of a drawn triangle, m². */
export function projectedArea(t: DrawTri): number {
  const [a, b, c] = t.points
  return Math.abs((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)) / 2
}
