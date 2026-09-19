/**
 * The React driver: loads the WASM module, owns the `Sim`, and runs the one
 * `requestAnimationFrame` loop in the application.
 *
 * Physics is decoupled from rAF (brief §21): the frame callback only hands the
 * elapsed wall time to the clock, which converts it into whole `advance(n)`
 * calls. Rendering reads the most recent snapshot.
 *
 * **This is the only file under `web/src/sim/` that may mention
 * `requestAnimationFrame`.** `clock.ts` contains none at all.
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
}

const ZERO_SNAPSHOT: Snapshot = readSnapshot(new Float64Array(SNAPSHOT_FIELDS.length))

/** Sim-seconds between recorded trajectory points, and how many to keep. */
const TRAJECTORY_INTERVAL_S = 0.2
const TRAJECTORY_MAX_POINTS = 3000

export function useSimulation(input: InputConfig = DEFAULT_INPUT): SimulationHandle {
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
        setSnapshot(readSnapshot(sim.snapshot()))

        clockRef.current = createClock(sim.dt(), {
          advance: (n) => sim.advance(n),
          reset: () => {
            sim.reset('{}')
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
    }

    const onKeyUp = (e: KeyboardEvent) => {
      held.delete(normaliseKey(e.key))
    }
    const onBlur = () => held.clear()

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
  }
}
