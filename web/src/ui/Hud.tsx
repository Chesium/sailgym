import type { Diagnostics } from '../sim/diagnostics'
import type { Snapshot } from '../sim/snapshot'
import { apparentWindDegrees, heelDegrees, heelSideDegrees, radiansToDegrees } from '../sim/units'

/**
 * Sail Mode's instrumentation: brief §29's list, and nothing else.
 *
 * Seven readouts, one `[data-testid^="sail-readout-"]` wrapper each — wind,
 * boat speed, heading, heel, rudder, sheet, capsize state. The wrappers are
 * what `modes.spec.ts` counts, and the count is the point: Sail Mode's value
 * is what it omits, so "exactly seven" has to be checkable rather than
 * asserted in a comment.
 *
 * Section 07's `[data-testid]`s are unchanged and still nested where they
 * were; only the grouping around them is new (section 08, task 8.2).
 *
 * The wind readout is passed in rather than rendered here: it belongs to the
 * wind layer (section 03) and reads `wind_at_boat()`, which this component
 * has no business fetching. Everything else comes from the diagnostics record
 * and the F8.3 snapshot, and nothing is derived here beyond radians →
 * degrees, which lives in `units.ts` (F8).
 */
export function Hud({
  diagnostics: d,
  snapshot,
  wind,
}: {
  diagnostics: Diagnostics | null
  /** The F8.3 snapshot, for the heading and the two actuator readouts. */
  snapshot: Snapshot
  /** The wind direction/speed readout, rendered by the caller. */
  wind: React.ReactNode
}) {
  if (d === null) return null
  // `heel_deg` crosses the boundary already in degrees (F1 permits `_deg` at
  // a UI boundary), so there is nothing to convert here.
  const heel = d.heel_deg
  const side = heelSideDegrees(heel)
  const headingDeg = radiansToDegrees(snapshot.psi)
  const rudderDeg = radiansToDegrees(snapshot.deltaR)
  return (
    <span
      data-testid="sail-hud"
      data-diagnostics={JSON.stringify(d)}
      style={{ display: 'flex', gap: 10, flexWrap: 'wrap', alignItems: 'center' }}
    >
      <span data-testid="sail-readout-wind">
        {wind}
        <span data-testid="apparent-wind-speed" data-value={d.apparent_wind_speed}>
          {' '}
          · apparent {d.apparent_wind_speed.toFixed(1)} m/s
        </span>
        <span
          data-testid="apparent-wind-angle"
          data-value={apparentWindDegrees(d.apparent_wind_angle)}
        >
          {' '}
          at {apparentWindDegrees(d.apparent_wind_angle).toFixed(1)}° off bow (+ starboard)
        </span>
      </span>

      <span data-testid="sail-readout-speed">
        <span data-testid="boat-speed" data-value={d.speed_over_ground}>
          Boat {d.speed_over_ground.toFixed(2)} m/s
        </span>
      </span>

      <span data-testid="sail-readout-heading" data-value={headingDeg}>
        Heading {headingDeg.toFixed(1)}°
      </span>

      <span data-testid="sail-readout-heel">
        <span data-testid="heel-angle" data-value={heel}>
          Heel {Math.abs(heel).toFixed(1)}°{side === 'level' ? '' : ` ${side}`}
        </span>
      </span>

      {/* brief §29 asks for a rudder *indication*, not a number to steer by:
          the control is a rate (brief §13) and the angle is what it produced. */}
      <span data-testid="sail-readout-rudder" data-value={rudderDeg}>
        Rudder {rudderDeg >= 0 ? '▸' : '◂'} {Math.abs(rudderDeg).toFixed(1)}°
      </span>

      {/* Likewise the sheet: the available length `L`, which is what the
          player's haul and ease actually move (F6.8). */}
      <span data-testid="sail-readout-sheet" data-value={snapshot.lSheet}>
        Sheet {snapshot.lSheet.toFixed(2)} m
      </span>

      <span data-testid="sail-readout-capsize">
        <span
          data-testid="capsize-state"
          data-capsized={d.capsize.capsized ? 'true' : 'false'}
          data-since={d.capsize.since}
          data-max-heel-deg={heelDegrees(d.capsize.max_heel)}
          style={{ color: d.capsize.capsized ? '#b03030' : undefined }}
        >
          {d.capsize.capsized ? 'CAPSIZED' : 'upright'} (peak{' '}
          {heelDegrees(d.capsize.max_heel).toFixed(1)}°)
        </span>
      </span>
    </span>
  )
}
