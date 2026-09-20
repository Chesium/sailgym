import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

import { composeControls, DEFAULT_INPUT, IDLE_SOURCES } from '../../src/sim/controls'
import {
  anyPadHeld,
  IDLE_TOUCH_INPUT,
  PAD_IDS,
  reduceTouchInput,
  touchCommand,
  type PadId,
  type TouchEventType,
  type TouchInputState,
} from '../../src/sim/touchInput'

/**
 * v2 section 09, task 9.3 — the two-pointer pads and the release button.
 *
 * Synthetic pointers only. The component calls `setPointerCapture` and hands
 * coordinates to this reducer; everything that decides *what a gesture means*
 * is here, so the awkward cases — two fingers at once, a stray `pointerup`, a
 * lift outside the pad, a cancel, a lost capture — are testable without a
 * browser. `tests/e2e/mobile-controls.spec.ts` drives the real events.
 */

/** Apply a sequence of events, left to right. */
function play(
  events: [TouchEventType, PadId, number, number, number][],
  from: TouchInputState = IDLE_TOUCH_INPUT,
): TouchInputState {
  return events.reduce(
    (s, [type, pad, pointerId, x, y]) => reduceTouchInput(s, { type, pad, pointerId, x, y }),
    from,
  )
}

const cmd = (s: TouchInputState) => touchCommand(s, DEFAULT_INPUT)

/** Pixels of travel that produce a full-scale command on each pad. */
const FULL = {
  rudder: 1 / DEFAULT_INPUT.touchRudderGain,
  sheet: 1 / DEFAULT_INPUT.touchSheetGain,
}

describe('touchInput — a grab is relative and starts neutral', () => {
  it('commands nothing on the frame the finger lands, wherever it lands', () => {
    for (const [x, y] of [
      [0, 0],
      [17, 400],
      [-250, -900],
    ]) {
      const s = play([['down', 'rudder', 1, x, y]])
      expect(cmd(s).rudder, `grab at ${x},${y}`).toBe(0)
      // …and it is a command of zero, not the absence of one: the pad owns
      // the channel from the moment it is touched.
      expect(cmd(s).rudder).not.toBeNull()
    }
  })

  it('scales travel from the grab, with F2.2 signs and a clamp', () => {
    const right = play([
      ['down', 'rudder', 1, 100, 100],
      ['move', 'rudder', 1, 100 + FULL.rudder / 2, 100],
    ])
    expect(cmd(right).rudder).toBeCloseTo(0.5, 12)
    const left = play([
      ['down', 'rudder', 1, 100, 100],
      ['move', 'rudder', 1, 100 - FULL.rudder / 2, 100],
    ])
    expect(cmd(left).rudder).toBeCloseTo(-0.5, 12)
    // Dragging right steers the bow to starboard, exactly as `D` does.
    expect(cmd(right).rudder!).toBeGreaterThan(0)
    const hard = play([
      ['down', 'rudder', 1, 0, 0],
      ['move', 'rudder', 1, 10 * FULL.rudder, 0],
    ])
    expect(cmd(hard).rudder).toBe(1)
  })

  it('drags down to haul on the sheet pad, the same rule the mouse uses', () => {
    const haul = play([
      ['down', 'sheet', 1, 0, 0],
      ['move', 'sheet', 1, 0, FULL.sheet / 2],
    ])
    expect(cmd(haul).sheet).toBeCloseTo(-0.5, 12)
    const ease = play([
      ['down', 'sheet', 1, 0, 0],
      ['move', 'sheet', 1, 0, -FULL.sheet / 2],
    ])
    expect(cmd(ease).sheet).toBeCloseTo(0.5, 12)
    // Vertical travel only: sliding sideways on the sheet pad trims nothing.
    const sideways = play([
      ['down', 'sheet', 1, 0, 0],
      ['move', 'sheet', 1, 900, 0],
    ])
    expect(cmd(sideways).sheet).toBe(-0)
  })

  it('ignores the axis each pad does not use', () => {
    const vertical = play([
      ['down', 'rudder', 1, 0, 0],
      ['move', 'rudder', 1, 0, 900],
    ])
    expect(cmd(vertical).rudder).toBe(0)
  })
})

describe('touchInput — one pointer per pad, two pads at once', () => {
  it('steers and trims simultaneously, on independent channels', () => {
    const s = play([
      ['down', 'rudder', 1, 0, 0],
      ['down', 'sheet', 2, 0, 0],
      ['move', 'rudder', 1, FULL.rudder, 0],
      ['move', 'sheet', 2, 0, FULL.sheet],
    ])
    expect(cmd(s).rudder).toBe(1)
    expect(cmd(s).sheet).toBe(-1)
    expect(anyPadHeld(s)).toBe(true)

    // And the composed `Controls` carries both, with the release clear.
    const c = composeControls({ ...IDLE_SOURCES, touch: cmd(s) }, DEFAULT_INPUT)
    expect(c.rudderRateCmd).toBe(1)
    expect(c.sheetRateCmd).toBe(-1)
    expect(c.sheetRelease).toBe(false)
  })

  it('ignores a second pointer on a pad that is already held', () => {
    const s = play([
      ['down', 'rudder', 1, 0, 0],
      ['move', 'rudder', 1, FULL.rudder / 2, 0],
      // A second finger lands on the same pad, far to the left, and moves.
      ['down', 'rudder', 2, -500, 0],
      ['move', 'rudder', 2, -900, 0],
    ])
    // The first finger still owns the pad: its command is untouched, and the
    // neutral has not moved under it.
    expect(cmd(s).rudder).toBeCloseTo(0.5, 12)
    expect(s.rudder.pointerId).toBe(1)
  })

  it('…and the other pad stays independently usable while it does', () => {
    const s = play([
      ['down', 'rudder', 1, 0, 0],
      ['down', 'rudder', 2, 0, 0],
      ['down', 'sheet', 3, 0, 0],
      ['move', 'sheet', 3, 0, FULL.sheet],
    ])
    expect(s.rudder.pointerId).toBe(1)
    expect(s.sheet.pointerId).toBe(3)
    expect(cmd(s).sheet).toBe(-1)
  })

  it('lifting the ignored second pointer does not end the first one grab', () => {
    const s = play([
      ['down', 'rudder', 1, 0, 0],
      ['move', 'rudder', 1, FULL.rudder, 0],
      ['down', 'rudder', 2, 0, 0],
      ['up', 'rudder', 2, 0, 0],
    ])
    expect(s.rudder.pointerId).toBe(1)
    expect(cmd(s).rudder).toBe(1)
  })
})

describe('touchInput — every way a grab can end', () => {
  const held = play([
    ['down', 'rudder', 1, 0, 0],
    ['down', 'sheet', 2, 0, 0],
    ['move', 'rudder', 1, FULL.rudder, 0],
    ['move', 'sheet', 2, 0, FULL.sheet],
  ])

  it.each(['up', 'cancel', 'lostcapture'] as const)(
    'a %s on the helm releases the helm and leaves the sheet exactly as it was',
    (ending) => {
      const s = play([[ending, 'rudder', 1, FULL.rudder, 0]], held)
      expect(cmd(s).rudder, 'the helm hands the channel back, it does not command 0').toBeNull()
      expect(cmd(s).sheet, 'the sheet channel must be untouched').toBe(-1)
      expect(s.sheet).toEqual(held.sheet)
    },
  )

  it.each(['up', 'cancel', 'lostcapture'] as const)(
    'a %s on the sheet releases the sheet and leaves the helm exactly as it was',
    (ending) => {
      const s = play([[ending, 'sheet', 2, 0, FULL.sheet]], held)
      expect(cmd(s).sheet).toBeNull()
      expect(cmd(s).rudder).toBe(1)
      expect(s.rudder).toEqual(held.rudder)
    },
  )

  it('an unrelated pointerup cannot cancel either grab', () => {
    // A third finger lifting off — on either pad, with an id neither pad
    // owns — changes nothing at all (RV53).
    for (const pad of PAD_IDS) {
      const s = play([['up', pad, 99, 0, 0]], held)
      expect(s, `stray pointerup on ${pad}`).toEqual(held)
      expect(cmd(s).rudder).toBe(1)
      expect(cmd(s).sheet).toBe(-1)
    }
  })

  it('a lift outside the pad still ends the grab, because the pointer is captured', () => {
    // The pad captured the pointer, so the `pointerup` arrives here with
    // coordinates far outside the element. Nothing in the reducer looks at
    // whether the point is inside anything — it cannot, and it must not.
    const s = play([['up', 'rudder', 1, -5000, 9000]], held)
    expect(cmd(s).rudder).toBeNull()
    expect(s.rudder.pointerId).toBeNull()
  })

  it('a move that arrives after the grab ended is ignored', () => {
    const s = play(
      [
        ['up', 'rudder', 1, FULL.rudder, 0],
        ['move', 'rudder', 1, -FULL.rudder, 0],
      ],
      held,
    )
    expect(cmd(s).rudder).toBeNull()
  })

  it('lifting is not a sheet dump', () => {
    const s = play([['up', 'sheet', 2, 0, FULL.sheet]], held)
    const c = composeControls({ ...IDLE_SOURCES, touch: cmd(s) }, DEFAULT_INPUT)
    expect(c.sheetRelease, 'lifting must never latch the release').toBe(false)
    expect(c.sheetRateCmd, 'and it must not command an ease either').toBe(0)
  })
})

describe('touchInput — the release button', () => {
  it('is held, and only held', () => {
    const down = play([['down', 'release', 7, 0, 0]])
    expect(cmd(down).release).toBe(true)
    expect(composeControls({ ...IDLE_SOURCES, touch: cmd(down) }, DEFAULT_INPUT).sheetRelease).toBe(
      true,
    )
    const up = play([['up', 'release', 7, 0, 0]], down)
    expect(cmd(up).release).toBe(false)
  })

  it('a cancelled or lost contact clears it, with no further input needed', () => {
    for (const ending of ['cancel', 'lostcapture'] as const) {
      const s = play([['down', 'release', 7, 0, 0], [ending, 'release', 7, 0, 0]])
      expect(cmd(s).release, ending).toBe(false)
    }
  })

  it('releasing does not disturb the helm', () => {
    const s = play([
      ['down', 'rudder', 1, 0, 0],
      ['move', 'rudder', 1, FULL.rudder, 0],
      ['down', 'release', 2, 0, 0],
    ])
    const c = composeControls({ ...IDLE_SOURCES, touch: cmd(s) }, DEFAULT_INPUT)
    expect(c.rudderRateCmd).toBe(1)
    expect(c.sheetRelease).toBe(true)
    // Release precedence still zeroes the sheet **command** (see
    // `composeControls`); the core would ignore it either way.
    expect(c.sheetRateCmd).toBe(0)
  })

  it('an idle pad set is exactly no command at all', () => {
    expect(cmd(IDLE_TOUCH_INPUT)).toEqual({ rudder: null, sheet: null, release: false })
    expect(anyPadHeld(IDLE_TOUCH_INPUT)).toBe(false)
    expect(
      composeControls({ ...IDLE_SOURCES, touch: cmd(IDLE_TOUCH_INPUT) }, DEFAULT_INPUT),
    ).toEqual({ rudderRateCmd: 0, sheetRateCmd: 0, sheetRelease: false })
  })
})

/**
 * A source file with its comment lines removed.
 *
 * The greps below forbid things from the **code**. A doc comment that names
 * one in order to say it is absent — "no ratchet, no cleat, no vibration" — is
 * the documentation doing its job, and a grep that could not tell the two
 * apart would push exactly that explanation out of the file.
 */
function codeOnly(source: string): string {
  return source
    .split('\n')
    .filter((line) => !/^\s*(\*|\/\/|\/\*)/.test(line))
    .join('\n')
}

describe('touchInput — what the component may not contain', () => {
  const reducer = codeOnly(readFileSync('src/sim/touchInput.ts', 'utf8'))
  const component = codeOnly(readFileSync('src/ui/TouchControls.tsx', 'utf8'))
  const boat = codeOnly(readFileSync('src/render/BoatSvg.tsx', 'utf8'))

  it('neither the reducer nor the component advances physics or writes controls', () => {
    for (const [name, source] of [
      ['touchInput.ts', reducer],
      ['TouchControls.tsx', component],
      ['BoatSvg.tsx', boat],
    ] as const) {
      expect(source, `${name} advances physics`).not.toMatch(/\.advance\(|set_controls\(/)
    }
  })

  it('has no hand model, ratchet, cleat or required vibration', () => {
    // RV56, and the PRD's own exclusion list. A ratchet or a tension-dependent
    // slip law changes the effective plant even when it is written in
    // TypeScript, and `navigator.vibrate` specifies durations rather than
    // force and cannot be required of a browser.
    for (const [name, source] of [
      ['touchInput.ts', reducer],
      ['TouchControls.tsx', component],
    ] as const) {
      expect(source, `${name}`).not.toMatch(/ratchet|cleat|vibrate|navigator\.vibrate|hapt/i)
      // No tension, no force, no slip: the pads produce a normalised command
      // and know nothing about what the rope is carrying.
      expect(source, `${name} reads a tension`).not.toMatch(/tension|slip|newton/i)
    }
  })

  it('restates no catalogue value: every limit and rate comes from params', () => {
    // The PRD's named ones — `0.9`, `4.5`, and the 2.4 s / 0.6 s haul and
    // release times of the superseded proposal — and any other metre value.
    expect(component, 'a hard-coded sheet or rudder limit').not.toMatch(
      /\b(0\.9|4\.5|2\.4|0\.6|1\.5|3\.0|6\.0|0\.698|2\.09)\b/,
    )
    for (const path of [
      'params.rudder.delta_r_max',
      'params.rudder.delta_r_rate_max',
      'params.sheet.l_sheet_min',
      'params.sheet.l_sheet_max',
      'params.sheet.sheet_haul_rate',
      'params.sheet.sheet_ease_rate',
      'params.sheet.sheet_release_rate',
    ]) {
      expect(component, `${path} must come from the catalogue`).toContain(path)
    }
  })

  it('draws the actual state and the command as separate, labelled things', () => {
    // RV54: a rate command and a position gauge have different units and must
    // not appear as one position error.
    for (const id of [
      'touch-gauge-rudder',
      'touch-gauge-sheet',
      'touch-command-rudder',
      'touch-command-sheet',
    ]) {
      expect(component, id).toContain(`data-testid="${id}"`)
    }
    // The gauges read the snapshot; the commands read the pads.
    expect(component).toContain('data-testid="touch-gauge-rudder" data-value={snapshot.deltaR}')
    expect(component).toContain('data-testid="touch-gauge-sheet" data-value={snapshot.lSheet}')
    expect(component).toContain('(actual)')
    expect(component).toContain('command (rate)')
  })

  it('names every control and gives it focus and a key', () => {
    expect(component.match(/aria-label=/g)?.length ?? 0).toBeGreaterThanOrEqual(4)
    expect(component.match(/aria-keyshortcuts=/g)?.length ?? 0).toBeGreaterThanOrEqual(3)
    expect(component).toContain('tabIndex={0}')
    expect(component, 'focus indication').toContain(':focus-visible')
    // Release and reset are native buttons, so they are in the tab order and
    // activate from the keyboard without anything being re-implemented.
    expect(component.match(/<button\s/g)?.length).toBe(2)
  })

  it('meets the 44 × 44 CSS-pixel minimum on every target', () => {
    const minTarget = Number(/const TARGET_MIN_PX = (\d+)/.exec(component)?.[1])
    const minPad = Number(/const PAD_MIN_PX = (\d+)/.exec(component)?.[1])
    expect(minTarget).toBeGreaterThanOrEqual(44)
    expect(minPad).toBeGreaterThanOrEqual(44)
    // A pad must hold a centred grab plus full travel either side of it.
    expect(minPad).toBeGreaterThanOrEqual(2 * FULL.rudder)
  })

  it('prevents the browser gesture only where a control owns the pointer', () => {
    // In the component, `preventDefault` appears only in the pad handlers, and
    // both of those bail out first unless this pad owns the pointer.
    expect(component).toContain("if (state.current[pad].pointerId !== null) {")
    expect(component).toContain('if (state.current[pad].pointerId !== e.pointerId) {')
    // On the world view, capture happens only for a gesture it actually owns.
    expect(boat).toContain('if (panning || trimming) {')
    expect(boat).toContain("e.pointerType !== 'touch'")
  })

  it('handles pointercancel, lost capture and unmount', () => {
    expect(component).toContain('onPointerCancel')
    expect(component).toContain('onLostPointerCapture')
    expect(component, 'unmount clears the pads').toContain('onCommandRef.current(IDLE_TOUCH)')
    expect(boat).toContain('onLostPointerCapture')
  })
})
