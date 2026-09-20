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
import {
  DEFAULT_SCENARIO,
  readCurrentScenario,
  readScenarios,
  summarise,
  type Scenario,
  type ScenarioSummary,
} from './scenarioTypes'
import { readSnapshot, SNAPSHOT_FIELDS, type Snapshot } from './snapshot'
import { beginSpan, endSpan, noteRudderCommand } from '../render/perfMarks'

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
  /** The six shipped scenarios (brief §32), as the core lists them. */
  scenarios: readonly ScenarioSummary[]
  /** The scenario the current run started from. */
  scenario: Scenario | null
  /**
   * Load a scenario by id and restart from it.
   *
   * Parameters, initial state, wind and seed move together — a scenario is
   * only meaningful whole. The clock is reset, so the new run starts at
   * `t = 0`, and every later reset replays *this* scenario.
   */
  loadScenario(id: string): void
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

/**
 * Browser test fixtures from sections 02–08: ad-hoc initial conditions that
 * are **not** scenarios and are deliberately not shipped as documents.
 *
 * `free_sail` used to be one of these and is now the shipped scenario of
 * brief §32, authored to reproduce the fixture exactly so the section 05–08
 * specs that drive it are testing the same boat.
 *
 * They stay because each one exists for a named test — `coast` for the
 * rudder specs, `capsize` and `knockdown` for the heel indicator, `fast` for
 * the R6 warning (section 08 handoff §10.4), `sheet` for the mainsheet drag
 * — and none of them is a configuration brief §32 asks the product to ship.
 */
const LEGACY_FIXTURES = ['coast', 'sheet', 'capsize', 'knockdown', 'fast'] as const

/**
 * The document `Sim.reset` is given at load and at every reset.
 *
 * A shipped scenario id wins; then a legacy fixture; then, for anything else
 * including the empty string, the default scenario (brief §32: `free_sail`
 * is "the default on load").
 */
function scenarioDocument(sim: SimHandle, name: string): string {
  const shipped = readScenarios(sim)
  const wanted = name === '' ? DEFAULT_SCENARIO : name
  const found = shipped.find((s) => s.name === wanted)
  if (found !== undefined) {
    return JSON.stringify(found)
  }
  if ((LEGACY_FIXTURES as readonly string[]).includes(wanted)) {
    return legacyFixture(sim, wanted)
  }
  const fallback = shipped.find((s) => s.name === DEFAULT_SCENARIO) ?? shipped[0]
  return JSON.stringify(fallback)
}

/** Minimal M3–M7 fixtures; see {@link LEGACY_FIXTURES}. */
function legacyFixture(sim: SimHandle, name: string): string {
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
    // `docs/v1/progress/07-handoff.md`) — so this is a capsize the boat is
    // *driven* into, not one it is placed in.
    wind.speed = 7
    wind.bearing_deg = 0
  } else {
    // `knockdown`: already on its ear and still rolling — the state a gust
    // and a wave leave the boat in. It carries `φ` past 100° in the first
    // fifth of a second and then falls back, which is what the heel
    // indicator has to render. The roll rate is an initial condition, not a
    // force.
    wind.speed = 8
    wind.bearing_deg = 0
    state.phi = 1.5
    state.p = 8
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
  renderHz = 0,
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
  const [scenarios, setScenarios] = useState<readonly ScenarioSummary[]>([])
  const [scenario, setScenario] = useState<Scenario | null>(null)

  const simRef = useRef<SimHandle | null>(null)
  /**
   * The scenario document every reset replays.
   *
   * Held in a ref rather than captured by the clock's sink: the picker can
   * change it at runtime, and a captured constant would quietly send `R` back
   * to whichever scenario the page happened to load with.
   */
  const scenarioDocRef = useRef<string>('{}')
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
  // brief §37: physics must stay independent of the render rate. `renderHz`
  // caps how often the frame loop publishes to React — the clock still ticks
  // on every animation frame, so the simulation advances at wall-clock rate
  // whatever the page is drawing. `0` means "publish every frame", which is
  // the shipped behaviour; `tests/e2e/perf.spec.ts` sets 20 through
  // `?renderHz=` to measure the two against each other (task 10.5).
  const renderHzRef = useRef(renderHz)
  renderHzRef.current = renderHz
  const lastPublishRef = useRef(-Infinity)

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

        setScenarios(summarise(readScenarios(sim)))
        scenarioDocRef.current = scenarioDocument(sim, scenarioNameRef.current)
        sim.reset(scenarioDocRef.current)
        setScenario(readCurrentScenario(sim))
        setSnapshot(readSnapshot(sim.snapshot()))
        setDiagnostics(readDiagnostics(sim))

        clockRef.current = createClock(sim.dt(), {
          advance: (n) => sim.advance(n),
          reset: () => {
            // `restart`, not `reset`: a reset replays the scenario's initial
            // condition but keeps the parameter catalogue in force, so a
            // live brief §31 edit — including the reset-required `sim.dt` —
            // survives the reset that is supposed to make it good.
            sim.restart()
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
      // Task 10.5's input-lag measurement starts here, on the key-down that
      // actually changes the helm, against the rudder angle currently drawn.
      // The far end is in `render/BoatSvg.tsx`, after the commit.
      if (!e.repeat && (action === 'steerPort' || action === 'steerStarboard')) {
        const sim = simRef.current
        if (sim !== null) {
          noteRudderCommand(readSnapshot(sim.snapshot()).deltaR)
        }
      }
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
      beginSpan('frame')
      const delta = previous === null ? 0 : now - previous
      previous = now

      // Every call across the F8 boundary in this frame, and nothing else.
      beginSpan('wasm')
      const c = controlsFromInput(heldRef.current, inputRef.current, sheetRateRef.current)
      sim.set_controls(c.rudderRateCmd, c.sheetRateCmd, c.sheetRelease)
      clock.tick(delta)
      const next = readSnapshot(sim.snapshot())
      const diag = readDiagnostics(sim)
      endSpan('wasm')

      // The physics above has already advanced. Everything below is
      // presentation, and this is the only thing `renderHz` skips.
      const hz = renderHzRef.current
      if (hz > 0 && now - lastPublishRef.current < 1000 / hz) {
        endSpan('frame')
        return
      }
      lastPublishRef.current = now

      frameHookRef.current?.(sim, next, delta)
      setSnapshot(next)
      setDiagnostics(diag)
      setClockState(clock.getState())
      endSpan('frame')

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

  const loadScenario = useCallback(
    (id: string) => {
      const sim = simRef.current
      if (sim === null) {
        return
      }
      // `reset`, not `restart`: choosing a scenario is the case where its
      // own `parameter_overrides` are the point (see `Sim::restart`).
      scenarioDocRef.current = scenarioDocument(sim, id)
      sim.reset(scenarioDocRef.current)
      // Then through the clock, so `simTime` and `stepsTaken` are zeroed and
      // the trajectory is cleared by the one code path that already does
      // both. `restart` replays the document just installed, so this is
      // idempotent.
      withClock((c) => c.reset())
      setScenario(readCurrentScenario(sim))
    },
    [withClock],
  )

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
    scenarios,
    scenario,
    loadScenario,
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
