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
