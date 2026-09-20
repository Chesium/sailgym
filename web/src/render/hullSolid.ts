/**
 * The hull as a closed, low-poly solid (v2 section 01, task 1.2).
 *
 * Three rings of 13 vertices each — sheer, chine, keel — stitched into a
 * closed manifold: a deck cap, a topsides strip, a bilge strip and a keel cap.
 * Every triangle is wound counter-clockwise seen from **outside** the solid,
 * which is what lets `project3d` cull by the sign of the rolled normal's `z`
 * alone.
 *
 * ## The outline is v1's outline, not a copy of it
 *
 * The plan-form comes from `geometry.hullPath`, evaluated at unit dimensions
 * and parsed back into fractions. `geometry.ts`'s `HULL_OUTLINE` is not
 * exported and this section does not own that file, so reading it through the
 * one function that already publishes it is both the only route available and
 * the stronger one: the drawn boat cannot drift from the v1 boat, because
 * there is still exactly one outline in the repository.
 *
 * ## Two deviations from the task 1.2 ring table, both forced
 *
 * 1. **The rocker is applied to the chine ring as well as the keel ring.** The
 *    table has the chine ring planar. `rockerRise` (0.12 × beam) is larger than
 *    the canoe depth (0.08 × beam), so raising only the keel lifts it *above*
 *    the chine at bow and transom and turns the bilge strip inside out there —
 *    four triangles then fail `normals_point_outward`. Raising both keeps the
 *    canoe body's depth constant from end to end, which is what a rockered
 *    hull actually does.
 *
 * 2. **The keel cap is a transverse ladder of 11 triangles, not a 13-triangle
 *    fan from the ring's centroid.** The section's Visibility rule states, as
 *    an exactness, that the keel cap's outward normal is `−ẑ_B` and that the
 *    underside therefore appears "at exactly 90° of heel and not before". A
 *    fan over a *non-planar* ring cannot satisfy that: its triangles have a
 *    non-zero `n_y`, and by the hull's port/starboard mirror symmetry half of
 *    those point to port, which makes them visible from about 65° of heel
 *    onwards. A ladder triangulation — each rung joining a vertex to its
 *    mirror, which shares its `x` and therefore its rocker height — has
 *    `n_y = 0` identically, so the exactness holds. The cost is two triangles
 *    and the total drops from 78 to 76.
 *
 * Both are recorded in `docs/v2/progress/01-handoff.md`.
 */

import { hullPath, type HullDims } from './geometry'
import type { Tri, Vec3 } from './model3d'
import type { VisualDims } from './visualDims'

/**
 * The v1 hull outline, as fractions of `(loa, beam)`, recovered from
 * `hullPath` at unit dimensions.
 *
 * `hullPath` writes `${fx * loa},${fy * beam}`, so at `loa = beam = 1` each
 * coordinate round-trips exactly: JavaScript prints a double as its shortest
 * round-tripping decimal, and `Number` reads it back bit for bit.
 */
export function hullOutline(): ReadonlyArray<readonly [number, number]> {
  if (cachedOutline === null) {
    cachedOutline = parsePath(hullPath({ loa: 1, beam: 1 }))
  }
  return cachedOutline
}

let cachedOutline: ReadonlyArray<readonly [number, number]> | null = null

/** `M x,y L x,y … Z` → the points. Exported so the unit test parses the same way. */
export function parsePath(d: string): ReadonlyArray<readonly [number, number]> {
  return d
    .replace(/^M\s*/, '')
    .replace(/\s*Z$/, '')
    .split(/\s*L\s*/)
    .map((pair) => {
      const [x, y] = pair.trim().split(',')
      return [Number(x), Number(y)] as const
    })
}

/** The three rings, in `B`, metres. Exposed for the tests and for `rigSolid`. */
export interface HullRings {
  sheer: readonly Vec3[]
  chine: readonly Vec3[]
  keel: readonly Vec3[]
}

export function hullRings(hull: HullDims, dims: VisualDims): HullRings {
  const outline = hullOutline()
  const rocker = (fx: number) => dims.rockerRise * (2 * fx) ** 2
  return {
    sheer: outline.map(([fx, fy]) => ({
      x: fx * hull.loa,
      y: fy * hull.beam,
      // Linear in `x`, so the sheer ring is planar and the deck cap's outward
      // normal has no `y` component. That is what makes "the deck is visible
      // iff cos φ > 0" exact.
      z: dims.deckZ + dims.sheerRise * (fx + 0.5),
    })),
    chine: outline.map(([fx, fy]) => ({
      x: fx * hull.loa,
      y: fy * hull.beam,
      z: dims.chineZ + rocker(fx),
    })),
    keel: outline.map(([fx, fy]) => ({
      x: fx * hull.loa * dims.keelLengthScale,
      y: fy * hull.beam * dims.keelWidthScale,
      z: dims.keelZ + rocker(fx),
    })),
  }
}

/**
 * The height of the sheer ring at a station `x`, by linear interpolation in
 * the sheer plane. Used by `rigSolid` to stand the mast on the deck.
 */
export function sheerHeightAt(x: number, hull: HullDims, dims: VisualDims): number {
  return dims.deckZ + dims.sheerRise * (x / hull.loa + 0.5)
}

const mean = (r: readonly Vec3[], k: 'x' | 'y' | 'z') =>
  r.reduce((s, p) => s + p[k], 0) / r.length

/**
 * The closed hull solid: 76 triangles, every one `closed: true` and
 * `group: 'hull'`.
 *
 * Counts: deck cap 13, topsides 26, bilge 26, keel cap 11.
 */
export function hullSolid(hull: HullDims, dims: VisualDims): Tri[] {
  const { sheer, chine, keel } = hullRings(hull, dims)
  const n = sheer.length
  const tris: Tri[] = []
  const face = (a: Vec3, b: Vec3, c: Vec3, material: Tri['material']) =>
    tris.push({ a, b, c, material, group: 'hull', closed: true })

  // Deck cap: a fan from the sheer ring's centroid. The ring is planar, so the
  // centroid lies in its plane and every triangle shares its normal direction.
  const apex: Vec3 = { x: mean(sheer, 'x'), y: mean(sheer, 'y'), z: mean(sheer, 'z') }
  for (let i = 0; i < n; i += 1) {
    face(apex, sheer[i], sheer[(i + 1) % n], 'deck')
  }

  // Topsides and bilge: quad strips between consecutive rings. Sheer and chine
  // share their `(x, y)`, so every topside quad lies in a vertical plane and
  // its normal is exactly horizontal — which makes "a topside is visible iff
  // its side is the one rising" exact too.
  for (let i = 0; i < n; i += 1) {
    const j = (i + 1) % n
    face(sheer[i], chine[i], chine[j], 'topsides')
    face(sheer[i], chine[j], sheer[j], 'topsides')
  }
  for (let i = 0; i < n; i += 1) {
    const j = (i + 1) % n
    face(chine[i], keel[i], keel[j], 'bilge')
    face(chine[i], keel[j], chine[j], 'bilge')
  }

  // Keel cap: a transverse ladder. Index 0 is the bow, on the centreline; the
  // rest pair off across it, `i` with `n − i`. Both members of a rung share
  // their station and so their rocker height, which is what forces `n_y = 0`.
  const mirror = (i: number) => n - i
  face(keel[0], keel[mirror(1)], keel[1], 'underside')
  const rungs = Math.floor(n / 2)
  for (let i = 1; i < rungs; i += 1) {
    const port = keel[i]
    const stbd = keel[mirror(i)]
    const portNext = keel[i + 1]
    const stbdNext = keel[mirror(i + 1)]
    face(port, stbd, stbdNext, 'underside')
    face(port, stbdNext, portNext, 'underside')
  }

  return tris
}
