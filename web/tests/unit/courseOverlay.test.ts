import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

import {
  createCamera,
  MAX_ZOOM,
  MIN_ZOOM,
  PIXELS_PER_METRE,
  type Camera,
  type Vec2,
  type Viewport,
} from '../../src/render/Camera'
import {
  chevronFor,
  courseDraw,
  formatDistance,
  isInViewport,
  CHEVRON_INSET_PX,
  CourseOverlay,
  WAYPOINT_LABEL_PX,
  type CourseBlock,
  type CourseWaypoint,
  type WaypointState,
} from '../../src/render/CourseOverlay'
import { GhostBoat, ghostPoseAt, ghostTrack } from '../../src/render/GhostBoat'
import { createReplaySource } from '../../src/sim/replay'
import { SNAPSHOT_FIELDS } from '../../src/sim/snapshot'
import type { Episode, EpisodeFrame, EpisodeHeader } from '../../src/sim/scenarioTypes'

/**
 * The course overlay, the ghost and the **Show course** camera (v2 section 12,
 * task 12.5).
 *
 * ## The shape this file takes, and why
 *
 * `vitest.config.ts` runs these tests in a `node` environment and the project
 * has **no React renderer** in its dev dependencies — and this section may not
 * add one. So the assertions are made against the *pure exported functions*
 * the components are built from: `courseDraw`, `chevronFor`, `formatDistance`,
 * `isInViewport`, `ghostPoseAt` and `ghostTrack`. Each component is a thin
 * map over those records, which is the same arrangement
 * `ui/TouchControls.tsx` (`fullTravel`) and `ui/Timeline.tsx`
 * (`timelineState`) already use and that `parameterSchema.test.ts` and
 * `replay.test.ts` already test through. The DOM-level half — that a
 * `waypoint-<n>` handle with its `data-state` actually reaches the page — is
 * task 12.9's `course.spec.ts`, in a real browser. `Camera.fitBounds`, the
 * **Show course** camera this task also adds, is tested beside the rest of
 * the camera in `camera.test.ts`.
 *
 * ## What is being asserted, stated once
 *
 * That the overlay **decides nothing** (RV73). Every waypoint state, every
 * post and every "which leg is current" flag that comes out of `courseDraw`
 * is the one that went in, and the only arithmetic is projection. The
 * geometric assertions here are about the *drawing* — to scale, constant
 * label size, the right edge of the viewport — never about passage.
 */

const VIEWPORT: Viewport = { width: 780, height: 520 }
const SQUARE: Viewport = { width: 520, height: 520 }

function cameraAt(zoom: number, centre: Vec2 = { x: 0, y: 0 }, viewport = VIEWPORT): Camera {
  return createCamera({ mode: 'northUp', zoom, centre, heading: 0, viewport })
}

/**
 * A waypoint as Rust hands one over: the posts are **given**, square to the
 * leg and centred on the mark (D2), and are never recomputed here.
 */
function waypoint(
  n: number,
  x: number,
  y: number,
  state: WaypointState,
  posts: readonly [readonly [number, number], readonly [number, number]],
  radius = 5,
): CourseWaypoint {
  return { n, x, y, radius, posts, state }
}

/** `courses/reach.json`'s geometry, with the posts `Route::waypoints` gives it. */
function reachBlock(overrides: Partial<CourseBlock> = {}): CourseBlock {
  return {
    start: { x: 0, y: 0 },
    waypoints: [
      waypoint(1, 30, 0, 'passed', [
        [30, -5],
        [30, 5],
      ]),
      waypoint(2, 60, -10, 'next', [
        [56.84, -13.95],
        [63.16, -6.05],
      ]),
      waypoint(3, 90, 0, 'pending', [
        [86.84, -3.95],
        [93.16, 3.95],
      ]),
    ],
    next: 2,
    distance_to_next: 18.4,
    splits: [12.5],
    ...overrides,
  }
}

describe('course overlay', () => {
  it('label_count_and_states_follow_the_props', () => {
    const camera = cameraAt(1, { x: 45, y: -5 })
    const block = reachBlock()
    const draw = courseDraw(camera, block)

    // One label per waypoint, in the props' order, carrying the props' number.
    expect(draw.waypoints).toHaveLength(block.waypoints.length)
    expect(draw.waypoints.map((w) => w.n)).toEqual([1, 2, 3])
    expect(draw.waypoints.map((w) => w.label.px)).toEqual([
      WAYPOINT_LABEL_PX,
      WAYPOINT_LABEL_PX,
      WAYPOINT_LABEL_PX,
    ])

    // The state is the props' word, verbatim, in the record, the test handle
    // and the class. Nothing here asks where the boat is — the component is
    // not even given a boat.
    expect(draw.waypoints.map((w) => w.state)).toEqual(['passed', 'next', 'pending'])
    expect(draw.waypoints.map((w) => w.testId)).toEqual([
      'waypoint-1',
      'waypoint-2',
      'waypoint-3',
    ])
    expect(draw.waypoints.map((w) => w.className)).toEqual([
      'course-waypoint course-waypoint--passed',
      'course-waypoint course-waypoint--next',
      'course-waypoint course-waypoint--pending',
    ])

    // All four of Rust's words survive the round trip, `missed` included —
    // the overlay never second-guesses a cut.
    const states: WaypointState[] = ['passed', 'next', 'pending', 'missed']
    const all = courseDraw(camera, {
      ...block,
      waypoints: states.map((s, i) =>
        waypoint(i + 1, 10 * i, 0, s, [
          [10 * i, -5],
          [10 * i, 5],
        ]),
      ),
    })
    expect(all.waypoints.map((w) => w.state)).toEqual(states)
    expect(all.waypoints.map((w) => w.className.endsWith(w.state))).toEqual([
      true,
      true,
      true,
      true,
    ])

    // One leg per waypoint when the route has a start, and the current one is
    // the leg arriving at `next` — read from `next`, not inferred.
    expect(draw.legs.map((l) => l.n)).toEqual([1, 2, 3])
    expect(draw.legs.map((l) => l.current)).toEqual([false, true, false])
    expect(draw.legs.map((l) => l.className)).toEqual([
      'course-leg course-leg--other',
      'course-leg course-leg--current',
      'course-leg course-leg--other',
    ])

    // A circuit has no `start`, so leg 0 has no beginning and is not drawn.
    // Nothing invents one.
    const circuit = courseDraw(camera, { ...reachBlock(), start: null })
    expect(circuit.legs.map((l) => l.n)).toEqual([2, 3])

    // With the course complete there is no current leg and no chevron.
    const done = courseDraw(camera, {
      ...reachBlock(),
      next: null,
      distance_to_next: null,
    })
    expect(done.legs.every((l) => !l.current)).toBe(true)
    expect(done.chevron).toBe(null)

    expect(typeof CourseOverlay).toBe('function')
  })

  it('posts_and_positions_are_projected_and_nothing_else', () => {
    const camera = cameraAt(1.7, { x: 45, y: -5 })
    const block = reachBlock()
    const draw = courseDraw(camera, block)

    block.waypoints.forEach((wp, i) => {
      const record = draw.waypoints[i]
      // The mark is where the camera says the mark's own coordinates are.
      expect(record.centre).toEqual(camera.worldToScreen({ x: wp.x, y: wp.y }))
      // The two posts are the **given** posts, projected. Not `p ± w · n̂`
      // recomputed here, and `half_width` is not recovered from them (RV65).
      expect(record.posts[0]).toEqual(
        camera.worldToScreen({ x: wp.posts[0][0], y: wp.posts[0][1] }),
      )
      expect(record.posts[1]).toEqual(
        camera.worldToScreen({ x: wp.posts[1][0], y: wp.posts[1][1] }),
      )
      // The label sits on the mark; the leg ends on it.
      expect(record.label.x).toBe(record.centre.x)
      expect(record.label.y).toBe(record.centre.y)
      expect(draw.legs[i].b).toEqual(record.centre)
    })
    // Leg 0 begins at `Route::start`, projected.
    expect(draw.legs[0].a).toEqual(camera.worldToScreen({ x: 0, y: 0 }))
  })

  it('marks_are_to_scale_and_label_size_is_constant_across_zoom', () => {
    const block = reachBlock()
    const sizes = new Set<number>()
    const radii: number[] = []
    for (const zoom of [MIN_ZOOM, 0.37, 1, 2.5, 9, MAX_ZOOM]) {
      const camera = cameraAt(zoom, { x: 45, y: -5 })
      const draw = courseDraw(camera, block)
      for (const wp of draw.waypoints) {
        sizes.add(wp.label.px)
      }
      // To scale: the drawn radius is the mark's own world radius at this
      // zoom, to the bit.
      for (let i = 0; i < block.waypoints.length; i += 1) {
        expect(draw.waypoints[i].radiusPx).toBe(
          block.waypoints[i].radius * PIXELS_PER_METRE * zoom,
        )
      }
      radii.push(draw.waypoints[0].radiusPx)
    }
    // One label size across the whole zoom range, and the circles did move.
    expect([...sizes]).toEqual([WAYPOINT_LABEL_PX])
    expect(new Set(radii).size).toBe(radii.length)
  })

  it('the_chevron_is_absent_when_the_next_waypoint_is_in_view', () => {
    // Zoomed out enough that all three waypoints of `reach` are on screen.
    const camera = cameraAt(0.39, { x: 45, y: -5 })
    const draw = courseDraw(camera, reachBlock())
    expect(draw.waypoints.every((w) => w.inView)).toBe(true)
    expect(draw.chevron).toBe(null)

    // Zoomed in on the start, waypoint 2 is off screen and the chevron is back.
    const near = cameraAt(4, { x: 0, y: 0 })
    const nearDraw = courseDraw(near, reachBlock())
    expect(nearDraw.waypoints[1].inView).toBe(false)
    expect(nearDraw.chevron).not.toBe(null)

    // Exactly on the edge of the viewport still counts as in view, so the
    // chevron does not flicker in and out along the boundary.
    const edge = cameraAt(1, { x: 0, y: 0 })
    const atEdge = edge.screenToWorld({ x: VIEWPORT.width, y: VIEWPORT.height / 2 })
    expect(isInViewport(edge, edge.worldToScreen(atEdge))).toBe(true)
    expect(
      chevronFor(edge, atEdge, { n: 2, state: 'next', distanceLabel: '' }),
    ).toBe(null)
  })

  it('the_chevron_sits_on_the_correct_edge_in_all_eight_octants', () => {
    const camera = cameraAt(1, { x: 0, y: 0 })
    const cx = VIEWPORT.width / 2
    const cy = VIEWPORT.height / 2
    const insetX = cx - CHEVRON_INSET_PX
    const insetY = cy - CHEVRON_INSET_PX

    // World directions (F2: +x east, +y north) and, for each, the screen side
    // the chevron must be on and the edge it must be pinned to on a 780 × 520
    // view. The view is wider than it is tall, so a diagonal leaves through
    // the top or the bottom — which is a fact about the viewport's aspect
    // ratio, and the reason the four diagonals do not expect a corner here.
    const octants: {
      name: string
      dir: Vec2
      edge: string
      /** Expected sign of `x − cx` and of `y − cy`; 0 means exactly centred. */
      sx: number
      sy: number
      angleDeg: number
    }[] = [
      { name: 'N', dir: { x: 0, y: 1 }, edge: 'top', sx: 0, sy: -1, angleDeg: 0 },
      { name: 'NE', dir: { x: 1, y: 1 }, edge: 'top', sx: 1, sy: -1, angleDeg: 45 },
      { name: 'E', dir: { x: 1, y: 0 }, edge: 'right', sx: 1, sy: 0, angleDeg: 90 },
      { name: 'SE', dir: { x: 1, y: -1 }, edge: 'bottom', sx: 1, sy: 1, angleDeg: 135 },
      { name: 'S', dir: { x: 0, y: -1 }, edge: 'bottom', sx: 0, sy: 1, angleDeg: 180 },
      { name: 'SW', dir: { x: -1, y: -1 }, edge: 'bottom', sx: -1, sy: 1, angleDeg: -135 },
      { name: 'W', dir: { x: -1, y: 0 }, edge: 'left', sx: -1, sy: 0, angleDeg: -90 },
      { name: 'NW', dir: { x: -1, y: 1 }, edge: 'top', sx: -1, sy: -1, angleDeg: -45 },
    ]

    for (const o of octants) {
      const target = { x: o.dir.x * 100, y: o.dir.y * 100 }
      const chevron = chevronFor(camera, target, {
        n: 7,
        state: 'next',
        distanceLabel: '100 m',
      })
      expect(chevron, o.name).not.toBe(null)
      if (chevron === null) {
        continue
      }
      expect(chevron.edge, o.name).toBe(o.edge)
      expect(chevron.n).toBe(7)
      expect(chevron.distanceLabel).toBe('100 m')
      expect(chevron.angleDeg, o.name).toBeCloseTo(o.angleDeg, 9)

      // On the inset rectangle, and wholly inside the viewport.
      const onVertical = Math.abs(Math.abs(chevron.at.x - cx) - insetX) < 1e-9
      const onHorizontal = Math.abs(Math.abs(chevron.at.y - cy) - insetY) < 1e-9
      expect(onVertical || onHorizontal, o.name).toBe(true)
      expect(Math.abs(chevron.at.x - cx), o.name).toBeLessThanOrEqual(insetX + 1e-9)
      expect(Math.abs(chevron.at.y - cy), o.name).toBeLessThanOrEqual(insetY + 1e-9)

      // And on the correct side of centre for this octant.
      expect(Math.sign(Number((chevron.at.x - cx).toFixed(9))), o.name).toBe(o.sx)
      expect(Math.sign(Number((chevron.at.y - cy).toFixed(9))), o.name).toBe(o.sy)
    }

    // On a square viewport an exact diagonal reaches both half-extents at
    // once, and the chevron is pinned to the **corner**. All four of them.
    const square = cameraAt(1, { x: 0, y: 0 }, SQUARE)
    const corners: [Vec2, string][] = [
      [{ x: 100, y: 100 }, 'top-right'],
      [{ x: 100, y: -100 }, 'bottom-right'],
      [{ x: -100, y: -100 }, 'bottom-left'],
      [{ x: -100, y: 100 }, 'top-left'],
    ]
    for (const [target, edge] of corners) {
      const chevron = chevronFor(square, target, {
        n: 1,
        state: 'next',
        distanceLabel: '',
      })
      expect(chevron?.edge, edge).toBe(edge)
    }
  })

  it('the_chevron_carries_the_number_and_rusts_distance_formatted', () => {
    const camera = cameraAt(4, { x: 0, y: 0 })
    const draw = courseDraw(camera, reachBlock())
    expect(draw.chevron?.n).toBe(2)
    expect(draw.chevron?.state).toBe('next')
    expect(draw.chevron?.distanceLabel).toBe('18 m')

    // Formatting only: metres at a UI boundary (F1), and "not known" is the
    // empty string rather than a zero.
    expect(formatDistance(18.4)).toBe('18 m')
    expect(formatDistance(4.26)).toBe('4.3 m')
    expect(formatDistance(0)).toBe('0.0 m')
    expect(formatDistance(null)).toBe('')
    expect(formatDistance(undefined)).toBe('')
    expect(formatDistance(Number.NaN)).toBe('')

    // A `next` that names no waypoint draws no chevron rather than guessing.
    expect(courseDraw(camera, { ...reachBlock(), next: 9 }).chevron).toBe(null)
  })
})

describe('ghost boat', () => {
  /**
   * A minimal episode. `createReplaySource` reads only `log_hz` and
   * `schema_version` off the header, and the ghost reads only `x`, `y` and
   * `psi` off a frame, so the cast keeps the fixture to the fields under test
   * — `replay.test.ts` owns the full-header cases.
   */
  const HEADER = { schema_version: 2, log_hz: 10 } as unknown as EpisodeHeader

  function frame(t: number, x: number, y: number, psi: number): EpisodeFrame {
    const state = SNAPSHOT_FIELDS.map(() => 0)
    state[SNAPSHOT_FIELDS.indexOf('x')] = x
    state[SNAPSHOT_FIELDS.indexOf('y')] = y
    state[SNAPSHOT_FIELDS.indexOf('psi')] = psi
    return {
      t,
      state,
      controls: [0, 0, 0],
      wind_at_boat: [0, 0],
      forces: Array.from({ length: 12 }, () => 0),
      moments: Array.from({ length: 4 }, () => 0),
      sheet_tension: 0,
      reward: 0,
      capsized: false,
      diag: null,
    }
  }

  const episode: Episode = {
    header: HEADER,
    frames: [frame(0, 0, 0, 0), frame(1, 10, 0, 0.5), frame(2, 20, 10, 1)],
  }

  it('the_pose_comes_from_replays_own_sampler', () => {
    const source = createReplaySource(episode)

    // Exactly on a frame: that frame's values, unchanged.
    expect(ghostPoseAt(source, 1)).toEqual({ x: 10, y: 0, psi: 0.5 })

    // Between frames: `sampleAt`'s interpolation, not a second one. Asserted
    // by agreeing with `sampleAt` to the bit rather than by recomputing it.
    const mid = source.sampleAt(1.5)
    expect(mid).not.toBe(null)
    expect(ghostPoseAt(source, 1.5)).toEqual({
      x: mid?.state[SNAPSHOT_FIELDS.indexOf('x')],
      y: mid?.state[SNAPSHOT_FIELDS.indexOf('y')],
      psi: mid?.state[SNAPSHOT_FIELDS.indexOf('psi')],
    })

    // An empty episode is a source with no frames, not a throw.
    expect(ghostPoseAt(createReplaySource({ header: HEADER, frames: [] }), 0)).toBe(null)
    expect(typeof GhostBoat).toBe('function')
  })

  it('the_track_is_read_out_of_the_episode', () => {
    const source = createReplaySource(episode)

    // Every stored sample at or before the playhead, then the pose at it.
    expect(ghostTrack(source, 1)).toEqual([
      { x: 0, y: 0 },
      { x: 10, y: 0 },
    ])
    const part = ghostTrack(source, 1.5)
    expect(part).toHaveLength(3)
    expect(part[2]).toEqual({ x: 15, y: 5 })
    expect(ghostTrack(source, 9)).toEqual([
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 20, y: 10 },
    ])
    expect(ghostTrack(createReplaySource({ header: HEADER, frames: [] }), 0)).toEqual([])
  })
})

describe('the discipline', () => {
  const FILES = ['src/render/CourseOverlay.tsx', 'src/render/GhostBoat.tsx']

  it('every_visual_constant_has_provenance', () => {
    // The same walk-back as `visualDims.test.ts`'s own provenance test, over
    // this task's two new files: an exported constant must be documented as
    // presentation only, immediately above its declaration.
    for (const file of FILES) {
      const lines = readFileSync(file, 'utf8').split('\n')
      const exported: string[] = []
      const documented: string[] = []
      lines.forEach((line, i) => {
        const match = /^export const (\w+)/.exec(line)
        if (match === null) {
          return
        }
        exported.push(match[1])
        const block: string[] = []
        for (let k = i - 1; k >= 0; k -= 1) {
          const above = lines[k].trim()
          if (above === '') {
            break
          }
          if (!(above.startsWith('*') || above.startsWith('/*') || above.startsWith('//'))) {
            break
          }
          block.unshift(above)
        }
        if (/presentation only/.test(block.join(' '))) {
          documented.push(match[1])
        }
      })
      expect(exported.length, file).toBeGreaterThan(0)
      expect(documented, file).toEqual(exported)
    }
  })

  it('no_metre_value_is_hard_coded_in_the_overlay_or_the_ghost', () => {
    // Section 01's rule (normative delta D2), extended to this task's files:
    // the ILCA defaults for LOA, LWL, beam, boom length, sail area, CE height
    // and mast height may not appear in the render layer. Every number in
    // these two files is a count of screen pixels or a dimensionless ratio;
    // every length they draw arrives through a prop.
    const forbidden = /4\.23|3\.81|1\.37|2\.72|7\.06|2\.40|5\.89/
    for (const file of FILES) {
      expect(forbidden.test(readFileSync(file, 'utf8')), file).toBe(false)
    }
  })

  it('no_course_logic_lives_in_the_overlay', () => {
    // RV73, as far as a source check can carry it: the overlay names no
    // passage concept of its own. The browser-side proof is that every state
    // it draws is a word it was handed, which
    // `label_count_and_states_follow_the_props` asserts; this is the guard
    // against a helper creeping in later.
    const source = readFileSync('src/render/CourseOverlay.tsx', 'utf8')
    const body = source
      .split('\n')
      .filter((line) => {
        const t = line.trim()
        return !(t.startsWith('*') || t.startsWith('/*') || t.startsWith('//'))
      })
      .join('\n')
    for (const forbidden of ['passed_between', 'cut_between', 'left_normal', 'half_width']) {
      expect(body.includes(forbidden), forbidden).toBe(false)
    }
  })
})
