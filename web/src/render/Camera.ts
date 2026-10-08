/**
 * Camera for the top-down SVG view (brief §27).
 *
 * Two modes:
 *
 * - `northUp` — world orientation fixed. The camera centre tracks the boat
 *   only once it leaves a central dead zone, so the boat stays in view while
 *   still visibly moving across the world.
 * - `follow` — the boat sits at the centre with the bow pointing up; the world
 *   moves and rotates around it.
 *
 * Pure value object: `createCamera` takes the current view state and returns
 * the two conversions plus the SVG transform for the world layer. It holds no
 * mutable state and touches no DOM.
 */

export interface Vec2 {
  x: number
  y: number
}

export type CameraMode = 'follow' | 'northUp'

export interface Viewport {
  width: number
  height: number
}

/** Screen pixels per world metre at `zoom === 1`. */
export const PIXELS_PER_METRE = 20

export const MIN_ZOOM = 0.1
export const MAX_ZOOM = 20

export interface Camera {
  mode: CameraMode
  zoom: number
  centre: Vec2
  /** Radians the world is rotated by on screen. Always 0 in `northUp`. */
  rotation: number
  viewport: Viewport
  /** Screen pixels per world metre, including `zoom`. */
  scale: number
  worldToScreen(p: Vec2): Vec2
  screenToWorld(p: Vec2): Vec2
}

export interface CameraOptions {
  mode: CameraMode
  zoom: number
  centre: Vec2
  /** Boat heading ψ, radians. Only used in `follow` mode. */
  heading: number
  viewport: Viewport
}

export function createCamera(opts: CameraOptions): Camera {
  const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, opts.zoom))
  const scale = PIXELS_PER_METRE * zoom
  // Bow up in follow mode: a world direction of ψ must appear as screen "up",
  // which is world +y after removing the camera rotation.
  const rotation = opts.mode === 'follow' ? opts.heading - Math.PI / 2 : 0
  const cx = opts.viewport.width / 2
  const cy = opts.viewport.height / 2
  const cos = Math.cos(rotation)
  const sin = Math.sin(rotation)

  return {
    mode: opts.mode,
    zoom,
    centre: opts.centre,
    rotation,
    viewport: opts.viewport,
    scale,
    worldToScreen(p: Vec2): Vec2 {
      const dx = p.x - opts.centre.x
      const dy = p.y - opts.centre.y
      // R(−rotation), then flip y because screen y grows downward.
      const rx = dx * cos + dy * sin
      const ry = -dx * sin + dy * cos
      return { x: cx + scale * rx, y: cy - scale * ry }
    },
    screenToWorld(p: Vec2): Vec2 {
      const rx = (p.x - cx) / scale
      const ry = -(p.y - cy) / scale
      // R(+rotation)
      const dx = rx * cos - ry * sin
      const dy = rx * sin + ry * cos
      return { x: dx + opts.centre.x, y: dy + opts.centre.y }
    },
  }
}

/**
 * SVG transform mapping world metres to screen pixels, for the world layer.
 *
 * In `northUp` the `rotate(...)` term is **omitted entirely**, not emitted as
 * `rotate(0)`: "no rotation component" is asserted literally by
 * `tests/e2e/render.spec.ts`.
 */
export function worldTransform(cam: Camera): string {
  const cx = cam.viewport.width / 2
  const cy = cam.viewport.height / 2
  const rot = cam.rotation === 0 ? '' : ` rotate(${toDegrees(cam.rotation)})`
  return (
    `translate(${cx} ${cy})${rot} scale(${cam.scale} ${-cam.scale})` +
    ` translate(${-cam.centre.x} ${-cam.centre.y})`
  )
}

/**
 * SVG transform placing a boat-fixed drawing (x forward, y to port, metres) at
 * the boat's screen position and heading.
 */
export function boatTransform(cam: Camera, position: Vec2, heading: number): string {
  const s = cam.worldToScreen(position)
  const spin = toDegrees(cam.rotation - heading)
  return `translate(${s.x} ${s.y}) rotate(${spin}) scale(${cam.scale} ${-cam.scale})`
}

export function toDegrees(radians: number): number {
  return (radians * 180) / Math.PI
}

/**
 * Where the camera centre should be next.
 *
 * `follow` pins the centre to the boat. `northUp` leaves the centre alone
 * until the boat passes `deadZone` (a fraction of the half-viewport), then
 * pushes it just enough to bring the boat back to the edge of that zone.
 */
export function trackedCentre(
  previous: Vec2,
  boat: Vec2,
  mode: CameraMode,
  viewport: Viewport,
  scale: number,
  deadZone = 0.35,
): Vec2 {
  if (mode === 'follow') {
    return boat
  }
  const marginX = ((viewport.width / 2) * deadZone) / scale
  const marginY = ((viewport.height / 2) * deadZone) / scale
  return {
    x: clampTo(previous.x, boat.x, marginX),
    y: clampTo(previous.y, boat.y, marginY),
  }
}

function clampTo(centre: number, target: number, margin: number): number {
  if (target - centre > margin) {
    return target - margin
  }
  if (centre - target > margin) {
    return target + margin
  }
  return centre
}

/**
 * The camera that shows a whole course — **Show course** (v2 section 12,
 * task 12.5).
 *
 * Pure: no DOM, no state, no React. It takes world points and a viewport and
 * returns the `centre`/`zoom` pair {@link createCamera} takes, which is also
 * the pair `App.tsx` already holds in state — so applying a fit is two
 * assignments and no new camera concept.
 *
 * `margin` is in **world metres**, not pixels: the points are world metres,
 * so a world margin keeps the whole computation in one unit and lets the
 * caller say "a hull length of water round the course" with a number it
 * already has. A pixel inset would have to be divided by the very scale this
 * function is solving for.
 *
 * ## What it guarantees, and the one case it cannot
 *
 * The returned camera contains every point's `margin` box **whenever the
 * returned zoom is above `MIN_ZOOM`**. The fit zoom is the largest that fits,
 * so clamping it *down* to `MAX_ZOOM` only shows more; clamping it *up* to
 * `MIN_ZOOM` — a course too large for the supported zoom range — cannot fit,
 * and the camera is then centred on the bounds and nothing is invented. That
 * is reported honestly by the returned `zoom` being `MIN_ZOOM` rather than by
 * a silent extra zoom step outside the range the rest of the camera accepts.
 *
 * ## Degenerate inputs, each handled rather than divided by
 *
 * - **No points** (or none finite): there are no bounds to fit. The result is
 *   the world origin at zoom 1 — the application's own default zoom — and a
 *   caller with nothing to show may equally ignore it.
 * - **One point, or several coincident ones**: the bounds have zero extent,
 *   so the extent is `2 · margin` and the fit is finite. With `margin` zero
 *   as well the extent is zero, which would be a division by zero; the zoom
 *   is `MAX_ZOOM` instead, because "fit a point" has no other answer.
 * - **A non-finite point** is dropped, so one bad prop cannot turn the camera
 *   into `NaN` and blank the page.
 * - **A non-positive viewport** (a hidden element measured at zero) yields
 *   `MIN_ZOOM`, not `NaN`.
 *
 * ## Why it is axis-aligned, and therefore a `northUp` operation
 *
 * The box fitted is the world axis-aligned bounding box, which is exact when
 * the camera applies no rotation. In `follow` the world is rotated by
 * `heading − π/2` (see {@link createCamera}), so the same box covers a
 * different screen region and the fit is conservative rather than exact.
 * **Show course** is a `northUp` action; this function does not take a
 * heading, so it cannot pretend otherwise.
 */
export function fitBounds(
  points: readonly Vec2[],
  viewport: Viewport,
  margin: number,
): { centre: Vec2; zoom: number } {
  const finite = points.filter(
    (p) => Number.isFinite(p.x) && Number.isFinite(p.y),
  )
  if (finite.length === 0) {
    return { centre: { x: 0, y: 0 }, zoom: 1 }
  }

  let minX = finite[0].x
  let maxX = finite[0].x
  let minY = finite[0].y
  let maxY = finite[0].y
  for (const p of finite) {
    minX = Math.min(minX, p.x)
    maxX = Math.max(maxX, p.x)
    minY = Math.min(minY, p.y)
    maxY = Math.max(maxY, p.y)
  }

  const centre = { x: (minX + maxX) / 2, y: (minY + maxY) / 2 }
  const pad = Number.isFinite(margin) ? Math.max(0, margin) : 0
  const spanX = maxX - minX + 2 * pad
  const spanY = maxY - minY + 2 * pad

  // `Infinity` for a zero span is the honest answer — any zoom fits a point —
  // and `Math.min` then takes the other axis, or the clamp takes `MAX_ZOOM`.
  const zoomX = spanX > 0 ? viewport.width / (PIXELS_PER_METRE * spanX) : Infinity
  const zoomY = spanY > 0 ? viewport.height / (PIXELS_PER_METRE * spanY) : Infinity
  const wanted = Math.min(zoomX, zoomY)
  const zoom = Number.isNaN(wanted)
    ? MIN_ZOOM
    : Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, wanted))
  return { centre, zoom }
}
