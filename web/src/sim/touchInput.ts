/**
 * Touch pads → normalised commands (v2 section 09, task 9.3).
 *
 * Two rate pads and a hold-to-release button, each owning exactly one
 * `pointerId`. The reducer is pure — no DOM, no capture calls, no React — so
 * simultaneous steering and trimming, an unrelated `pointerup`, a lift outside
 * the pad, a `pointercancel` and a lost capture are all unit-testable without
 * a browser. `ui/TouchControls.tsx` is the thin component that turns real
 * `PointerEvent`s into the events below and calls `setPointerCapture`.
 *
 * ## Rate control, and why it is not position control
 *
 * A drag from the point it started at produces a **rate**, bounded and
 * normalised, exactly as the keyboard and the mouse do. It is not a target
 * angle and not a target length: a zero rudder command currently means
 * *self-centre* in the core (`dynamics::rudder_rate`), so a P controller that
 * reached zero error could not hold a non-zero rudder angle, and hiding that
 * with an epsilon command or by toggling `delta_r_self_centre` per frame would
 * be a second plant in TypeScript. Position targets wait for an explicit
 * engaged/released contract in a shared Rust adapter (v2 F18.2,
 * `discussions/mobile-controls.md`).
 *
 * ## Lifting is not a dump
 *
 * A pad that is not being touched reports `null` on its channel — *no command*
 * — rather than `0`. Letting go of the helm hands it back to the core's
 * self-centring; letting go of the sheet leaves the rope where it is. Only the
 * release button, held deliberately, pays the sheet out (RV53).
 *
 * **No physics here** (F8, RV56). Every number this file produces is
 * dimensionless and lies in `[−1, 1]`; the metres per second and radians per
 * second it becomes are `parameters.rs`'s, applied in `dynamics.rs`.
 */

import {
  type InputConfig,
  type TouchCommand,
  IDLE_TOUCH,
} from './controls'
import { normalisedDrag } from './sheetInput'

/** The three controls a pointer can land on. */
export type PadId = 'rudder' | 'sheet' | 'release'

export const PAD_IDS: readonly PadId[] = ['rudder', 'sheet', 'release']

export type TouchEventType = 'down' | 'move' | 'up' | 'cancel' | 'lostcapture'

/** One pointer event, already resolved to the pad that received it. */
export interface TouchPointerEvent {
  type: TouchEventType
  pad: PadId
  /** `PointerEvent.pointerId`. Pads are told apart by this and nothing else. */
  pointerId: number
  /** Client pixels. Screen `y` grows **downward**. */
  x: number
  y: number
}

/**
 * One pad's grab.
 *
 * `pointerId === null` is the whole of "nobody is touching this pad": there is
 * no separate `active` flag to fall out of step with it.
 */
export interface PadState {
  pointerId: number | null
  /** Where the grab began — the neutral, so a fresh grab commands nothing. */
  originX: number
  originY: number
  /** Where the pointer is now. */
  x: number
  y: number
}

export const IDLE_PAD: PadState = { pointerId: null, originX: 0, originY: 0, x: 0, y: 0 }

export interface TouchInputState {
  rudder: PadState
  sheet: PadState
  release: PadState
}

export const IDLE_TOUCH_INPUT: TouchInputState = {
  rudder: IDLE_PAD,
  sheet: IDLE_PAD,
  release: IDLE_PAD,
}

/**
 * Pure reducer: `(state, event) → state`.
 *
 * The rules, all of which are asserted in `tests/unit/touchInput.test.ts`:
 *
 * 1. **One pointer per pad.** A `down` on a pad that already has a pointer is
 *    ignored outright — the first finger keeps the pad, and a second finger
 *    resting on it cannot snatch the grab or move the neutral.
 * 2. **Pads are independent.** An event names its pad, so a second finger on
 *    the *other* pad is a normal grab and the two channels run at once.
 * 3. **Only the owning pointer can move or end a grab.** A `move`, `up`,
 *    `cancel` or `lostcapture` carrying a different `pointerId` leaves the pad
 *    exactly as it was. This is what stops an unrelated `pointerup` — a second
 *    finger lifting off elsewhere, a stray pen — from cancelling the helm
 *    (RV53).
 * 4. **Every ending ends it.** `up`, `cancel` and `lostcapture` all return the
 *    pad to {@link IDLE_PAD}. `pointercancel` and `lostpointercapture` are the
 *    two the browser sends when it takes the pointer away — a system gesture,
 *    a scroll taking over, the element unmounting — and a pad that only
 *    handled `pointerup` would stay stuck on.
 * 5. **The grab is relative and begins neutral.** `origin` is set from the
 *    `down`, so the command starts at exactly zero however far into the pad
 *    the finger landed: no jump.
 */
export function reduceTouchInput(
  s: TouchInputState,
  ev: TouchPointerEvent,
): TouchInputState {
  const pad = s[ev.pad]

  if (ev.type === 'down') {
    if (pad.pointerId !== null) {
      return s
    }
    return {
      ...s,
      [ev.pad]: { pointerId: ev.pointerId, originX: ev.x, originY: ev.y, x: ev.x, y: ev.y },
    }
  }

  if (pad.pointerId !== ev.pointerId) {
    return s
  }

  if (ev.type === 'move') {
    return { ...s, [ev.pad]: { ...pad, x: ev.x, y: ev.y } }
  }
  // 'up', 'cancel', 'lostcapture'.
  return { ...s, [ev.pad]: IDLE_PAD }
}

/** Whether any pad currently holds a pointer. */
export function anyPadHeld(s: TouchInputState): boolean {
  return PAD_IDS.some((pad) => s[pad].pointerId !== null)
}

/**
 * The command the pads are asking for, for `composeControls`.
 *
 * - **Helm**: horizontal travel from the grab. Dragging right is a positive
 *   command, which per F2.2 steers the bow to starboard — the same sign `D`
 *   and `→` produce, so the two devices cannot disagree about which way is
 *   right.
 * - **Sheet**: vertical travel from the grab, through the shared
 *   {@link normalisedDrag} rule, so dragging down hauls exactly as a mouse
 *   drag on the boat does and `sheetInvert` flips both together.
 * - **Release**: the button is held, or it is not.
 *
 * A pad with no pointer contributes `null`, not `0`; see the module header.
 */
export function touchCommand(s: TouchInputState, cfg: InputConfig): TouchCommand {
  const helm = s.rudder
  const sheet = s.sheet
  return {
    rudder:
      helm.pointerId === null
        ? null
        : clamp(cfg.touchRudderGain * (helm.x - helm.originX)),
    sheet:
      sheet.pointerId === null
        ? null
        : normalisedDrag(sheet.y - sheet.originY, cfg.touchSheetGain, cfg.sheetInvert),
    release: s.release.pointerId !== null,
  }
}

/** `IDLE_TOUCH`, re-exported so a caller clearing the pads needs one import. */
export const NO_TOUCH_COMMAND: TouchCommand = IDLE_TOUCH

function clamp(v: number): number {
  return Math.max(-1, Math.min(1, v))
}
