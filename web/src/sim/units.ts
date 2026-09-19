/** Display boundaries only. Physics angles remain radians. */
export function radiansToDegrees(radians: number): number {
  return radians * 180 / Math.PI
}

/** Rust already reports a FROM angle, positive to starboard. */
export function apparentWindDegrees(angle: number): number {
  return radiansToDegrees(angle)
}

/**
 * Heel in degrees, signed the way `φ` is signed in F2: positive is starboard
 * down. **Not wrapped** — `φ` is not wrapped in the core either (F3), and the
 * heel indicator has to be able to show an inversion as such.
 */
export function heelDegrees(phi: number): number {
  return radiansToDegrees(phi)
}

/** The side the deck is going down, for the heel readout. */
export function heelSide(phi: number): 'level' | 'port' | 'starboard' {
  return heelSideDegrees(heelDegrees(phi))
}

/**
 * The same answer for a heel already in degrees.
 *
 * The diagnostics record publishes `heel_deg`; the heel indicator reads `φ`
 * straight off the snapshot in radians. One implementation, two entry points,
 * so the two cannot disagree about where 0.5° sits.
 */
export function heelSideDegrees(degrees: number): 'level' | 'port' | 'starboard' {
  const wrapped = wrapDegrees(degrees)
  if (Math.abs(wrapped) < 0.5) return 'level'
  return wrapped > 0 ? 'starboard' : 'port'
}

/** Degrees folded into `(−180, 180]`, for labels only. */
export function wrapDegrees(degrees: number): number {
  const wrapped = ((degrees + 180) % 360 + 360) % 360 - 180
  return wrapped === -180 ? 180 : wrapped
}

/**
 * Which of brief §26's four regimes the boat is in. Display only: nothing in
 * the physics knows these exist, and no force branches on them.
 */
export type HeelRegime = 'upright' | 'moderate' | 'severe' | 'inverted'

export function heelRegime(phi: number): HeelRegime {
  const magnitude = Math.abs(wrapDegrees(heelDegrees(phi)))
  // Inclusive at 45°, so the four probe angles of task 7.5 — 0, 45, 95, 185 —
  // land one in each regime.
  if (magnitude < 10) return 'upright'
  if (magnitude <= 45) return 'moderate'
  if (magnitude < 135) return 'severe'
  return 'inverted'
}
