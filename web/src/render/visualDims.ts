/**
 * Drawing dimensions for the 3-D boat model (v2 section 01, task 1.1,
 * normative delta **D2**).
 *
 * Hull depth, freeboard, sheer rise, mast height and the sail's planform are
 * not in the F7 catalogue, because nothing in the physics needs them. They are
 * therefore **not** added to `BoatParameters`; they live here, in the web
 * layer, as *dimensionless ratios* of quantities F7 does define — exactly as
 * `geometry.ts` already holds the hull outline.
 *
 * ## The two kinds of number, and why they are never mixed
 *
 * A **`VISUAL`** constant may be chosen because it looks right. A physical
 * coefficient may not (brief §43). The two must never be confused, which is
 * why they live in different languages in different crates: nothing in this
 * file crosses the F8 boundary, and no value here is ever handed back to the
 * core.
 *
 * Every exported constant below is dimensionless and carries a provenance
 * comment saying what it is worth in metres at the ILCA defaults and whether
 * it is *derived* from an F7 parameter or *assumed*. `visualDims` is the only
 * place any of them becomes a length.
 *
 * Five further dimensions are pure derivations and so are computed in
 * {@link visualDims} rather than declared here:
 *
 * - `deckZ` **derived (F7)** `= sheet.block_pos_b.z` (0.10 m). The transom
 *   block sits on the deck, so its height *is* the deck height aft.
 * - `keelLengthScale` **derived (F7)** `= hull.lwl / hull.loa` (0.901). The
 *   keel ring is the sheer outline shortened to the waterline length.
 * - `mastHeight` **derived (F7)**
 *   `= sheet.z_boom + 2 · sail.area / sail.boom_length`. See below.
 * - `boardSpan` **derived (F7)** `= board.area / boardChord`.
 * - `rudderSpan` **derived (F7)** `= rudder.area / rudderChord`.
 *
 * ## Why the mast height is a derivation and not a `VISUAL` constant
 *
 * The drawn sail is the triangle tack → head → clew, with the tack and the
 * clew both at `z_boom` and the head at the masthead, so its area is
 * `½ · (mastHeight − z_boom) · boom_length`. Setting that equal to `sail.area`
 * gives the expression above, and at the ILCA defaults it yields a shade under
 * 5.9 m — which is, to a centimetre, the real ILCA mast height. It is a
 * derivation, not a fit; do not replace it with a constant.
 */

import type { RenderParams } from '../sim/useSimulation'

/**
 * `VISUAL` — the deck rises forward, as a fraction of LOA. 0.233 m at the ILCA
 * defaults; chosen so the bow deck clears the water in the drawing.
 *
 * The rise is **linear in `x`**, which is not only a shape choice: it keeps the
 * sheer ring planar, and a planar deck cap has an outward normal with no `y`
 * component at all. That is what makes "the deck is visible iff `cos φ > 0`"
 * exact rather than approximate (see `project3d.ts`).
 */
export const SHEER_RISE_PER_LOA = 0.055

/** `VISUAL` — topside height, sheer to chine, as a fraction of beam. 0.301 m. */
export const CHINE_DROP_PER_BEAM = 0.22

/** `VISUAL` — canoe depth, chine to keel amidships, as a fraction of beam. 0.110 m. */
export const CANOE_DEPTH_PER_BEAM = 0.08

/**
 * `VISUAL` — how far the chine and keel lines rise at bow and transom, as a
 * fraction of beam. 0.164 m, applied `∝ (2·fx)²`.
 *
 * Applied to the chine ring **and** the keel ring together, so the canoe body
 * keeps its depth from end to end. Raising only the keel would lift it above
 * the chine at the ends — the rise exceeds the canoe depth — and turn the
 * bilge strip inside out there. See `docs/v2/progress/01-handoff.md`.
 */
export const ROCKER_RISE_PER_BEAM = 0.12

/** `VISUAL` — the keel ring is the sheer outline narrowed in `y` by this much. */
export const KEEL_WIDTH_FRACTION = 0.3

/**
 * `VISUAL` — mast above the head of the sail, as a fraction of the luff
 * (`mastHeight − z_boom`). 0.104 m at the ILCA defaults.
 */
export const MASTHEAD_EXTENSION_FRACTION = 0.02

/**
 * **derived** — maximum sail camber as a fraction of the local chord. Parity
 * with the 2-D bulge the v1 `SailShape.ts` drew, which used the same 0.12.
 */
export const SAIL_CAMBER_FRACTION = 0.12

/** `VISUAL` — chordwise position of maximum camber, luff = 0, leech = 1. */
export const SAIL_CAMBER_POSITION = 0.4

/**
 * **derived** — centreboard chord as a fraction of LOA. The existing
 * `geometry.ts` constant, moved here, not changed. ≈ 0.42 m.
 */
export const BOARD_CHORD_FRACTION = 0.1

/**
 * **derived** — rudder chord as a fraction of LOA. Likewise the existing
 * `geometry.ts` constant, moved, not changed. 0.338 m.
 */
export const RUDDER_CHORD_FRACTION = 0.08

/** Every drawing dimension the model needs, in metres, in `B`. */
export interface VisualDims {
  /** m, deck height above the CG at the transom. */
  deckZ: number
  /** m, how much higher the deck is at the bow than at the transom. */
  sheerRise: number
  /** m, chine height above the CG amidships. */
  chineZ: number
  /** m, keel height above the CG amidships (negative: below). */
  keelZ: number
  /** m, chine and keel rise at bow and transom. */
  rockerRise: number
  /** Dimensionless, the keel ring's width as a fraction of the sheer ring's. */
  keelWidthScale: number
  /** Dimensionless, the keel ring's length as a fraction of the sheer ring's. */
  keelLengthScale: number
  /** m, height of the head of the sail above the CG. */
  mastHeight: number
  /** m, height of the top of the mast above the CG. */
  mastTopZ: number
  boardChord: number
  boardSpan: number
  rudderChord: number
  rudderSpan: number
}

/** Pure. The only place F7 parameters become drawing dimensions. */
export function visualDims(p: RenderParams): VisualDims {
  const deckZ = p.sheet.block_pos_b.z
  const chineZ = deckZ - CHINE_DROP_PER_BEAM * p.hull.beam
  const boardChord = BOARD_CHORD_FRACTION * p.hull.loa
  const rudderChord = RUDDER_CHORD_FRACTION * p.hull.loa
  const mastHeight = p.sheet.z_boom + (2 * p.sail.area) / p.sail.boom_length
  return {
    deckZ,
    sheerRise: SHEER_RISE_PER_LOA * p.hull.loa,
    chineZ,
    keelZ: chineZ - CANOE_DEPTH_PER_BEAM * p.hull.beam,
    rockerRise: ROCKER_RISE_PER_BEAM * p.hull.beam,
    keelWidthScale: KEEL_WIDTH_FRACTION,
    keelLengthScale: p.hull.lwl / p.hull.loa,
    mastHeight,
    mastTopZ: mastHeight + MASTHEAD_EXTENSION_FRACTION * (mastHeight - p.sheet.z_boom),
    boardChord,
    boardSpan: p.board.area / boardChord,
    rudderChord,
    rudderSpan: p.rudder.area / rudderChord,
  }
}
