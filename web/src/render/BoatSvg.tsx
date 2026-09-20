import { useLayoutEffect, useMemo, useRef } from 'react'

import { boatTransform, worldTransform, type Camera, type Vec2 } from './Camera'
import { type HullDims } from './geometry'
import { hullSolid } from './hullSolid'
import type { BoatGroup, Material, Vec3 } from './model3d'
import {
  isTri,
  projectModel,
  rollToH,
  type DrawItem,
  type DrawLine,
  type DrawTri,
} from './project3d'
import { boatModel } from './rigSolid'
import { sailCorners, sailEdges } from './SailShape'
import { SheetRope, type SheetRigDims } from './SheetRope'
import { visualDims } from './visualDims'
import { Trajectory } from './Trajectory'
import type { SheetEvent } from '../sim/sheetInput'
import type { RenderParams } from '../sim/useSimulation'
import { endSpanFrom, noteRudderRendered } from './perfMarks'

/** Boat pose, straight off the snapshot. */
export interface BoatPose {
  x: number
  y: number
  psi: number
  /** rad, roll (F3). Never wrapped, so this may exceed ±π. */
  phi: number
  beta: number
  deltaR: number
}

export interface BoatSvgProps {
  camera: Camera
  pose: BoatPose
  alpha: number
  /** The F7 catalogue, as `parameters_json()` shapes it. */
  params: RenderParams
  hull: HullDims
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

/**
 * Flat material colours, matching the v1 palette.
 *
 * **There is no lighting term.** Heel reads from the silhouette and from which
 * materials are on show, not from shading — a later section may add shading,
 * and it is far easier to add to a depth sort already proven correct than to
 * debug through one.
 */
const MATERIALS: Record<Material, { fill: string; stroke: string; opacity?: number }> = {
  deck: { fill: '#fdfdfd', stroke: '#2b3a45' },
  topsides: { fill: '#e8eef2', stroke: '#2b3a45' },
  bilge: { fill: '#cfdbe3', stroke: '#2b3a45' },
  underside: { fill: '#b9c7d1', stroke: '#2b3a45' },
  sail: { fill: '#d64d3f', stroke: '#d64d3f', opacity: 0.22 },
  foil: { fill: '#7a8a96', stroke: '#5d6b76' },
}

/** The three thin-plate groups the DOM contract requires to be single elements. */
const PLATE_GROUPS: readonly BoatGroup[] = ['sail', 'board', 'rudder']

const pointsOf = (t: DrawTri) => t.points.map((p) => `${p.x},${p.y}`).join(' ')

function Face({ tri, index }: { tri: DrawTri; index: number }) {
  const m = MATERIALS[tri.material]
  return (
    <polygon
      key={index}
      data-material={tri.material}
      points={pointsOf(tri)}
      fill={m.fill}
      fillOpacity={m.opacity}
      stroke={m.stroke}
      strokeWidth={0.8}
      strokeLinejoin="round"
      vectorEffect="non-scaling-stroke"
    />
  )
}

/**
 * The boat, drawn as 3-D geometry projected into the top-down SVG
 * (v2 section 01).
 *
 * The model is built once per rig pose in `B` — the boom's `β` and the
 * rudder's `δr` baked in — and then rolled into `H` by the live `φ` and
 * depth-sorted, every frame. Yaw and the camera are still
 * `Camera.boatTransform`'s job, on the `boat-hull` group, exactly as they were
 * in v1: three E2E specs read that group's screen CTM.
 *
 * ## Why the sail, board and rudder are single groups
 *
 * `projectModel` returns a fully, globally depth-sorted draw list, and that is
 * the list `tests/unit/boat3d.test.ts` measures against a ray reference. SVG
 * paints in document order, so a `<g data-testid="boat-sail">` containing all
 * the sail's polygons necessarily paints them contiguously — the DOM cannot
 * express a global sort *and* keep those three handles, and the handles are
 * what four existing specs and the browser checks address. Each plate group is
 * therefore emitted as one `<g>`, placed at the position of its **lowest**
 * member in the sorted order, which is the choice that keeps the board under
 * the hull when the boat is upright and the sail over it. The residual is
 * recorded as a debt in `docs/v2/progress/01-handoff.md`, next to the one the
 * PRD already accepts for the mast and boom lines.
 */
export function BoatSvg({
  camera,
  pose,
  alpha,
  params,
  hull,
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

  // Task 10.5's `svg` span, and the far end of its input-lag measurement.
  //
  // The start is here, at the top of the render, and the end is in the layout
  // effect below — which React runs after this subtree has been committed to
  // the DOM — so the span covers render **and** commit rather than only the
  // component function. The effect has no dependency array on purpose: it must
  // run on every commit, because every commit is a frame the rudder could have
  // moved in.
  const renderStartedAt = typeof performance === 'undefined' ? 0 : performance.now()
  const deltaR = pose.deltaR
  useLayoutEffect(() => {
    endSpanFrom('svg', renderStartedAt)
    noteRudderRendered(deltaR)
  })

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
        // Middle button, or Shift + any button, pans. A plain left-drag with a
        // mouse or a pen is the mainsheet; a **finger** is neither — trimming
        // by touch belongs to the pads of `ui/TouchControls.tsx`, and
        // `sheetInput` drops it (v2 section 09, task 9.3).
        onSheet({
          type: 'down',
          y: e.clientY,
          button: e.button,
          shiftKey: e.shiftKey,
          pointerType: e.pointerType,
        })
        const panning = e.button === 1 || e.shiftKey
        const trimming = !panning && e.button === 0 && e.pointerType !== 'touch'
        // Capture, and suppress the browser's own gesture, **only** once this
        // view owns the pointer. A finger that owns nothing here is left alone,
        // so the page can still be scrolled and a later camera gesture is not
        // swallowed by an element that was not using the pointer.
        if (panning || trimming) {
          e.currentTarget.setPointerCapture(e.pointerId)
        }
        if (panning) {
          e.preventDefault()
          dragging.current = { x: e.clientX, y: e.clientY }
        }
      }}
      onPointerMove={(e) => {
        onSheet({
          type: 'move',
          y: e.clientY,
          button: e.button,
          shiftKey: e.shiftKey,
          pointerType: e.pointerType,
        })
        const from = dragging.current
        if (from === null) {
          return
        }
        onPan(e.clientX - from.x, e.clientY - from.y)
        dragging.current = { x: e.clientX, y: e.clientY }
      }}
      onPointerUp={(e) => {
        onSheet({
          type: 'up',
          y: e.clientY,
          button: e.button,
          shiftKey: e.shiftKey,
          pointerType: e.pointerType,
        })
        dragging.current = null
        if (e.currentTarget.hasPointerCapture(e.pointerId)) {
          e.currentTarget.releasePointerCapture(e.pointerId)
        }
      }}
      onPointerCancel={(e) => {
        onSheet({ type: 'cancel', y: e.clientY, pointerType: e.pointerType })
        dragging.current = null
      }}
      onLostPointerCapture={(e) => {
        // The browser took the pointer away — a system gesture, a scroll
        // taking over. Same ending as a cancel: the drag is over and the
        // command goes back to nothing (RV53).
        onSheet({ type: 'cancel', y: e.clientY, pointerType: e.pointerType })
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
        <BoatFigure
          params={params}
          hull={hull}
          phi={pose.phi}
          beta={pose.beta}
          deltaR={pose.deltaR}
          alpha={alpha}
        />
        <SheetRope
          rig={sheet}
          beta={pose.beta}
          phi={pose.phi}
          lSheet={lSheet}
          ropeLength={ropeLength}
        />
      </g>
    </svg>
  )
}


export interface BoatFigureProps {
  params: RenderParams
  hull: HullDims
  /** rad, roll (F3). Never wrapped. */
  phi: number
  beta: number
  deltaR: number
  alpha: number
  /**
   * Prefixes every `data-testid` this subtree emits.
   *
   * The fixed-angle probes render a second, third and fourth copy of the boat
   * on the same page, and a bare `boat-boom` would then match five elements
   * and break every spec that addresses it. Same device, and same reason, as
   * `HeelIndicator`'s `testId` override.
   */
  prefix?: string
}

/**
 * The boat itself: model, projection, draw list.
 *
 * Separate from {@link BoatSvg} so the fixed-angle probes can render exactly
 * the same geometry through exactly the same code path, with only `φ` pinned.
 */
export function BoatFigure({
  params,
  hull,
  phi,
  beta,
  deltaR,
  alpha,
  prefix = '',
}: BoatFigureProps) {
  // The model is *geometry*, not a frame: it depends on the parameters and on
  // the rig's articulation, and not at all on `φ`. Only the projection runs
  // every frame (RV4).
  const dims = useMemo(() => visualDims(params), [params])
  const hullTris = useMemo(() => hullSolid(hull, dims), [hull, dims])
  const model = useMemo(
    () => boatModel(params, hull, dims, { beta, deltaR, alpha }, hullTris),
    [params, hull, dims, beta, deltaR, alpha, hullTris],
  )
  const edges = useMemo(() => {
    const corners = sailCorners(
      {
        mastX: params.sail.mast_pos_b.x,
        boomLength: params.sail.boom_length,
        zBoom: params.sheet.z_boom,
        mastHeight: dims.mastHeight,
      },
      beta,
    )
    return sailEdges(corners, alpha)
  }, [params, dims, beta, alpha])

  return <>{drawItems(projectModel(model, phi), edges, phi, prefix)}</>
}

/**
 * The sorted draw list as SVG, with the three plate groups collapsed into
 * single elements at the position of their lowest member.
 */
function drawItems(
  items: readonly DrawItem[],
  edges: readonly Vec3[][],
  phi: number,
  prefix: string,
): React.ReactNode[] {
  const firstOf = new Map<BoatGroup, number>()
  items.forEach((item, i) => {
    if (!isTri(item) || !PLATE_GROUPS.includes(item.group)) {
      return
    }
    if (!firstOf.has(item.group)) {
      firstOf.set(item.group, i)
    }
  })

  const nodes: React.ReactNode[] = []
  items.forEach((item, i) => {
    if (!isTri(item)) {
      nodes.push(<Rope key={`l${i}`} line={item} index={i} prefix={prefix} />)
      return
    }
    if (!PLATE_GROUPS.includes(item.group)) {
      nodes.push(<Face key={`t${i}`} tri={item} index={i} />)
      return
    }
    if (firstOf.get(item.group) !== i) {
      return
    }
    const faces = items.filter((o): o is DrawTri => isTri(o) && o.group === item.group)
    nodes.push(
      <g key={`g${i}`} data-testid={`${prefix}boat-${item.group}`}>
        {faces.map((f, k) => (
          <Face key={k} tri={f} index={k} />
        ))}
        {item.group === 'sail' &&
          edges.map((edge, k) => (
            <polyline
              key={`e${k}`}
              points={edge
                .map((p) => rollToH(p, phi))
                .map((p) => `${p.x},${p.y}`)
                .join(' ')}
              fill="none"
              stroke={MATERIALS.sail.stroke}
              strokeWidth={2}
              vectorEffect="non-scaling-stroke"
            />
          ))}
      </g>,
    )
  })
  return nodes
}

/**
 * A `Line3`, still a `<line>`.
 *
 * The boom keeps its `<g data-testid="boom">` wrapper — a useful handle — but
 * **no** `transform`: `β` is baked into the projected endpoints now, so there
 * is no honest transform left to emit, and emitting a decorative `rotate(0)`
 * so a spec keeps passing would be exactly the test-shaped fiction F13.4
 * forbids. Task 1.7 moves that assertion onto the line's own endpoints, which
 * is what it was really trying to observe.
 */
function Rope({
  line,
  index,
  prefix,
}: {
  line: DrawLine
  index: number
  prefix: string
}) {
  const element = (
    <line
      key={index}
      data-testid={`${prefix}${line.testId}`}
      x1={line.a.x}
      y1={line.a.y}
      x2={line.b.x}
      y2={line.b.y}
      stroke={line.colour}
      strokeWidth={line.widthPx}
      strokeLinecap="round"
      vectorEffect="non-scaling-stroke"
    />
  )
  if (line.testId === 'boat-boom') {
    return <g data-testid={`${prefix}boom`}>{element}</g>
  }
  return element
}
