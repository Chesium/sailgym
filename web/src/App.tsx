import { useRef, useState } from 'react'

import {
  createCamera,
  trackedCentre,
  PIXELS_PER_METRE,
  MIN_ZOOM,
  MAX_ZOOM,
  type CameraMode,
  type Vec2,
} from './render/Camera'
import { BoatSvg } from './render/BoatSvg'
import { useSimulation } from './sim/useSimulation'
import { ClockControls } from './ui/ClockControls'

const VIEWPORT = { width: 780, height: 520 }

/**
 * The M1 application: a steerable boat, a trajectory, two camera modes and the
 * full clock control set.
 *
 * `data-testid="wasm-status"` and `data-ready` are the contract frozen by
 * task 1.3 — do not rename them. `data-testid="snapshot"` carries the raw F8.3
 * values as attributes so the E2E suite can assert on numbers rather than
 * pixels (brief §42).
 */
export default function App() {
  const sim = useSimulation()
  const [mode, setMode] = useState<CameraMode>('northUp')
  const [zoom, setZoom] = useState(1)
  const [pan, setPan] = useState<Vec2>({ x: 0, y: 0 })
  const baseCentre = useRef<Vec2>({ x: 0, y: 0 })

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

      <BoatSvg
        camera={camera}
        pose={{ x: s.x, y: s.y, psi: s.psi, beta: s.beta, deltaR: s.deltaR }}
        hull={hull}
        rig={rig}
        trajectory={sim.trajectory}
        onPan={(dxPixels, dyPixels) => {
          const a = camera.screenToWorld({ x: 0, y: 0 })
          const b = camera.screenToWorld({ x: dxPixels, y: dyPixels })
          setPan((p) => ({ x: p.x - (b.x - a.x), y: p.y - (b.y - a.y) }))
        }}
        onZoom={(factor) => {
          setZoom((z) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z * factor)))
        }}
      />

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
      >
        t {s.t.toFixed(3)} s · x {s.x.toFixed(2)} m · y {s.y.toFixed(2)} m · ψ{' '}
        {((s.psi * 180) / Math.PI).toFixed(1)}° · u {s.u.toFixed(2)} m/s · δr{' '}
        {((s.deltaR * 180) / Math.PI).toFixed(1)}°
      </div>

      <div style={{ color: '#667' }}>
        A / ← and D / → steer · Space eases the sheet · P pauses · . single-steps · R resets ·
        wheel zooms · middle-drag or Shift+drag pans
      </div>
    </div>
  )
}
