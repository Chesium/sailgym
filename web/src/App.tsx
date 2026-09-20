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
import { importEpisode } from './sim/episodeIo'
import {
  createReplaySource,
  snapshotFromFrame,
  LIVE,
  type PlaybackMode,
  type ReplayProbe,
} from './sim/replay'
import type { Episode } from './sim/scenarioTypes'
import { IDLE_SHEET_INPUT, reduceSheetInput, type SheetInputState } from './sim/sheetInput'
import { useSimulation, type RenderParams } from './sim/useSimulation'
import { Charts, useChartSampler } from './ui/Charts'
import { ClockControls } from './ui/ClockControls'
import { DebugPanel } from './ui/DebugPanel'
import { Hud } from './ui/Hud'
import { Layout } from './ui/Layout'
import { ModeSwitch } from './ui/ModeSwitch'
import { ParameterPanel } from './ui/ParameterPanel'
import { RecordControls } from './ui/RecordControls'
import { ScenarioPicker } from './ui/ScenarioPicker'
import { Timeline, type PlaybackSpeed } from './ui/Timeline'
import { useUiStore } from './ui/store'
import { WindReadout } from './ui/WindReadout'
import { ArrowProbe, buildArrows, type ArrowField } from './wind/ArrowOverlay'
import { DeckOverlay } from './wind/DeckOverlay'
import { particleLayers } from './wind/WindLayer'
import { useWindField } from './wind/useWindField'

const VIEWPORT = { width: 780, height: 520 }

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
  rudder: { pos_b: { x: 0, y: 0, z: 0 }, area: 1 },
  board: { pos_b: { x: 0, y: 0, z: 0 }, area: 1 },
  sheet: {
    d_sheet: 0,
    z_boom: 1,
    block_pos_b: { x: 0, y: 0, z: 0 },
    l_sheet_min: 0,
    l_sheet_max: 1,
  },
}

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

  const onFrame = useCallback(
    (sim: Parameters<typeof wind.onFrame>[0], snapshot: { t: number }) => {
      const camera = cameraRef.current
      if (camera !== null) {
        wind.onFrame(sim, camera, snapshot.t)
      }
    },
    [wind],
  )

  const sim = useSimulation(undefined, onFrame, scenarioFromUrl(), renderHzFromUrl())
  const ui = useUiStore()
  const charts = useChartSampler(sim.diagnostics, ui.sampleHz)
  const [mode, setMode] = useState<CameraMode>('northUp')
  const [zoom, setZoom] = useState(1)
  const [pan, setPan] = useState<Vec2>({ x: 0, y: 0 })
  const [windMode, setWindMode] = useState<WindModeName>('gust')
  const sheetInput = useRef<SheetInputState>(IDLE_SHEET_INPUT)
  const [showArrows, setShowArrows] = useState(false)
  const [showBoatProbes] = useState(boatProbesRequested)
  const baseCentre = useRef<Vec2>({ x: 0, y: 0 })

  // --- recording and replay (brief §33) ----------------------------------
  const [episode, setEpisode] = useState<Episode | null>(null)
  const [playback, setPlayback] = useState<PlaybackMode>(LIVE)
  const [replayTime, setReplayTime] = useState(0)
  const [replayPlaying, setReplayPlaying] = useState(false)
  const [replaySpeed, setReplaySpeed] = useState<PlaybackSpeed>(1)

  const enterReplay = useCallback(() => {
    if (episode === null) {
      return
    }
    const source = createReplaySource(episode)
    // The live boat and the replayed one would otherwise animate past each
    // other in the same view. Pausing is also what makes "replay works with
    // the physics clock paused" the ordinary case rather than a special one.
    sim.pause()
    setPlayback({ kind: 'replay', source })
    setReplayTime(source.startTime)
    setReplayPlaying(false)
  }, [episode, sim])

  const exitReplay = useCallback(() => {
    setReplayPlaying(false)
    setPlayback(LIVE)
  }, [])

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

  // The E2E probe. See `ReplayProbe`.
  const { withSim } = sim
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
        playback.kind === 'replay' ? playback.source.frameAt(index).t : null,
      frameState: (index) =>
        playback.kind === 'replay' ? [...playback.source.frameAt(index).state] : null,
      patchFrame: (index, field, value) => {
        if (playback.kind === 'replay') {
          // Mutates the stored frame in place. If the render follows this,
          // the renderer is reading stored data and not recomputing it.
          playback.source.frameAt(index).state[field] = value
        }
      },
      frameCount: () => (playback.kind === 'replay' ? playback.source.frameCount : 0),
    }
    window.__sailgym = probe
    return () => {
      delete window.__sailgym
    }
  }, [episode, playback, withSim])

  // In replay the renderer reads a stored frame exactly as it reads a live
  // snapshot — same layout, same components, no physics (brief §33).
  const replayFrame = playback.kind === 'replay' ? playback.source.sampleAt(replayTime) : null
  const s = replayFrame === null ? sim.snapshot : snapshotFromFrame(replayFrame)
  const scale = PIXELS_PER_METRE * zoom
  baseCentre.current = trackedCentre(
    baseCentre.current,
    { x: s.x, y: s.y },
    mode,
    VIEWPORT,
    scale,
  )
  const camera = createCamera({
    mode,
    zoom,
    centre: { x: baseCentre.current.x + pan.x, y: baseCentre.current.y + pan.y },
    heading: s.psi,
    viewport: VIEWPORT,
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

  const grid = wind.grid()
  const arrows: ArrowField | null =
    showArrows && grid !== null ? buildArrows(grid, camera) : null
  const layers =
    grid === null
      ? []
      : [
          ...particleLayers({
            particles: wind.particles(),
            heads: wind.heads(),
            tails: wind.tails(),
          }),
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
    <Hud diagnostics={sim.diagnostics} snapshot={s} wind={<WindReadout wind={wind.windAtBoat()} />} />
  )

  const world = (
    <div
      style={{
        position: 'relative',
        width: VIEWPORT.width,
        height: VIEWPORT.height,
        // The sea. It lives here rather than on the SVG because the wind
        // canvas sits between the two.
        background: '#eaf2f8',
      }}
    >
      {sim.ready && <DeckOverlay viewport={VIEWPORT} layers={layers} />}
      <div style={{ position: 'relative', zIndex: 1, pointerEvents: 'auto' }}>
        <BoatSvg
          camera={camera}
          pose={{ x: s.x, y: s.y, psi: s.psi, phi: s.phi, beta: s.beta, deltaR: s.deltaR }}
          alpha={sim.diagnostics?.alpha_sail ?? 0}
          params={params}
          hull={hull}
          sheet={sheetRig}
          lSheet={s.lSheet}
          ropeLength={sim.diagnostics?.sheet_rope_length ?? 0}
          trajectory={sim.trajectory}
          onSheet={(ev) => {
            const [next, rate] = reduceSheetInput(sheetInput.current, ev, DEFAULT_INPUT)
            sheetInput.current = next
            sim.setSheetRate(rate)
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
      {/* Over the boat, and only in Debug Mode. `pointerEvents: none` inside,
          so the mainsheet drag and the camera pan still reach the SVG. */}
      {ui.mode === 'debug' && (
        <ForceOverlay
          camera={camera}
          pose={{ x: s.x, y: s.y, psi: s.psi }}
          diagnostics={sim.diagnostics}
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
        diagnostics={sim.diagnostics}
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
      <DebugPanel diagnostics={sim.diagnostics} />
    </>
  )

  const instruments = (
    <>
      <div style={{ display: 'grid', gap: 6, flex: '1 1 420px', minWidth: 320 }}>
        <RecordControls
          ready={sim.ready}
          withSim={sim.withSim}
          episode={episode}
          onEpisode={setEpisode}
          onReplay={enterReplay}
          replaying={playback.kind === 'replay'}
        />
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
        capsized={sim.diagnostics?.capsize.capsized ?? false}
        maxHeel={sim.diagnostics?.capsize.max_heel ?? 0}
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
        data-sheet-tension={sim.diagnostics?.sheet_tension ?? 0}
        data-rope-length={sim.diagnostics?.sheet_rope_length ?? 0}
        data-gz={sim.diagnostics?.gz ?? 0}
        data-k-restore={sim.diagnostics?.righting_moment ?? 0}
        data-capsized={sim.diagnostics?.capsize.capsized ? 'true' : 'false'}
        data-max-heel={sim.diagnostics?.capsize.max_heel ?? 0}
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
        data-particles={wind.particles().count}
        style={{ color: '#667' }}
      >
        wind grid {grid?.nx ?? 0}×{grid?.ny ?? 0} in {stats.gridMs.toFixed(2)} ms · frame{' '}
        {stats.frameMs.toFixed(1)} ms · {wind.particles().count} particles
      </div>

      <ArrowProbe field={arrows} />
      <HeelProbe />
      {showBoatProbes && <BoatProbe params={params} hull={hull} />}

      <div style={{ color: '#667' }}>
        A / ← and D / → steer · <strong>drag down to haul the mainsheet in, drag up to
        ease</strong> · Space releases the sheet · P pauses · . single-steps · R resets · M
        switches mode · wheel zooms · middle-drag or Shift+drag pans
      </div>
    </>
  )

  return (
    <Layout
      mode={ui.mode}
      header={header}
      readouts={readouts}
      world={world}
      instruments={instruments}
      debug={debug}
      footer={footer}
    />
  )
}
