import { describe, expect, it } from 'vitest'

import { readSnapshot, SNAPSHOT_FIELDS, SNAPSHOT_LENGTH } from '../../src/sim/snapshot'

describe('snapshot', () => {
  it('mirrors the 13-field F8.3 layout', () => {
    expect(SNAPSHOT_FIELDS.length).toBe(13)
    expect(SNAPSHOT_LENGTH).toBe(13)
  })

  it('maps buffer indices to the documented fields', () => {
    const buf = Float64Array.from({ length: 13 }, (_, i) => i)
    const s = readSnapshot(buf)
    // The two the PRD calls out by index, plus the ends of the buffer.
    expect(s.phi).toBe(3)
    expect(s.deltaR).toBe(10)
    expect(s.x).toBe(0)
    expect(s.t).toBe(12)
    expect(SNAPSHOT_FIELDS[3]).toBe('phi')
    expect(SNAPSHOT_FIELDS[10]).toBe('deltaR')
  })

  it('rejects a buffer of the wrong length', () => {
    expect(() => readSnapshot(new Float64Array(12))).toThrow(/expected 13/)
    expect(() => readSnapshot(new Float64Array(14))).toThrow(/expected 13/)
  })
})
