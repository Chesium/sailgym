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
import {
  isRecorded,
  NOT_RECORDED,
  type DiagLoad,
  type DiagVec2,
  type DiagVec3,
  type Diagnostics,
  type DiagnosticsSample,
  type PartialDiagnostics,
} from '../sim/diagnostics'
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
 * rotates, and that is all (F8).
 *
 * ## An overlay whose numbers were not recorded is not drawn
 *
 * v2 section 10 hands this component a {@link DiagnosticsSample} rather than a
 * live record, and in replay that sample is an episode's own. An overlay needs
 * **every** quantity it draws from — a vector needs its components *and* its
 * application point — so {@link resolveOverlays} marks one unavailable the
 * moment any of them is missing. An unavailable overlay draws nothing and its
 * legend row reads `Not recorded`: a force arrow at a guessed origin, or drawn
 * from the live simulation behind a recorded boat, is exactly the mixed
 * timeline RV57 names and the re-simulation RV59 forbids. In particular the *horizontal* components of
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
  diagnostics: DiagnosticsSample | null
  enabled: Record<OverlayKey, boolean>
  /** Newtons per pixel when `auto` is off. */
  newtonsPerPixel: number
  auto: boolean
}

/** One drawable thing, resolved from the diagnostics record. */
export interface ResolvedOverlay {
  def: OverlayDef
  /**
   * The episode being inspected does not carry everything this overlay needs.
   * Nothing is drawn and the legend says so; `value` is meaningless and is
   * held at zero purely so the shape stays uniform.
   */
  unavailable: boolean
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
export function resolveOverlays(d: PartialDiagnostics): ResolvedOverlay[] {
  /** An overlay every one of whose inputs was recorded. */
  const ok = (r: Omit<ResolvedOverlay, 'unavailable'>): ResolvedOverlay => ({
    ...r,
    unavailable: false,
  })
  /** An overlay the episode does not carry. Nothing is drawn for it. */
  const gone = (def: OverlayDef): ResolvedOverlay => ({
    def,
    unavailable: true,
    value: 0,
    at: ORIGIN,
  })
  /** `ok(build(...))` when every named key is present, `gone(def)` otherwise. */
  function when<K extends keyof Diagnostics>(
    def: OverlayDef,
    keys: readonly K[],
    build: (v: Required<Pick<Diagnostics, K>>) => Omit<ResolvedOverlay, 'unavailable' | 'def'>,
  ): ResolvedOverlay {
    for (const k of keys) {
      if (!isRecorded(d, k)) {
        return gone(def)
      }
    }
    return ok({ def, ...build(d as Required<Pick<Diagnostics, K>>) })
  }

  const at = (l: DiagLoad) => l.r
  return [
    when(OVERLAYS_BY_KEY.trueWind, ['true_wind_body'], (v) => ({
      value: magnitude(v.true_wind_body),
      at: ORIGIN,
      vector: v.true_wind_body,
    })),
    when(
      OVERLAYS_BY_KEY.apparentWind,
      ['apparent_wind_speed', 'apparent_wind_body'],
      (v) => ({
        value: v.apparent_wind_speed,
        at: ORIGIN,
        vector: { x: v.apparent_wind_body.x, y: v.apparent_wind_body.y },
      }),
    ),
    when(OVERLAYS_BY_KEY.velocity, ['velocity_body'], (v) => ({
      value: magnitude(v.velocity_body),
      at: ORIGIN,
      vector: v.velocity_body,
    })),
    when(OVERLAYS_BY_KEY.acceleration, ['acceleration_body'], (v) => ({
      value: magnitude(v.acceleration_body),
      at: ORIGIN,
      vector: v.acceleration_body,
    })),
    when(OVERLAYS_BY_KEY.sailForce, ['sail'], (v) => ({
      value: magnitude(v.sail.f),
      at: at(v.sail),
      vector: horizontal(v.sail),
    })),
    when(OVERLAYS_BY_KEY.boardForce, ['board'], (v) => ({
      value: magnitude(v.board.f),
      at: at(v.board),
      vector: horizontal(v.board),
    })),
    when(OVERLAYS_BY_KEY.rudderForce, ['rudder'], (v) => ({
      value: magnitude(v.rudder.f),
      at: at(v.rudder),
      vector: horizontal(v.rudder),
    })),
    when(OVERLAYS_BY_KEY.hullForce, ['hull'], (v) => ({
      value: magnitude(v.hull.f),
      at: ORIGIN,
      vector: horizontal(v.hull),
    })),
    when(OVERLAYS_BY_KEY.totalForce, ['total_force_h'], (v) => ({
      value: magnitude(v.total_force_h),
      at: ORIGIN,
      vector: v.total_force_h,
    })),
    when(OVERLAYS_BY_KEY.yawMoment, ['yaw_moment'], (v) => ({
      value: v.yaw_moment,
      at: ORIGIN,
      moment: v.yaw_moment,
    })),
    when(OVERLAYS_BY_KEY.heelingMoment, ['heeling_moment'], (v) => ({
      value: v.heeling_moment,
      at: ORIGIN,
      moment: v.heeling_moment,
    })),
    when(OVERLAYS_BY_KEY.rightingMoment, ['righting_moment'], (v) => ({
      value: v.righting_moment,
      at: ORIGIN,
      moment: v.righting_moment,
    })),
    when(OVERLAYS_BY_KEY.sailCe, ['sail_ce_b'], (v) => ({
      value: v.sail_ce_b.x,
      at: v.sail_ce_b,
    })),
    when(
      OVERLAYS_BY_KEY.foilCentres,
      ['board_centre_b', 'rudder_centre_b'],
      (v) => ({
        value: v.board_centre_b.x,
        at: v.board_centre_b,
        also: v.rudder_centre_b,
      }),
    ),
    // The boom's rate is an angular velocity about the mast, so it is drawn
    // the way the moments are: an arc, at the mast rather than at the CG.
    when(OVERLAYS_BY_KEY.boomRate, ['beta_dot', 'sheet_attach_b'], (v) => ({
      value: v.beta_dot,
      at: v.sheet_attach_b,
      moment: v.beta_dot,
    })),
    when(
      OVERLAYS_BY_KEY.sheetTension,
      ['sheet_tension', 'sheet_attach_b', 'sheet'],
      (v) => ({
        value: v.sheet_tension,
        at: v.sheet_attach_b,
        vector: horizontal(v.sheet),
      }),
    ),
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
      .filter((r) => enabled[r.def.key] && !r.unavailable && r.def.scale === scale)
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

  const resolved = resolveOverlays(diagnostics.values)
  const scales = overlayScales(resolved, enabled, newtonsPerPixel, auto)
  const theta = boatToScreenAngle(camera.rotation, pose.psi)
  const cg = camera.worldToScreen({ x: pose.x, y: pose.y })
  const point = (p: DiagVec3) => boatPointToScreen(cg, p.x, p.y, theta, camera.scale)

  const active = resolved.filter((r) => enabled[r.def.key])
  const missing = active.filter((r) => r.unavailable).length

  return (
    <svg
      data-testid="force-overlay"
      data-active={active.length}
      data-unavailable={missing}
      data-source={diagnostics.source}
      data-sample-t={diagnostics.t}
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

        if (r.unavailable) {
          // The group is still mounted — toggling an overlay must change the
          // DOM by the same amount whether or not the episode recorded it —
          // but nothing is drawn, because there is nothing to draw.
          return <g key={id} data-testid={id} data-empty="true" data-unavailable="true" />
        }

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
  diagnostics: DiagnosticsSample | null
  enabled: Record<OverlayKey, boolean>
  newtonsPerPixel: number
  auto: boolean
}) {
  if (diagnostics === null) {
    return null
  }
  const resolved = resolveOverlays(diagnostics.values)
  const scales = overlayScales(resolved, enabled, newtonsPerPixel, auto)
  const active = resolved.filter((r) => enabled[r.def.key])
  const missing = active.filter((r) => r.unavailable).length
  return (
    <div
      data-testid="overlay-legend"
      data-count={active.length}
      data-unavailable={missing}
      data-source={diagnostics.source}
      data-sample-t={diagnostics.t}
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
          data-recorded={r.unavailable ? 'false' : 'true'}
          data-value={r.unavailable ? '' : r.value}
          style={{ display: 'flex', gap: 6, alignItems: 'center' }}
        >
          <span
            aria-hidden
            style={{
              width: 10,
              height: 10,
              background: r.unavailable ? '#ccd' : r.def.colour,
              display: 'inline-block',
            }}
          />
          <span style={{ flex: 1 }}>{r.def.label}</span>
          <span style={{ color: r.unavailable ? '#889' : undefined }}>
            {r.unavailable ? NOT_RECORDED : `${r.value.toFixed(2)} ${r.def.unit}`}
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
