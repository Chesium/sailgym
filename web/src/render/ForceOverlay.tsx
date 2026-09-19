import { memo } from 'react'

import type { Camera } from './Camera'
import {
  arrowHead,
  autoScale,
  boatPointToScreen,
  boatToScreenAngle,
  boatVectorToScreen,
  MIN_DRAWN_PX,
  momentArc,
  pixelsFor,
  type ScreenVec,
} from './vectorScale'
import type { DiagLoad, DiagVec2, DiagVec3, Diagnostics } from '../sim/diagnostics'
import { OVERLAY_KEYS, type OverlayKey } from '../ui/store'

/**
 * brief §30's visual list, drawn over the world view (task 8.3).
 *
 * Sixteen independently toggleable overlays, each drawn at its application
 * point, each in its own colour, and each listed in the legend with the
 * number it is drawing — so the picture and the figure can never disagree.
 *
 * ## No physics here
 *
 * Every quantity comes out of the diagnostics record; this file scales and
 * rotates, and that is all (F8). In particular the *horizontal* components of
 * a boat-fixed load are drawn as they arrive: the top-down view has never
 * shown heel (brief §26 gives heel its own stern view, and `geometry.ts`
 * places the mast, board and rudder by their `x` alone), so applying `R_x(φ)`
 * here would be inventing a projection the rest of the drawing does not use —
 * and frame conversions live in `frames.rs` and nowhere else (F2).
 *
 * ## SVG is the right renderer for this
 *
 * brief §39 forbids SVG only for the *dense wind field*, which is deck.gl's
 * job and stays deck.gl's job. This is at most a few dozen elements with a
 * bounded count, and it needs per-element `data-*` for the browser tests to
 * read (brief §42).
 */

export type OverlayKind = 'vector' | 'moment' | 'point'

/**
 * The four independent scales. Angular *rate* is its own category rather than
 * sharing the moment scale: one is rad/s and the other N·m, and letting a
 * 500 N·m moment set the scale for a 1 rad/s boom would draw the boom's arc
 * at the minimum radius forever.
 */
export type ScaleKind = 'force' | 'moment' | 'velocity' | 'rate'

export interface OverlayDef {
  key: OverlayKey
  label: string
  kind: OverlayKind
  colour: string
  /** N, N·m or m/s — whichever the quantity is measured in. */
  unit: string
  /** Which shared scale the overlay is drawn at. */
  scale: ScaleKind
}

export const OVERLAYS: readonly OverlayDef[] = [
  { key: 'trueWind', label: 'True wind', kind: 'vector', colour: '#2f6f9f', unit: 'm/s', scale: 'velocity' },
  { key: 'apparentWind', label: 'Apparent wind', kind: 'vector', colour: '#5ab4e0', unit: 'm/s', scale: 'velocity' },
  { key: 'velocity', label: 'Boat velocity', kind: 'vector', colour: '#2b8a3e', unit: 'm/s', scale: 'velocity' },
  { key: 'acceleration', label: 'Acceleration', kind: 'vector', colour: '#74c476', unit: 'm/s²', scale: 'velocity' },
  { key: 'sailForce', label: 'Sail force', kind: 'vector', colour: '#d64d3f', unit: 'N', scale: 'force' },
  { key: 'boardForce', label: 'Centreboard force', kind: 'vector', colour: '#8c6bb1', unit: 'N', scale: 'force' },
  { key: 'rudderForce', label: 'Rudder force', kind: 'vector', colour: '#d9a02b', unit: 'N', scale: 'force' },
  { key: 'hullForce', label: 'Hull force', kind: 'vector', colour: '#7a8a96', unit: 'N', scale: 'force' },
  { key: 'totalForce', label: 'Total force', kind: 'vector', colour: '#1b1b1b', unit: 'N', scale: 'force' },
  { key: 'yawMoment', label: 'Yaw moment', kind: 'moment', colour: '#c2571a', unit: 'N·m', scale: 'moment' },
  { key: 'heelingMoment', label: 'Heeling moment', kind: 'moment', colour: '#b03030', unit: 'N·m', scale: 'moment' },
  { key: 'rightingMoment', label: 'Righting moment', kind: 'moment', colour: '#2b6fb0', unit: 'N·m', scale: 'moment' },
  { key: 'sailCe', label: 'Sail centre of effort', kind: 'point', colour: '#d64d3f', unit: 'm', scale: 'force' },
  { key: 'foilCentres', label: 'Board / rudder centres', kind: 'point', colour: '#8c6bb1', unit: 'm', scale: 'force' },
  { key: 'boomRate', label: 'Boom angular velocity', kind: 'moment', colour: '#7b5d34', unit: 'rad/s', scale: 'rate' },
  { key: 'sheetTension', label: 'Sheet tension', kind: 'vector', colour: '#0f8b8d', unit: 'N', scale: 'force' },
]

/** The definitions, in the store's key order, so the two cannot drift apart. */
export const OVERLAYS_BY_KEY: Record<OverlayKey, OverlayDef> = Object.fromEntries(
  OVERLAYS.map((o) => [o.key, o]),
) as Record<OverlayKey, OverlayDef>

/** Units per pixel, one per {@link ScaleKind}. */
export type OverlayScales = Record<ScaleKind, number>

export interface ForceOverlayProps {
  camera: Camera
  /** Boat world position and heading, straight off the snapshot. */
  pose: { x: number; y: number; psi: number }
  diagnostics: Diagnostics | null
  enabled: Record<OverlayKey, boolean>
  /** Newtons per pixel when `auto` is off. */
  newtonsPerPixel: number
  auto: boolean
}

/** One drawable thing, resolved from the diagnostics record. */
export interface ResolvedOverlay {
  def: OverlayDef
  /** The number the legend prints. */
  value: number
  /** Application point in `B`, metres — for vectors and points. */
  at: DiagVec3
  /** Boat-frame components, for a vector. */
  vector?: DiagVec2
  /** Signed magnitude, for a moment. */
  moment?: number
  /** A second point, for the pair of foil centres. */
  also?: DiagVec3
}

const ORIGIN: DiagVec3 = { x: 0, y: 0, z: 0 }

function magnitude(v: DiagVec2 | DiagVec3): number {
  return Math.hypot(v.x, v.y)
}

function horizontal(load: DiagLoad): DiagVec2 {
  return { x: load.f.x, y: load.f.y }
}

/**
 * Every overlay's current numbers, whether or not it is switched on.
 *
 * Resolved for all sixteen so the legend, the auto scale and the drawing all
 * read one table; the caller filters by `enabled`.
 */
export function resolveOverlays(d: Diagnostics): ResolvedOverlay[] {
  const at = (l: DiagLoad) => l.r
  return [
    { def: OVERLAYS_BY_KEY.trueWind, value: magnitude(d.true_wind_body), at: ORIGIN, vector: d.true_wind_body },
    {
      def: OVERLAYS_BY_KEY.apparentWind,
      value: d.apparent_wind_speed,
      at: ORIGIN,
      vector: { x: d.apparent_wind_body.x, y: d.apparent_wind_body.y },
    },
    { def: OVERLAYS_BY_KEY.velocity, value: magnitude(d.velocity_body), at: ORIGIN, vector: d.velocity_body },
    {
      def: OVERLAYS_BY_KEY.acceleration,
      value: magnitude(d.acceleration_body),
      at: ORIGIN,
      vector: d.acceleration_body,
    },
    { def: OVERLAYS_BY_KEY.sailForce, value: magnitude(d.sail.f), at: at(d.sail), vector: horizontal(d.sail) },
    { def: OVERLAYS_BY_KEY.boardForce, value: magnitude(d.board.f), at: at(d.board), vector: horizontal(d.board) },
    { def: OVERLAYS_BY_KEY.rudderForce, value: magnitude(d.rudder.f), at: at(d.rudder), vector: horizontal(d.rudder) },
    { def: OVERLAYS_BY_KEY.hullForce, value: magnitude(d.hull.f), at: ORIGIN, vector: horizontal(d.hull) },
    {
      def: OVERLAYS_BY_KEY.totalForce,
      value: magnitude(d.total_force_h),
      at: ORIGIN,
      vector: d.total_force_h,
    },
    { def: OVERLAYS_BY_KEY.yawMoment, value: d.yaw_moment, at: ORIGIN, moment: d.yaw_moment },
    { def: OVERLAYS_BY_KEY.heelingMoment, value: d.heeling_moment, at: ORIGIN, moment: d.heeling_moment },
    { def: OVERLAYS_BY_KEY.rightingMoment, value: d.righting_moment, at: ORIGIN, moment: d.righting_moment },
    { def: OVERLAYS_BY_KEY.sailCe, value: d.sail_ce_b.x, at: d.sail_ce_b },
    {
      def: OVERLAYS_BY_KEY.foilCentres,
      value: d.board_centre_b.x,
      at: d.board_centre_b,
      also: d.rudder_centre_b,
    },
    // The boom's rate is an angular velocity about the mast, so it is drawn
    // the way the moments are: an arc, at the mast rather than at the CG.
    { def: OVERLAYS_BY_KEY.boomRate, value: d.beta_dot, at: d.sheet_attach_b, moment: d.beta_dot },
    {
      def: OVERLAYS_BY_KEY.sheetTension,
      value: d.sheet_tension,
      at: d.sheet_attach_b,
      vector: horizontal(d.sheet),
    },
  ]
}

/** The scale each category is drawn at, given what is currently switched on. */
export function overlayScales(
  resolved: readonly ResolvedOverlay[],
  enabled: Record<OverlayKey, boolean>,
  newtonsPerPixel: number,
  auto: boolean,
): OverlayScales {
  const largest = (scale: ScaleKind) =>
    resolved
      .filter((r) => enabled[r.def.key] && r.def.scale === scale)
      .reduce((m, r) => Math.max(m, Math.abs(r.value)), 0)
  return {
    force: auto ? autoScale(largest('force')) : newtonsPerPixel,
    moment: autoScale(largest('moment')),
    velocity: autoScale(largest('velocity')),
    rate: autoScale(largest('rate')),
  }
}

function Vector({
  id,
  def,
  origin,
  offset,
}: {
  id: string
  def: OverlayDef
  origin: ScreenVec
  offset: ScreenVec
}) {
  const tip = { x: origin.x + offset.x, y: origin.y + offset.y }
  const head = arrowHead(tip, offset.x, offset.y)
  return (
    <g data-testid={id} stroke={def.colour} fill={def.colour}>
      <line x1={origin.x} y1={origin.y} x2={tip.x} y2={tip.y} strokeWidth={2.5} />
      {head !== '' && <polygon points={head} stroke="none" />}
    </g>
  )
}

export function ForceOverlay({
  camera,
  pose,
  diagnostics,
  enabled,
  newtonsPerPixel,
  auto,
}: ForceOverlayProps) {
  const { width, height } = camera.viewport
  if (diagnostics === null) {
    return null
  }

  const resolved = resolveOverlays(diagnostics)
  const scales = overlayScales(resolved, enabled, newtonsPerPixel, auto)
  const theta = boatToScreenAngle(camera.rotation, pose.psi)
  const cg = camera.worldToScreen({ x: pose.x, y: pose.y })
  const point = (p: DiagVec3) => boatPointToScreen(cg, p.x, p.y, theta, camera.scale)

  const active = resolved.filter((r) => enabled[r.def.key])

  return (
    <svg
      data-testid="force-overlay"
      data-active={active.length}
      data-force-scale={scales.force}
      data-moment-scale={scales.moment}
      data-velocity-scale={scales.velocity}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      style={{
        position: 'absolute',
        inset: 0,
        // The overlay is a read-out, not a control: the mainsheet drag and the
        // camera pan must still reach the boat SVG underneath it.
        pointerEvents: 'none',
        zIndex: 2,
      }}
    >
      {active.map((r) => {
        const id = `overlay-${r.def.key}`
        const origin = point(r.at)

        if (r.vector !== undefined) {
          const perPixel = scales[r.def.scale]
          const length = pixelsFor(magnitude(r.vector), perPixel)
          if (!(length > MIN_DRAWN_PX)) {
            // Still mount the group, so a toggle always changes the DOM by the
            // same amount whether or not the quantity happens to be zero.
            return <g key={id} data-testid={id} data-empty="true" />
          }
          const offset = boatVectorToScreen(r.vector.x, r.vector.y, theta, 1 / perPixel)
          return <Vector key={id} id={id} def={r.def} origin={origin} offset={offset} />
        }

        if (r.moment !== undefined) {
          const arc = momentArc(origin, r.moment, scales[r.def.scale])
          if (arc === null) {
            return <g key={id} data-testid={id} data-empty="true" />
          }
          const head = arrowHead(arc.tip, arc.tangent.x, arc.tangent.y)
          return (
            <g key={id} data-testid={id} stroke={r.def.colour} fill="none">
              <path d={arc.path} strokeWidth={2} strokeDasharray="6 4" />
              {head !== '' && <polygon points={head} fill={r.def.colour} stroke="none" />}
            </g>
          )
        }

        return (
          <g key={id} data-testid={id} stroke={r.def.colour} fill="none">
            <circle cx={origin.x} cy={origin.y} r={5} strokeWidth={2} />
            {r.also !== undefined && (
              <circle cx={point(r.also).x} cy={point(r.also).y} r={5} strokeWidth={2} />
            )}
          </g>
        )
      })}
    </svg>
  )
}

/**
 * The legend: one row per active overlay, carrying the number that overlay is
 * drawing.
 *
 * brief §30 asks for the values to be inspectable numerically "where
 * practical". Putting the number beside the arrow, from the same resolved
 * table the arrow is drawn from, is what makes the two impossible to
 * disagree.
 */
export function OverlayLegend({
  diagnostics,
  enabled,
  newtonsPerPixel,
  auto,
}: {
  diagnostics: Diagnostics | null
  enabled: Record<OverlayKey, boolean>
  newtonsPerPixel: number
  auto: boolean
}) {
  if (diagnostics === null) {
    return null
  }
  const resolved = resolveOverlays(diagnostics)
  const scales = overlayScales(resolved, enabled, newtonsPerPixel, auto)
  const active = resolved.filter((r) => enabled[r.def.key])
  return (
    <div
      data-testid="overlay-legend"
      data-count={active.length}
      style={{ display: 'grid', gap: 2, fontVariantNumeric: 'tabular-nums' }}
    >
      <div style={{ color: '#667' }}>
        scale {scales.force.toPrecision(3)} N/px · {scales.moment.toPrecision(3)} N·m/px ·{' '}
        {scales.velocity.toPrecision(3)} (m/s)/px{auto ? ' (auto)' : ''}
      </div>
      {active.map((r) => (
        <div
          key={r.def.key}
          data-testid={`legend-${r.def.key}`}
          data-value={r.value}
          style={{ display: 'flex', gap: 6, alignItems: 'center' }}
        >
          <span
            aria-hidden
            style={{ width: 10, height: 10, background: r.def.colour, display: 'inline-block' }}
          />
          <span style={{ flex: 1 }}>{r.def.label}</span>
          <span>
            {r.value.toFixed(2)} {r.def.unit}
          </span>
        </div>
      ))}
    </div>
  )
}

/**
 * The sixteen toggles, plus the shared scale control (task 8.3).
 */
export interface OverlayControlsProps {
  enabled: Record<OverlayKey, boolean>
  onToggle: (key: OverlayKey, on: boolean) => void
  onAll: (on: boolean) => void
  newtonsPerPixel: number
  onNewtonsPerPixel: (value: number) => void
  auto: boolean
  onAuto: (on: boolean) => void
}

/** Memoised: sixteen checkboxes that change only when someone clicks one. */
export const OverlayControls = memo(function OverlayControls({
  enabled,
  onToggle,
  onAll,
  newtonsPerPixel,
  onNewtonsPerPixel,
  auto,
  onAuto,
}: OverlayControlsProps) {
  const count = OVERLAY_KEYS.filter((k) => enabled[k]).length
  return (
    <section
      data-testid="overlay-controls"
      data-on={count}
      style={{ border: '1px solid #ccd', borderRadius: 4, padding: 8 }}
    >
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 6 }}>
        <strong style={{ flex: 1 }}>Overlays ({count}/{OVERLAY_KEYS.length})</strong>
        <button type="button" data-testid="overlays-all-on" onClick={() => onAll(true)}>
          All
        </button>
        <button type="button" data-testid="overlays-all-off" onClick={() => onAll(false)}>
          None
        </button>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1px 8px' }}>
        {OVERLAYS.map((def) => (
          <label key={def.key} style={{ display: 'flex', gap: 5, alignItems: 'center' }}>
            <input
              type="checkbox"
              data-testid={`toggle-${def.key}`}
              checked={enabled[def.key]}
              onChange={(e) => onToggle(def.key, e.target.checked)}
            />
            <span
              aria-hidden
              style={{ width: 9, height: 9, background: def.colour, display: 'inline-block' }}
            />
            {def.label}
          </label>
        ))}
      </div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginTop: 6 }}>
        <label style={{ display: 'flex', gap: 4, alignItems: 'center' }}>
          <input
            type="checkbox"
            data-testid="overlay-auto-scale"
            checked={auto}
            onChange={(e) => onAuto(e.target.checked)}
          />
          auto scale
        </label>
        <label style={{ display: 'flex', gap: 4, alignItems: 'center', opacity: auto ? 0.5 : 1 }}>
          N/px
          <input
            type="number"
            data-testid="overlay-newtons-per-pixel"
            min={0.01}
            step={0.5}
            value={newtonsPerPixel}
            disabled={auto}
            onChange={(e) => onNewtonsPerPixel(Number(e.target.value))}
            style={{ width: 80 }}
          />
        </label>
      </div>
    </section>
  )
})
