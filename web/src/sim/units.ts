/** Display boundaries only. Physics angles remain radians. */
export function radiansToDegrees(radians: number): number {
  return radians * 180 / Math.PI
}

/** Rust already reports a FROM angle, positive to starboard. */
export function apparentWindDegrees(angle: number): number {
  return radiansToDegrees(angle)
}
