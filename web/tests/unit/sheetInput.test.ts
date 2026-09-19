import { describe, expect, it } from 'vitest'

import { DEFAULT_INPUT } from '../../src/sim/controls'
import { IDLE_SHEET_INPUT, reduceSheetInput, type SheetEvent } from '../../src/sim/sheetInput'

/** Press at y = 0, then drag to `y`, and report the resulting rate command. */
function drag(y: number, cfg = DEFAULT_INPUT, down: Partial<SheetEvent> = {}) {
  const [afterDown, downRate] = reduceSheetInput(
    IDLE_SHEET_INPUT,
    { type: 'down', y: 0, button: 0, ...down },
    cfg,
  )
  const [state, rate] = reduceSheetInput(afterDown, { type: 'move', y, ...down }, cfg)
  return { downRate, state, rate }
}

describe('sheetInput', () => {
  it('drags down to haul: 100 px at gain 0.004 is a −0.4 command', () => {
    expect(drag(100).rate).toBeCloseTo(-0.4, 9)
  })

  it('drags up to ease: the exact mirror', () => {
    expect(drag(-100).rate).toBeCloseTo(0.4, 9)
    expect(drag(-100).rate).toBeCloseTo(-drag(100).rate, 9)
  })

  it('sheetInvert flips both directions', () => {
    const cfg = { ...DEFAULT_INPUT, sheetInvert: true }
    expect(drag(100, cfg).rate).toBeCloseTo(0.4, 9)
    expect(drag(-100, cfg).rate).toBeCloseTo(-0.4, 9)
  })

  it('holds the command while the button is held, then returns to exactly 0', () => {
    // The rate follows the displacement from the press, so a held drag is a
    // held haul (brief §46 step 4).
    const { state, rate } = drag(100)
    expect(rate).toBeCloseTo(-0.4, 9)
    const [released, releasedRate] = reduceSheetInput(state, { type: 'up', y: 100 }, DEFAULT_INPUT)
    expect(releasedRate).toBe(0)
    expect(released).toEqual(IDLE_SHEET_INPUT)
    // A cancelled pointer does the same.
    expect(reduceSheetInput(state, { type: 'cancel', y: 100 }, DEFAULT_INPUT)[1]).toBe(0)
  })

  it('a Shift-drag belongs to the camera and moves the sheet by exactly 0', () => {
    expect(drag(100, DEFAULT_INPUT, { shiftKey: true }).rate).toBe(0)
    expect(drag(-250, DEFAULT_INPUT, { shiftKey: true }).rate).toBe(0)
    // As does a middle-button drag.
    expect(drag(100, DEFAULT_INPUT, { button: 1 }).rate).toBe(0)
  })

  it('a move with no button down produces no command', () => {
    expect(reduceSheetInput(IDLE_SHEET_INPUT, { type: 'move', y: 400 }, DEFAULT_INPUT)[1]).toBe(0)
  })

  it('accumulates over several moves and clamps to [−1, 1]', () => {
    let state = reduceSheetInput(IDLE_SHEET_INPUT, { type: 'down', y: 0 }, DEFAULT_INPUT)[0]
    let rate = 0
    for (const y of [50, 100, 150]) {
      ;[state, rate] = reduceSheetInput(state, { type: 'move', y }, DEFAULT_INPUT)
    }
    expect(state.accumulated).toBe(150)
    expect(rate).toBeCloseTo(-0.6, 9)
    ;[state, rate] = reduceSheetInput(state, { type: 'move', y: 5000 }, DEFAULT_INPUT)
    expect(rate).toBe(-1)
  })

  it('pressing down does not itself command anything', () => {
    expect(drag(0).downRate).toBe(0)
  })
})
