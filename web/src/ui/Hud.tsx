import { isRecorded, NOT_RECORDED, type DiagnosticsSample } from '../sim/diagnostics'
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
 *
 * ## Live or recorded, and never both (v2 section 10, task 10.3)
 *
 * The record arrives as a {@link DiagnosticsSample}, so every readout knows
 * whether it is showing the live simulation or an episode's own sample — and,
 * in replay, *which* sample, because a diagnostics value belongs to the
 * recorded sample before the playhead and not to the playhead itself. A field
 * the episode does not carry reads `Not recorded`; it is never filled from the
 * live simulation and never recomputed (RV57, RV59).
 *
 * The heading, the rudder angle and the sheet length come from the snapshot,
 * which in replay is the recorded state — so those three are available in
 * every episode, including a schema-1 one.
 */
export function Hud({
  diagnostics: sample,
  snapshot,
  wind,
  capsized,
}: {
  diagnostics: DiagnosticsSample | null
  /** The F8.3 snapshot, for the heading and the two actuator readouts. */
  snapshot: Snapshot
  /** The wind direction/speed readout, rendered by the caller. */
  wind: React.ReactNode
  /**
   * The capsize flag, which a schema-1 frame carries even though the rest of
   * the report does not. `null` when nothing is being inspected.
   */
  capsized: boolean | null
}) {
  if (sample === null) return null
  const d = sample.values
  const headingDeg = radiansToDegrees(snapshot.psi)
  const rudderDeg = radiansToDegrees(snapshot.deltaR)
  // `heel_deg` crosses the boundary already in degrees (F1 permits `_deg` at
  // a UI boundary), so there is nothing to convert here.
  const heel = isRecorded(d, 'heel_deg') ? d.heel_deg : null
  const side = heel === null ? 'level' : heelSideDegrees(heel)
  const speed = isRecorded(d, 'speed_over_ground') ? d.speed_over_ground : null
  const awSpeed = isRecorded(d, 'apparent_wind_speed') ? d.apparent_wind_speed : null
  const awAngle = isRecorded(d, 'apparent_wind_angle')
    ? apparentWindDegrees(d.apparent_wind_angle)
    : null
  const peak = isRecorded(d, 'capsize') ? heelDegrees(d.capsize.max_heel) : null

  return (
    <span
      data-testid="sail-hud"
      data-diagnostics={JSON.stringify(d)}
      // Which timeline these numbers belong to, and the simulated time they
      // describe. In replay that is the recorded sample's time, which is at or
      // before the playhead — stated rather than implied (task 10.3).
      data-source={sample.source}
      data-sample-t={sample.t}
      style={{ display: 'flex', gap: 10, flexWrap: 'wrap', alignItems: 'center' }}
    >
      <span data-testid="sail-readout-wind">
        {wind}
        <span data-testid="apparent-wind-speed" data-value={awSpeed ?? ''}>
          {awSpeed === null ? ` · apparent ${NOT_RECORDED}` : ` · apparent ${awSpeed.toFixed(1)} m/s`}
        </span>
        <span data-testid="apparent-wind-angle" data-value={awAngle ?? ''}>
          {awAngle === null ? '' : ` at ${awAngle.toFixed(1)}° off bow (+ starboard)`}
        </span>
      </span>

      <span data-testid="sail-readout-speed">
        <span data-testid="boat-speed" data-value={speed ?? ''}>
          {speed === null ? `Boat ${NOT_RECORDED}` : `Boat ${speed.toFixed(2)} m/s`}
        </span>
      </span>

      <span data-testid="sail-readout-heading" data-value={headingDeg}>
        Heading {headingDeg.toFixed(1)}°
      </span>

      <span data-testid="sail-readout-heel">
        <span data-testid="heel-angle" data-value={heel ?? ''}>
          {heel === null
            ? `Heel ${NOT_RECORDED}`
            : `Heel ${Math.abs(heel).toFixed(1)}°${side === 'level' ? '' : ` ${side}`}`}
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
          data-capsized={capsized === null ? '' : capsized ? 'true' : 'false'}
          data-since={isRecorded(d, 'capsize') ? d.capsize.since : ''}
          data-max-heel-deg={peak ?? ''}
          style={{ color: capsized === true ? '#b03030' : undefined }}
        >
          {capsized === null ? NOT_RECORDED : capsized ? 'CAPSIZED' : 'upright'}
          {peak === null ? ` (peak ${NOT_RECORDED})` : ` (peak ${peak.toFixed(1)}°)`}
        </span>
      </span>
    </span>
  )
}
