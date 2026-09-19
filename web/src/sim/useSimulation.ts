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

import { readDiagnostics, type Diagnostics } from './diagnostics'
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
  sheet: {
    d_sheet: number
    block_pos_b: { x: number; y: number; z: number }
    l_sheet_min: number
    l_sheet_max: number
  }
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
  diagnostics: Diagnostics | null
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
   * The mainsheet rate command, `[−1, 1]`, `−1` = haul (F3). Held until it is
   * set again: the mouse reducer in `sheetInput.ts` produces it, and the next
   * `set_controls` — from the frame loop or from a key event — carries it.
   */
  setSheetRate(rate: number): void
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

/** The `?scenario=` fixtures, replaced by the scenario system in section 09. */
const FIXTURES = ['coast', 'free_sail', 'sheet', 'capsize', 'knockdown', 'fast'] as const

/** Minimal M3/M4/M5/M6 fixtures, replaced by the scenario system in section 09. */
function initialScenario(sim: SimHandle, name: string): string {
  if (!(FIXTURES as readonly string[]).includes(name)) return '{}'
  const values = sim.snapshot()
  const state: Record<string, number> = {}
  SNAPSHOT_FIELDS.forEach((field, i) => {
    state[field.replace(/[A-Z]/g, (ch) => `_${ch.toLowerCase()}`)] = values[i]
  })
  const wind = JSON.parse(sim.wind_json() as string) as Record<string, unknown>
  // The sheet limits come from the core, never from a literal here (F7, F8).
  const sheet = (JSON.parse(sim.parameters_json() as string) as RenderParams).sheet
  if (name === 'coast') {
    state.u = 4
    wind.speed = 0
  } else if (name === 'fast') {
    // Above `diagnostics::HULL_MODEL_VALID_TO`, so the debug panel's R6
    // warning is on at t = 0 and switches itself off as the hull drags the
    // boat back below the limit. An initial condition, not a coefficient.
    state.u = 6.5
    wind.speed = 0
  } else if (name === 'sheet') {
    // Part-eased, so a haul has room to shorten the sheet and a release has
    // room to pay it out. Light wind: there is no righting moment until
    // section 07, so a long run at full power would lie the boat down.
    wind.speed = 4
    wind.bearing_deg = 0
    state.l_sheet = (sheet.l_sheet_min + sheet.l_sheet_max) / 2
  } else if (name === 'capsize') {
    // brief §46's `beam_reach_capsize`: a northerly on the port beam with the
    // sheet hard in, which is where the boat starts. 7 m/s is just over the
    // measured threshold — 6.90 m/s recovers, 6.95 m/s goes over (see
    // `docs/progress/07-handoff.md`) — so this is a capsize the boat is
    // *driven* into, not one it is placed in.
    wind.speed = 7
    wind.bearing_deg = 0
  } else if (name === 'knockdown') {
    // Already on its ear and still rolling: the state a gust and a wave leave
    // the boat in. It carries `φ` past 100° in the first fifth of a second and
    // then falls back, which is what the heel indicator has to render. The
    // roll rate is an initial condition, not a force.
    wind.speed = 8
    wind.bearing_deg = 0
    state.phi = 1.5
    state.p = 8
  } else {
    wind.speed = 5
    wind.bearing_deg = 0
    // `free_sail` means a free boom. Since section 06 the sheet is a real
    // rope and the default state is hauled hard in, which would pin the boom
    // on the centreline; fully eased, the rope is slack over the whole boom
    // range and contributes nothing.
    state.l_sheet = sheet.l_sheet_max
  }
  wind.mode = 'uniform'
  return JSON.stringify({ state, wind })
}

/** Sim-seconds between recorded trajectory points, and how many to keep. */
const TRAJECTORY_INTERVAL_S = 0.2
const TRAJECTORY_MAX_POINTS = 3000

export function useSimulation(
  input: InputConfig = DEFAULT_INPUT,
  onFrame?: FrameHook,
  scenarioName = '',
): SimulationHandle {
  const [ready, setReady] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [version, setVersion] = useState('')
  const [dt, setDt] = useState(0)
  const [params, setParams] = useState<RenderParams | null>(null)
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null)
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
  const sheetRateRef = useRef(0)
  // Held in a ref so a new closure per frame does not restart the loop.
  const frameHookRef = useRef<FrameHook | undefined>(onFrame)
  frameHookRef.current = onFrame
  // Read once, when the module loads; the simulation is not rebuilt if it
  // changes afterwards.
  const scenarioNameRef = useRef(scenarioName)
  scenarioNameRef.current = scenarioName

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

        const scenario = initialScenario(sim, scenarioNameRef.current)
        if (scenario !== '{}') {
          sim.reset(scenario)
        }
        setSnapshot(readSnapshot(sim.snapshot()))
        setDiagnostics(readDiagnostics(sim))

        clockRef.current = createClock(sim.dt(), {
          advance: (n) => sim.advance(n),
          reset: () => {
            sim.reset(scenario)
            // `sim.dt` is live-editable (brief §31) and `set_parameter`
            // reports such an edit as reset-required (F8.2): this is the
            // point at which the browser clock has to adopt it, or it goes on
            // converting wall time with the timestep the page loaded with.
            const nextDt = sim.dt()
            clockRef.current?.setDt(nextDt)
            setDt(nextDt)
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
      const c = controlsFromInput(held, inputRef.current, sheetRateRef.current)
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
        setDiagnostics(readDiagnostics(sim))
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

      const c = controlsFromInput(heldRef.current, inputRef.current, sheetRateRef.current)
      sim.set_controls(c.rudderRateCmd, c.sheetRateCmd, c.sheetRelease)
      clock.tick(delta)

      const next = readSnapshot(sim.snapshot())
      frameHookRef.current?.(sim, next, delta)
      setSnapshot(next)
      setDiagnostics(readDiagnostics(sim))
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
      setDiagnostics(readDiagnostics(sim))
    }
  }, [])

  return {
    ready,
    error,
    version,
    dt,
    snapshot,
    diagnostics,
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
    setSheetRate: useCallback((rate: number) => {
      sheetRateRef.current = rate
      const sim = simRef.current
      if (sim !== null) {
        // Push it immediately rather than waiting for the next frame: a drag
        // followed at once by a single step would otherwise advance on the
        // previous command, exactly as the keyboard path found in section 02.
        const c = controlsFromInput(heldRef.current, inputRef.current, rate)
        sim.set_controls(c.rudderRateCmd, c.sheetRateCmd, c.sheetRelease)
      }
    }, []),
    withSim: useCallback(<T,>(fn: (sim: SimHandle) => T): T | null => {
      const sim = simRef.current
      return sim === null ? null : fn(sim)
    }, []),
  }
}
