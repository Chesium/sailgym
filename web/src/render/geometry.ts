/**
 * Boat geometry for the SVG renderer (brief §25).
 *
 * Everything here is a **normalised** shape scaled by dimensions read from
 * `parameters_json()`. There is not one hard-coded metre value in this file or
 * anywhere else under `web/src/render/` — `loa`, `beam`, the mast position and
 * the boom length all come from the Rust parameter catalogue (F7).
 *
 * Coordinates are the boat-fixed frame `B`: `+x` forward, `+y` to port,
 * metres. `Camera.boatTransform` places the result on screen.
 */

export interface HullDims {
  /** m, length overall */
  loa: number
  /** m, maximum beam */
  beam: number
}

export interface RigDims {
  /** m, mast foot along `+x` from the CG */
  mastX: number
  /** m, boom length */
  boomLength: number
  /** m, rudder stock along `+x` from the CG (negative: aft) */
  rudderX: number
  /** m, centreboard case centre along `+x` from the CG */
  boardX: number
}

/**
 * Hull outline as fractions of `(loa, beam)`: `x` from −0.5 at the transom to
 * +0.5 at the bow, `y` from −0.5 (starboard) to +0.5 (port). Dimensionless, so
 * scaling it can never leak a hard-coded length.
 */
const HULL_OUTLINE: ReadonlyArray<readonly [number, number]> = [
  [0.5, 0.0],
  [0.42, 0.18],
  [0.25, 0.38],
  [0.05, 0.48],
  [-0.15, 0.5],
  [-0.35, 0.45],
  [-0.5, 0.36],
  [-0.5, -0.36],
  [-0.35, -0.45],
  [-0.15, -0.5],
  [0.05, -0.48],
  [0.25, -0.38],
  [0.42, -0.18],
]

/** Foil chord lengths, as a fraction of LOA. Indicative only (brief §25). */
const RUDDER_CHORD_FRACTION = 0.08
const BOARD_CHORD_FRACTION = 0.1

/** Closed SVG path of the hull, in metres in `B`. */
export function hullPath(d: HullDims): string {
  const points = HULL_OUTLINE.map(([fx, fy]) => `${fx * d.loa},${fy * d.beam}`)
  return `M ${points.join(' L ')} Z`
}

/** Bounding extent of [`hullPath`], in metres. */
export function hullExtent(d: HullDims): { length: number; beam: number } {
  const xs = HULL_OUTLINE.map(([fx]) => fx * d.loa)
  const ys = HULL_OUTLINE.map(([, fy]) => fy * d.beam)
  return {
    length: Math.max(...xs) - Math.min(...xs),
    beam: Math.max(...ys) - Math.min(...ys),
  }
}

/**
 * Boom unit vector `b̂(β) = (−cos β, −sin β)` (F2.1). The boom points aft, so
 * a positive `β` swings the tip to starboard (`y < 0`, since `+y` is port).
 */
export function boomDirection(beta: number): { x: number; y: number } {
  return { x: -Math.cos(beta), y: -Math.sin(beta) }
}

/** Boom, from the gooseneck at the mast to the clew. */
export function boomSegment(
  rig: RigDims,
  beta: number,
): { x1: number; y1: number; x2: number; y2: number } {
  const b = boomDirection(beta)
  return {
    x1: rig.mastX,
    y1: 0,
    x2: rig.mastX + rig.boomLength * b.x,
    y2: rig.boomLength * b.y,
  }
}

/**
 * Sail, drawn as the triangle mast → clew with a leeward bulge. Purely
 * indicative: the sail shape is not part of the physics, which uses a flat
 * chord from the mast to the clew (F6.3).
 */
export function sailPath(rig: RigDims, beta: number): string {
  const { x1, y1, x2, y2 } = boomSegment(rig, beta)
  // Control point offset perpendicular to the boom, on the leeward side.
  const bx = x2 - x1
  const by = y2 - y1
  const camber = 0.12
  const cx = x1 + bx * 0.5 - by * camber
  const cy = y1 + by * 0.5 + bx * camber
  return `M ${x1},${y1} Q ${cx},${cy} ${x2},${y2}`
}

/**
 * Rudder blade, from the stock aft along `ĉ_r(δr) = (−cos δr, −sin δr)`
 * (F2.2): a positive `δr` puts the trailing edge to starboard.
 */
export function rudderSegment(
  rig: RigDims,
  hull: HullDims,
  deltaR: number,
): { x1: number; y1: number; x2: number; y2: number } {
  const chord = RUDDER_CHORD_FRACTION * hull.loa
  return {
    x1: rig.rudderX,
    y1: 0,
    x2: rig.rudderX - chord * Math.cos(deltaR),
    y2: -chord * Math.sin(deltaR),
  }
}

/** Centreboard case, shown edge-on along the centreline. */
export function boardSegment(
  rig: RigDims,
  hull: HullDims,
): { x1: number; y1: number; x2: number; y2: number } {
  const chord = BOARD_CHORD_FRACTION * hull.loa
  return {
    x1: rig.boardX + chord / 2,
    y1: 0,
    x2: rig.boardX - chord / 2,
    y2: 0,
  }
}
