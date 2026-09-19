/**
 * Input → `Controls` mapping (brief §28).
 *
 * Every rate, gain and dead zone lives in [`InputConfig`], not scattered
 * through the code (brief §28, last line).
 *
 * **No physics here** (F8). In particular, rudder self-centring is *not*
 * implemented in the browser: with no steering key held this module sends
 * `rudderRateCmd = 0` and the Rust core applies `delta_r_return_rate`.
 */

import { actionFor } from './keymap'

/** The TypeScript mirror of the Rust `Controls` struct (F3). Rates, never angles. */
export interface Controls {
  /** normalised [−1, 1]; +1 = steer the bow to starboard (F2.2) */
  rudderRateCmd: number
  /** normalised [−1, 1]; +1 = ease (pay out), −1 = haul */
  sheetRateCmd: number
  /** Space: ease at the release rate, overrides `sheetRateCmd` */
  sheetRelease: boolean
}

export interface InputConfig {
  /** normalised command magnitude while a steering key is held */
  rudderKeyRate: number
  /** commands smaller than this are treated as neutral */
  rudderDeadZone: number
  /** normalised sheet command per pixel of vertical drag (section 06) */
  sheetDragGain: number
  /** invert the vertical sheet drag; the default is drag down = haul in */
  sheetInvert: boolean
}

export const DEFAULT_INPUT: InputConfig = {
  rudderKeyRate: 1.0,
  rudderDeadZone: 0.02,
  sheetDragGain: 0.004,
  sheetInvert: false,
}

export const NEUTRAL_CONTROLS: Controls = {
  rudderRateCmd: 0,
  sheetRateCmd: 0,
  sheetRelease: false,
}

/**
 * Pure: the set of currently held keys plus the config becomes a `Controls`.
 * No DOM access, so it is unit-testable and frame-rate independent.
 *
 * `D`/`ArrowRight` produce a **positive** command, which per F2.2 deflects the
 * rudder so the bow turns to starboard. Holding both directions cancels.
 *
 * `sheetRateCmd` is a mouse-drag command (brief §12), so it is not derived
 * from the held keys: `sheetInput.reduceSheetInput` produces it and it is
 * passed straight through here. `Space` (release) is a key and overrides it
 * inside the Rust core, which is the only place that precedence is written.
 */
export function controlsFromInput(
  held: ReadonlySet<string>,
  cfg: InputConfig,
  sheetRateCmd = 0,
): Controls {
  let rudder = 0
  let release = false

  for (const key of held) {
    switch (actionFor(key)) {
      case 'steerStarboard':
        rudder += cfg.rudderKeyRate
        break
      case 'steerPort':
        rudder -= cfg.rudderKeyRate
        break
      case 'sheetRelease':
        release = true
        break
      default:
        break
    }
  }

  if (Math.abs(rudder) < cfg.rudderDeadZone) {
    rudder = 0
  }

  return {
    rudderRateCmd: Math.max(-1, Math.min(1, rudder)),
    sheetRateCmd: Math.max(-1, Math.min(1, sheetRateCmd)),
    sheetRelease: release,
  }
}
