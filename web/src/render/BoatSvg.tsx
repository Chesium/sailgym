import { useRef } from 'react'

import { boatTransform, worldTransform, type Camera, type Vec2 } from './Camera'
import {
  boardSegment,
  boomSegment,
  hullPath,
  rudderSegment,
  type HullDims,
  type RigDims,
} from './geometry'
import { sailShape } from './SailShape'
import { SheetRope, type SheetRigDims } from './SheetRope'
import { radiansToDegrees } from '../sim/units'
import { Trajectory } from './Trajectory'
import type { SheetEvent } from '../sim/sheetInput'

/** Boat pose, straight off the snapshot. */
export interface BoatPose {
  x: number
  y: number
  psi: number
  beta: number
  deltaR: number
}

export interface BoatSvgProps {
  camera: Camera
  pose: BoatPose
  alpha: number
  hull: HullDims
  rig: RigDims
  /** Mainsheet geometry and lengths; the rope is drawn from these. */
  sheet: SheetRigDims
  /** m, available sheet length `L` (snapshot `lSheet`). */
  lSheet: number
  /** m, geometric rope path `ℓ(β)` (diagnostics `sheet_rope_length`). */
  ropeLength: number
  trajectory: readonly Vec2[]
  /** Drag with the middle button or Shift+drag. Left-drag is the mainsheet
   *  (brief §27); both pointer paths are fed to `onSheet` below, which decides
   *  which is which. */
  onPan: (dxPixels: number, dyPixels: number) => void
  /** Raw pointer events for the mainsheet reducer (section 06, task 6.3). */
  onSheet: (ev: SheetEvent) => void
  /** Wheel zoom; `factor` multiplies the current zoom. */
  onZoom: (factor: number) => void
}

/** World-space grid spacing, metres. */
const GRID_SPACING_M = 10

export function BoatSvg({
  camera,
  pose,
  alpha,
  hull,
  rig,
  sheet,
  lSheet,
  ropeLength,
  trajectory,
  onPan,
  onSheet,
  onZoom,
}: BoatSvgProps) {
  const dragging = useRef<{ x: number; y: number } | null>(null)
  const { width, height } = camera.viewport

  const boom = boomSegment(rig, 0)
  const rudder = rudderSegment(rig, hull, pose.deltaR)
  const board = boardSegment(rig, hull)

  // Grid lines covering the visible world, snapped to the spacing. A rotated
  // camera sees further into the corners, hence the diagonal margin.
  const reach = (Math.hypot(width, height) / 2 / camera.scale) * 1.05
  const first = (v: number) => Math.floor((v - reach) / GRID_SPACING_M) * GRID_SPACING_M
  const count = Math.ceil((2 * reach) / GRID_SPACING_M) + 1
  const x0 = first(camera.centre.x)
  const y0 = first(camera.centre.y)
  const gridLines: number[] = Array.from({ length: count }, (_, i) => i)

  return (
    <svg
      data-testid="world-view"
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      // Transparent, not the sea colour: section 03 puts the deck.gl wind
      // canvas *underneath* this SVG, and an opaque background here would hide
      // it completely. The sea colour moved to the wrapper in `App.tsx`.
      style={{ display: 'block', background: 'transparent', touchAction: 'none' }}
      onPointerDown={(e) => {
        // Middle button, or Shift + any button, pans. A plain left-drag is the
        // mainsheet. Both go to `onSheet`, whose reducer ignores the camera's.
        onSheet({ type: 'down', y: e.clientY, button: e.button, shiftKey: e.shiftKey })
        e.currentTarget.setPointerCapture(e.pointerId)
        if (e.button === 1 || e.shiftKey) {
          e.preventDefault()
          dragging.current = { x: e.clientX, y: e.clientY }
        }
      }}
      onPointerMove={(e) => {
        onSheet({ type: 'move', y: e.clientY, button: e.button, shiftKey: e.shiftKey })
        const from = dragging.current
        if (from === null) {
          return
        }
        onPan(e.clientX - from.x, e.clientY - from.y)
        dragging.current = { x: e.clientX, y: e.clientY }
      }}
      onPointerUp={(e) => {
        onSheet({ type: 'up', y: e.clientY, button: e.button, shiftKey: e.shiftKey })
        dragging.current = null
        if (e.currentTarget.hasPointerCapture(e.pointerId)) {
          e.currentTarget.releasePointerCapture(e.pointerId)
        }
      }}
      onPointerCancel={(e) => {
        onSheet({ type: 'cancel', y: e.clientY })
        dragging.current = null
      }}
      onWheel={(e) => {
        onZoom(Math.exp(-e.deltaY / 500))
      }}
    >
      <g
        data-testid="world-grid"
        transform={worldTransform(camera)}
        stroke="#c6d8e4"
        strokeWidth={1}
        vectorEffect="non-scaling-stroke"
      >
        {gridLines.map((i) => (
          <line
            key={`v${i}`}
            x1={x0 + i * GRID_SPACING_M}
            y1={y0}
            x2={x0 + i * GRID_SPACING_M}
            y2={y0 + (count - 1) * GRID_SPACING_M}
            vectorEffect="non-scaling-stroke"
          />
        ))}
        {gridLines.map((i) => (
          <line
            key={`h${i}`}
            x1={x0}
            y1={y0 + i * GRID_SPACING_M}
            x2={x0 + (count - 1) * GRID_SPACING_M}
            y2={y0 + i * GRID_SPACING_M}
            vectorEffect="non-scaling-stroke"
          />
        ))}
      </g>

      <Trajectory camera={camera} points={trajectory} />

      <g
        data-testid="boat-hull"
        transform={boatTransform(camera, { x: pose.x, y: pose.y }, pose.psi)}
      >
        <path
          d={hullPath(hull)}
          fill="#fdfdfd"
          stroke="#2b3a45"
          strokeWidth={1}
          vectorEffect="non-scaling-stroke"
        />
        <line
          data-testid="boat-board"
          x1={board.x1}
          y1={board.y1}
          x2={board.x2}
          y2={board.y2}
          stroke="#7a8a96"
          strokeWidth={3}
          vectorEffect="non-scaling-stroke"
        />
        <path
          data-testid="boat-sail"
          d={sailShape(rig, pose.beta, alpha)}
          fill="none"
          stroke="#d64d3f"
          strokeWidth={2.5}
          vectorEffect="non-scaling-stroke"
        />
        <g data-testid="boom" transform={`rotate(${radiansToDegrees(pose.beta)} ${rig.mastX} 0)`}>
        <line
          data-testid="boat-boom"
          x1={boom.x1}
          y1={boom.y1}
          x2={boom.x2}
          y2={boom.y2}
          stroke="#2b3a45"
          strokeWidth={2}
          vectorEffect="non-scaling-stroke"
        />
        </g>
        <SheetRope rig={sheet} beta={pose.beta} lSheet={lSheet} ropeLength={ropeLength} />
        <circle data-testid="boat-mast" cx={rig.mastX} cy={0} r={0.12} fill="#2b3a45" />
        <line
          data-testid="boat-rudder"
          x1={rudder.x1}
          y1={rudder.y1}
          x2={rudder.x2}
          y2={rudder.y2}
          stroke="#2b3a45"
          strokeWidth={3}
          vectorEffect="non-scaling-stroke"
        />
      </g>
    </svg>
  )
}
