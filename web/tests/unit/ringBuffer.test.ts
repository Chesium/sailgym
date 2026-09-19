import { describe, expect, it } from 'vitest'

import { RingBuffer } from '../../src/ui/ringBuffer'

describe('RingBuffer', () => {
  it('respects its capacity', () => {
    const rb = new RingBuffer<number>(4)
    expect(rb.capacity).toBe(4)
    expect(rb.length).toBe(0)
    for (let i = 0; i < 100; i += 1) {
      rb.push(i)
      expect(rb.length).toBeLessThanOrEqual(4)
    }
    expect(rb.length).toBe(4)
    expect(rb.toArray()).toHaveLength(4)
  })

  it('keeps the newest values and drops the oldest, in order', () => {
    const rb = new RingBuffer<number>(3)
    rb.push(1)
    expect(rb.toArray()).toEqual([1])
    rb.push(2)
    rb.push(3)
    expect(rb.toArray()).toEqual([1, 2, 3])
    rb.push(4)
    expect(rb.toArray()).toEqual([2, 3, 4])
    rb.push(5)
    rb.push(6)
    rb.push(7)
    expect(rb.toArray()).toEqual([5, 6, 7])
  })

  it('reports the newest value', () => {
    const rb = new RingBuffer<string>(2)
    expect(rb.last).toBeUndefined()
    rb.push('a')
    expect(rb.last).toBe('a')
    rb.push('b')
    rb.push('c')
    expect(rb.last).toBe('c')
    expect(rb.toArray()).toEqual(['b', 'c'])
  })

  it('allocates nothing after construction', () => {
    const rb = new RingBuffer<number>(8)
    const backing = rb.storage
    expect(backing).toHaveLength(8)
    for (let i = 0; i < 10_000; i += 1) {
      rb.push(i)
      // Same array object, still the same length: `push` writes into a slot
      // that already exists and never grows or replaces the buffer.
      expect(rb.storage).toBe(backing)
    }
    expect(backing).toHaveLength(8)
    expect(rb.toArray()).toEqual([9992, 9993, 9994, 9995, 9996, 9997, 9998, 9999])
  })

  it('clears back to empty without replacing the buffer', () => {
    const rb = new RingBuffer<number>(3)
    const backing = rb.storage
    rb.push(1)
    rb.push(2)
    rb.clear()
    expect(rb.length).toBe(0)
    expect(rb.toArray()).toEqual([])
    expect(rb.last).toBeUndefined()
    expect(rb.storage).toBe(backing)
    rb.push(9)
    expect(rb.toArray()).toEqual([9])
  })

  it('rejects a capacity that is not a positive integer', () => {
    expect(() => new RingBuffer<number>(0)).toThrow(RangeError)
    expect(() => new RingBuffer<number>(-1)).toThrow(RangeError)
    expect(() => new RingBuffer<number>(2.5)).toThrow(RangeError)
  })
})
