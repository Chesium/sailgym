/**
 * Rig and foils as 3-D primitives (v2 section 01, task 1.3).
 *
 * Everything here is built in the boat-fixed frame `B` with the articulation
 * **already applied** — the boom's `β` (F2.1) and the rudder's `δr` (F2.2),
 * both right-handed rotations about `+z_B` — so the caller only has to roll
 * the result. That order matters: articulating after the roll would express
 * the boom angle in the horizontal frame, which is not what `β` is.
 * `articulation_precedes_roll` in `tests/unit/rigSolid.test.ts` is the guard.
 *
 * The sail, the centreboard and the rudder are **plates**: `closed: false`, so
 * they are never culled and are drawn from both sides. Only the hull solid is
 * culled.
 *
 * The mast and the boom stay `Line3`, not prisms, because two E2E specs read
 * the boom's endpoints to verify the sign of `β` — the R3 guard the whole
 * project is built around. See task 1.5 and the debt table in the PRD.
 */

import { boomDirection, type HullDims } from './geometry'
import { sheerHeightAt } from './hullSolid'
import type { BoatModel, Line3, Tri, Vec3 } from './model3d'
import { sailCorners, sailPatch } from './SailShape'
import type { VisualDims } from './visualDims'
import type { RenderParams } from '../sim/useSimulation'

export interface RigPose {
  /** rad, boom angle (F2.1); `+β` puts the clew to starboard. */
  beta: number
  /** rad, rudder angle (F2.2); `+δr` puts the trailing edge to starboard. */
  deltaR: number
  /** rad, sail angle of attack. Sets the sign of the cosmetic camber only. */
  alpha: number
}

/** Spanwise strips, chosen so the depth sort stays local on the long thin plates. */
const BOARD_STRIPS = 3
const RUDDER_STRIPS = 2

const MAST_COLOUR = '#2b3a45'
const BOOM_COLOUR = '#2b3a45'

/**
 * A flat plate spanning `z ∈ [zLow, zHigh]`, with a chord running from `le` to
 * `te` in the horizontal plane, cut into `strips` spanwise bands.
 */
function plate(
  le: { x: number; y: number },
  te: { x: number; y: number },
  zLow: number,
  zHigh: number,
  strips: number,
  group: 'board' | 'rudder',
): Tri[] {
  const tris: Tri[] = []
  const at = (edge: { x: number; y: number }, z: number): Vec3 => ({ x: edge.x, y: edge.y, z })
  for (let i = 0; i < strips; i += 1) {
    const z0 = zLow + ((zHigh - zLow) * i) / strips
    const z1 = zLow + ((zHigh - zLow) * (i + 1)) / strips
    const push = (a: Vec3, b: Vec3, c: Vec3) =>
      tris.push({ a, b, c, material: 'foil', group, closed: false })
    push(at(le, z0), at(te, z0), at(te, z1))
    push(at(le, z0), at(te, z1), at(le, z1))
  }
  return tris
}

/**
 * The rig: sail patch, centreboard, rudder, and the mast and boom lines.
 *
 * At most 26 triangles — 12 sail, 6 board, 4 rudder — which
 * `triangle_budget` pins down.
 */
export function rigSolid(
  p: RenderParams,
  hull: HullDims,
  dims: VisualDims,
  pose: RigPose,
): BoatModel {
  const mastX = p.sail.mast_pos_b.x
  const corners = sailCorners(
    {
      mastX,
      boomLength: p.sail.boom_length,
      zBoom: p.sheet.z_boom,
      mastHeight: dims.mastHeight,
    },
    pose.beta,
  )

  // Centreboard: a plate on the centreline, clipped at the keel so it can
  // never poke up through the deck when the board is deep and the boat small.
  const boardHigh = Math.min(p.board.pos_b.z + dims.boardSpan / 2, dims.keelZ)
  const board = plate(
    { x: p.board.pos_b.x + dims.boardChord / 2, y: 0 },
    { x: p.board.pos_b.x - dims.boardChord / 2, y: 0 },
    p.board.pos_b.z - dims.boardSpan / 2,
    boardHigh,
    BOARD_STRIPS,
    'board',
  )

  // Rudder: the leading edge is the stock, the trailing edge is one chord aft
  // along `ĉ_r(δr) = (−cos δr, −sin δr, 0)` (F2.2). No clipping — the stock is
  // abaft the transom.
  const chord = boomDirection(pose.deltaR)
  const rudder = plate(
    { x: p.rudder.pos_b.x, y: p.rudder.pos_b.y },
    {
      x: p.rudder.pos_b.x + dims.rudderChord * chord.x,
      y: p.rudder.pos_b.y + dims.rudderChord * chord.y,
    },
    p.rudder.pos_b.z - dims.rudderSpan / 2,
    p.rudder.pos_b.z + dims.rudderSpan / 2,
    RUDDER_STRIPS,
    'rudder',
  )

  const lines: Line3[] = [
    {
      a: { x: mastX, y: 0, z: sheerHeightAt(mastX, hull, dims) },
      b: { x: mastX, y: 0, z: dims.mastTopZ },
      testId: 'boat-mast',
      widthPx: 2.5,
      colour: MAST_COLOUR,
    },
    {
      a: corners.tack,
      b: corners.clew,
      testId: 'boat-boom',
      widthPx: 2,
      colour: BOOM_COLOUR,
    },
  ]

  return {
    tris: [...sailPatch(corners, pose.alpha), ...board, ...rudder],
    lines,
  }
}

/** The whole boat: hull solid plus rig, in one model. */
export function boatModel(
  p: RenderParams,
  hull: HullDims,
  dims: VisualDims,
  pose: RigPose,
  hullTris: readonly Tri[],
): BoatModel {
  const rig = rigSolid(p, hull, dims, pose)
  return { tris: [...hullTris, ...rig.tris], lines: rig.lines }
}
