import type { Diagnostics } from '../sim/diagnostics'
import { apparentWindDegrees } from '../sim/units'

export function Hud({ diagnostics: d }: { diagnostics: Diagnostics | null }) {
  if (d === null) return null
  return (
    <span data-testid="sail-hud" data-diagnostics={JSON.stringify(d)}>
      <span data-testid="apparent-wind-speed" data-value={d.apparent_wind_speed}>
        Apparent wind {d.apparent_wind_speed.toFixed(1)} m/s
      </span>{' · '}
      <span data-testid="apparent-wind-angle" data-value={apparentWindDegrees(d.apparent_wind_angle)}>
        {apparentWindDegrees(d.apparent_wind_angle).toFixed(1)}° off bow (+ starboard)
      </span>{' · '}
      <span data-testid="boat-speed" data-value={d.speed_over_ground}>
        Boat {d.speed_over_ground.toFixed(2)} m/s
      </span>
    </span>
  )
}
