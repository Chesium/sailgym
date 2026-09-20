/**
 * Mouse → mainsheet rate (brief §12, section 06 task 6.3).
 *
 * The player drags vertically to haul or ease. The command is a **rate**, not
 * an angle: what the drag sets is `sheetRateCmd`, which the Rust core turns
 * into `L̇` (F4.3). Nothing here knows what the boom will do about it.
 *
 * ## Direction convention — fixed here
 *
 * **Drag down = haul in. Drag up = ease.** Pulling the mouse toward you is
 * pulling the rope toward you. `sheetInvert` in `InputConfig` is the escape
 * hatch; the help overlay in `App.tsx` states the convention, so the two must
 * change together.
 *
 * The rate follows the *displacement from where the drag started*, not the
 * speed of the mouse, so holding the button down at an offset holds a steady
 * haul — which is what brief §46 step 4 ("hauls and holds the mainsheet")
 * asks for. Releasing the button returns the command to exactly zero.
 *
 * ## It must not fight the camera
 *
 * Left-drag is the mainsheet; middle-drag and Shift+drag pan the camera
 * (brief §27, reserved in section 02). The reducer ignores anything that is
 * not a plain primary-button drag, so a camera pan produces a sheet rate of
 * exactly `0`. That decision lives here, in the pure function, rather than in
 * the component, so it is unit-testable.
 *
 * ## Touch belongs to the pads
 *
 * A `touch` pointer on the world view produces **no** sheet command (v2
 * section 09). Trimming by touch is the sheet pad's job — it has a labelled
 * neutral grab, a gauge and a release button next to it — and letting a finger
 * on the boat haul as well would mean a player who put a thumb down to look at
 * the boat trimmed it by accident, with no way to tell which of the two had
 * the channel. The pad's own reducer is `touchInput.ts`, and it reuses
 * {@link rateFor} so the two agree about what a pixel of drag means.
 *
 * **No physics here** (F8): this file maps pixels to a normalised command and
 * does nothing else.
 */

import type { InputConfig } from './controls'

export type SheetEventType = 'down' | 'move' | 'up' | 'cancel'

export interface SheetEvent {
  type: SheetEventType
  /** Pointer position in screen pixels. Screen `y` grows **downward**. */
  y: number
  /** `PointerEvent.button`; `0` is the primary button. Defaults to primary. */
  button?: number
  /** `PointerEvent.shiftKey`. A Shift-drag belongs to the camera. */
  shiftKey?: boolean
  /**
   * `PointerEvent.pointerType`. Absent means `'mouse'`, which is what every
   * section 02–08 caller and every `page.mouse` event in the E2E suite is.
   * `'touch'` belongs to the pads; see the module header.
   */
  pointerType?: string
}

export interface SheetInputState {
  dragging: boolean
  lastY: number | null
  /** Pixels dragged since the drag started; positive is downward. */
  accumulated: number
}

export const IDLE_SHEET_INPUT: SheetInputState = {
  dragging: false,
  lastY: null,
  accumulated: 0,
}

function clamp(v: number): number {
  return Math.max(-1, Math.min(1, v))
}

/**
 * Pure reducer: `(state, event) → [state, sheetRateCmd]`.
 *
 * The returned command is normalised to `[−1, 1]`, where `−1` is a full haul
 * and `+1` a full ease, matching the Rust `Controls` field (F3).
 */
export function reduceSheetInput(
  s: SheetInputState,
  ev: SheetEvent,
  cfg: InputConfig,
): [SheetInputState, number] {
  switch (ev.type) {
    case 'down': {
      const primary = (ev.button ?? 0) === 0
      if (!primary || ev.shiftKey === true) {
        // The camera's drag. Not ours, and it must not move the sheet.
        return [IDLE_SHEET_INPUT, 0]
      }
      if ((ev.pointerType ?? 'mouse') === 'touch') {
        // The sheet pad's finger, or a finger on the boat. Either way this
        // reducer does not own it.
        return [IDLE_SHEET_INPUT, 0]
      }
      return [{ dragging: true, lastY: ev.y, accumulated: 0 }, 0]
    }
    case 'move': {
      if (!s.dragging || s.lastY === null) {
        return [s, 0]
      }
      const accumulated = s.accumulated + (ev.y - s.lastY)
      const next = { dragging: true, lastY: ev.y, accumulated }
      return [next, rateFor(accumulated, cfg)]
    }
    default:
      // 'up' and 'cancel': the rope is let go, the command is exactly zero.
      return [IDLE_SHEET_INPUT, 0]
  }
}

/**
 * Whether a drag currently **owns** the sheet channel.
 *
 * `composeControls` needs the difference between "the mouse is driving the
 * sheet and asking for zero" and "the mouse is not driving the sheet at all":
 * only the second falls through to another device. A bare rate of `0` cannot
 * express it (RV53).
 */
export function ownsSheet(s: SheetInputState): boolean {
  return s.dragging
}

/** The command a given accumulated **mouse** drag produces. Down (`+y`) hauls (`−`). */
export function rateFor(accumulated: number, cfg: InputConfig): number {
  return normalisedDrag(accumulated, cfg.sheetDragGain, cfg.sheetInvert)
}

/**
 * Pixels of vertical drag → a normalised sheet command in `[−1, 1]`.
 *
 * The sign rule of this module, stated once and shared: **down (`+y`) hauls
 * (`−`)**, `sheetInvert` flips it. The mouse reducer above and the touch pad's
 * reducer in `touchInput.ts` differ only in their gain, so they call this
 * rather than each writing the same product and the same clamp — which is how
 * two drags on the same axis come to disagree about which way is "in".
 */
export function normalisedDrag(pixels: number, gain: number, invert: boolean): number {
  const sign = invert ? 1 : -1
  return clamp(sign * gain * pixels)
}
