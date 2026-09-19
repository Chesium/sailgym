import { useCallback, useEffect, useRef, useState } from 'react'

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
import { BoatSvg } from './render/BoatSvg'
import { HeelIndicator, HeelProbe } from './render/HeelIndicator'
import { DEFAULT_INPUT } from './sim/controls'
import { IDLE_SHEET_INPUT, reduceSheetInput, type SheetInputState } from './sim/sheetInput'
import { useSimulation } from './sim/useSimulation'
import { ClockControls } from './ui/ClockControls'
import { Hud } from './ui/Hud'
import { WindReadout } from './ui/WindReadout'
import { ArrowProbe, buildArrows, type ArrowField } from './wind/ArrowOverlay'
import { DeckOverlay } from './wind/DeckOverlay'
import { particleLayers } from './wind/WindLayer'
import { useWindField } from './wind/useWindField'

const VIEWPORT = { width: 780, height: 520 }

/** M3/M4 fixtures; section 09 supplies the full scenario selector. */
function scenarioFromUrl(): string {
  return new URLSearchParams(window.location.search).get('scenario') ?? ''
}

/** The three F6.1 wind modes, as the scenario JSON spells them. */
const WIND_MODES = ['uniform', 'spatial', 'gust'] as const
type WindModeName = (typeof WIND_MODES)[number]

/**
 * The M2 application: the M1 boat, plus an animated deck.gl wind field driven
 * by the same Rust field the simulation uses.
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

  const sim = useSimulation(undefined, onFrame, scenarioFromUrl())
  const [mode, setMode] = useState<CameraMode>('northUp')
  const [zoom, setZoom] = useState(1)
  const [pan, setPan] = useState<Vec2>({ x: 0, y: 0 })
  const [windMode, setWindMode] = useState<WindModeName>('gust')
  const sheetInput = useRef<SheetInputState>(IDLE_SHEET_INPUT)
  const [showArrows, setShowArrows] = useState(false)
  const baseCentre = useRef<Vec2>({ x: 0, y: 0 })

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

  const s = sim.snapshot
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

  const hull = sim.params === null ? { loa: 1, beam: 1 } : sim.params.hull
  const rig =
    sim.params === null
      ? { mastX: 0, boomLength: 1, rudderX: 0, boardX: 0 }
      : {
          mastX: sim.params.sail.mast_pos_b.x,
          boomLength: sim.params.sail.boom_length,
          rudderX: sim.params.rudder.pos_b.x,
          boardX: sim.params.board.pos_b.x,
        }
  const sheetRig =
    sim.params === null
      ? { mastX: 0, dSheet: 0, block: { x: 0, y: 0 } }
      : {
          mastX: sim.params.sail.mast_pos_b.x,
          dSheet: sim.params.sheet.d_sheet,
          block: { x: sim.params.sheet.block_pos_b.x, y: sim.params.sheet.block_pos_b.y },
        }

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

  return (
    <div style={{ font: '13px system-ui, sans-serif', padding: 12, display: 'grid', gap: 8 }}>
      <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
        <strong>sailgym</strong>
        <div data-testid="wasm-status" data-ready={sim.ready ? 'true' : 'false'}>
          {sim.error !== null
            ? `wasm failed to load: ${sim.error}`
            : sim.ready
              ? `sailgym ${sim.version}`
              : 'loading sailgym…'}
        </div>
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
      </div>

      <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
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
        <WindReadout wind={wind.windAtBoat()} />
        <Hud diagnostics={sim.diagnostics} />
      </div>

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
            pose={{ x: s.x, y: s.y, psi: s.psi, beta: s.beta, deltaR: s.deltaR }}
            alpha={sim.diagnostics?.alpha_sail ?? 0}
            hull={hull}
            rig={rig}
            sheet={sheetRig}
            lSheet={s.lSheet}
            ropeLength={sim.diagnostics?.rope_length ?? 0}
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
      </div>

      <div style={{ display: 'flex', gap: 12, alignItems: 'flex-start', flexWrap: 'wrap' }}>
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
      </div>

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
        data-rope-length={sim.diagnostics?.rope_length ?? 0}
        data-gz={sim.diagnostics?.gz ?? 0}
        data-k-restore={sim.diagnostics?.k_restore ?? 0}
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

      <div style={{ color: '#667' }}>
        A / ← and D / → steer · <strong>drag down to haul the mainsheet in, drag up to
        ease</strong> · Space releases the sheet · P pauses · . single-steps · R resets · wheel
        zooms · middle-drag or Shift+drag pans
      </div>
    </div>
  )
}
