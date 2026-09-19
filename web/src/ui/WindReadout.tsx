/**
 * Wind speed and direction at the boat (brief section 19, section 30).
 *
 * The **"from" bearing in degrees** is what sailors read, and it is computed in
 * Rust by `environment::wind_to_bearing` and delivered by `Sim::wind_at_boat`.
 * It is deliberately **not** derived here from `wx`/`wy`: that conversion
 * exists in exactly one place (F6.1, F8), because a second implementation is
 * how the from/toward flip gets into a codebase.
 */

export interface WindAtBoat {
  /** World-frame air velocity, m/s — the direction the wind blows *toward*. */
  wx: number
  wy: number
  /** Wind speed, m/s. */
  speed: number
  /** Meteorological FROM bearing, degrees clockwise from north. */
  bearingDeg: number
}

/** `[wx, wy, speed, bearing_deg]`, exactly as `Sim::wind_at_boat` returns it. */
export function readWindAtBoat(raw: Float64Array): WindAtBoat {
  return { wx: raw[0], wy: raw[1], speed: raw[2], bearingDeg: raw[3] }
}

export const ZERO_WIND: WindAtBoat = { wx: 0, wy: 0, speed: 0, bearingDeg: 0 }

export function WindReadout({ wind }: { wind: WindAtBoat }) {
  return (
    <div
      data-testid="wind-readout"
      data-wx={wind.wx}
      data-wy={wind.wy}
      data-speed={wind.speed}
      data-bearing={wind.bearingDeg}
      style={{ fontVariantNumeric: 'tabular-nums' }}
    >
      wind {wind.speed.toFixed(2)} m/s from {Math.round(wind.bearingDeg) % 360}°
    </div>
  )
}
