import type { Camera, Vec2 } from './Camera'

/**
 * The numbered waypoint course, drawn over the world view (v2 section 12,
 * task 12.5).
 *
 * ## Nothing here decides anything about the course
 *
 * **RV73 is the governing risk and this file is where it would fire.** No
 * passage decision, no gate post, no waypoint state and no split is computed
 * in TypeScript (F8, and section 11's rule for `PracticePanel.tsx` applied to
 * the overlay). Every number below arrives through {@link CourseBlock}, which
 * mirrors `practice_state_json`'s `course` block field for field; this file
 * projects world metres to screen pixels, formats a distance, and chooses
 * colours. It subtracts nothing geometric.
 *
 * In particular:
 *
 * - a waypoint's two **posts** arrive as world points, because the posts are
 *   `Rounding::Gate`'s own and `Route::waypoints` decided them (D2). They are
 *   never derived here from the waypoint and a normal — that derivation is a
 *   second definition of a gate, which is exactly RV65/RV66 in the web layer;
 *   nor is `half_width` recovered by halving the distance between them;
 * - a waypoint's **state** arrives as one of four words, because
 *   `sailgym-task` decided it on a physics step. The overlay maps the word to
 *   a class, a `data-state` and a colour and does not ask whether the boat is
 *   past the line;
 * - which leg is **current** is `next`, read, not inferred from a position;
 * - the **distance** to the next waypoint arrives in metres from Rust.
 *   {@link formatDistance} rounds it for display. It is never measured here.
 *
 * ## Why the drawing is projected rather than placed in the world group
 *
 * Exactly `Trajectory.tsx`'s reason: points are projected to screen pixels
 * here, so every stroke keeps a constant width and every label a constant
 * type size at any zoom. The *marks themselves* are still drawn to scale —
 * their radius is a world length multiplied by `camera.scale` — so a waypoint
 * grows as you zoom in while its number does not.
 *
 * ## The decisions are exported functions, not JSX
 *
 * `web/tests/unit/` runs under vitest in a `node` environment with no React
 * renderer, and this section may not add a dependency. The component is
 * therefore a thin map over {@link courseDraw}, {@link chevronFor} and
 * {@link formatDistance}, which are pure and are what
 * `tests/unit/courseOverlay.test.ts` asserts against. The precedent is
 * `ui/TouchControls.tsx`'s `fullTravel` and `ui/Timeline.tsx`'s
 * `timelineState`, both already tested this way.
 *
 * ## Every visual constant is presentation only
 *
 * Each one below carries a provenance note, in the discipline
 * `render/visualDims.ts` and `App.tsx`'s `WORLD_PX` already use. They are
 * counts of screen pixels and dimensionless opacities; none is a length,
 * none is a physical coefficient, none is a value from the F7 catalogue, and
 * none of them crosses the F8 boundary. None was chosen to make a course look
 * easier than it is.
 */

// ---------------------------------------------------------------------------
// The props, which are `practice_state_json`'s `course` block
// ---------------------------------------------------------------------------

/**
 * `sailgym_task`'s verdict on one waypoint, as the `course` block spells it.
 *
 * Four words, decided in Rust on a physics step (D3): `passed` is through the
 * gate, `next` is the one being sailed to, `pending` is later in the order and
 * `missed` is a cut — the attempt continues and the player must come back
 * through the line.
 */
export type WaypointState = 'passed' | 'next' | 'pending' | 'missed'

/** A world point as the `course` block writes a post: the pair `[x, y]`, metres (F2). */
export type PostXY = readonly [number, number]

/**
 * One numbered waypoint, as `practice_state_json`'s `course` block carries it
 * (D5: `waypoints: [{n, x, y, posts, state}]`).
 *
 * `radius` is the one field this interface names that D5's sketch does not
 * list. It is **not** invented here: it is Rust's `Mark::radius`, which
 * `Route::waypoints` sets to the course's `half_width` (D2), and the overlay
 * needs it because the numbered circle is drawn *to scale* in world metres.
 * Task 12.7 emits it; the alternative — halving the distance between the two
 * posts — would be this file deciding a piece of course geometry, which is
 * the thing RV73 forbids.
 */
export interface CourseWaypoint {
  /** 1-based waypoint number, as the player sees it and as events report it. */
  n: number
  /** m, world `x` (F2: east). */
  x: number
  /** m, world `y` (F2: north). */
  y: number
  /** m, `Mark::radius` — the course's `half_width` (D2). Drawn, never used to decide. */
  radius: number
  /** The gate's two posts, in world metres, straight out of `Rounding::Gate`. */
  posts: readonly [PostXY, PostXY]
  /** Rust's verdict. */
  state: WaypointState
}

/**
 * `practice_state_json`'s `course` block.
 *
 * Mirrors the Rust shape, `snake_case` included, exactly as
 * `sim/scenarioTypes.ts`'s practice types already do — so the block can be
 * handed straight through by task 12.8 with no translation layer to drift.
 *
 * `start` is the second field D5's sketch does not list and this file needs:
 * leg 0 of an open route runs from `Route::start` (D1), so without it the
 * first leg line has no beginning. It is spelled as Rust's `Vec2`
 * serialises — `{ "x": …, "y": … }`, exactly as `courses/*.json` writes it —
 * so task 12.7 can serialise `Route.start` into it unchanged. When it is
 * `null` the first leg line is simply **not drawn**: a guessed origin is an
 * invented piece of course geometry.
 */
export interface CourseBlock {
  waypoints: readonly CourseWaypoint[]
  /** `Route::start`, or `null` for a circuit (D1). */
  start: Vec2 | null
  /** The 1-based number of the waypoint being sailed to, or `null` when the course is complete. */
  next: number | null
  /** m, Rust's distance from the boat to `next`; `null` when there is none. */
  distance_to_next: number | null
  /** s, the elapsed time at each passage so far, in passage order. */
  splits: readonly number[]
}

// ---------------------------------------------------------------------------
// Visual constants — presentation only, every one with its provenance
// ---------------------------------------------------------------------------

/**
 * `VISUAL` — **presentation only.** Stroke width of a waypoint circle and its
 * gate bar, in screen pixels.
 *
 * 2.5 px, which is `Trajectory.tsx`'s 1.5 px track plus a pixel: the course is
 * the thing being sailed *to* and must read in front of the boat's own track
 * and the 1 px world grid, at the 344 × 202 world view a 360 × 640 phone gets
 * (`progress/09-handoff.md` §3.3). It is a count of screen pixels and is
 * independent of zoom, which is why the overlay projects rather than scales.
 */
export const MARK_STROKE_PX = 2.5

/**
 * `VISUAL` — **presentation only.** Stroke width of a gate bar, screen pixels.
 *
 * 3.5 px: one pixel heavier than the circle, because the bar is the thing the
 * boat must cross and the circle is only where the mark is. Nothing in the
 * rule depends on it; `passage.rs` ignores a gate's drawn width entirely.
 */
export const GATE_STROKE_PX = 3.5

/**
 * `VISUAL` — **presentation only.** Stroke width of a leg line, screen pixels.
 *
 * 2 px, lighter than the gate bar it ends at, so the eye follows the route
 * without the legs competing with the waypoints.
 */
export const LEG_STROKE_PX = 2

/**
 * `VISUAL` — **presentation only.** Dash pattern for a leg that is not the
 * current one, in screen pixels: 8 px on, 4 px off.
 *
 * A 2:1 ratio reads as a dashed line rather than a dotted one at phone sizes,
 * and the current leg is drawn with no dash at all, so "which leg am I on" is
 * carried by the stroke and not only by a colour (one of the two channels, for
 * a colour-blind player).
 */
export const LEG_DASH_PX: readonly [number, number] = [8, 4]

/**
 * `VISUAL` — **presentation only.** Type size of a waypoint's number, in
 * screen pixels, **constant at every zoom**.
 *
 * 13 px: one pixel above the 12 px the page's muted captions already use
 * (`ui/PracticePanel.tsx`'s `MUTED`), because this label sits over water and
 * a moving boat rather than on a panel. It is deliberately *not* a world
 * length: a label that scaled with the camera would be unreadable at the
 * zoom that shows the whole course, which is precisely the zoom **Show
 * course** selects.
 */
export const WAYPOINT_LABEL_PX = 13

/**
 * `VISUAL` — **presentation only.** How far inside the viewport edge the
 * off-screen chevron sits, in screen pixels.
 *
 * 26 px ≈ 2 × the 13 px label, so the chevron and the number it carries are
 * wholly inside the view at the 344 px-wide world view of a 360 × 640 phone
 * and are not clipped by the SVG's own edge.
 */
export const CHEVRON_INSET_PX = 26

/**
 * `VISUAL` — **presentation only.** Half-height of the chevron triangle, in
 * screen pixels.
 *
 * 11 px, so the arrow is about the size of the number beside it and the pair
 * reads as one marker. A count of pixels; it has no world meaning.
 */
export const CHEVRON_SIZE_PX = 11

/**
 * `VISUAL` — **presentation only.** Width of the white halo behind a label,
 * in screen pixels.
 *
 * 3 px, drawn under the glyph with `paint-order: stroke`, so a number stays
 * legible over the wind field, the grid and the boat's own track. A contrast
 * device and nothing more.
 */
export const LABEL_HALO_PX = 3

/**
 * `VISUAL` — **presentation only.** Opacity of everything that is not the
 * current leg or the next waypoint.
 *
 * 0.42: faint enough that the next waypoint is unmistakable, opaque enough
 * that the shape of the whole course is still legible — which is what the
 * **Show course** camera exists to show.
 */
export const FAINT_OPACITY = 0.42

/**
 * `VISUAL` — **presentation only.** One colour per Rust-decided state.
 *
 * Taken from the palette the page already uses (`render/BoatSvg.tsx`'s
 * `MATERIALS`, `Trajectory.tsx`): a green for done, an amber for the one in
 * hand, the existing slate `#2b3a45` muted for later, and a red for a cut.
 * The state is carried by the `data-state` attribute and the class as well as
 * by the colour, so no decision rests on a hue alone.
 */
export const STATE_COLOUR: Record<WaypointState, string> = {
  passed: '#2f7d4f',
  next: '#e08a1e',
  pending: '#6b7a85',
  missed: '#b5342a',
}

// ---------------------------------------------------------------------------
// Pure geometry of the *drawing* — projection and placement, nothing else
// ---------------------------------------------------------------------------

/** Whether a screen point lies within the camera's viewport rectangle. */
export function isInViewport(camera: Camera, screen: Vec2): boolean {
  const { width, height } = camera.viewport
  return screen.x >= 0 && screen.x <= width && screen.y >= 0 && screen.y <= height
}

/**
 * A distance, formatted for a chevron label.
 *
 * Metres are F1's unit and this is a UI boundary, so the number is shown as
 * metres with one decimal below 10 m and rounded above it — a boat length of
 * precision where it matters and none where it does not. `null` and any
 * non-finite value format as the empty string, because "the distance is not
 * known" is not "0 m".
 */
export function formatDistance(metres: number | null | undefined): string {
  if (metres === null || metres === undefined || !Number.isFinite(metres)) {
    return ''
  }
  return metres < 10 ? `${metres.toFixed(1)} m` : `${Math.round(metres)} m`
}

/** One waypoint, ready to draw: screen pixels throughout. */
export interface WaypointDraw {
  n: number
  state: WaypointState
  /** px, the mark's centre on screen. */
  centre: Vec2
  /** px, the mark's own radius at this zoom — `radius · camera.scale`. */
  radiusPx: number
  /** px, the two gate posts on screen, in the order Rust gave them. */
  posts: readonly [Vec2, Vec2]
  /** px, where the number goes and the **constant** size it is set at. */
  label: { x: number; y: number; px: number }
  /** `waypoint-<n>`, the handle task 12.8's browser spec addresses. */
  testId: string
  className: string
  /** Whether the mark's centre is inside the viewport. */
  inView: boolean
}

/** One leg line, ready to draw. */
export interface LegDraw {
  /** The waypoint this leg arrives at. */
  n: number
  /** px, from the previous waypoint — or `Route::start` for leg 0. */
  a: Vec2
  /** px, the waypoint. */
  b: Vec2
  /** The leg being sailed, i.e. the one arriving at `next`. Drawn solid. */
  current: boolean
  className: string
}

/** Which edge, or corner, of the viewport a chevron is pinned to. */
export type ChevronEdge =
  | 'top'
  | 'bottom'
  | 'left'
  | 'right'
  | 'top-left'
  | 'top-right'
  | 'bottom-left'
  | 'bottom-right'

/** The off-screen marker for the next waypoint, ready to draw. */
export interface ChevronDraw {
  n: number
  state: WaypointState
  /** px, on the viewport rectangle inset by {@link CHEVRON_INSET_PX}. */
  at: Vec2
  /** deg, clockwise from screen up — the SVG `rotate` the arrow needs to point at the waypoint. */
  angleDeg: number
  edge: ChevronEdge
  /** {@link formatDistance} of the block's `distance_to_next`; `''` when unknown. */
  distanceLabel: string
  /** px, half-height of the arrow. */
  sizePx: number
}

/**
 * Where the off-screen marker for `target` goes, or `null` when `target` is
 * in view.
 *
 * The marker is the intersection of the ray from the viewport centre towards
 * the target with the viewport rectangle **inset** by
 * {@link CHEVRON_INSET_PX}, so it is always wholly on screen. The edge it
 * lands on is whichever of the two half-extents the ray reaches first, which
 * is a statement about the viewport's aspect ratio and not about the world: a
 * target due north-east of a wide, short view leaves through the *top*, and
 * the eight-octant test in `courseOverlay.test.ts` asserts exactly that.
 *
 * A ray that reaches both at once leaves through a **corner**, and the edge
 * is then the pair. The comparison is relative rather than exact because the
 * two parameters are each a quotient: on a square viewport an exact diagonal
 * gives them the same value mathematically and can give them neighbouring
 * doubles.
 */
export function chevronFor(
  camera: Camera,
  target: Vec2,
  marker: { n: number; state: WaypointState; distanceLabel: string },
): ChevronDraw | null {
  const screen = camera.worldToScreen(target)
  if (!Number.isFinite(screen.x) || !Number.isFinite(screen.y)) {
    return null
  }
  if (isInViewport(camera, screen)) {
    return null
  }
  const { width, height } = camera.viewport
  const cx = width / 2
  const cy = height / 2
  const hx = Math.max(0, cx - CHEVRON_INSET_PX)
  const hy = Math.max(0, cy - CHEVRON_INSET_PX)
  const dx = screen.x - cx
  const dy = screen.y - cy
  // `screen` is outside the rectangle, so at least one component is non-zero
  // and at least one parameter below is finite.
  const tx = dx === 0 ? Infinity : hx / Math.abs(dx)
  const ty = dy === 0 ? Infinity : hy / Math.abs(dy)
  const t = Math.min(tx, ty)
  if (!Number.isFinite(t)) {
    return null
  }

  const vertical = dy < 0 ? 'top' : 'bottom'
  const horizontal = dx < 0 ? 'left' : 'right'
  // Both parameters must be *finite* to be equal: a target due north has
  // `dx === 0`, so `tx` is `Infinity`, and `Infinity − ty` compared against
  // `1e-9 · Infinity` would call that a corner.
  const corner =
    Number.isFinite(tx) &&
    Number.isFinite(ty) &&
    Math.abs(tx - ty) <= 1e-9 * Math.max(tx, ty)
  let edge: ChevronEdge
  if (corner) {
    edge = `${vertical}-${horizontal}` as ChevronEdge
  } else if (tx < ty) {
    edge = horizontal
  } else {
    edge = vertical
  }

  return {
    n: marker.n,
    state: marker.state,
    at: { x: cx + dx * t, y: cy + dy * t },
    // Screen up is `(0, −1)`; SVG rotation is clockwise. `atan2(dx, −dy)` is
    // therefore 0 for up, +90 for right, 180 for down.
    angleDeg: (Math.atan2(dx, -dy) * 180) / Math.PI,
    edge,
    distanceLabel: marker.distanceLabel,
    sizePx: CHEVRON_SIZE_PX,
  }
}

/** Everything the overlay draws this frame. */
export interface CourseDraw {
  waypoints: readonly WaypointDraw[]
  legs: readonly LegDraw[]
  /** Non-null only when the **next** waypoint is off screen. */
  chevron: ChevronDraw | null
}

/**
 * The whole overlay as draw records: one per waypoint, one per leg that has
 * both ends, and at most one chevron.
 *
 * Pure, and the component is a map over it. Every state, every post and every
 * `current` flag is read from `course`; the only arithmetic is projection
 * (`camera.worldToScreen`), one multiplication by `camera.scale` for the
 * drawn radius, and the chevron's placement in screen pixels.
 */
export function courseDraw(camera: Camera, course: CourseBlock): CourseDraw {
  const toScreen = (p: Vec2) => camera.worldToScreen(p)
  const waypoints: WaypointDraw[] = course.waypoints.map((wp) => {
    const centre = toScreen({ x: wp.x, y: wp.y })
    return {
      n: wp.n,
      state: wp.state,
      centre,
      radiusPx: wp.radius * camera.scale,
      posts: [
        toScreen({ x: wp.posts[0][0], y: wp.posts[0][1] }),
        toScreen({ x: wp.posts[1][0], y: wp.posts[1][1] }),
      ] as const,
      label: { x: centre.x, y: centre.y, px: WAYPOINT_LABEL_PX },
      testId: `waypoint-${wp.n}`,
      className: `course-waypoint course-waypoint--${wp.state}`,
      inView: isInViewport(camera, centre),
    }
  })

  const legs: LegDraw[] = []
  course.waypoints.forEach((wp, i) => {
    const from =
      i === 0
        ? course.start === null
          ? null
          : { x: course.start.x, y: course.start.y }
        : { x: course.waypoints[i - 1].x, y: course.waypoints[i - 1].y }
    if (from === null) {
      return
    }
    const current = course.next !== null && course.next === wp.n
    legs.push({
      n: wp.n,
      a: toScreen(from),
      b: toScreen({ x: wp.x, y: wp.y }),
      current,
      className: `course-leg course-leg--${current ? 'current' : 'other'}`,
    })
  })

  const nextWaypoint =
    course.next === null ? undefined : course.waypoints.find((wp) => wp.n === course.next)
  const chevron =
    nextWaypoint === undefined
      ? null
      : chevronFor(
          camera,
          { x: nextWaypoint.x, y: nextWaypoint.y },
          {
            n: nextWaypoint.n,
            state: nextWaypoint.state,
            distanceLabel: formatDistance(course.distance_to_next),
          },
        )

  return { waypoints, legs, chevron }
}

// ---------------------------------------------------------------------------
// The component — a thin map over the records above
// ---------------------------------------------------------------------------

export interface CourseOverlayProps {
  camera: Camera
  /** `practice_state_json`'s `course` block, handed through unchanged. */
  course: CourseBlock
}

/**
 * The isoceles chevron, in a local frame where it points up the screen.
 *
 * The two factors are **dimensionless shape ratios**, presentation only: half
 * the base is 0.8 of the half-height and the base sits 0.5 of it behind the
 * apex, which is the proportion of a conventional direction arrowhead. They
 * are not lengths and never become any.
 */
function chevronPoints(size: number): string {
  return `0,${-size} ${size * 0.8},${size * 0.5} ${-size * 0.8},${size * 0.5}`
}

/**
 * The course overlay: leg lines, gate bars, numbered circles and the
 * off-screen chevron.
 *
 * Drawn in that order, so a waypoint is never hidden by the line that arrives
 * at it. It renders inside the world `<svg>` in screen pixels, as
 * `Trajectory` does, and takes no pointer events.
 */
export function CourseOverlay({ camera, course }: CourseOverlayProps) {
  const draw = courseDraw(camera, course)
  return (
    <g data-testid="course-overlay" style={{ pointerEvents: 'none' }}>
      {draw.legs.map((leg) => (
        <line
          key={`leg-${leg.n}`}
          data-testid={`course-leg-${leg.n}`}
          data-current={leg.current ? 'true' : 'false'}
          className={leg.className}
          x1={leg.a.x}
          y1={leg.a.y}
          x2={leg.b.x}
          y2={leg.b.y}
          stroke={STATE_COLOUR.pending}
          strokeWidth={LEG_STROKE_PX}
          strokeDasharray={leg.current ? undefined : LEG_DASH_PX.join(' ')}
          strokeOpacity={leg.current ? 1 : FAINT_OPACITY}
          fill="none"
        />
      ))}
      {draw.waypoints.map((wp) => (
        <g
          key={wp.n}
          data-testid={wp.testId}
          data-state={wp.state}
          className={wp.className}
          opacity={wp.state === 'pending' ? FAINT_OPACITY : 1}
        >
          <line
            data-testid={`waypoint-${wp.n}-gate`}
            x1={wp.posts[0].x}
            y1={wp.posts[0].y}
            x2={wp.posts[1].x}
            y2={wp.posts[1].y}
            stroke={STATE_COLOUR[wp.state]}
            strokeWidth={GATE_STROKE_PX}
            strokeLinecap="round"
          />
          <circle
            data-testid={`waypoint-${wp.n}-mark`}
            cx={wp.centre.x}
            cy={wp.centre.y}
            r={wp.radiusPx}
            fill="none"
            stroke={STATE_COLOUR[wp.state]}
            strokeWidth={MARK_STROKE_PX}
          />
          <text
            data-testid={`waypoint-${wp.n}-label`}
            x={wp.label.x}
            y={wp.label.y}
            fontSize={wp.label.px}
            fontWeight={600}
            textAnchor="middle"
            dominantBaseline="central"
            fill={STATE_COLOUR[wp.state]}
            stroke="#ffffff"
            strokeWidth={LABEL_HALO_PX}
            paintOrder="stroke"
          >
            {wp.n}
          </text>
        </g>
      ))}
      {draw.chevron !== null && (
        <g
          data-testid="course-chevron"
          data-edge={draw.chevron.edge}
          data-waypoint={draw.chevron.n}
          className="course-chevron"
          transform={`translate(${draw.chevron.at.x} ${draw.chevron.at.y})`}
        >
          <polygon
            points={chevronPoints(draw.chevron.sizePx)}
            transform={`rotate(${draw.chevron.angleDeg})`}
            fill={STATE_COLOUR[draw.chevron.state]}
          />
          <text
            data-testid="course-chevron-label"
            x={0}
            y={draw.chevron.sizePx + WAYPOINT_LABEL_PX}
            fontSize={WAYPOINT_LABEL_PX}
            fontWeight={600}
            textAnchor="middle"
            fill={STATE_COLOUR[draw.chevron.state]}
            stroke="#ffffff"
            strokeWidth={LABEL_HALO_PX}
            paintOrder="stroke"
          >
            {draw.chevron.distanceLabel === ''
              ? `${draw.chevron.n}`
              : `${draw.chevron.n} · ${draw.chevron.distanceLabel}`}
          </text>
        </g>
      )}
    </g>
  )
}
