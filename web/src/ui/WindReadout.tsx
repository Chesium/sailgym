/**
 * Wind speed and direction at the boat (brief §19, §30).
 *
 * The **"from" bearing in degrees** is what sailors read, and it is computed in
 * Rust by `environment::wind_to_bearing` and delivered by `Sim::wind_at_boat`.
 * It is deliberately **not** derived here from `wx`/`wy`: that conversion
 * exists in exactly one place (F6.1, F8), because a second implementation is
 * how the from/toward flip gets into a codebase.
 *
 * That rule is why a **schema-1 replay** shows a vector and no bearing. The
 * world-frame vector is a schema-1 field and is recorded; the speed and the
 * bearing are the output of `wind_to_bearing` and are not, so they read
 * `Not recorded` rather than being worked out on this side of the boundary
 * (v2 section 10, `docs/v2/recording-format.md` §4).
 */

import { NOT_RECORDED } from '../sim/diagnostics'

/**
 * Wind at the boat, live or recorded.
 *
 * `speed` and `bearingDeg` are `null` when the episode did not record them.
 * A live reading always has all four.
 */
export interface WindReading {
  /** Where the numbers came from. */
  source: 'live' | 'recorded'
  /** World-frame air velocity, m/s — the direction the wind blows *toward*. */
  wx: number
  wy: number
  /** Wind speed, m/s, or `null` when not recorded. */
  speed: number | null
  /** Meteorological FROM bearing, degrees clockwise from north, or `null`. */
  bearingDeg: number | null
}

/** The live shape, for the frame loop. Kept for callers that had it. */
export type WindAtBoat = WindReading

/** `[wx, wy, speed, bearing_deg]`, exactly as `Sim::wind_at_boat` returns it. */
export function readWindAtBoat(raw: Float64Array): WindReading {
  return { source: 'live', wx: raw[0], wy: raw[1], speed: raw[2], bearingDeg: raw[3] }
}

export const ZERO_WIND: WindReading = {
  source: 'live',
  wx: 0,
  wy: 0,
  speed: 0,
  bearingDeg: 0,
}

export function WindReadout({ wind }: { wind: WindReading | null }) {
  if (wind === null) {
    return (
      <div data-testid="wind-readout" data-source="unavailable" data-recorded="false">
        wind {NOT_RECORDED}
      </div>
    )
  }
  const known = wind.speed !== null && wind.bearingDeg !== null
  return (
    <div
      data-testid="wind-readout"
      data-source={wind.source}
      data-recorded={known ? 'true' : 'false'}
      data-wx={wind.wx}
      data-wy={wind.wy}
      data-speed={wind.speed ?? ''}
      data-bearing={wind.bearingDeg ?? ''}
      style={{ fontVariantNumeric: 'tabular-nums' }}
    >
      {known ? (
        <>
          wind {(wind.speed as number).toFixed(2)} m/s from{' '}
          {Math.round(wind.bearingDeg as number) % 360}°
        </>
      ) : (
        <>
          wind ({wind.wx.toFixed(2)}, {wind.wy.toFixed(2)}) m/s · speed and bearing{' '}
          {NOT_RECORDED}
        </>
      )}
    </div>
  )
}
