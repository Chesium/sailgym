import { beforeEach, describe, expect, it } from 'vitest'

import { createClock, MAX_STEPS_PER_TICK, type Clock, type ClockSink } from '../../src/sim/clock'

const DT = 0.005

/** Counts what the clock asked for, standing in for the WASM `Sim`. */
function makeSink(): ClockSink & { steps: number; resets: number } {
  return {
    steps: 0,
    resets: 0,
    advance(n: number) {
      this.steps += n
      return n
    },
    reset() {
      this.resets += 1
      this.steps = 0
    },
  }
}

/** Feed `ms` of wall time as realistic animation frames. */
function feed(clock: Clock, ms: number, frames = 60): void {
  for (let i = 0; i < frames; i += 1) {
    clock.tick(ms / frames)
  }
}

describe('clock', () => {
  let sink: ReturnType<typeof makeSink>
  let clock: Clock

  beforeEach(() => {
    sink = makeSink()
    clock = createClock(DT, sink)
  })

  it('issues 200 steps for 1000 ms at 1x', () => {
    feed(clock, 1000)
    expect(Math.abs(sink.steps - 200)).toBeLessThanOrEqual(1)
    expect(Math.abs(clock.getState().simTime - 1)).toBeLessThan(2 * DT)
  })

  it('issues 800 steps for 1000 ms at 4x', () => {
    clock.setSpeed(4)
    feed(clock, 1000)
    expect(Math.abs(sink.steps - 800)).toBeLessThanOrEqual(1)
  })

  it('issues 50 steps for 1000 ms at 0.25x', () => {
    clock.setSpeed(0.25)
    feed(clock, 1000)
    expect(Math.abs(sink.steps - 50)).toBeLessThanOrEqual(1)
  })

  it('caps a stalled tab at MAX_STEPS_PER_TICK', () => {
    // One 10 s frame: without the cap this would be 2000 steps.
    const issued = clock.tick(10_000)
    expect(issued).toBe(MAX_STEPS_PER_TICK)
    expect(sink.steps).toBe(MAX_STEPS_PER_TICK)
    // The dropped time is not banked into the next frame.
    sink.steps = 0
    clock.tick(16)
    expect(sink.steps).toBeLessThanOrEqual(4)
  })

  it('issues nothing while paused', () => {
    clock.pause()
    feed(clock, 1000)
    expect(sink.steps).toBe(0)
    expect(clock.getState().running).toBe(false)
  })

  it('ignores singleStep while running', () => {
    clock.singleStep()
    expect(sink.steps).toBe(0)
  })

  it('issues exactly one step for singleStep while paused', () => {
    clock.pause()
    clock.singleStep()
    expect(sink.steps).toBe(1)
    expect(clock.getState().simTime).toBeCloseTo(DT, 12)
    clock.singleStep()
    expect(sink.steps).toBe(2)
  })

  it('reset clears the sink and the counters', () => {
    feed(clock, 1000)
    clock.reset()
    expect(sink.resets).toBe(1)
    expect(clock.getState().simTime).toBe(0)
    expect(clock.getState().stepsTaken).toBe(0)
  })

  it('rejects a non-positive timestep', () => {
    expect(() => createClock(0, makeSink())).toThrow(/dt must be positive/)
  })
})
