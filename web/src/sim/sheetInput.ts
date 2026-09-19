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

/** The command a given accumulated drag produces. Down (`+y`) hauls (`−`). */
export function rateFor(accumulated: number, cfg: InputConfig): number {
  const sign = cfg.sheetInvert ? 1 : -1
  return clamp(sign * cfg.sheetDragGain * accumulated)
}
