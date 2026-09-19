/**
 * The mainsheet, drawn from the boom attachment to the transom block
 * (brief §12: "the displayed rope should visibly correspond to sheet state").
 *
 * Two lengths decide what is drawn, and both come from the core: `ℓ(β)`, the
 * geometric rope path, arrives in the diagnostics record as `rope_length`, and
 * `L`, the available sheet, is `lSheet` in the F8.3 snapshot. Taut when
 * `e = ℓ − L > 0`; otherwise there is `L − ℓ` of rope to hang, and the path
 * bows by a fixed fraction of it.
 *
 * **The sag is cosmetic.** It is a top-down view, so a rope that really hangs
 * downward has to be suggested by bowing the path outboard instead. Nothing
 * computed here is fed back into the physics, and no rope geometry is derived
 * here either (F8) — the attachment point is the same `d_sheet · b̂(β)` the
 * core uses, with the parameters read from `parameters_json()`.
 */

export interface SheetRigDims {
  /** m, mast foot along `+x` from the CG */
  mastX: number
  /** m, boom attachment distance from the mast, `d_sheet` */
  dSheet: number
  /** m, block position in `B`; the view is top-down so `z` is not drawn */
  block: { x: number; y: number }
}

export interface SheetRopeProps {
  rig: SheetRigDims
  /** rad, boom angle (F2.1) */
  beta: number
  /** m, available sheet length `L` — snapshot field `lSheet` */
  lSheet: number
  /** m, geometric rope path `ℓ(β)` — diagnostics field `rope_length` */
  ropeLength: number
}

/** How much of the spare rope shows as bow in the path. Cosmetic only. */
const SAG_FRACTION = 0.35

/**
 * Boom attachment in `B`, from `b̂(β) = (−cos β, −sin β)` (F2.1). The same
 * expression as `geometry.boomDirection`, at `d_sheet` rather than the clew.
 */
export function attachPoint(rig: SheetRigDims, beta: number): { x: number; y: number } {
  return {
    x: rig.mastX - rig.dSheet * Math.cos(beta),
    y: -rig.dSheet * Math.sin(beta),
  }
}

/** The SVG path, in boat-fixed metres. Straight when taut, bowed when slack. */
export function ropePath(props: SheetRopeProps): string {
  const { rig, beta, lSheet, ropeLength } = props
  const a = attachPoint(rig, beta)
  const b = rig.block
  const slack = Math.max(0, lSheet - ropeLength)
  if (slack <= 0) {
    return `M ${a.x},${a.y} L ${b.x},${b.y}`
  }
  const dx = b.x - a.x
  const dy = b.y - a.y
  const span = Math.hypot(dx, dy)
  if (span === 0) {
    return `M ${a.x},${a.y} L ${b.x},${b.y}`
  }
  // Bow the path away from the centreline, on the side the boom is on, so the
  // spare rope reads as hanging outboard rather than crossing the hull.
  let nx = -dy / span
  let ny = dx / span
  const side = a.y === 0 ? 1 : Math.sign(a.y)
  if (ny * side < 0) {
    nx = -nx
    ny = -ny
  }
  const bow = SAG_FRACTION * slack
  const cx = (a.x + b.x) / 2 + nx * bow
  const cy = (a.y + b.y) / 2 + ny * bow
  return `M ${a.x},${a.y} Q ${cx},${cy} ${b.x},${b.y}`
}

export function SheetRope(props: SheetRopeProps) {
  const { rig, beta, lSheet, ropeLength } = props
  const slack = Math.max(0, lSheet - ropeLength)
  const block = rig.block
  const attach = attachPoint(rig, beta)
  return (
    <g data-testid="sheet">
      <path
        data-testid="sheet-rope"
        data-slack={slack}
        data-rope-length={ropeLength}
        data-l-sheet={lSheet}
        d={ropePath(props)}
        fill="none"
        stroke={slack > 0 ? '#9aa7b1' : '#38495a'}
        strokeWidth={slack > 0 ? 1.5 : 2}
        vectorEffect="non-scaling-stroke"
      />
      <circle data-testid="sheet-block" cx={block.x} cy={block.y} r={0.1} fill="#38495a" />
      <circle data-testid="sheet-attach" cx={attach.x} cy={attach.y} r={0.08} fill="#38495a" />
    </g>
  )
}
