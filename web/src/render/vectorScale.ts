/**
 * Turning physical quantities into screen pixels, and boat-fixed directions
 * into screen directions (task 8.3).
 *
 * There is no physics here. Every number in comes from the diagnostics record
 * the core publishes; this module only decides how long to draw it and which
 * way round (F8). The one piece of geometry is the boat → screen rotation,
 * and it is the *same* mapping `Camera.boatTransform` emits, written out in
 * numbers because a force vector's length must not scale with the camera zoom
 * while its origin must.
 */

/** The window the auto scale keeps the largest active vector inside. */
export const MIN_VECTOR_PX = 40
export const MAX_VECTOR_PX = 200

/** What the auto scale aims for, comfortably inside that window. */
export const TARGET_VECTOR_PX = 120

/** A vector shorter than this is not worth an arrowhead. */
export const MIN_DRAWN_PX = 1e-6

/**
 * Pixels for a magnitude at a given scale.
 *
 * `unitsPerPixel` is newtons per pixel for a force, N·m per pixel for a
 * moment, m/s per pixel for a velocity — one function, because the arithmetic
 * does not care which.
 */
export function pixelsFor(magnitude: number, unitsPerPixel: number): number {
  if (!Number.isFinite(magnitude) || !Number.isFinite(unitsPerPixel) || unitsPerPixel <= 0) {
    return 0
  }
  return magnitude / unitsPerPixel
}

/**
 * A scale that puts `largest` at roughly {@link TARGET_VECTOR_PX}, snapped up
 * to the next 1–2–5 step so the legend reads `2 N/px`, not `1.8473 N/px`.
 *
 * Snapping up can only shorten the vector, and never by more than 2.5× (the
 * widest gap in the 1–2–5 ladder), so the drawn length stays in
 * `[TARGET/2.5, TARGET]` = `[48, 120]` px — inside the
 * `[MIN_VECTOR_PX, MAX_VECTOR_PX]` window at every magnitude, which is what
 * makes the window hold across any force range rather than only a tested one.
 */
export function autoScale(largest: number): number {
  if (!Number.isFinite(largest) || largest <= 0) {
    return 1
  }
  const ideal = largest / TARGET_VECTOR_PX
  const decade = 10 ** Math.floor(Math.log10(ideal))
  for (const step of [1, 2, 5]) {
    if (ideal <= step * decade) {
      return step * decade
    }
  }
  return 10 * decade
}

/**
 * The rotation from boat-fixed axes (`+x` forward, `+y` to port, metres) to
 * screen axes (`+x` right, `+y` **down**), in radians.
 *
 * This is `Camera.boatTransform`'s `rotate(cam.rotation − heading)` expressed
 * once as an angle, so the overlay and the boat drawing can never disagree
 * about which way the bow points.
 */
export function boatToScreenAngle(cameraRotation: number, heading: number): number {
  return cameraRotation - heading
}

export interface ScreenVec {
  x: number
  y: number
}

/**
 * A boat-fixed direction `(bx, by)`, scaled by `pixelsPerUnit`, as a screen
 * offset in pixels.
 *
 * Expanding `translate(...) rotate(θ) scale(s, −s)`: SVG's `rotate` turns
 * clockwise because screen `y` grows downward, and the `−s` is the `y` flip,
 * so `(bx, by) ↦ s·(bx cos θ + by sin θ, bx sin θ − by cos θ)`. With the
 * camera north-up and the bow east (`θ = 0`) that sends forward to the right
 * and port up the screen, which is what the boat drawing does.
 */
export function boatVectorToScreen(
  bx: number,
  by: number,
  theta: number,
  pixelsPerUnit: number,
): ScreenVec {
  const cos = Math.cos(theta)
  const sin = Math.sin(theta)
  return {
    x: pixelsPerUnit * (bx * cos + by * sin),
    y: pixelsPerUnit * (bx * sin - by * cos),
  }
}

/**
 * A boat-fixed point in metres, as an absolute screen position.
 *
 * `origin` is the boat's CG on screen (`camera.worldToScreen` of its world
 * position) and `metresToPixels` is `camera.scale`, so application points move
 * and zoom with the boat while the vectors drawn from them do not.
 */
export function boatPointToScreen(
  origin: ScreenVec,
  bx: number,
  by: number,
  theta: number,
  metresToPixels: number,
): ScreenVec {
  const offset = boatVectorToScreen(bx, by, theta, metresToPixels)
  return { x: origin.x + offset.x, y: origin.y + offset.y }
}

/**
 * The three points of an arrowhead at `tip`, for a shaft arriving along
 * `(dx, dy)`. Returns an empty string for a vector too short to have a
 * direction, so a zero force draws nothing rather than a `NaN` polygon.
 */
export function arrowHead(
  tip: ScreenVec,
  dx: number,
  dy: number,
  size = 7,
): string {
  const length = Math.hypot(dx, dy)
  if (!(length > MIN_DRAWN_PX)) {
    return ''
  }
  const ux = dx / length
  const uy = dy / length
  const back = { x: tip.x - ux * size, y: tip.y - uy * size }
  const half = size * 0.45
  const p1 = { x: back.x - uy * half, y: back.y + ux * half }
  const p2 = { x: back.x + uy * half, y: back.y - ux * half }
  return `${tip.x},${tip.y} ${p1.x},${p1.y} ${p2.x},${p2.y}`
}

/**
 * An SVG arc of `magnitude` (signed) about `centre`, for drawing a moment.
 *
 * The radius grows with `|magnitude|` at the given scale and is clamped into
 * the same pixel window the vectors use, so a moment arc cannot swallow the
 * viewport or vanish. A positive moment is drawn counter-clockwise on screen
 * when the camera is north-up, matching the right-hand rule about `+z`.
 */
export function momentArc(
  centre: ScreenVec,
  magnitude: number,
  unitsPerPixel: number,
  sweepRadians = Math.PI * 1.2,
): { path: string; radius: number; tip: ScreenVec; tangent: ScreenVec } | null {
  const raw = pixelsFor(Math.abs(magnitude), unitsPerPixel)
  if (!(raw > MIN_DRAWN_PX)) {
    return null
  }
  const radius = Math.min(MAX_VECTOR_PX, Math.max(MIN_VECTOR_PX * 0.4, raw))
  // Screen `y` is down, so a positive (counter-clockwise in the world) moment
  // sweeps with a negative angle here.
  const direction = magnitude >= 0 ? -1 : 1
  const start = 0
  const end = direction * sweepRadians
  const point = (a: number): ScreenVec => ({
    x: centre.x + radius * Math.cos(a),
    y: centre.y + radius * Math.sin(a),
  })
  const from = point(start)
  const to = point(end)
  const largeArc = sweepRadians > Math.PI ? 1 : 0
  const sweepFlag = direction > 0 ? 1 : 0
  return {
    path: `M ${from.x} ${from.y} A ${radius} ${radius} 0 ${largeArc} ${sweepFlag} ${to.x} ${to.y}`,
    radius,
    tip: to,
    // Tangent at the far end, so the arrowhead points the way the arc turns.
    tangent: { x: -Math.sin(end) * direction, y: Math.cos(end) * direction },
  }
}
