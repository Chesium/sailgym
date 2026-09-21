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
import {
  clearTransient,
  composeControls,
  DEFAULT_INPUT,
  IDLE_TOUCH,
  type InputConfig,
  type InputSources,
  type TouchCommand,
} from './controls'
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

/**
 * The parameters the renderer needs, as `parameters_json()` shapes them.
 *
 * v2 section 01 added `hull.lwl`, `sail.area`, `sail.z_ce`, `board.area`,
 * `rudder.area` and `sheet.z_boom` for the 3-D boat model. All six were
 * **already** emitted by `parameters_json()` — the foil sections are
 * `#[serde(flatten)]`ed, so `area` sits directly under `sail`, `board` and
 * `rudder` — so no Rust changed and none was permitted to (F8, and the
 * section's own acceptance criterion 3).
 *
 * v2 section 09 (task 9.1) added the six actuator limits and rates the touch
 * gauges need — `rudder.delta_r_max`, `rudder.delta_r_rate_max`,
 * `rudder.delta_r_return_rate`, `sheet.sheet_haul_rate`,
 * `sheet.sheet_ease_rate` and `sheet.sheet_release_rate`. The same thing is
 * true of them: `parameters_json()` is the whole F7 catalogue and already
 * carried every one, so **no Rust changed** and task 9.2 added no field (see
 * its entry in `docs/v2/progress/09-handoff.md`). The limits stay
 * authoritative in `BoatParameters`; this interface is a *view* of them, and
 * nothing in `web/` may restate one as a literal (F7, F8, RV56).
 */
export interface RenderParams {
  hull: { loa: number; beam: number; lwl: number }
  sail: {
    area: number
    boom_length: number
    z_ce: number
    mast_pos_b: { x: number; y: number; z: number }
  }
  rudder: {
    pos_b: { x: number; y: number; z: number }
    area: number
    /** rad — maximum rudder deflection. */
    delta_r_max: number
    /** rad/s — the rate a full-scale `rudderRateCmd` commands. */
    delta_r_rate_max: number
    /** rad/s — the rate the tiller returns to neutral at with no command. */
    delta_r_return_rate: number
  }
  board: { pos_b: { x: number; y: number; z: number }; area: number }
  sheet: {
    d_sheet: number
    z_boom: number
    block_pos_b: { x: number; y: number; z: number }
    l_sheet_min: number
    l_sheet_max: number
    /** m/s — the rate a full-scale haul (`−1`) commands. */
    sheet_haul_rate: number
    /** m/s — the rate a full-scale ease (`+1`) commands. */
    sheet_ease_rate: number
    /** m/s — the rate the release button and `Space` command. */
    sheet_release_rate: number
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
   * The **mouse** sheet channel: a rate command in `[−1, 1]`, `−1` = haul
   * (F3), or `null` when no drag owns the channel.
   *
   * Held until it is set again. `sheetInput.reduceSheetInput` produces the
   * number and `sheetInput.ownsSheet` the `null`; `composeControls` decides
   * whether it is the command in force (task 9.1).
   */
  setSheetRate(rate: number | null): void
  /**
   * The **touch** channels: rudder, sheet and the release latch, as
   * `touchInput.reduceTouchInput` produces them.
   *
   * `null` on a channel means no pad owns it, so it falls back to the mouse
   * and the keyboard. Lifting a finger is not a sheet dump (RV53).
   */
  setTouchCommand(cmd: TouchCommand): void
  /**
   * Drop every transient input and every release latch (RV53).
   *
   * Called from here on blur, on a hidden page, on pause, on reset and on a
   * scenario switch; exposed so `App.tsx` can call it on entering replay,
   * which is the one clear path this hook cannot see.
   */
  clearInput(): void
  /**
   * Suspend or resume **live advancement**, for replay (v2 section 10, F18.3).
   *
   * While inspecting:
   *
   * * the frame loop does not tick the clock, so no wall time can reach
   *   `advance` however the clock controls are used;
   * * {@link applyControls} pushes an idle `Controls` across the boundary, so
   *   a key or a finger that is still down steers nothing.
   *
   * Both transitions pause the clock and clear the input. **Leaving replay
   * leaves the simulation paused**, which is the PRD's rule: the run the
   * viewer left is still where they left it, and resuming is their decision.
   */
  setInspecting(on: boolean): void
  /** Whether live advancement is currently suspended. */
  inspecting: boolean
  /**
   * Bumped by every {@link SimulationHandle.clearInput}.
   *
   * A component that holds gesture state of its own — the touch pads — cannot
   * be cleared by this hook reaching into it, and clearing only the *composed*
   * command would leave the pad still showing a full-scale helm and re-sending
   * it on the next `pointermove`. So the clear is published as a value the pad
   * watches, and the two halves of "the gesture is over" happen together.
   */
  inputGeneration: number
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
 * The held-keys set used while inspecting a replay: empty, and shared.
 *
 * Deliberately **not** the hook's own `heldRef` set. Clearing that one would
 * throw away what the keyboard is holding; this composes an idle command
 * without touching it, so nothing has to be restored on the way back to live.
 * It is never written to.
 */
const EMPTY_HELD: ReadonlySet<string> = new Set<string>()

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
    // sheet hard in, which is where the boat starts. 9 m/s is inside the
    // window v2 section 08 measured — below 7.98 m/s the corrected boat never
    // goes over, above 9.65 m/s it cannot be saved by releasing at four
    // seconds (`docs/v2/physics-validation.md` §4.6) — so this is a capsize
    // the boat is *driven* into, not one it is placed in. It tracks
    // `scenarios/beam_reach_capsize.json`, which carries the same speed.
    wind.speed = 9
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

/**
 * Wall milliseconds between re-reads of `parameters_json()`.
 *
 * brief §31 makes the whole F7 catalogue live-editable, and section 09's
 * gauges quote it: the rudder's limit, the sheet's travel, and the full-travel
 * reference times derived from the two. Before this, `params` was read **once**
 * at load, so a live edit moved the boat and left every gauge quoting the
 * catalogue the page had started with.
 *
 * Polling rather than a callback because `Sim::set_parameter` is reached
 * through `withSim` from the parameter panel, the wind control and the E2E
 * probe, and a notification that any of them could forget to send is a
 * notification that goes stale. The poll compares the returned **text** and
 * re-parses only on a change, so the steady state costs one boundary call
 * every 500 ms and allocates no new `RenderParams` — which matters, because a
 * fresh object identity here invalidates the renderer's model memo (section 01
 * handoff, RV4).
 */
const PARAMS_POLL_MS = 500

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
  const [inputGeneration, setInputGeneration] = useState(0)
  const [inspecting, setInspectingState] = useState(false)
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
  const trackRef = useRef<WorldPoint[]>([])
  const lastTrackRef = useRef(-Infinity)
  const inputRef = useRef(input)
  inputRef.current = input
  /**
   * The held keys, as one `Set` for the lifetime of the hook.
   *
   * Its **identity never changes**, and that is load-bearing: the key handlers
   * capture it once when their effect runs, so replacing it — which spreading
   * a freshly built `InputSources` over the ref in the clear path quietly does
   * — leaves them adding keys to an orphaned set that nothing reads, and the
   * keyboard stops steering the moment anything pauses. Clearing is
   * `held.clear()`; the set is constructed exactly once, on this line.
   */
  const heldRef = useRef<Set<string>>(new Set<string>())
  /**
   * Every device, in one value (task 9.1, v2 F18.2).
   *
   * The mouse and touch channels are replaced wholesale; `held` is mutated in
   * place, for the reason above. {@link applyControls} is the only reader.
   */
  const sourcesRef = useRef<InputSources>({
    held: heldRef.current,
    mouseSheet: null,
    touch: IDLE_TOUCH,
  })
  /**
   * The rudder command last pushed across the boundary.
   *
   * Task 10.5's input-lag measurement starts on the transition from *no helm
   * command* to *some helm command*, whichever device produced it, so the
   * measurement is of the pipeline and not of one input path (brief §37).
   */
  const lastRudderCmdRef = useRef(0)
  /**
   * The last `parameters_json()` text, so the catalogue is re-parsed only when
   * it has actually changed. See {@link PARAMS_POLL_MS}.
   */
  const paramsTextRef = useRef<string>('')
  const lastParamsPollRef = useRef(-Infinity)
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
  /**
   * Live advancement is suspended (replay).
   *
   * A ref as well as state because the frame loop and {@link applyControls}
   * both read it, and neither may be rebuilt when it changes: restarting the
   * frame loop on entering replay would drop the `previous` timestamp and give
   * the next live frame a delta of zero.
   */
  const inspectingRef = useRef(false)

  // --- the one control-application boundary (task 9.1, v2 F18.2) -----------

  /**
   * Compose every device into one `Controls` and push it across the F8
   * boundary. **The only `set_controls` call site in the application.**
   *
   * Called from the key handlers, from the pointer paths and from the frame
   * loop. Doing it only in the frame loop leaves a race: an input followed
   * immediately by a single step advances the physics with the *previous*
   * frame's controls, and how often that happens depends on the frame rate. It
   * showed up as `determinism.spec.ts` sampling a rudder that had not moved
   * yet, on a machine whose frames had become slow.
   *
   * Nothing here advances the simulation. Input composition and the clock are
   * separate concerns: the clock's `advance` sink is the only caller of
   * `sim.advance`, and no key or pointer handler reaches it except through the
   * clock's own `singleStep`/`reset` API.
   */
  const applyControls = useCallback(() => {
    const sim = simRef.current
    if (sim === null) {
      return
    }
    // In replay the controls belong to the episode, not to the keyboard: push
    // an idle command so a key or a finger that is still down cannot steer a
    // simulation the viewer is not looking at (task 10.3).
    const c = inspectingRef.current
      ? composeControls({ ...clearTransient(), held: EMPTY_HELD }, inputRef.current)
      : composeControls(sourcesRef.current, inputRef.current)
    // Task 10.5's input-lag measurement starts on the frame the helm first
    // asks for something, against the rudder angle currently drawn. The far
    // end is in `render/BoatSvg.tsx`, after the commit. Keyed on the composed
    // command rather than on a key-down, so a touch pad is measured the same
    // way a keyboard is.
    if (lastRudderCmdRef.current === 0 && c.rudderRateCmd !== 0) {
      noteRudderCommand(readSnapshot(sim.snapshot()).deltaR)
    }
    lastRudderCmdRef.current = c.rudderRateCmd
    sim.set_controls(c.rudderRateCmd, c.sheetRateCmd, c.sheetRelease)
  }, [])

  /** The one clear path (RV53). See {@link SimulationHandle.clearInput}. */
  const clearInput = useCallback(() => {
    // `heldRef.current` is emptied rather than replaced; see its declaration.
    heldRef.current.clear()
    sourcesRef.current = { ...clearTransient(), held: heldRef.current }
    applyControls()
    // …and tell the pads, which hold gesture state this hook cannot reach.
    setInputGeneration((g) => g + 1)
  }, [applyControls])

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
        const paramsText = sim.parameters_json() as string
        paramsTextRef.current = paramsText
        setParams(JSON.parse(paramsText) as RenderParams)

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
          // A reset is one of the clear paths (RV53): the run it was steering
          // no longer exists, and a key or a finger that was down across it
          // must not carry a command into the new one.
          clearInput()
        } else if (action === 'pause') {
          if (clock.getState().running) {
            clock.pause()
            clearInput()
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
    /**
     * Focus left, or the page went to the background.
     *
     * Both are clear paths, and for the same reason: the `keyup` for a key
     * that is down when focus leaves is delivered to whoever has focus next,
     * and the `pointerup` for a finger that is down when a tab is hidden may
     * never be delivered at all. Without this the boat sails away with the
     * helm hard over while nobody is looking (RV53).
     */
    const onClear = () => clearInput()
    const onVisibility = () => {
      if (document.visibilityState === 'hidden') {
        clearInput()
      }
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
    window.addEventListener('blur', onClear)
    window.addEventListener('pagehide', onClear)
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
      window.removeEventListener('blur', onClear)
      window.removeEventListener('pagehide', onClear)
      document.removeEventListener('visibilitychange', onVisibility)
      held.clear()
    }
  }, [ready, applyControls, clearInput])

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
      applyControls()
      // While a recorded episode is being inspected the live run does not
      // advance — not by a tick, not by a rounding of one. This is the one
      // `advance` path in the application, so gating it here is what makes
      // "the paused live simulation cannot change under a replay" a property
      // of the code rather than of the clock's current state (RV57).
      if (!inspectingRef.current) {
        clock.tick(delta)
      }
      const next = readSnapshot(sim.snapshot())
      const diag = readDiagnostics(sim)
      // A live brief §31 edit has to reach the gauges that quote it
      // (task 9.2). Twice a second, and only re-parsed when the text moves.
      let nextParams: RenderParams | null = null
      if (now - lastParamsPollRef.current >= PARAMS_POLL_MS) {
        lastParamsPollRef.current = now
        const text = sim.parameters_json() as string
        if (text !== paramsTextRef.current) {
          paramsTextRef.current = text
          nextParams = JSON.parse(text) as RenderParams
        }
      }
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
      if (nextParams !== null) {
        setParams(nextParams)
      }
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
  }, [ready, applyControls])

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
      // A scenario switch is a clear path (RV53): whatever was held belonged
      // to the run that has just been replaced.
      clearInput()
      setScenario(readCurrentScenario(sim))
    },
    [withClock, clearInput],
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
    pause: useCallback(() => {
      withClock((c) => c.pause())
      clearInput()
    }, [withClock, clearInput]),
    toggleRunning: useCallback(() => {
      let paused = false
      withClock((c) => {
        paused = c.getState().running
        if (paused) {
          c.pause()
        } else {
          c.start()
        }
      })
      if (paused) {
        clearInput()
      }
    }, [withClock, clearInput]),
    reset: useCallback(() => {
      withClock((c) => c.reset())
      clearInput()
    }, [withClock, clearInput]),
    singleStep: useCallback(() => withClock((c) => c.singleStep()), [withClock]),
    setSpeed: useCallback(
      (s: SpeedMultiplier) => withClock((c) => c.setSpeed(s)),
      [withClock],
    ),
    setSheetRate: useCallback(
      (rate: number | null) => {
        sourcesRef.current = { ...sourcesRef.current, mouseSheet: rate }
        // Push it immediately rather than waiting for the next frame: a drag
        // followed at once by a single step would otherwise advance on the
        // previous command, exactly as the keyboard path found in section 02.
        applyControls()
      },
      [applyControls],
    ),
    inputGeneration,
    inspecting,
    setInspecting: useCallback(
      (on: boolean) => {
        inspectingRef.current = on
        setInspectingState(on)
        // Both directions pause and clear. Entering: the live boat must not
        // animate past the replayed one. Leaving: the run stays exactly where
        // it was until the viewer resumes it themselves.
        withClock((c) => c.pause())
        clearInput()
      },
      [withClock, clearInput],
    ),
    setTouchCommand: useCallback(
      (touch: TouchCommand) => {
        sourcesRef.current = { ...sourcesRef.current, touch }
        applyControls()
      },
      [applyControls],
    ),
    clearInput,
    withSim: useCallback(<T,>(fn: (sim: SimHandle) => T): T | null => {
      const sim = simRef.current
      return sim === null ? null : fn(sim)
    }, []),
  }
}
