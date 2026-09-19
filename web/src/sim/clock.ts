/**
 * The simulation clock (brief §21, §22).
 *
 * Physics is decoupled from the browser's animation frames (brief §21): the
 * clock accumulates wall time and issues whole `advance(n)` steps; rendering
 * reads whatever snapshot is most recent. **The animation-frame loop itself
 * lives in `useSimulation.ts`**, which is the only file under `web/src/sim/`
 * allowed to name it; this file just receives the elapsed milliseconds.
 *
 * No physics is implemented here (F8): the clock counts steps, the Rust core
 * takes them.
 */

export type SpeedMultiplier = 0.25 | 1 | 2 | 4

/** The four speeds brief §22 requires, in display order. */
export const SPEEDS: readonly SpeedMultiplier[] = [0.25, 1, 2, 4]

export interface ClockState {
  running: boolean
  speed: SpeedMultiplier
  simTime: number
  stepsTaken: number
}

/**
 * Max steps issued in one tick, so a stalled tab cannot produce a spiral of
 * death. Time beyond this is dropped, not banked: banking it would just move
 * the stall to the next frame.
 */
export const MAX_STEPS_PER_TICK = 240

export interface Clock {
  start(): void
  pause(): void
  reset(): void
  /** Exactly one dt, and only while paused. */
  singleStep(): void
  setSpeed(s: SpeedMultiplier): void
  /**
   * Adopt a new fixed timestep.
   *
   * `sim.dt` is a live-editable parameter (F7, brief §31) and `set_parameter`
   * reports the edit as reset-required (F8.2) precisely because it changes
   * what every step means. The core picks the new value up on its next step;
   * without this the browser clock would go on converting wall time with the
   * old one, and 1× would stop being real time. Section 08 calls it from the
   * reset path, which is where a reset-required edit is made good.
   */
  setDt(dt: number): void
  /** Called once per animation frame. Returns the physics steps issued. */
  tick(wallDeltaMs: number): number
  /** Current clock state, for rendering the controls. */
  getState(): ClockState
}

/** What the clock drives. Supplied by `useSimulation`. */
export interface ClockSink {
  /** Advance the simulation by `n` whole steps; returns steps actually taken. */
  advance(n: number): number
  /** Restore the simulation's initial state. */
  reset(): void
}

/**
 * @param dt fixed physics timestep, seconds — read from the WASM module, never
 *           hard-coded on this side (F8).
 */
export function createClock(dt: number, sink: ClockSink): Clock {
  if (!(dt > 0)) {
    throw new Error(`clock: dt must be positive, got ${dt}`)
  }
  // Reassignable, for `setDt`; every read below goes through this binding.

  let running = true
  let speed: SpeedMultiplier = 1
  let simTime = 0
  let stepsTaken = 0
  /** Unspent simulated seconds, always < dt after a tick. */
  let accumulator = 0

  const issue = (n: number): number => {
    if (n <= 0) {
      return 0
    }
    const taken = sink.advance(n)
    stepsTaken += taken
    simTime += taken * dt
    return taken
  }

  return {
    start() {
      running = true
    },
    pause() {
      running = false
      // Drop the partial step: resuming must not replay buffered wall time.
      accumulator = 0
    },
    reset() {
      sink.reset()
      simTime = 0
      stepsTaken = 0
      accumulator = 0
    },
    singleStep() {
      if (running) {
        return
      }
      issue(1)
    },
    setSpeed(s: SpeedMultiplier) {
      speed = s
    },
    setDt(next: number) {
      if (!(next > 0)) {
        throw new Error(`clock: dt must be positive, got ${next}`)
      }
      dt = next
      // The unspent fraction was measured in the old timestep and means
      // nothing in the new one.
      accumulator = 0
    },
    tick(wallDeltaMs: number): number {
      if (!running || !Number.isFinite(wallDeltaMs) || wallDeltaMs <= 0) {
        return 0
      }
      accumulator += (wallDeltaMs / 1000) * speed
      let steps = Math.floor(accumulator / dt)
      if (steps <= 0) {
        return 0
      }
      if (steps > MAX_STEPS_PER_TICK) {
        // Stalled tab: run the cap and discard the rest.
        steps = MAX_STEPS_PER_TICK
        accumulator = 0
      } else {
        accumulator -= steps * dt
      }
      return issue(steps)
    },
    getState(): ClockState {
      return { running, speed, simTime, stepsTaken }
    },
  }
}
