import type { Diagnostics } from '../sim/diagnostics'
import { apparentWindDegrees, heelDegrees, heelSide } from '../sim/units'

/**
 * Sail Mode's minimal instrumentation (brief §29). Heel and capsize state join
 * it in section 07; both are read from the diagnostics record the core
 * publishes, and neither is derived here (F8).
 */
export function Hud({ diagnostics: d }: { diagnostics: Diagnostics | null }) {
  if (d === null) return null
  const heel = heelDegrees(d.heel)
  const side = heelSide(d.heel)
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
      </span>{' · '}
      <span data-testid="heel-angle" data-value={heel}>
        Heel {Math.abs(heel).toFixed(1)}°{side === 'level' ? '' : ` ${side}`}
      </span>{' · '}
      <span
        data-testid="capsize-state"
        data-capsized={d.capsize.capsized ? 'true' : 'false'}
        data-since={d.capsize.since}
        data-max-heel-deg={heelDegrees(d.capsize.max_heel)}
        style={{ color: d.capsize.capsized ? '#b03030' : undefined }}
      >
        {d.capsize.capsized ? 'CAPSIZED' : 'upright'} (peak {heelDegrees(d.capsize.max_heel).toFixed(1)}°)
      </span>
    </span>
  )
}
