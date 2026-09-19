/**
 * The heel indicator of brief §26: a compact stern (transverse-section) view
 * plus a numeric heel angle. No 3-D scene, and no physics — `φ` arrives from
 * the F8.3 snapshot and the capsize report arrives from the diagnostics; the
 * only arithmetic here is the radians→degrees conversion, which lives once in
 * `sim/units.ts` (F8).
 *
 * ## Which way round the section is drawn
 *
 * The view looks **forward from astern**, so the boat's port side is on the
 * left of the screen and its starboard side on the right. `φ > 0` is starboard
 * down (F2), and SVG's `rotate()` is clockwise because `+y` is down the screen
 * — so the transform is `rotate(φ in degrees)` with no sign flip anywhere.
 * `heel.spec.ts` asserts that correspondence directly off the rendered matrix.
 *
 * ## Past 90° and past 180°
 *
 * `φ` is never wrapped (F3), and neither is the readout: a boat that has rolled
 * through inversion reads 185°, not −175°. The section rotates with it, so the
 * hull goes past horizontal and then upside down on its own, with no special
 * case for the four regimes brief §26 lists. The regime only picks a colour and
 * a word.
 */

import { memo } from 'react'

import { heelDegrees, heelRegime, heelSide, wrapDegrees } from '../sim/units'

export interface HeelIndicatorProps {
  /** rad, roll from the snapshot (F3). Not wrapped. */
  phi: number
  /** The capsize report (F6.10). Informational — nothing here acts on it. */
  capsized: boolean
  /** rad, the episode's largest `|φ|`, from the same report. */
  maxHeel?: number
  /**
   * Overrides `data-testid`, so the fixed-angle probes at the bottom of the
   * page can be addressed one at a time. The section group is always
   * `<testId>-hull`.
   */
  testId?: string
}

const SIZE = { width: 132, height: 108 }

/** Fill and stroke per regime. Cosmetic; never fed back (brief §26). */
const REGIME_COLOUR: Record<string, string> = {
  upright: '#2c6e9b',
  moderate: '#2c6e9b',
  severe: '#c26a1e',
  inverted: '#b03030',
}

/**
 * A transverse section of the hull, in the section view's own units: beam 2,
 * freeboard 0.55, with the deck at the top and a rockered bottom. Drawn about
 * the roll axis, which is the origin.
 */
const HULL_SECTION =
  'M -1,-0.35 L 1,-0.35 L 1,0.1 Q 0.55,0.62 0,0.66 Q -0.55,0.62 -1,0.1 Z'

export function HeelIndicator({
  phi,
  capsized,
  maxHeel,
  testId = 'heel-indicator',
}: HeelIndicatorProps) {
  const degrees = heelDegrees(phi)
  const regime = heelRegime(phi)
  const side = heelSide(phi)
  const colour = REGIME_COLOUR[regime]
  const peak = maxHeel === undefined ? undefined : heelDegrees(maxHeel)

  return (
    <div
      data-testid={testId}
      data-phi={phi}
      data-heel-deg={degrees}
      data-heel-wrapped-deg={wrapDegrees(degrees)}
      data-regime={regime}
      data-side={side}
      data-capsized={capsized ? 'true' : 'false'}
      data-max-heel-deg={peak ?? ''}
      style={{ display: 'grid', justifyItems: 'center', gap: 2 }}
    >
      <svg
        width={SIZE.width}
        height={SIZE.height}
        viewBox="-2.2 -1.9 4.4 3.6"
        role="img"
        aria-label={`heel ${degrees.toFixed(1)} degrees ${side}`}
      >
        {/* Sea surface: the one thing that stays level. */}
        <rect x={-2.2} y={0} width={4.4} height={1.7} fill="#dce9f2" />
        <line x1={-2.2} y1={0} x2={2.2} y2={0} stroke="#7aa0bb" strokeWidth={0.04} />
        {/* The horizon mark, so "past horizontal" is legible at a glance. */}
        <line x1={0} y1={-1.85} x2={0} y2={-1.55} stroke="#9ab" strokeWidth={0.03} />
        <g data-testid={`${testId}-hull`} transform={`rotate(${degrees})`}>
          <path d={HULL_SECTION} fill={colour} fillOpacity={0.35} stroke={colour} strokeWidth={0.06} />
          {/* Mast, and a masthead the geometric checks can follow. */}
          <line x1={0} y1={-0.35} x2={0} y2={-1.5} stroke={colour} strokeWidth={0.07} />
          <circle data-testid={`${testId}-masthead`} cx={0} cy={-1.5} r={0.12} fill={colour} />
        </g>
      </svg>
      <span data-testid={`${testId}-readout`} data-value={degrees}>
        {`Heel ${Math.abs(degrees).toFixed(1)}°`}
        {side === 'level' ? '' : ` ${side}`}
      </span>
    </div>
  )
}

/**
 * The four regimes of brief §26 at fixed angles, rendered off-screen so the
 * E2E suite can check the geometry at each one without having to catch the
 * live boat there. Same component, same code path — only `phi` is pinned.
 *
 * Laid out (rather than `hidden`) because the checks read the rendered
 * transform matrix, and a `display: none` subtree has no matrix at all. Same
 * role as `ArrowProbe` in the wind overlay.
 */
export const HEEL_PROBE_DEGREES = [0, 45, -45, 95, -95, 185, -185] as const

export const HeelProbe = memo(function HeelProbe() {
  return (
    <div
      data-testid="heel-probe"
      aria-hidden
      style={{ position: 'absolute', left: -9999, top: 0, display: 'flex' }}
    >
      {HEEL_PROBE_DEGREES.map((degrees) => (
        <HeelIndicator
          key={degrees}
          phi={(degrees * Math.PI) / 180}
          capsized={false}
          testId={`heel-probe-${degrees}`}
        />
      ))}
    </div>
  )
})
