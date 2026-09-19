import { boomSegment, type RigDims } from './geometry'

/** Cosmetic curvature only. Its sign follows Rust's sail angle of attack. */
export function sailShape(rig: RigDims, beta: number, alpha: number): string {
  const { x1, y1, x2, y2 } = boomSegment(rig, beta)
  const bx = x2 - x1
  const by = y2 - y1
  const camber = 0.12 * Math.sign(alpha)
  const cx = x1 + bx * 0.5 - by * camber
  const cy = y1 + by * 0.5 + bx * camber
  return `M ${x1},${y1} Q ${cx},${cy} ${x2},${y2}`
}
