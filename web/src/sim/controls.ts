/**
 * Input → `Controls` mapping (brief §28), and the **one** place the devices
 * are composed into a single command (v2 F18.2, task 9.1).
 *
 * Every rate, gain and dead zone lives in [`InputConfig`], not scattered
 * through the code (brief §28, last line).
 *
 * **No physics here** (F8, RV56). In particular:
 *
 * - rudder self-centring is *not* implemented in the browser: with nothing
 *   driving the helm this module sends `rudderRateCmd = 0` and the Rust core
 *   applies `delta_r_return_rate` (`dynamics::rudder_rate`). A zero command
 *   therefore means **self-centre**, not *hold* — which is exactly why
 *   position-target control waits for an explicit engaged/released contract in
 *   a shared Rust adapter (v2 F18.2, `discussions/mobile-controls.md`);
 * - no gain here is ever multiplied by a rate, a length or a tension. A gain
 *   turns pixels into a **normalised** command in `[−1, 1]`; the rates that
 *   command is scaled by are `rudder.delta_r_rate_max`,
 *   `sheet.sheet_haul_rate`, `sheet.sheet_ease_rate` and
 *   `sheet.sheet_release_rate`, all of which live in `parameters.rs` and are
 *   applied in `dynamics.rs`.
 */

import { actionFor } from './keymap'

/** The TypeScript mirror of the Rust `Controls` struct (F3). Rates, never angles. */
export interface Controls {
  /** normalised [−1, 1]; +1 = steer the bow to starboard (F2.2) */
  rudderRateCmd: number
  /** normalised [−1, 1]; +1 = ease (pay out), −1 = haul */
  sheetRateCmd: number
  /** Space, or the hold-to-release button: ease at the release rate */
  sheetRelease: boolean
}

export interface InputConfig {
  /** normalised command magnitude while a steering key is held */
  rudderKeyRate: number
  /** commands smaller than this are treated as neutral */
  rudderDeadZone: number
  /** normalised sheet command per pixel of vertical **mouse** drag (section 06) */
  sheetDragGain: number
  /** invert the vertical sheet drag; the default is drag down = haul in */
  sheetInvert: boolean
  /** normalised rudder command per pixel of horizontal drag on a touch pad */
  touchRudderGain: number
  /** normalised sheet command per pixel of vertical drag on a touch pad */
  touchSheetGain: number
}

/**
 * Pixels of drag from the grab point that produce a full-scale (`±1`) command
 * on a touch pad.
 *
 * **Provenance.** Both are *input* constants — they scale a normalised command
 * and nothing else — and both are anchored to the 44 × 44 CSS-pixel minimum
 * touch target the section PRD requires:
 *
 * - `rudder: 64` is ≈ 1.5 × 44 px. A full-scale helm command therefore needs a
 *   deliberate drag of about a finger-width and a half, which the tremor of
 *   holding a finger still on glass (a few pixels) cannot reach, while the
 *   whole range still fits inside a thumb's arc on a 360 px-wide phone.
 * - `sheet: 96` is ≈ 2.2 × 44 px. Trimming is a sustained action rather than a
 *   twitchy one, and the pads are laid out taller than they are wide, so the
 *   sheet pad has the travel to spend on finer control.
 *
 * Neither is a physical coefficient and neither may reach the physics core
 * (brief §43, RV56). Changing them changes how far a finger moves for a given
 * *command*; the metres per second that command turns into are the core's.
 */
export const TOUCH_FULL_SCALE_PX = { rudder: 64, sheet: 96 } as const

export const DEFAULT_INPUT: InputConfig = {
  rudderKeyRate: 1.0,
  rudderDeadZone: 0.02,
  sheetDragGain: 0.004,
  sheetInvert: false,
  touchRudderGain: 1 / TOUCH_FULL_SCALE_PX.rudder,
  touchSheetGain: 1 / TOUCH_FULL_SCALE_PX.sheet,
}

export const NEUTRAL_CONTROLS: Controls = {
  rudderRateCmd: 0,
  sheetRateCmd: 0,
  sheetRelease: false,
}

/**
 * What the touch pads are asking for.
 *
 * `null` on a channel means **no pad owns it**, which is not the same as a pad
 * owning it and sitting at neutral: the first falls through to the mouse and
 * the keyboard, the second does not. Lifting a finger therefore returns the
 * channel to whatever else is driving it — it does not dump the sheet, and it
 * does not leave a stale command behind (RV53).
 */
export interface TouchCommand {
  rudder: number | null
  sheet: number | null
  release: boolean
}

export const IDLE_TOUCH: TouchCommand = { rudder: null, sheet: null, release: false }

/**
 * Every device, as one value.
 *
 * This is the whole input state of the application. `useSimulation` holds one
 * of these and pushes the composed `Controls` across the F8 boundary from a
 * single call site; nothing else writes controls (v2 F18.2).
 */
export interface InputSources {
  /** Keyboard: the keys currently held. */
  held: ReadonlySet<string>
  /** Mouse drag on the world view: a sheet command, `null` when no drag owns it. */
  mouseSheet: number | null
  /** The touch pads and the release button. */
  touch: TouchCommand
}

/** Nothing held, nothing dragged, nothing latched. */
export const IDLE_SOURCES: InputSources = {
  held: new Set<string>(),
  mouseSheet: null,
  touch: IDLE_TOUCH,
}

/**
 * Drop every transient input and every release latch.
 *
 * The one clear path (RV53). Blur, a hidden page, pause, reset, a scenario
 * switch and entering replay all go through it, so a gesture that was
 * interrupted rather than finished cannot survive as a held command: the
 * browser delivers no `pointerup` for a pointer that navigated away, and a
 * `keyup` that arrives after the window lost focus arrives at nobody.
 *
 * It returns a fresh `held` set rather than {@link IDLE_SOURCES}'s, so a
 * caller that keeps a mutable set cannot mutate the shared constant.
 */
export function clearTransient(): InputSources {
  return { held: new Set<string>(), mouseSheet: null, touch: IDLE_TOUCH }
}

function clamp(v: number): number {
  return Math.max(-1, Math.min(1, v))
}

/**
 * Pure: the set of currently held keys plus the config becomes a `Controls`.
 * No DOM access, so it is unit-testable and frame-rate independent.
 *
 * `D`/`ArrowRight` produce a **positive** command, which per F2.2 deflects the
 * rudder so the bow turns to starboard. Holding both directions cancels.
 *
 * `sheetRateCmd` is a drag command (brief §12), so it is not derived from the
 * held keys: `sheetInput.reduceSheetInput` and `touchInput.reduceTouchInput`
 * produce it and {@link composeControls} decides which one is in force.
 * `Space` (release) is a key and is composed with the release button there.
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
    rudderRateCmd: clamp(rudder),
    sheetRateCmd: clamp(sheetRateCmd),
    sheetRelease: release,
  }
}

/**
 * The one composition (v2 F18.2, task 9.1).
 *
 * ## Priority
 *
 * Per channel, independently:
 *
 * | channel | 1st | 2nd | 3rd |
 * |---|---|---|---|
 * | rudder | touch pad | — | keyboard |
 * | sheet | touch pad | mouse drag | — (the keyboard has no sheet rate) |
 * | release | touch button **or** `Space` | | |
 *
 * A source owns a channel while it is *driving* it — `touch.rudder !== null`,
 * `mouseSheet !== null` — not while it merely holds a remembered number. That
 * is what makes "dropping ownership cannot resurrect a stale held gesture"
 * structural rather than a matter of remembering to zero something: there is
 * no stored command to resurrect, only an absent one (RV53).
 *
 * The two channels are independent, so a finger on the helm pad and a finger
 * on the sheet pad steer and trim at the same time, and a `pointerup` from an
 * unrelated pointer changes neither.
 *
 * ## Release precedence
 *
 * `sheetRelease` is the **or** of every source that can ask for it, and while
 * it is set the composed `sheetRateCmd` is exactly `0`.
 *
 * That zero is **physically inert** and is not a second implementation of the
 * precedence rule (RV56): `dynamics::sheet_rate` already ignores
 * `sheet_rate_cmd` whenever `sheet_release` is set, so no trajectory can
 * depend on what is sent in that field. It is here so that the command-rate
 * feedback the UI draws is the command actually in force — showing a haul
 * arrow while the rope is being dumped is exactly the "apparent control lag
 * misread as physics" of RV54.
 */
export function composeControls(sources: InputSources, cfg: InputConfig): Controls {
  const keyboard = controlsFromInput(sources.held, cfg)

  const rudderRaw = sources.touch.rudder ?? keyboard.rudderRateCmd
  const sheetRaw = sources.touch.sheet ?? sources.mouseSheet ?? 0
  const sheetRelease = sources.touch.release || keyboard.sheetRelease

  const rudder = clamp(rudderRaw)
  return {
    rudderRateCmd: Math.abs(rudder) < cfg.rudderDeadZone ? 0 : rudder,
    sheetRateCmd: sheetRelease ? 0 : clamp(sheetRaw),
    sheetRelease,
  }
}
