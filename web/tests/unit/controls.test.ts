import { describe, expect, it } from 'vitest'

import { controlsFromInput, DEFAULT_INPUT } from '../../src/sim/controls'
import { actionFor, KEYMAP } from '../../src/sim/keymap'

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
