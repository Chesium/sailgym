/**
 * Fixed-angle boat probes (v2 section 01, task 1.5).
 *
 * The same device `HeelProbe` uses, and for the same reason: the browser
 * checks need the geometry at heel angles the live boat will not conveniently
 * visit, and catching it there with a poll is a race. Each probe renders the
 * boat through {@link BoatFigure} — the same model, the same projection, the
 * same draw list — with only `φ` pinned.
 *
 * Laid out off-screen rather than `display: none`, because a hidden subtree has
 * no transform matrix to read and no rendered area to measure.
 *
 * Every `data-testid` inside is prefixed `boat-probe-<deg>-`, so the live boat
 * remains the only `boat-boom`, `boat-sail`, `boat-board` and `boat-rudder` on
 * the page and the specs that address those keep working unmodified.
 */

import { memo } from 'react'

import { BoatFigure } from './BoatSvg'
import type { HullDims } from './geometry'
import type { RenderParams } from '../sim/useSimulation'

/** The angles the section's browser checks read. */
export const BOAT_PROBE_DEGREES = [0, 30, -30, 60, -60, 90, -90, 135, 180] as const

/** The rig pose every probe is drawn in — off the centreline, so `β` and `δr` show. */
export const PROBE_BETA = 0.35
export const PROBE_DELTA_R = 0.2

/** Metres of `B` visible across a probe; the masthead is the tall part. */
const PROBE_REACH = 7

export interface BoatProbeProps {
  params: RenderParams
  hull: HullDims
}

export const BoatProbe = memo(function BoatProbe({ params, hull }: BoatProbeProps) {
  return (
    <div
      data-testid="boat-probe"
      aria-hidden
      style={{ position: 'absolute', left: -9999, top: 0, display: 'flex' }}
    >
      {BOAT_PROBE_DEGREES.map((degrees) => (
        <svg
          key={degrees}
          width={120}
          height={120}
          viewBox={`${-PROBE_REACH} ${-PROBE_REACH} ${2 * PROBE_REACH} ${2 * PROBE_REACH}`}
        >
          {/* `scale(1 −1)` for the same reason `Camera.boatTransform` carries
              it: `+y` is to port and SVG's `+y` runs down the screen. */}
          <g data-testid={`boat-probe-${degrees}`} transform="scale(1 -1)">
            <BoatFigure
              params={params}
              hull={hull}
              phi={(degrees * Math.PI) / 180}
              beta={PROBE_BETA}
              deltaR={PROBE_DELTA_R}
              alpha={0}
              prefix={`boat-probe-${degrees}-`}
            />
          </g>
        </svg>
      ))}
    </div>
  )
})
