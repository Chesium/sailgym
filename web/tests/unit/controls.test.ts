import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

import {
  clearTransient,
  composeControls,
  controlsFromInput,
  DEFAULT_INPUT,
  IDLE_SOURCES,
  IDLE_TOUCH,
  NEUTRAL_CONTROLS,
  TOUCH_FULL_SCALE_PX,
  type InputSources,
  type TouchCommand,
} from '../../src/sim/controls'
import { actionFor, KEYMAP } from '../../src/sim/keymap'
import { rateFor } from '../../src/sim/sheetInput'

describe('controls', () => {
  it('D steers the bow to starboard: a positive rudder command (F2.2)', () => {
    expect(controlsFromInput(new Set(['d']), DEFAULT_INPUT).rudderRateCmd).toBeGreaterThan(0)
    expect(controlsFromInput(new Set(['ArrowRight']), DEFAULT_INPUT).rudderRateCmd).toBeGreaterThan(
      0,
    )
  })

  it('A steers to port: a negative rudder command', () => {
    expect(controlsFromInput(new Set(['a']), DEFAULT_INPUT).rudderRateCmd).toBeLessThan(0)
    expect(controlsFromInput(new Set(['ArrowLeft']), DEFAULT_INPUT).rudderRateCmd).toBeLessThan(0)
  })

  it('holding both directions cancels', () => {
    expect(controlsFromInput(new Set(['a', 'd']), DEFAULT_INPUT).rudderRateCmd).toBe(0)
    expect(controlsFromInput(new Set(), DEFAULT_INPUT).rudderRateCmd).toBe(0)
  })

  it('Space requests a sheet release', () => {
    expect(controlsFromInput(new Set([' ']), DEFAULT_INPUT).sheetRelease).toBe(true)
    expect(controlsFromInput(new Set(['d']), DEFAULT_INPUT).sheetRelease).toBe(false)
  })

  it('keeps the command inside [-1, 1]', () => {
    const cfg = { ...DEFAULT_INPUT, rudderKeyRate: 5 }
    expect(controlsFromInput(new Set(['d']), cfg).rudderRateCmd).toBe(1)
    expect(controlsFromInput(new Set(['a']), cfg).rudderRateCmd).toBe(-1)
  })

  it('applies the dead zone from the config, not from a literal', () => {
    const cfg = { ...DEFAULT_INPUT, rudderKeyRate: 0.01 }
    expect(controlsFromInput(new Set(['d']), cfg).rudderRateCmd).toBe(0)
  })

  it('leaves the sheet rate to section 06', () => {
    expect(controlsFromInput(new Set(['d']), DEFAULT_INPUT).sheetRateCmd).toBe(0)
  })

  it('maps the brief §28 keys and nothing else', () => {
    expect(actionFor('D')).toBe('steerStarboard')
    expect(actionFor('p')).toBe('pause')
    expect(actionFor('.')).toBe('singleStep')
    expect(actionFor('r')).toBe('reset')
    expect(actionFor('q')).toBeUndefined()
    expect(Object.keys(KEYMAP)).toHaveLength(8)
  })
})

/**
 * v2 section 09, task 9.1 — the one input-composition path.
 *
 * `composeControls` is where keyboard, mouse and touch become a single
 * `Controls` value. Everything below is about *which* device is in force and
 * what happens when one stops driving; nothing here knows what a rate means in
 * metres or radians, which is the core's business (F8, RV56).
 */

/** A source set with only the named fields moved off idle. */
function sources(over: Partial<InputSources> = {}): InputSources {
  return { ...IDLE_SOURCES, held: new Set<string>(), ...over }
}

const touch = (over: Partial<TouchCommand> = {}): TouchCommand => ({ ...IDLE_TOUCH, ...over })

describe('composeControls — source priority', () => {
  it('an active touch pad owns the helm, and the keyboard does not fight it', () => {
    const both = sources({ held: new Set(['d']), touch: touch({ rudder: -0.5 }) })
    expect(composeControls(both, DEFAULT_INPUT).rudderRateCmd).toBeCloseTo(-0.5, 12)
    // …including when the pad is sitting at its neutral grab. Owning the
    // channel and asking for nothing is not the same as not owning it.
    const neutral = sources({ held: new Set(['d']), touch: touch({ rudder: 0 }) })
    expect(composeControls(neutral, DEFAULT_INPUT).rudderRateCmd).toBe(0)
  })

  it('falls back to the keyboard the moment the pad lets go', () => {
    const dropped = sources({ held: new Set(['d']), touch: touch({ rudder: null }) })
    expect(composeControls(dropped, DEFAULT_INPUT).rudderRateCmd).toBeGreaterThan(0)
  })

  it('the sheet goes touch, then mouse, then nothing', () => {
    const all = sources({ touch: touch({ sheet: -0.9 }), mouseSheet: 0.3 })
    expect(composeControls(all, DEFAULT_INPUT).sheetRateCmd).toBeCloseTo(-0.9, 12)

    const mouse = sources({ touch: touch({ sheet: null }), mouseSheet: 0.3 })
    expect(composeControls(mouse, DEFAULT_INPUT).sheetRateCmd).toBeCloseTo(0.3, 12)

    // The keyboard has no sheet-rate channel at all (brief §12): the sheet is
    // a drag, and `Space` is a release rather than a rate.
    const keys = sources({ held: new Set(['d', ' ']) })
    expect(composeControls({ ...keys, touch: IDLE_TOUCH }, DEFAULT_INPUT).sheetRateCmd).toBe(0)
  })

  it('steering and trimming are independent channels', () => {
    const two = sources({ touch: touch({ rudder: 0.75, sheet: -0.25 }) })
    const c = composeControls(two, DEFAULT_INPUT)
    expect(c.rudderRateCmd).toBeCloseTo(0.75, 12)
    expect(c.sheetRateCmd).toBeCloseTo(-0.25, 12)
    // Dropping one leaves the other exactly where it was.
    const helmOnly = sources({ touch: touch({ rudder: 0.75, sheet: null }) })
    expect(composeControls(helmOnly, DEFAULT_INPUT).rudderRateCmd).toBeCloseTo(0.75, 12)
    expect(composeControls(helmOnly, DEFAULT_INPUT).sheetRateCmd).toBe(0)
  })

  it('dropping ownership cannot resurrect a stale held gesture', () => {
    // There is no stored command to come back: `null` is the absence of a
    // command, so the composition has nothing to fall back *to* (RV53).
    const after = sources({ touch: touch({ rudder: null, sheet: null }), mouseSheet: null })
    expect(composeControls(after, DEFAULT_INPUT)).toEqual(NEUTRAL_CONTROLS)
  })
})

describe('composeControls — clamps, dead zone and signs', () => {
  it('clamps every channel into [−1, 1] whatever the source asks for', () => {
    const big = sources({ touch: touch({ rudder: 7, sheet: -12 }) })
    expect(composeControls(big, DEFAULT_INPUT).rudderRateCmd).toBe(1)
    expect(composeControls(big, DEFAULT_INPUT).sheetRateCmd).toBe(-1)
    const small = sources({ touch: touch({ rudder: -7, sheet: 12 }) })
    expect(composeControls(small, DEFAULT_INPUT).rudderRateCmd).toBe(-1)
    expect(composeControls(small, DEFAULT_INPUT).sheetRateCmd).toBe(1)
  })

  it('applies the dead zone to the winning helm command, from the config', () => {
    const jitter = sources({ touch: touch({ rudder: 0.01 }) })
    expect(composeControls(jitter, DEFAULT_INPUT).rudderRateCmd).toBe(0)
    expect(composeControls(jitter, { ...DEFAULT_INPUT, rudderDeadZone: 0 }).rudderRateCmd).toBe(
      0.01,
    )
    // Just outside it, the command survives with its sign.
    const real = sources({ touch: touch({ rudder: -0.05 }) })
    expect(composeControls(real, DEFAULT_INPUT).rudderRateCmd).toBeCloseTo(-0.05, 12)
  })

  it('keeps F2.2/F3 signs: + is bow to starboard, − is haul', () => {
    expect(composeControls(sources({ held: new Set(['d']) }), DEFAULT_INPUT).rudderRateCmd)
      .toBeGreaterThan(0)
    expect(composeControls(sources({ held: new Set(['a']) }), DEFAULT_INPUT).rudderRateCmd)
      .toBeLessThan(0)
    // Drag down on either device hauls, which is a negative command.
    expect(composeControls(sources({ mouseSheet: rateFor(100, DEFAULT_INPUT) }), DEFAULT_INPUT)
      .sheetRateCmd).toBeLessThan(0)
  })
})

describe('composeControls — release precedence', () => {
  it('either the key or the button latches the release', () => {
    expect(composeControls(sources({ held: new Set([' ']) }), DEFAULT_INPUT).sheetRelease).toBe(
      true,
    )
    expect(
      composeControls(sources({ touch: touch({ release: true }) }), DEFAULT_INPUT).sheetRelease,
    ).toBe(true)
    expect(composeControls(sources(), DEFAULT_INPUT).sheetRelease).toBe(false)
  })

  it('release beats a simultaneous haul from any device', () => {
    for (const s of [
      sources({ held: new Set([' ']), mouseSheet: -1 }),
      sources({ held: new Set([' ']), touch: touch({ sheet: -1 }) }),
      sources({ touch: touch({ sheet: -1, release: true }) }),
      sources({ touch: touch({ sheet: 1, release: true }) }),
    ]) {
      const c = composeControls(s, DEFAULT_INPUT)
      expect(c.sheetRelease).toBe(true)
      expect(c.sheetRateCmd).toBe(0)
    }
  })

  it('leaves the helm alone while the sheet is being dumped', () => {
    const s = sources({ held: new Set(['d']), touch: touch({ release: true }) })
    expect(composeControls(s, DEFAULT_INPUT).rudderRateCmd).toBeGreaterThan(0)
  })
})

describe('clearTransient — the one clear path', () => {
  it('drops every channel and every latch', () => {
    const busy = sources({
      held: new Set(['d', ' ']),
      mouseSheet: -0.8,
      touch: touch({ rudder: 1, sheet: -1, release: true }),
    })
    expect(composeControls(busy, DEFAULT_INPUT)).not.toEqual(NEUTRAL_CONTROLS)

    const cleared = clearTransient()
    expect(cleared).toEqual(IDLE_SOURCES)
    expect(composeControls(cleared, DEFAULT_INPUT)).toEqual(NEUTRAL_CONTROLS)
  })

  it('the hook empties the held set rather than replacing it', () => {
    // Replacing it orphans the reference the key handlers captured when their
    // effect ran, and the keyboard silently stops steering after the first
    // pause — which is exactly what the brief §46 demonstrations caught.
    const hook = readFileSync('src/sim/useSimulation.ts', 'utf8')
    expect(hook).toContain('heldRef.current.clear()')
    expect(hook).toContain('held: heldRef.current')
    expect(hook.match(/held: new Set/g), 'the held set is built exactly once').toBeNull()
  })

  it('hands back a fresh held set, so the shared idle constant cannot be mutated', () => {
    const cleared = clearTransient()
    expect(cleared.held).not.toBe(IDLE_SOURCES.held)
    ;(cleared.held as Set<string>).add('d')
    expect(IDLE_SOURCES.held.size).toBe(0)
  })

  it('is what every lifecycle transition calls', () => {
    // The clear paths the PRD enumerates: blur, a hidden page, pause, reset,
    // a scenario switch and entering replay. The first five are wired in
    // `useSimulation`; replay entry is `App.tsx`'s, through `clearInput`.
    const hook = readFileSync('src/sim/useSimulation.ts', 'utf8')
    for (const path of [
      "window.addEventListener('blur', onClear)",
      "window.addEventListener('pagehide', onClear)",
      "document.addEventListener('visibilitychange', onVisibility)",
    ]) {
      expect(hook, path).toContain(path)
    }
    // Pause, reset and the scenario switch, plus the exported escape hatch
    // `App.tsx` uses on entering replay — which `tests/e2e/mobile-controls.spec.ts`
    // checks from the outside, because it is a property of the page and not of
    // this module.
    expect(hook.match(/clearInput\(\)/g)?.length ?? 0).toBeGreaterThanOrEqual(5)
    expect(hook).toContain('clearInput,')
  })
})

describe('one application boundary', () => {
  it('equal normalized commands are the same Controls whatever produced them', () => {
    // The keyboard's full-scale helm command and a touch pad's full-scale helm
    // command are the same number, so they are the same `Controls` value, so
    // the core integrates the same trajectory. Nothing downstream can tell
    // them apart — which is the whole point of composing once (v2 F18.2).
    const byKey = composeControls(sources({ held: new Set(['d']) }), DEFAULT_INPUT)
    const byTouch = composeControls(sources({ touch: touch({ rudder: 1 }) }), DEFAULT_INPUT)
    expect(byTouch).toEqual(byKey)

    const byMouse = composeControls(sources({ mouseSheet: -0.6 }), DEFAULT_INPUT)
    const byPad = composeControls(sources({ touch: touch({ sheet: -0.6 }) }), DEFAULT_INPUT)
    expect(byPad).toEqual(byMouse)

    const bySpace = composeControls(sources({ held: new Set([' ']) }), DEFAULT_INPUT)
    const byButton = composeControls(sources({ touch: touch({ release: true }) }), DEFAULT_INPUT)
    expect(byButton).toEqual(bySpace)
  })

  it('there is exactly one set_controls call site, and no reducer advances physics', () => {
    const hook = readFileSync('src/sim/useSimulation.ts', 'utf8')
    expect(hook.match(/sim\.set_controls\(/g)?.length, 'set_controls call sites').toBe(1)
    // …and it is inside `applyControls`, which every input path goes through.
    const apply = hook.slice(hook.indexOf('const applyControls'))
    expect(apply.slice(0, apply.indexOf('}, [])'))).toContain('sim.set_controls(')

    // The physics is advanced by the clock's sink and by nothing else: no key
    // handler and no pointer handler reaches `advance` directly.
    expect(hook.match(/sim\.advance\(/g)?.length, 'advance call sites').toBe(1)
    // The reducers this task owns touch neither. The touch reducer and its
    // component are held to the same rule by `tests/unit/touchInput.test.ts`.
    for (const file of ['src/sim/controls.ts', 'src/sim/sheetInput.ts']) {
      const source = readFileSync(file, 'utf8')
      expect(source, `${file} advances physics`).not.toMatch(/\.advance\(|set_controls\(/)
    }
  })

  it('no input gain is a physical rate: the touch scales are pixel counts', () => {
    // RV56: the UI may scale a *command*, never a rate. The two touch gains
    // are reciprocals of pixel travels, and the rates they are eventually
    // multiplied by live in `parameters.rs`.
    expect(DEFAULT_INPUT.touchRudderGain).toBe(1 / TOUCH_FULL_SCALE_PX.rudder)
    expect(DEFAULT_INPUT.touchSheetGain).toBe(1 / TOUCH_FULL_SCALE_PX.sheet)
    expect(TOUCH_FULL_SCALE_PX.rudder).toBeGreaterThanOrEqual(44)
    expect(TOUCH_FULL_SCALE_PX.sheet).toBeGreaterThanOrEqual(44)
  })
})
