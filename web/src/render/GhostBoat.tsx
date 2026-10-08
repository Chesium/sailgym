import { boatTransform, type Camera, type Vec2 } from './Camera'
import { hullPath, type HullDims } from './geometry'
import type { ReplaySource } from '../sim/replay'
import { SNAPSHOT_FIELDS } from '../sim/snapshot'

/**
 * The baseline's recorded boat, drawn as a translucent ghost (v2 section 12,
 * task 12.5).
 *
 * ## It is a recording, not a second boat
 *
 * The ghost is the rule sailor's **recorded** episode played back beside the
 * player's attempt: no shared clock, no collision, no right-of-way and no
 * wind shadow (`brief.md` §3; S5 was not selected). This file therefore
 * reads no simulation, holds no state, computes no pose and runs no physics
 * (F8). It is handed a pose and it draws it.
 *
 * ## It does not interpolate the pose itself
 *
 * Interpolation between logged samples already exists, exactly once, in
 * `sim/replay.ts`'s `createReplaySource(episode).sampleAt(t)` — display only,
 * by its own documentation, and the one implementation the replay viewer uses.
 * {@link ghostPoseAt} and {@link ghostTrack} are thin, read-only adapters over
 * it so that task 12.8 does not have to write a second interpolator to put a
 * ghost on the water. A second one would be the `replay.ts` header's warning
 * in a new place: two slightly different smoothings of the same episode.
 *
 * ## It is a ghost, not a boat
 *
 * A plan-view hull outline and its track, and nothing else: no rig, no sail,
 * no sheet, no foils, no force arrows, no heel. Those belong to the boat the
 * player is sailing, and drawing them twice invites the picture to be read as
 * two live boats racing — which is the thing this section does **not** build.
 * The outline comes from `geometry.hullPath`, so there is still exactly one
 * hull outline in the repository and the ghost cannot drift from the boat.
 *
 * Every visual constant below is presentation only and carries its
 * provenance, in the discipline `render/visualDims.ts` already uses. None is a
 * length, and none is a value from the F7 catalogue.
 */

/**
 * `VISUAL` — **presentation only.** Opacity of the ghost's hull.
 *
 * 0.32: the hull reads as a shape and a heading while the water, the wind
 * field and the player's own boat stay legible straight through it. It is
 * what makes a ghost a ghost, and it is the reason nothing on it needs a
 * second colour to say "this is not you".
 */
export const GHOST_OPACITY = 0.32

/**
 * `VISUAL` — **presentation only.** Stroke width of the ghost's track, in
 * screen pixels.
 *
 * 1.5 px, the same as `Trajectory.tsx`'s own track: the two lines are the same
 * kind of thing — where a boat has been — and should be told apart by their
 * colour and opacity, not by weight.
 */
export const GHOST_TRACK_PX = 1.5

/**
 * `VISUAL` — **presentation only.** The ghost's colour.
 *
 * A neutral slate, taken from the page's existing hull outline colour
 * (`render/BoatSvg.tsx`'s `MATERIALS`): it is deliberately *not* the sail red
 * or the track blue the player's own boat uses, and deliberately not one of
 * `CourseOverlay`'s four state colours, so nothing on the water can be
 * mistaken for a verdict.
 */
export const GHOST_COLOUR = '#2b3a45'

/** Indices into an episode frame's F3 state (F8.3 order). */
const X = SNAPSHOT_FIELDS.indexOf('x')
const Y = SNAPSHOT_FIELDS.indexOf('y')
const PSI = SNAPSHOT_FIELDS.indexOf('psi')

/**
 * The pose a ghost is drawn at: a plan-view position and heading, nothing more.
 *
 * Structurally a subset of `sim/snapshot.ts`'s `Snapshot` and of
 * `render/BoatSvg.tsx`'s `BoatPose`, so either can be handed over directly.
 */
export interface GhostPose {
  /** m, world `x` (F2). */
  x: number
  /** m, world `y` (F2). */
  y: number
  /** rad, heading `ψ` (F2, F3). */
  psi: number
}

/**
 * The recorded pose at simulated time `t`, through `replay.ts`'s own sampler.
 *
 * Read-only and display-only: `sampleAt` is the single interpolator and this
 * is an index into the frame it returns. `null` for an empty episode.
 */
export function ghostPoseAt(source: ReplaySource, t: number): GhostPose | null {
  const frame = source.sampleAt(t)
  if (frame === null) {
    return null
  }
  return { x: frame.state[X], y: frame.state[Y], psi: frame.state[PSI] }
}

/**
 * The ghost's track up to simulated time `t`: the recorded samples at or
 * before `t`, then the interpolated pose at `t` itself.
 *
 * Every point is a stored frame's own `(x, y)` — the track is *read* out of
 * the episode, never integrated — and the final point is `sampleAt`'s, so the
 * line ends exactly where the drawn hull is.
 */
export function ghostTrack(source: ReplaySource, t: number): Vec2[] {
  const last = source.indexAt(t)
  if (last < 0) {
    return []
  }
  const points: Vec2[] = []
  for (let i = 0; i <= last; i += 1) {
    const frame = source.frameAt(i)
    if (frame !== null) {
      points.push({ x: frame.state[X], y: frame.state[Y] })
    }
  }
  const head = ghostPoseAt(source, t)
  if (head !== null) {
    const tail = points[points.length - 1]
    if (tail === undefined || tail.x !== head.x || tail.y !== head.y) {
      points.push({ x: head.x, y: head.y })
    }
  }
  return points
}

export interface GhostBoatProps {
  camera: Camera
  /** The pose the ghost is **handed** — from {@link ghostPoseAt}. */
  pose: GhostPose
  /** The hull's drawn dimensions, from `parameters_json()` (F7). */
  hull: HullDims
  /** World metres. The ghost's track, from {@link ghostTrack}. */
  track: readonly Vec2[]
}

/**
 * The ghost: a translucent plan-view hull at the pose it was handed, and its
 * track.
 *
 * The track is projected to screen pixels, as `Trajectory` does, so its
 * stroke keeps a constant width at any zoom. The hull is placed by
 * `Camera.boatTransform`, the same transform the player's boat uses, so the
 * two are drawn at the same scale and in the same frame.
 */
export function GhostBoat({ camera, pose, hull, track }: GhostBoatProps) {
  const projected = track
    .map((p) => {
      const s = camera.worldToScreen(p)
      return `${s.x.toFixed(2)},${s.y.toFixed(2)}`
    })
    .join(' ')
  return (
    <g data-testid="ghost-boat" style={{ pointerEvents: 'none' }}>
      <polyline
        data-testid="ghost-track"
        points={projected}
        fill="none"
        stroke={GHOST_COLOUR}
        strokeWidth={GHOST_TRACK_PX}
        strokeOpacity={GHOST_OPACITY}
      />
      <g
        data-testid="ghost-hull"
        transform={boatTransform(camera, { x: pose.x, y: pose.y }, pose.psi)}
      >
        <path
          d={hullPath(hull)}
          fill={GHOST_COLOUR}
          fillOpacity={GHOST_OPACITY}
          stroke={GHOST_COLOUR}
          strokeOpacity={GHOST_OPACITY}
          strokeWidth={GHOST_TRACK_PX}
          vectorEffect="non-scaling-stroke"
        />
      </g>
    </g>
  )
}
