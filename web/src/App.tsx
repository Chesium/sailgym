import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import {
  createCamera,
  trackedCentre,
  PIXELS_PER_METRE,
  MIN_ZOOM,
  MAX_ZOOM,
  type Camera,
  type CameraMode,
  type Vec2,
} from './render/Camera'
import { BoatProbe } from './render/BoatProbe'
import { BoatSvg } from './render/BoatSvg'
import { ForceOverlay, OverlayControls, OverlayLegend } from './render/ForceOverlay'
import { HeelIndicator, HeelProbe } from './render/HeelIndicator'
import { DEFAULT_INPUT } from './sim/controls'
import {
  compareEpisodes,
  describeIdentity,
  importEpisode,
  megabytes,
  readEpisodeIdentity,
  readRecordingLimit,
  recordingSeconds,
  type RecordingLimit,
} from './sim/episodeIo'
import { NOT_RECORDED } from './sim/diagnostics'
import {
  createReplaySource,
  selectInspection,
  LIVE,
  type InspectionView,
  type PlaybackMode,
  type ReplayProbe,
} from './sim/replay'
import type { Episode } from './sim/scenarioTypes'
import {
  IDLE_SHEET_INPUT,
  ownsSheet,
  reduceSheetInput,
  type SheetInputState,
} from './sim/sheetInput'
import { useSimulation, type RenderParams } from './sim/useSimulation'
import {
  Charts,
  practiceCompareData,
  replayChartData,
  useChartSampler,
  type CompareChartData,
} from './ui/Charts'
import { ClockControls } from './ui/ClockControls'
import { DebugPanel } from './ui/DebugPanel'
import { Hud } from './ui/Hud'
import { Layout } from './ui/Layout'
import { ModeSwitch } from './ui/ModeSwitch'
import { ParameterPanel } from './ui/ParameterPanel'
import { PracticePanel } from './ui/PracticePanel'
import { RecordControls } from './ui/RecordControls'
import { ScenarioPicker } from './ui/ScenarioPicker'
import { Timeline, type PlaybackSpeed } from './ui/Timeline'
import { TouchControls } from './ui/TouchControls'
import { useUiStore, type PracticeAttempt } from './ui/store'
import { WindReadout } from './ui/WindReadout'
import { ArrowProbe, buildArrows, type ArrowField } from './wind/ArrowOverlay'
import { DeckOverlay } from './wind/DeckOverlay'
import { DEBUG_MODE_WIND, SAIL_MODE_WIND, visibleCount } from './wind/particles'
import { particleLayers } from './wind/WindLayer'
import { useWindField } from './wind/useWindField'

/**
 * The world view's size, in CSS pixels, before anything has been measured and
 * as the floor and ceiling of what a measurement may produce (v2 section 09,
 * task 9.4).
 *
 * **Provenance.** Presentation only — every one of these is a count of screen
 * pixels and none of them reaches the camera's metres-per-pixel, the wind grid
 * or anything in Rust.
 *
 * - `fallback` is v1's fixed 780 × 520, kept as the pre-measurement size so a
 *   browser without `ResizeObserver`, and the first frame of every load, draw
 *   exactly what v1 drew.
 * - `min` is the smallest view the boat still reads in: at the default zoom of
 *   20 px/m a 4.23 m hull is 85 px long, so 260 × 190 holds the boat with
 *   about a hull-length of water around it. Below that the view stops being a
 *   view; the main row scrolls instead of shrinking it further.
 * - `maxAspect` keeps a tall narrow phone from turning the world into a
 *   letterbox slot on its side: the height never exceeds 1.15 × the width.
 */
const WORLD_PX = {
  fallback: { width: 780, height: 520 },
  min: { width: 260, height: 190 },
  maxAspect: 1.15,
} as const

/**
 * Viewport width, in CSS pixels, at or below which the layout goes compact.
 *
 * **Provenance.** Presentation only. 760 px is just above the widest phone in
 * landscape the acceptance criteria name (844 × 390 is 844 *wide*, so it is
 * not compact) and below the narrowest tablet-ish width where the header's
 * controls fit on two rows. It is a layout threshold and nothing else.
 */
const COMPACT_MAX_WIDTH_PX = 760

/**
 * Viewport height, in CSS pixels, at or below which the layout also goes
 * compact.
 *
 * **Provenance.** Presentation only, and the same threshold the landscape
 * arrangement in `ui/Layout.tsx` uses. A phone in landscape is wide but short:
 * 844 × 390 is not narrow, yet an uncapped header and readouts take 150 px of
 * its 390, which is most of what the boat needed. Capping them is the same
 * remedy for the same problem, so it is the same flag.
 */
const COMPACT_MAX_HEIGHT_PX = 560

/**
 * What the renderer draws before `parameters_json()` has been read.
 *
 * Unit-ish placeholders, not dimensions: the real catalogue arrives a frame or
 * two later and replaces every one of them. No metre value from F7 appears
 * here — that is the whole point of reading them from the core (F7, F8).
 */
const PENDING_PARAMS: RenderParams = {
  hull: { loa: 1, beam: 1, lwl: 1 },
  sail: { area: 1, boom_length: 1, z_ce: 1, mast_pos_b: { x: 0, y: 0, z: 0 } },
  rudder: {
    pos_b: { x: 0, y: 0, z: 0 },
    area: 1,
    delta_r_max: 1,
    delta_r_rate_max: 1,
    delta_r_return_rate: 1,
  },
  board: { pos_b: { x: 0, y: 0, z: 0 }, area: 1 },
  sheet: {
    d_sheet: 0,
    z_boom: 1,
    block_pos_b: { x: 0, y: 0, z: 0 },
    l_sheet_min: 0,
    l_sheet_max: 1,
    sheet_haul_rate: 1,
    sheet_ease_rate: 1,
    sheet_release_rate: 1,
  },
}

/**
 * Samples of **simulated** time per second a practice attempt is recorded at.
 *
 * **Provenance.** A recording rate, not a physical quantity and not a
 * scoring rate: the evaluator runs on every physics step whatever this is
 * (v2 F18.4), and changing it changes only how finely the result can be
 * inspected afterwards. 20 Hz is `ui/RecordControls.tsx`'s own default and
 * the rate `docs/v2/recording-format.md` §8 quotes its cap in; the longest
 * attempt any shipped challenge allows is 45 s, which is 900 samples against
 * a cap of 13 443.
 */
const PRACTICE_LOG_HZ = 20

/**
 * `?scenario=` — one of the six shipped ids (brief §32), one of the legacy
 * browser fixtures, or absent for the default. Resolved in
 * `sim/useSimulation.ts`; nothing here decides what a name means.
 */
function scenarioFromUrl(): string {
  return new URLSearchParams(window.location.search).get('scenario') ?? ''
}

/**
 * `?renderHz=` — cap how often the frame loop publishes to React.
 *
 * brief §37 requires the physics to be independent of the render rate, and
 * task 10.5 requires that to be *measured* rather than asserted by reading the
 * code. This is the knob the measurement turns: the clock still ticks on every
 * animation frame, so `t` advances at the same wall-clock rate; only the
 * drawing is throttled. Absent or unparseable means "every frame", which is
 * the shipped behaviour.
 */
function renderHzFromUrl(): number {
  const raw = new URLSearchParams(window.location.search).get('renderHz')
  const hz = raw === null ? 0 : Number(raw)
  return Number.isFinite(hz) && hz > 0 ? hz : 0
}

/**
 * `?probes=boat`, or `window.__sailgymProbes` — mount the fixed-angle boat
 * probes of `render/BoatProbe.tsx`.
 *
 * Opt-in, unlike `HeelProbe`, because each probe is a whole second boat: nine
 * of them add some four hundred SVG nodes to every page, which is a real cost
 * to the shipped application and no benefit to anyone using it. (It is also
 * what `tests/e2e/debug.spec.ts` counts when it holds the page to 300 SVG
 * elements — a budget about the world view and the force overlay, not about
 * test scaffolding.) The section's own browser checks ask for them explicitly.
 */
function boatProbesRequested(): boolean {
  return (
    new URLSearchParams(window.location.search).get('probes') === 'boat' ||
    window.__sailgymProbes === true
  )
}

declare global {
  interface Window {
    /** Set by `tests/e2e/boat3d.spec.ts` before load; see above. */
    __sailgymProbes?: boolean
  }
}

/**
 * Measure the space the world view may have, and keep measuring it.
 *
 * `main` is the layout's `1fr` row: its height comes from the grid, so it does
 * not depend on what is drawn inside it. `world` is a flex cell of width
 * `100%`, so its width does not depend on its children either. Taking the
 * height from one and the width from the other is what makes this loop-free —
 * an observer on a box that its own content sizes would resize forever.
 *
 * `ResizeObserver` covers everything the acceptance criteria name at once: a
 * window resize, a device rotation, a mobile browser's chrome sliding in and
 * out, and the debug column mounting or unmounting beside the world. There is
 * no `resize` listener and no orientation listener, because those report the
 * *window* and what matters is the box.
 */
function useWorldViewport(): {
  viewport: { width: number; height: number }
  mainRef: React.RefObject<HTMLDivElement | null>
  worldRef: React.RefObject<HTMLDivElement | null>
} {
  const mainRef = useRef<HTMLDivElement | null>(null)
  const worldRef = useRef<HTMLDivElement | null>(null)
  const [box, setBox] = useState<{ width: number; height: number }>(WORLD_PX.fallback)

  useEffect(() => {
    const main = mainRef.current
    const world = worldRef.current
    if (main === null || world === null || typeof ResizeObserver === 'undefined') {
      return
    }
    const measure = () => {
      const width = world.clientWidth
      const height = main.clientHeight
      if (width <= 0 || height <= 0) {
        return
      }
      setBox((previous) =>
        previous.width === width && previous.height === height ? previous : { width, height },
      )
    }
    const observer = new ResizeObserver(measure)
    observer.observe(main)
    observer.observe(world)
    measure()
    return () => observer.disconnect()
  }, [])

  // The SVG is drawn at whole pixels: a fractional viewport makes every grid
  // line land on a half-pixel and the whole view goes soft.
  const width = Math.max(WORLD_PX.min.width, Math.floor(box.width))
  const height = Math.max(
    WORLD_PX.min.height,
    Math.floor(Math.min(box.height, width * WORLD_PX.maxAspect)),
  )
  return { viewport: { width, height }, mainRef, worldRef }
}

/** Whether the layout should go compact, from the measured window width. */
const COMPACT_QUERY =
  `(max-width: ${COMPACT_MAX_WIDTH_PX}px), (max-height: ${COMPACT_MAX_HEIGHT_PX}px)`

function useCompactLayout(): boolean {
  const [compact, setCompact] = useState(
    () =>
      typeof window !== 'undefined' &&
      (window.innerWidth <= COMPACT_MAX_WIDTH_PX ||
        window.innerHeight <= COMPACT_MAX_HEIGHT_PX),
  )
  useEffect(() => {
    const query = window.matchMedia(COMPACT_QUERY)
    const update = () => setCompact(query.matches)
    update()
    query.addEventListener('change', update)
    return () => query.removeEventListener('change', update)
  }, [])
  return compact
}

/** The three F6.1 wind modes, as the scenario JSON spells them. */
const WIND_MODES = ['uniform', 'spatial', 'gust'] as const
type WindModeName = (typeof WIND_MODES)[number]

/**
 * The application: the boat, the deck.gl wind field, and — in Debug Mode —
 * brief §30's instrumentation and brief §31's parameter panel.
 *
 * Composition only. Every number displayed comes from the core through
 * `useSimulation`; nothing here computes a physical quantity (F8). The two
 * arrangements live in `ui/Layout.tsx` and the mode in the `zustand` UI store,
 * so switching modes mounts and unmounts components and touches nothing else —
 * in particular it never goes near the clock or the one animation frame loop.
 *
 * `data-testid="wasm-status"` and `data-ready` are the contract frozen by
 * task 1.3 — do not rename them. `data-testid="snapshot"` carries the raw F8.3
 * values as attributes so the E2E suite can assert on numbers rather than
 * pixels (brief §42); `data-testid="wind-stats"` does the same for the
 * once-per-frame guarantee of brief §19 and for the frame timings section 03
 * asks to be recorded.
 */
export default function App() {
  const wind = useWindField()
  const cameraRef = useRef<Camera | null>(null)

  /**
   * The world view is showing a recorded episode.
   *
   * A ref as well as state because the frame hook below is handed to
   * `useSimulation` once and must not be rebuilt when playback changes.
   */
  const inspectingRef = useRef(false)

  const onFrame = useCallback(
    (sim: Parameters<typeof wind.onFrame>[0], snapshot: { t: number }) => {
      // In replay the wind layer is not drawn at all (see `windField` below),
      // and sampling the **live** field behind a recorded boat is precisely
      // the mixed timeline RV57 names. So the field is not sampled, the
      // particles are not advected, and `wind_at_boat()` is not read: the
      // replay's wind comes from the episode.
      if (inspectingRef.current) {
        return
      }
      const camera = cameraRef.current
      if (camera !== null) {
        wind.onFrame(sim, camera, snapshot.t)
      }
    },
    [wind],
  )

  const sim = useSimulation(undefined, onFrame, scenarioFromUrl(), renderHzFromUrl())
  const ui = useUiStore()
  const { viewport, mainRef, worldRef } = useWorldViewport()
  const compact = useCompactLayout()
  const [mode, setMode] = useState<CameraMode>('northUp')
  const [zoom, setZoom] = useState(1)
  const [pan, setPan] = useState<Vec2>({ x: 0, y: 0 })
  const [windMode, setWindMode] = useState<WindModeName>('gust')
  const sheetInput = useRef<SheetInputState>(IDLE_SHEET_INPUT)
  /**
   * The world view owns a pointer drag — a mainsheet trim or a camera pan.
   *
   * Handed to the layout, which stops the main row scrolling for the duration.
   * See `LayoutProps.dragging` for the measurement that made it necessary.
   */
  const [worldDrag, setWorldDrag] = useState(false)
  const [showArrows, setShowArrows] = useState(false)
  const [showBoatProbes] = useState(boatProbesRequested)
  const baseCentre = useRef<Vec2>({ x: 0, y: 0 })
  /** The current inspection view, for the E2E probe. Written every render. */
  const viewRef = useRef<InspectionView | null>(null)

  // --- recording and replay (brief §33) ----------------------------------
  const [episode, setEpisode] = useState<Episode | null>(null)
  const [playback, setPlayback] = useState<PlaybackMode>(LIVE)
  const [replayTime, setReplayTime] = useState(0)
  const [replayPlaying, setReplayPlaying] = useState(false)
  const [replaySpeed, setReplaySpeed] = useState<PlaybackSpeed>(1)

  /**
   * The recording bound, read once from the core when it is ready.
   *
   * The dependency list is `ready` and the **stable** `withSim` callback, not
   * the handle. `useSimulation` returns a fresh object every render, so
   * depending on it would re-run this effect on every frame and set a fresh
   * `RecordingLimit` object each time — which React reports as
   * "Maximum update depth exceeded" and which is the same identity trap
   * section 01 recorded as RV4.
   */
  const { withSim } = sim
  const [limit, setLimit] = useState<RecordingLimit | null>(null)
  useEffect(() => {
    if (!sim.ready) {
      return
    }
    setLimit(withSim((core) => readRecordingLimit(core)))
  }, [sim.ready, withSim])

  /**
   * The held episode's canonical identity, and the core's verdict on comparing
   * it with itself.
   *
   * Both are the **core's** answers (`Sim.episode_identity_json`,
   * `Sim.episode_comparability_json`). Comparing an episode with itself is the
   * honest test of whether it names a baseline at all: a legacy or
   * dirty-source episode is `indeterminate` even against itself, which is
   * F18.1d's rule and is what stops the page calling it a same-conditions
   * experiment (RV58).
   */
  // Memoised on the **episode**, not on the handle: both calls serialise the
  // whole episode across the boundary, which is far too expensive to repeat
  // sixty times a second, and neither answer changes until the episode does.
  const identity = useMemo(
    () =>
      episode === null || !sim.ready
        ? null
        : withSim((core) => readEpisodeIdentity(core, episode)),
    [episode, sim.ready, withSim],
  )
  const comparability = useMemo(
    () =>
      episode === null || !sim.ready
        ? null
        : withSim((core) => compareEpisodes(core, episode, episode)),
    [episode, sim.ready, withSim],
  )
  const identityIsBaseline = comparability?.verdict === 'same_conditions'

  // ---------------------------------------------------------------------
  // Guided practice (v2 section 11, task 11.3)
  // ---------------------------------------------------------------------
  //
  // The attempt itself lives in Rust and arrives through `sim.practice`;
  // everything here is capture, memory and wiring. Nothing below decides an
  // outcome, and the only arithmetic is the two subtractions the comparison
  // shows as deltas.
  const { practice, stopRecording, startPractice, retryPractice, cancelPractice } = sim
  const { attempts, rememberAttempt, forgetAttempts } = ui
  const practiceActive = practice.active && practice.status === 'active'
  /** The status the previous render saw, so a capture happens exactly once. */
  const lastAttemptStatus = useRef<string>('none')
  const attemptCount = useRef(0)

  /**
   * When an attempt stops being active, take its episode.
   *
   * Three cases, and they are not the same thing:
   *
   * * **finished** — the evaluator reached a terminal outcome. The episode is
   *   the attempt's record: it is held for export and for **Inspect**, and it
   *   is remembered so the next attempt can be compared with it.
   * * **conditions_changed** — a parameter or the wind moved underneath it.
   *   The episode is still worth looking at, so it is held; it is **not**
   *   remembered, because no comparison it took part in could mean anything
   *   (RV63).
   * * **cancelled** — a reset or a scenario change. The core has already
   *   discarded the recorder, so `stopRecording` returns `null` and there is
   *   nothing to hold.
   */
  useEffect(() => {
    const status = practice.active ? practice.status : 'none'
    const was = lastAttemptStatus.current
    lastAttemptStatus.current = status
    if (was !== 'active' || status === 'active' || !practice.active) {
      return
    }
    const recorded = stopRecording()
    if (recorded === null) {
      return
    }
    setEpisode(recorded)
    if (status !== 'finished') {
      return
    }
    attemptCount.current += 1
    rememberAttempt({
      id: `${practice.report.task.id}-${attemptCount.current}`,
      taskId: practice.report.task.id,
      taskVersion: practice.report.task.version,
      report: practice.report,
      episode: recorded,
    })
  }, [practice, stopRecording, rememberAttempt])

  /**
   * The core's verdict on comparing the two remembered attempts.
   *
   * `ExperimentIdentity::compare`, in Rust (F18.3): a different seed, `dt`,
   * parameter, physics source, initial condition **or task version** each
   * makes them incomparable on its own, and only `same_conditions` licenses
   * the deltas the panel shows (RV58, RV61). Memoised on the attempts, since
   * both episodes cross the boundary as JSON.
   */
  const attemptComparison = useMemo(
    () =>
      attempts.length === 2 && sim.ready
        ? withSim((core) => compareEpisodes(core, attempts[0].episode, attempts[1].episode))
        : null,
    [attempts, sim.ready, withSim],
  )
  /**
   * Both attempts' recorded traces on one elapsed-task-time axis.
   *
   * Only when they are the same challenge: two different challenges plot two
   * different quantities, and drawing them on one axis would be the mixed
   * picture section 10 spent itself removing.
   */
  const attemptCompare: CompareChartData | null = useMemo(() => {
    if (attempts.length !== 2 || attempts[0].taskId !== attempts[1].taskId) {
      return null
    }
    return practiceCompareData(
      attempts[0].taskId,
      [attempts[0].episode, attempts[1].episode],
      ['earlier attempt', 'latest attempt'],
    )
  }, [attempts])

  /**
   * Open a replay.
   *
   * `from` and `at` are section 11's **Inspect**: it hands in the attempt's
   * own episode and the recorded time of the event to jump to, because the
   * `episode` state it has just set is not visible until the next render.
   * `at` is clamped into the episode — a playhead outside it would be a claim
   * about data the recording does not have.
   */
  const enterReplay = useCallback(
    (from?: Episode, at?: number) => {
      const target = from ?? episode
      if (target === null) {
        return
      }
      const source = createReplaySource(target)
      // The live boat and the replayed one would otherwise animate past each
      // other in the same view. `setInspecting(true)` does three things at once:
      // it pauses the clock, it clears every held input (RV53 — a finger or a
      // key that was down belongs to the live run), and it stops the frame loop
      // ticking the clock at all, so no route back to `advance` remains however
      // the clock controls are used afterwards (task 10.3, RV57).
      inspectingRef.current = true
      sim.setInspecting(true)
      setPlayback({ kind: 'replay', source })
      setReplayTime(
        at === undefined ? source.startTime : Math.min(source.endTime, Math.max(source.startTime, at)),
      )
      setReplayPlaying(false)
    },
    [episode, sim],
  )

  const exitReplay = useCallback(() => {
    setReplayPlaying(false)
    setPlayback(LIVE)
    // **Only when actually leaving a replay.** `selectScenario` calls this
    // unconditionally to make sure a scenario switch never happens underneath
    // a replay, and `setInspecting(false)` pauses the clock — so doing it when
    // no replay was open would pause the live run every time the picker
    // changed. Caught by `scenarios.spec.ts`, in all three browsers.
    if (!inspectingRef.current) {
      return
    }
    // The PRD's rule: leaving replay leaves the simulation **paused**. The run
    // is where the viewer left it and resuming is their decision, not a side
    // effect of closing the timeline.
    inspectingRef.current = false
    sim.setInspecting(false)
  }, [sim])

  // Read the wind mode the core actually started with, and take the 128×128
  // grid timing once, in this browser (task 3.3).
  useEffect(() => {
    if (!sim.ready) {
      return
    }
    sim.withSim((s) => {
      const cfg = JSON.parse(s.wind_json() as string) as { mode: WindModeName }
      setWindMode(cfg.mode)
      wind.benchmarkLargeGrid(s)
    })
    // `wind` is a stable handle of refs; re-running on it would re-benchmark.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sim.ready])

  const applyWindMode = (next: WindModeName) => {
    sim.withSim((s) => {
      const cfg = JSON.parse(s.wind_json() as string) as Record<string, unknown>
      s.set_wind(JSON.stringify({ ...cfg, mode: next }))
    })
    setWindMode(next)
  }

  // The scenario's camera is a *suggestion* (brief §32): applied once when a
  // scenario loads, and overridden by anything the player does afterwards.
  const suggestedFor = useRef<string | null>(null)
  useEffect(() => {
    const scenario = sim.scenario
    if (scenario === null || suggestedFor.current === scenario.name) {
      return
    }
    suggestedFor.current = scenario.name
    setMode(scenario.camera.mode)
    setZoom(scenario.camera.zoom)
    setPan({ x: 0, y: 0 })
  }, [sim.scenario])

  const selectScenario = useCallback(
    (id: string) => {
      exitReplay()
      sim.loadScenario(id)
      // Keep the URL honest, so a reload — and a copied link — reproduce the
      // run. `replaceState` rather than `pushState`: switching scenarios is
      // not navigation.
      const url = new URL(window.location.href)
      url.searchParams.set('scenario', id)
      window.history.replaceState(null, '', url)
    },
    [exitReplay, sim],
  )

  /**
   * Start a challenge.
   *
   * A replay is closed first — the boat is about to sail again — and the
   * remembered attempts are dropped when the challenge changes, because two
   * results from two different challenges are not a comparison and holding
   * them side by side would only invite one.
   */
  /**
   * Put the boat back in front of the player.
   *
   * The practice panel is in the layout's `instruments` slot, which sits
   * below the world view inside the scrolling `main` row — as the recording
   * controls and the heel indicator already do. Measured at 1280 × 720: the
   * world view fills the whole 335 px row, so pressing Start means the boat
   * is off the top of the scrollport at the moment the attempt begins.
   *
   * Scrolling the world cell back to the top of the row is the narrowest fix
   * there is. It changes no layout, no size and nothing in the core — only
   * where the one scroll container is looking — and it is here rather than in
   * `ui/Layout.tsx` because `App.tsx` already holds the ref the layout
   * publishes. A slot of its own, above the world, is the better answer and
   * belongs to whichever PRD next owns `ui/Layout.tsx`.
   */
  const showTheBoat = useCallback(() => {
    worldRef.current?.scrollIntoView({ block: 'start' })
  }, [worldRef])

  const onStartPractice = useCallback(
    (id: string) => {
      exitReplay()
      if (attempts.length > 0 && attempts[0].taskId !== id) {
        forgetAttempts()
      }
      startPractice(id, PRACTICE_LOG_HZ)
      showTheBoat()
    },
    [attempts, exitReplay, forgetAttempts, showTheBoat, startPractice],
  )

  const onRetryPractice = useCallback(() => {
    exitReplay()
    retryPractice()
    showTheBoat()
  }, [exitReplay, retryPractice, showTheBoat])

  const onCancelPractice = useCallback(() => {
    exitReplay()
    cancelPractice()
  }, [cancelPractice, exitReplay])

  /**
   * **Inspect**: open this attempt's own episode at its highlight event.
   *
   * The event is the evaluator's — the tack's crossing, the peak heel, the
   * moment the target speed was reached — and it carries the **physics step**
   * it was decided on as well as the time (v2 F18.4), so the playhead lands
   * on a moment that happened rather than on one interpolated between two
   * that did.
   */
  const onInspectAttempt = useCallback(
    (attempt: PracticeAttempt) => {
      setEpisode(attempt.episode)
      // The highlight if the attempt produced one; otherwise the last event
      // it did produce; otherwise the start of the episode. An attempt that
      // timed out before anything happened is still worth looking at, and
      // inventing a moment for it would be the one thing not to do.
      const events = attempt.report.events
      const at = attempt.report.highlight ?? (events.length > 0 ? events[events.length - 1] : null)
      enterReplay(attempt.episode, at?.t)
    },
    [enterReplay],
  )

  // The E2E probe. See `ReplayProbe`.
  useEffect(() => {
    const probe: ReplayProbe = {
      episodeJson: () => (episode === null ? null : JSON.stringify(episode)),
      binary: () => {
        if (episode === null) {
          return null
        }
        const bytes = withSim((core) =>
          core.episode_to_binary(JSON.stringify(episode)),
        )
        return bytes === null ? null : Array.from(bytes)
      },
      load: (data) => {
        const loaded = withSim((core) =>
          importEpisode(core, typeof data === 'string' ? data : Uint8Array.from(data)),
        )
        if (loaded !== null) {
          setEpisode(loaded)
        }
      },
      frameTime: (index) =>
        playback.kind === 'replay' ? (playback.source.frameAt(index)?.t ?? null) : null,
      frameState: (index) => {
        const frame = playback.kind === 'replay' ? playback.source.frameAt(index) : null
        return frame === null ? null : [...frame.state]
      },
      patchFrame: (index, field, value) => {
        if (playback.kind === 'replay') {
          // Mutates the stored frame in place. If the render follows this,
          // the renderer is reading stored data and not recomputing it.
          const frame = playback.source.frameAt(index)
          if (frame !== null) {
            frame.state[field] = value
          }
        }
      },
      frameCount: () => (playback.kind === 'replay' ? playback.source.frameCount : 0),
      identityJson: () =>
        episode === null
          ? null
          : (withSim((core) => JSON.stringify(readEpisodeIdentity(core, episode))) ?? null),
      comparabilityJson: () =>
        episode === null
          ? null
          : (withSim((core) => JSON.stringify(compareEpisodes(core, episode, episode))) ??
            null),
      unavailable: () => [...(viewRef.current?.unavailable ?? [])],
      inspection: () => ({
        source: viewRef.current?.source ?? 'live',
        sampleIndex: viewRef.current?.sampleIndex ?? null,
        sampleT: viewRef.current?.diagnostics?.t ?? null,
      }),
    }
    window.__sailgym = probe
    return () => {
      delete window.__sailgym
    }
  }, [episode, playback, withSim])

  // ---------------------------------------------------------------------
  // The one display selection (v2 section 10, task 10.3)
  // ---------------------------------------------------------------------
  //
  // Chosen **once**, here, and handed to every consumer below: the boat, the
  // HUD, the wind readout, the force overlay, the charts and the debug panel.
  // Before this, a replay showed a recorded pose inside the live run's
  // numbers, which is RV57. `sim.snapshot`, `sim.diagnostics` and
  // `wind.windAtBoat()` appear on this line and nowhere else in the render.
  const view = selectInspection(playback, replayTime, {
    snapshot: sim.snapshot,
    diagnostics: sim.diagnostics,
    wind: wind.windAtBoat(),
  })
  // An empty episode has no pose to show; the last live one stays on screen
  // and the timeline says the episode has no samples.
  const s = view.emptyEpisode ? sim.snapshot : view.snapshot
  const diagnostics = view.diagnostics
  const replaying = view.source === 'replay'
  const capsized = replaying
    ? (playback.kind === 'replay'
        ? (playback.source.frameAt(view.sampleIndex ?? 0)?.capsized ?? null)
        : null)
    : (sim.diagnostics?.capsize.capsized ?? null)
  viewRef.current = view
  // The **live** chart history, sampled from the live record only; and, in
  // replay, the episode's own samples bounded by the playhead. A replay can
  // neither append to the live buffer nor carry it into playback (task 10.4).
  const liveCharts = useChartSampler(sim.diagnostics, ui.sampleHz, !replaying)
  // Memoised on the source and the playhead. `replayChartData` is pure and
  // walks every sample up to the playhead, which at the recording cap is
  // thirteen thousand of them — cheap once per scrub, not once per frame.
  const replayCharts = useMemo(
    () => (playback.kind === 'replay' ? replayChartData(playback.source, replayTime) : null),
    [playback, replayTime],
  )
  const charts = replayCharts ?? liveCharts
  const scale = PIXELS_PER_METRE * zoom
  baseCentre.current = trackedCentre(
    baseCentre.current,
    { x: s.x, y: s.y },
    mode,
    viewport,
    scale,
  )
  const camera = createCamera({
    mode,
    zoom,
    centre: { x: baseCentre.current.x + pan.x, y: baseCentre.current.y + pan.y },
    heading: s.psi,
    viewport,
  })
  cameraRef.current = camera

  const params = sim.params ?? PENDING_PARAMS
  // Memoised on the *values*, not rebuilt per frame. A fresh object here
  // invalidates `BoatSvg`'s model memo and defeats `BoatProbe`'s `memo`, which
  // rebuilds ten boats' worth of geometry sixty times a second — measured at
  // 17.8 ms in the `svg` span against a 12 ms budget (RV4).
  const hull = useMemo(
    () => ({ loa: params.hull.loa, beam: params.hull.beam }),
    [params.hull.loa, params.hull.beam],
  )
  const sheetRig = useMemo(
    () => ({
      mastX: params.sail.mast_pos_b.x,
      dSheet: params.sheet.d_sheet,
      zBoom: params.sheet.z_boom,
      block: params.sheet.block_pos_b,
    }),
    [params],
  )

  // The dense field and the arrow lattice are the *live* field, sampled from
  // the live simulation. A replay may draw them only if the episode carries
  // enough version-matched data to reconstruct its own field — which this
  // build cannot do, so in replay they are hidden and the reason is shown.
  // The recorded vector at the boat does not identify the whole field
  // (`replayWindField`), and drawing the live one behind a recorded boat is
  // RV57 exactly.
  const showWindField = view.windField.available
  const grid = showWindField ? wind.grid() : null
  const arrows: ArrowField | null =
    showArrows && showWindField && grid !== null ? buildArrows(grid, camera) : null
  // Sail Mode draws the field thinner and fainter so the hull, the boom, the
  // heading and the wind's own direction all stay readable (RV55). It is a
  // **presentation** setting: the same field is sampled, the same particles
  // are advected through it at the same speed, and nothing the boat feels
  // changes. Debug Mode, which exists to be read, gets all of it.
  const windVisual = ui.mode === 'sail' ? SAIL_MODE_WIND : DEBUG_MODE_WIND
  const layers =
    grid === null
      ? []
      : [
          ...particleLayers(
            {
              particles: wind.particles(),
              heads: wind.heads(),
              tails: wind.tails(),
            },
            windVisual,
          ),
          ...(arrows?.layers ?? []),
        ]
  const stats = wind.stats()

  const header = (
    <>
      <strong>sailgym</strong>
      <div data-testid="wasm-status" data-ready={sim.ready ? 'true' : 'false'}>
        {sim.error !== null
          ? `wasm failed to load: ${sim.error}`
          : sim.ready
            ? `sailgym ${sim.version}`
            : 'loading sailgym…'}
      </div>
      <ModeSwitch />
      <ScenarioPicker
        scenarios={sim.scenarios}
        current={sim.scenario?.name ?? ''}
        onSelect={selectScenario}
        disabled={!sim.ready}
      />
      <ClockControls
        state={sim.clockState}
        onToggleRunning={sim.toggleRunning}
        onReset={sim.reset}
        onSingleStep={sim.singleStep}
        onSetSpeed={sim.setSpeed}
      />
      <button
        type="button"
        data-testid="camera-mode"
        data-mode={mode}
        onClick={() => setMode((m) => (m === 'follow' ? 'northUp' : 'follow'))}
      >
        Camera: {mode}
      </button>
      <button
        type="button"
        data-testid="camera-reset"
        onClick={() => {
          setZoom(1)
          setPan({ x: 0, y: 0 })
          baseCentre.current = { x: s.x, y: s.y }
        }}
      >
        Reset camera
      </button>
      <label>
        Wind:{' '}
        <select
          data-testid="wind-mode"
          value={windMode}
          onChange={(e) => applyWindMode(e.target.value as WindModeName)}
        >
          {WIND_MODES.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </label>
      <button
        type="button"
        data-testid="toggle-wind-arrows"
        data-on={showArrows ? 'true' : 'false'}
        onClick={() => setShowArrows((v) => !v)}
      >
        Arrows: {showArrows ? 'on' : 'off'}
      </button>
    </>
  )

  // brief §29's seven, and only those. The wind readout is handed in rather
  // than rendered inside the HUD: it belongs to the wind layer.
  const readouts = (
    <Hud
      diagnostics={diagnostics}
      snapshot={s}
      wind={<WindReadout wind={view.wind} />}
      capsized={capsized}
    />
  )

  const world = (
    <div
      style={{
        position: 'relative',
        width: viewport.width,
        height: viewport.height,
        // The sea. It lives here rather than on the SVG because the wind
        // canvas sits between the two.
        background: '#eaf2f8',
      }}
    >
      {sim.ready && <DeckOverlay viewport={viewport} layers={layers} />}
      <div style={{ position: 'relative', zIndex: 1, pointerEvents: 'auto' }}>
        <BoatSvg
          camera={camera}
          pose={{ x: s.x, y: s.y, psi: s.psi, phi: s.phi, beta: s.beta, deltaR: s.deltaR }}
          // The sail's drawn camber and the rope's drawn sag are the two
          // places a *picture* needs a diagnostic. A schema-1 episode records
          // neither, so both fall back to the neutral drawing — a flat sail
          // and a rope at its own path length, claiming no angle of attack and
          // no tension — and the replay notice below says which fields are not
          // recorded. Nothing is taken from the live simulation (RV57).
          alpha={diagnostics?.values.alpha_sail ?? 0}
          params={params}
          hull={hull}
          sheet={sheetRig}
          lSheet={s.lSheet}
          ropeLength={diagnostics?.values.sheet_rope_length ?? s.lSheet}
          trajectory={sim.trajectory}
          onSheet={(ev) => {
            const [next, rate] = reduceSheetInput(sheetInput.current, ev, DEFAULT_INPUT)
            sheetInput.current = next
            // `null` when no drag owns the channel, so the composition falls
            // through to whatever else is driving instead of reading a stale
            // zero as a command (task 9.1, RV53).
            sim.setSheetRate(ownsSheet(next) ? rate : null)
            // Every gesture the world view can own arrives here, the camera's
            // pans included, so this is the one place that knows a drag is in
            // progress. `cancel` covers `pointercancel` and a lost capture.
            if (ev.type === 'down') {
              setWorldDrag(true)
            } else if (ev.type === 'up' || ev.type === 'cancel') {
              setWorldDrag(false)
            }
          }}
          onPan={(dxPixels, dyPixels) => {
            const a = camera.screenToWorld({ x: 0, y: 0 })
            const b = camera.screenToWorld({ x: dxPixels, y: dyPixels })
            setPan((p) => ({ x: p.x - (b.x - a.x), y: p.y - (b.y - a.y) }))
          }}
          onZoom={(factor) => {
            setZoom((z) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z * factor)))
          }}
        />
      </div>
      {replaying && (
        <div
          data-testid="replay-notice"
          data-unavailable={view.unavailable.length}
          data-empty={view.emptyEpisode ? 'true' : 'false'}
          data-interpolated={view.interpolated ? 'true' : 'false'}
          data-wind-field={view.windField.available ? 'true' : 'false'}
          style={{
            position: 'absolute',
            left: 8,
            top: 8,
            zIndex: 3,
            maxWidth: 'calc(100% - 16px)',
            padding: '4px 8px',
            borderRadius: 4,
            background: 'rgba(255, 255, 255, 0.88)',
            border: '1px solid #cbd',
            color: '#334',
            fontSize: 12,
            pointerEvents: 'none',
          }}
        >
          <strong>Replay</strong> · every number below is this episode's ·{' '}
          {view.emptyEpisode
            ? 'this episode has no samples'
            : `pose at t = ${view.t.toFixed(2)} s${
                view.interpolated ? ' (interpolated)' : ''
              }, values from the sample at t = ${(diagnostics?.t ?? 0).toFixed(2)} s`}
          <br />
          wind field hidden — {view.windField.reason}
          {view.unavailable.length > 0 && (
            <>
              <br />
              {view.unavailable.length} diagnostic
              {view.unavailable.length === 1 ? '' : 's'} {NOT_RECORDED.toLowerCase()} in this
              episode
            </>
          )}
        </div>
      )}
      {/* Over the boat, and only in Debug Mode. `pointerEvents: none` inside,
          so the mainsheet drag and the camera pan still reach the SVG. */}
      {ui.mode === 'debug' && (
        <ForceOverlay
          camera={camera}
          pose={{ x: s.x, y: s.y, psi: s.psi }}
          diagnostics={diagnostics}
          enabled={ui.overlays}
          newtonsPerPixel={ui.newtonsPerPixel}
          auto={ui.autoScale}
        />
      )}
    </div>
  )

  const debug = (
    <>
      <OverlayControls
        enabled={ui.overlays}
        onToggle={ui.setOverlay}
        onAll={ui.setAllOverlays}
        newtonsPerPixel={ui.newtonsPerPixel}
        onNewtonsPerPixel={ui.setNewtonsPerPixel}
        auto={ui.autoScale}
        onAuto={ui.setAutoScale}
      />
      <OverlayLegend
        diagnostics={diagnostics}
        enabled={ui.overlays}
        newtonsPerPixel={ui.newtonsPerPixel}
        auto={ui.autoScale}
      />
      <Charts
        data={charts}
        enabled={ui.charts}
        onToggle={ui.setChart}
        sampleHz={ui.sampleHz}
        onSampleHz={ui.setSampleHz}
        source={replaying ? 'recorded' : 'live'}
        resolutionHz={playback.kind === 'replay' ? playback.source.logHz : ui.sampleHz}
      />
      <ParameterPanel
        withSim={sim.withSim}
        ready={sim.ready}
        open={ui.parametersOpen}
        onOpenChange={ui.setParametersOpen}
        resetRequired={ui.resetRequired}
        onResetRequired={ui.setResetRequired}
        onReset={sim.reset}
      />
      <DebugPanel diagnostics={diagnostics} />
    </>
  )

  const instruments = (
    <>
      <div style={{ display: 'grid', gap: 6, flex: '1 1 420px', minWidth: 320 }}>
        <PracticePanel
          ready={sim.ready}
          challenges={sim.challenges}
          state={practice}
          attempts={attempts}
          comparison={attemptComparison}
          compare={attemptCompare}
          onStart={onStartPractice}
          onRetry={onRetryPractice}
          onCancel={onCancelPractice}
          onInspect={onInspectAttempt}
          replaying={playback.kind === 'replay'}
        />
        {/* Hidden while an attempt is running, and only then. The attempt owns
            the recorder — pressing Record would start a second episode over the
            top of it and throw away the events already in the first — and there
            is nothing to export until the attempt has produced something. It
            comes back the moment the attempt ends or is abandoned, which is
            what keeps free sail and the whole v1 recording path reachable. */}
        {!practiceActive && (
          <RecordControls
            ready={sim.ready}
            withSim={sim.withSim}
            episode={episode}
            onEpisode={setEpisode}
            onReplay={() => enterReplay()}
            replaying={playback.kind === 'replay'}
          />
        )}
        {/* The recording bound, visible before it is reached. The cap and the
            bytes per sample are the core's (`recording::MAX_EPISODE_FRAMES`,
            derived from a stated byte budget); nothing here restates one. */}
        {limit !== null && (
          <div data-testid="record-limit" data-frames={limit.frames} data-bytes={limit.bytes}
               style={{ color: '#667', fontSize: 12 }}>
            recording limit {limit.frames.toLocaleString()} samples ·{' '}
            {megabytes(limit.bytes)} · {(recordingSeconds(limit, 20) / 60).toFixed(1)} min at
            20 Hz
          </div>
        )}
        {episode !== null && identity !== null && (
          <div
            data-testid="episode-identity"
            data-baseline={identityIsBaseline ? 'true' : 'false'}
            data-comparable={comparability?.verdict ?? ''}
            style={{ color: '#667', fontSize: 12 }}
          >
            episode identity: {describeIdentity(identity)}
            {' · '}
            {identityIsBaseline
              ? 'may be compared with another episode of the same conditions'
              : 'cannot be labelled a same-conditions experiment'}
          </div>
        )}
        {playback.kind === 'replay' && (
          <Timeline
            source={playback.source}
            time={replayTime}
            playing={replayPlaying}
            speed={replaySpeed}
            onTime={setReplayTime}
            onPlaying={setReplayPlaying}
            onSpeed={setReplaySpeed}
            onExit={exitReplay}
            unavailable={view.unavailable.length}
            windFieldReason={view.windField.reason}
            events={playback.source.header.practice?.events ?? []}
          />
        )}
      </div>
      {/* brief §26: top-down geometry cannot show roll, so heel gets its own
          stern view. `φ` comes from the snapshot, the capsize report from the
          diagnostics; both are the core's numbers (F8).

          It sits **below** the world view on purpose. The section 02-06 specs
          drive the mainsheet with absolute page coordinates, so anything
          added above the boat moves the target out from under them. */}
      <HeelIndicator
        phi={s.phi}
        capsized={capsized ?? false}
        maxHeel={diagnostics?.values.capsize?.max_heel ?? 0}
      />
    </>
  )

  const footer = (
    <>
      <div
        data-testid="snapshot"
        data-dt={sim.dt}
        data-t={s.t}
        data-x={s.x}
        data-y={s.y}
        data-psi={s.psi}
        data-phi={s.phi}
        data-u={s.u}
        data-v={s.v}
        data-r={s.r}
        data-p={s.p}
        data-beta={s.beta}
        data-beta-dot={s.betaDot}
        data-delta-r={s.deltaR}
        data-l-sheet={s.lSheet}
        // Every one of these is the *inspected* value: live while live,
        // recorded while replaying, and the empty string when the episode did
        // not record it. `''` rather than `0`, so a test can tell "not
        // recorded" from "measured zero" (task 10.3).
        //
        // **Numbers only.** `fixtures.ts`'s `readSnapshot` turns every
        // `data-*` on this element into a `Number`, so a word here becomes a
        // `NaN` in every spec that reads it. Which timeline these came from
        // lives on `[data-testid="inspection"]` below.
        data-sheet-tension={diagnostics?.values.sheet_tension ?? ''}
        data-rope-length={diagnostics?.values.sheet_rope_length ?? ''}
        data-gz={diagnostics?.values.gz ?? ''}
        data-k-restore={diagnostics?.values.righting_moment ?? ''}
        data-capsized={capsized === null ? '' : capsized ? 'true' : 'false'}
        data-max-heel={diagnostics?.values.capsize?.max_heel ?? ''}
      >
        t {s.t.toFixed(3)} s · x {s.x.toFixed(2)} m · y {s.y.toFixed(2)} m · ψ{' '}
        {((s.psi * 180) / Math.PI).toFixed(1)}° · u {s.u.toFixed(2)} m/s · δr{' '}
        {((s.deltaR * 180) / Math.PI).toFixed(1)}°
      </div>

      <div
        data-testid="wind-stats"
        data-frames={stats.frames}
        data-grid-calls={stats.gridCalls}
        data-grid-ms={stats.gridMs}
        data-wind-ms={stats.windMs}
        data-frame-ms={stats.frameMs}
        data-benchmark-ms={stats.benchmarkMs ?? ''}
        data-grid-nx={grid?.nx ?? 0}
        data-grid-ny={grid?.ny ?? 0}
        // `data-particles` is what the frame actually **draws**, which is what
        // `wind.spec.ts` claims to be measuring when it asserts the dense field
        // is WebGL and not DOM. Sail Mode draws a fraction of the population
        // (task 9.4); the population itself is beside it, so the two are
        // distinguishable rather than conflated.
        data-particles={visibleCount(wind.particles().count, windVisual.density)}
        data-particles-total={wind.particles().count}
        data-wind-density={windVisual.density}
        data-wind-contrast={windVisual.contrast}
        style={{ color: '#667' }}
      >
        wind grid {grid?.nx ?? 0}×{grid?.ny ?? 0} in {stats.gridMs.toFixed(2)} ms · frame{' '}
        {stats.frameMs.toFixed(1)} ms ·{' '}
        {visibleCount(wind.particles().count, windVisual.density)} of{' '}
        {wind.particles().count} particles
      </div>

      {/* Which timeline the page is showing, and which recorded sample the
          diagnostics came from. Separate from `snapshot` because that element
          is read as numbers (see the note there). */}
      <div
        data-testid="inspection"
        data-source={view.source}
        data-sample-index={view.sampleIndex ?? ''}
        data-sample-t={diagnostics?.t ?? ''}
        data-interpolated={view.interpolated ? 'true' : 'false'}
        data-unavailable={view.unavailable.length}
        data-empty={view.emptyEpisode ? 'true' : 'false'}
      />

      <ArrowProbe field={arrows} />
      <HeelProbe />
      {showBoatProbes && <BoatProbe params={params} hull={hull} />}

      <div style={{ color: '#667' }}>
        Drag the pads below to steer and trim, or: A / ← and D / → steer ·{' '}
        <strong>drag down on the boat to haul the mainsheet in, drag up to ease</strong> ·
        Space releases the sheet · P pauses · . single-steps · R resets · M switches mode ·
        wheel zooms · middle-drag or Shift+drag pans
      </div>
    </>
  )

  // The on-screen helm, trim and release (task 9.3). Always mounted, in both
  // modes and on every device: they are the controls, not a mobile fallback,
  // and the same pads work under a mouse. They read the boat's actual state
  // and the live catalogue, and they write through the one input boundary.
  const controls = (
    <TouchControls
      snapshot={s}
      params={params}
      onCommand={sim.setTouchCommand}
      onReset={sim.reset}
      clearSignal={sim.inputGeneration}
    />
  )

  return (
    <Layout
      mode={ui.mode}
      modeChosen={ui.modeChosen}
      compact={compact}
      dragging={worldDrag}
      header={header}
      readouts={readouts}
      world={world}
      instruments={instruments}
      debug={debug}
      footer={footer}
      controls={controls}
      mainRef={mainRef}
      worldRef={worldRef}
    />
  )
}
