import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { sailShape } from '../../src/render/SailShape'
import { apparentWindDegrees } from '../../src/sim/units'

const rig = { mastX: 1, boomLength: 2, rudderX: -1, boardX: 0 }

describe('sail display', () => {
  it('mirrors the cosmetic bulge when alpha changes sign', () => {
    expect(sailShape(rig, 0, 0.2)).toBe('M 1,0 Q 0,-0.24 -1,0')
    expect(sailShape(rig, 0, -0.2)).toBe('M 1,0 Q 0,0.24 -1,0')
    expect(sailShape(rig, 0, 0)).toBe('M 1,0 Q 0,0 -1,0')
  })
  it('preserves the FROM-angle sign at the degree boundary', () => {
    expect(apparentWindDegrees(Math.PI / 2)).toBe(90)
    expect(apparentWindDegrees(-Math.PI / 2)).toBe(-90)
  })
  it('keeps the M4 diagnostic field names identical across the boundary', () => {
    const rust = readFileSync('../crates/sailgym-physics/src/diagnostics.rs', 'utf8')
    const ts = readFileSync('src/sim/diagnostics.ts', 'utf8')
    const fields = [...rust.matchAll(/pub (\w+):/g)].map((m) => m[1]).sort()
    const mirror = [...ts.matchAll(/^  (\w+):/gm)].map((m) => m[1]).sort()
    expect(mirror).toEqual(fields)
  })
})
