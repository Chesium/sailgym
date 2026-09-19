/**
 * The React driver: loads the WASM module, owns the `Sim`, and runs the one
 * `requestAnimationFrame` loop in the application.
 *
 * Physics is decoupled from rAF (brief §21): the frame callback only hands the
 * elapsed wall time to the clock, which converts it into whole `advance(n)`
 * calls. Rendering reads the most recent snapshot.
 *
 * **This is the only file under `web/src/sim/` that may mention
 * `requestAnimationFrame`.** `clock.ts` contains none at all, and the deck.gl
 * wind overlay does not open a second loop — it hangs off {@link FrameHook}
 * below, so the visualization is always sampling the state the physics just
 * produced.
 */

import { useCallback, useEffect, useRef, useState } from 'react'

import { controlsFromInput, DEFAULT_INPUT, type InputConfig } from './controls'
import { createClock, type Clock, type ClockState, type SpeedMultiplier } from './clock'
import { actionFor, EDGE_ACTIONS, normaliseKey } from './keymap'
import { loadWasm, type SimHandle } from './loadWasm'
import { readSnapshot, SNAPSHOT_FIELDS, type Snapshot } from './snapshot'

/** The parameters the renderer needs, as `parameters_json()` shapes them. */
export interface RenderParams {
  hull: { loa: number; beam: number }
  sail: { boom_length: number; mast_pos_b: { x: number; y: number; z: number } }
  rudder: { pos_b: { x: number; y: number; z: number } }
  board: { pos_b: { x: number; y: number; z: number } }
}

export interface WorldPoint {
  x: number
  y: number
}

/**
 * Called once per animation frame, after the physics has advanced and the
 * snapshot has been read.
 *
 * Exists so the wind visualization can share the one frame loop (section 02
 * handoff, item 6) instead of starting its own. It is handed the live `Sim`
 * because the batched `sample_wind_grid` is the only sanctioned way to read
 * the field (brief section 19); it must not be used for per-entity queries.
 */
export type FrameHook = (sim: SimHandle, snapshot: Snapshot, dtMs: number) => void

export interface SimulationHandle {
  ready: boolean
  error: string | null
  version: string
  dt: number
  snapshot: Snapshot
  clockState: ClockState
  params: RenderParams | null
  trajectory: readonly WorldPoint[]
  start(): void
  pause(): void
  toggleRunning(): void
  reset(): void
  singleStep(): void
  setSpeed(s: SpeedMultiplier): void
  /**
   * Run an imperative call against the live `Sim`, or return `null` if the
   * module is not ready yet.
   *
   * The escape hatch for the coarse-grained calls that are not part of the
   * frame loop — switching the wind mode, editing a parameter. It keeps
   * ownership of the `Sim` here, so nothing else has to decide when to free
   * it.
   */
  withSim<T>(fn: (sim: SimHandle) => T): T | null
}

const ZERO_SNAPSHOT: Snapshot = readSnapshot(new Float64Array(SNAPSHOT_FIELDS.length))

/**
 * A `Sim::reset` scenario document: the core's own initial state with the
 * surge velocity `u` replaced.
 *
 * There is no sail until section 05, so after section 04 the boat cannot
 * accelerate itself and a browser test of steering has nothing to steer. This
 * is the sanctioned substitute — an initial condition, not a force — and it is
 * deliberately built from `snapshot()` rather than written out here, so that
 * every other field (notably `l_sheet`, which starts at `l_sheet_min`) keeps
 * the value Rust chose. No physics and no parameter is duplicated in
 * TypeScript (F8).
 *
 * Section 09 replaces this with the real scenario system (brief section 32).
 */
function surgeScenario(sim: SimHandle, u: number): string {
  const values = sim.snapshot()
  const state: Record<string, number> = {}
  SNAPSHOT_FIELDS.forEach((name, i) => {
    // `SNAPSHOT_FIELDS` is camel case; the Rust `BoatState` is snake case.
    state[name.replace(/[A-Z]/g, (ch) => `_${ch.toLowerCase()}`)] = values[i]
  })
  state.u = u
  return JSON.stringify({ state })
}

/** Sim-seconds between recorded trajectory points, and how many to keep. */
const TRAJECTORY_INTERVAL_S = 0.2
const TRAJECTORY_MAX_POINTS = 3000

export function useSimulation(
  input: InputConfig = DEFAULT_INPUT,
  onFrame?: FrameHook,
  initialSurge = 0,
): SimulationHandle {
  const [ready, setReady] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [version, setVersion] = useState('')
  const [dt, setDt] = useState(0)
  const [params, setParams] = useState<RenderParams | null>(null)
  const [snapshot, setSnapshot] = useState<Snapshot>(ZERO_SNAPSHOT)
  const [clockState, setClockState] = useState<ClockState>({
    running: true,
    speed: 1,
    simTime: 0,
    stepsTaken: 0,
  })
  const [trajectory, setTrajectory] = useState<readonly WorldPoint[]>([])

  const simRef = useRef<SimHandle | null>(null)
  const clockRef = useRef<Clock | null>(null)
  const heldRef = useRef<Set<string>>(new Set())
  const trackRef = useRef<WorldPoint[]>([])
  const lastTrackRef = useRef(-Infinity)
  const inputRef = useRef(input)
  inputRef.current = input
  // Held in a ref so a new closure per frame does not restart the loop.
  const frameHookRef = useRef<FrameHook | undefined>(onFrame)
  frameHookRef.current = onFrame
  // Read once, when the module loads; the simulation is not rebuilt if it
  // changes afterwards.
  const initialSurgeRef = useRef(initialSurge)
  initialSurgeRef.current = initialSurge

  // --- module + simulation lifetime ---------------------------------------
  useEffect(() => {
    let cancelled = false
    loadWasm()
      .then((wasm) => {
        if (cancelled) {
          return
        }
        const sim = new wasm.Sim('{}')
        simRef.current = sim
        setVersion(sim.version())
        setDt(sim.dt())
        setParams(JSON.parse(sim.parameters_json() as string) as RenderParams)

        const surge = initialSurgeRef.current
        const scenario = surge === 0 ? '{}' : surgeScenario(sim, surge)
        if (scenario !== '{}') {
          sim.reset(scenario)
        }
        setSnapshot(readSnapshot(sim.snapshot()))

        clockRef.current = createClock(sim.dt(), {
          advance: (n) => sim.advance(n),
          reset: () => {
            sim.reset(scenario)
            trackRef.current = []
            lastTrackRef.current = -Infinity
            setTrajectory([])
          },
        })
        setReady(true)
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setError(cause instanceof Error ? cause.message : String(cause))
        }
      })

    return () => {
      cancelled = true
      clockRef.current = null
      simRef.current?.free()
      simRef.current = null
    }
  }, [])

  // --- keyboard ------------------------------------------------------------
  useEffect(() => {
    if (!ready) {
      return
    }
    const held = heldRef.current

    /**
     * Push the currently held keys into the core.
     *
     * Called from the key handlers as well as from the frame loop. Doing it
     * only in the frame loop leaves a race: a key press followed immediately
     * by a single step advances the physics with the *previous* frame's
     * controls, and how often that happens depends on the frame rate. It
     * showed up as `determinism.spec.ts` sampling a rudder that had not moved
     * yet, on a machine whose frames had become slow.
     */
    const applyControls = () => {
      const sim = simRef.current
      if (sim === null) {
        return
      }
      const c = controlsFromInput(held, inputRef.current)
      sim.set_controls(c.rudderRateCmd, c.sheetRateCmd, c.sheetRelease)
    }

    const onKeyDown = (e: KeyboardEvent) => {
      const key = normaliseKey(e.key)
      const action = actionFor(key)
      if (action === undefined) {
        return
      }
      // Space and the arrows scroll the page otherwise.
      e.preventDefault()
      if (EDGE_ACTIONS.has(action)) {
        if (e.repeat) {
          return
        }
        const clock = clockRef.current
        if (clock === null) {
          return
        }
        // Before the step, never after it.
        applyControls()
        if (action === 'reset') {
          clock.reset()
        } else if (action === 'pause') {
          if (clock.getState().running) {
            clock.pause()
          } else {
            clock.start()
          }
        } else {
          clock.singleStep()
        }
        setClockState(clock.getState())
        pushSnapshot()
        return
      }
      held.add(key)
      applyControls()
    }

    const onKeyUp = (e: KeyboardEvent) => {
      held.delete(normaliseKey(e.key))
      applyControls()
    }
    const onBlur = () => {
      held.clear()
      applyControls()
    }

    const pushSnapshot = () => {
      const sim = simRef.current
      if (sim !== null) {
        setSnapshot(readSnapshot(sim.snapshot()))
      }
    }

    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('keyup', onKeyUp)
    window.addEventListener('blur', onBlur)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
      window.removeEventListener('blur', onBlur)
      held.clear()
    }
  }, [ready])

  // --- the frame loop ------------------------------------------------------
  useEffect(() => {
    if (!ready) {
      return
    }
    let frame = 0
    let previous: number | null = null

    const onFrame = (now: number) => {
      frame = requestAnimationFrame(onFrame)
      const sim = simRef.current
      const clock = clockRef.current
      if (sim === null || clock === null) {
        return
      }
      const delta = previous === null ? 0 : now - previous
      previous = now

      const c = controlsFromInput(heldRef.current, inputRef.current)
      sim.set_controls(c.rudderRateCmd, c.sheetRateCmd, c.sheetRelease)
      clock.tick(delta)

      const next = readSnapshot(sim.snapshot())
      frameHookRef.current?.(sim, next, delta)
      setSnapshot(next)
      setClockState(clock.getState())

      if (next.t - lastTrackRef.current >= TRAJECTORY_INTERVAL_S) {
        lastTrackRef.current = next.t
        const track = trackRef.current
        track.push({ x: next.x, y: next.y })
        if (track.length > TRAJECTORY_MAX_POINTS) {
          track.shift()
        }
        setTrajectory(track.slice())
      }
    }

    frame = requestAnimationFrame(onFrame)
    return () => cancelAnimationFrame(frame)
  }, [ready])

  // --- imperative controls, shared with ClockControls -----------------------
  const withClock = useCallback((fn: (clock: Clock) => void) => {
    const clock = clockRef.current
    if (clock === null) {
      return
    }
    fn(clock)
    setClockState(clock.getState())
    const sim = simRef.current
    if (sim !== null) {
      setSnapshot(readSnapshot(sim.snapshot()))
    }
  }, [])

  return {
    ready,
    error,
    version,
    dt,
    snapshot,
    clockState,
    params,
    trajectory,
    start: useCallback(() => withClock((c) => c.start()), [withClock]),
    pause: useCallback(() => withClock((c) => c.pause()), [withClock]),
    toggleRunning: useCallback(
      () => withClock((c) => (c.getState().running ? c.pause() : c.start())),
      [withClock],
    ),
    reset: useCallback(() => withClock((c) => c.reset()), [withClock]),
    singleStep: useCallback(() => withClock((c) => c.singleStep()), [withClock]),
    setSpeed: useCallback(
      (s: SpeedMultiplier) => withClock((c) => c.setSpeed(s)),
      [withClock],
    ),
    withSim: useCallback(<T,>(fn: (sim: SimHandle) => T): T | null => {
      const sim = simRef.current
      return sim === null ? null : fn(sim)
    }, []),
  }
}
