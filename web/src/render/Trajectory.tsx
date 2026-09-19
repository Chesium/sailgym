import type { Camera, Vec2 } from './Camera'

/**
 * The track the boat has sailed (brief §25).
 *
 * Points are world metres; they are projected to screen here rather than being
 * drawn inside the scaled world group, so the stroke keeps a constant width at
 * every zoom.
 */
export function Trajectory({
  camera,
  points,
}: {
  camera: Camera
  points: readonly Vec2[]
}) {
  if (points.length < 2) {
    return <polyline data-testid="trajectory" points="" fill="none" />
  }
  const projected = points
    .map((p) => {
      const s = camera.worldToScreen(p)
      return `${s.x.toFixed(2)},${s.y.toFixed(2)}`
    })
    .join(' ')
  return (
    <polyline
      data-testid="trajectory"
      points={projected}
      fill="none"
      stroke="#3b6ea5"
      strokeWidth={1.5}
      strokeOpacity={0.8}
    />
  )
}
