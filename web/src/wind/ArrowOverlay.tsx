/**
 * The optional coarse arrow/grid overlay of brief section 19.
 *
 * A deck.gl layer, not SVG: it shares the one canvas with the particles rather
 * than putting a second dense representation into the DOM (brief section 39).
 *
 * Exactly **one** layer is produced. `wind.spec.ts` asserts that toggling the
 * overlay changes the deck layer count by one in each direction, which is only
 * a meaningful check if the overlay is a fixed size.
 *
 * ## Live only
 *
 * The lattice reads the cached **live** grid, so it is not built while a
 * recorded episode is being inspected: `App.tsx` passes `null` and the layer
 * is absent. An arrow field drawn from the live simulation behind a recorded
 * boat would be two timelines in one picture (v2 section 10, RV57).
 *
 * ## No trigonometry
 *
 * The barbs are built from the arrow vector and its perpendicular
 * `(-vy, vx)` — vector arithmetic, no angle is ever formed. A bearing is never
 * computed here: that conversion lives in Rust
 * (`environment::wind_to_bearing`) and reaches the UI through
 * `Sim::wind_at_boat` (F6.1, F8).
 */

import { LineLayer } from '@deck.gl/layers'
import type { Layer } from '@deck.gl/core'

import type { Camera } from '../render/Camera'
import { bilinear, type WindGrid } from './sampleGrid'

/** Target spacing between arrows, in screen pixels. */
const ARROW_SPACING_PX = 56
/** Screen pixels of arrow per m/s of wind. */
const ARROW_PIXELS_PER_MS = 5.5
/** Barb length and half-width, as fractions of the arrow vector. */
const BARB_BACK = 0.32
const BARB_SIDE = 0.2
const ARROW_RGB: [number, number, number, number] = [24, 48, 72, 205]
const ARROW_WIDTH_PX = 1.6

/** One arrow: its screen-space base and tip, plus the wind it represents. */
export interface ArrowSample {
  x: number
  y: number
  dx: number
  dy: number
  wx: number
  wy: number
}

export interface ArrowField {
  samples: ArrowSample[]
  layers: Layer[]
}

/**
 * Lay arrows out on a regular **screen-space** lattice and read the wind for
 * each from the cached grid.
 *
 * Screen space rather than world space so the density stays readable at every
 * zoom, and `worldToScreen`/`screenToWorld` keep it registered with the boat.
 */
export function buildArrows(grid: WindGrid, camera: Camera): ArrowField {
  const { width, height } = camera.viewport
  const cols = Math.max(1, Math.floor(width / ARROW_SPACING_PX))
  const rows = Math.max(1, Math.floor(height / ARROW_SPACING_PX))
  const stepX = width / cols
  const stepY = height / rows

  const samples: ArrowSample[] = []
  for (let j = 0; j < rows; j += 1) {
    for (let i = 0; i < cols; i += 1) {
      const sx = (i + 0.5) * stepX
      const sy = (j + 0.5) * stepY
      const world = camera.screenToWorld({ x: sx, y: sy })
      const [wx, wy] = bilinear(grid, world.x, world.y)

      // The arrow points where the air is going. `worldToScreen` on the base
      // and on the displaced point is what turns a world vector into a screen
      // vector — including the camera rotation in `follow` mode.
      const tipWorld = {
        x: world.x + wx * (ARROW_PIXELS_PER_MS / camera.scale),
        y: world.y + wy * (ARROW_PIXELS_PER_MS / camera.scale),
      }
      const tip = camera.worldToScreen(tipWorld)
      samples.push({ x: sx, y: sy, dx: tip.x - sx, dy: tip.y - sy, wx, wy })
    }
  }

  return { samples, layers: [arrowLayer(samples)] }
}

/** Shaft plus two barbs per arrow, in one `LineLayer`. */
function arrowLayer(samples: ArrowSample[]): Layer {
  const segments = samples.length * 3
  const source = new Float32Array(segments * 2)
  const target = new Float32Array(segments * 2)

  samples.forEach((a, n) => {
    const tipX = a.x + a.dx
    const tipY = a.y + a.dy
    // Perpendicular of the arrow vector; no angle is formed.
    const px = -a.dy
    const py = a.dx
    const backX = tipX - BARB_BACK * a.dx
    const backY = tipY - BARB_BACK * a.dy

    const ends: Array<[number, number, number, number]> = [
      [a.x, a.y, tipX, tipY],
      [tipX, tipY, backX + BARB_SIDE * px, backY + BARB_SIDE * py],
      [tipX, tipY, backX - BARB_SIDE * px, backY - BARB_SIDE * py],
    ]
    ends.forEach((e, k) => {
      const at = 2 * (3 * n + k)
      source[at] = e[0]
      source[at + 1] = e[1]
      target[at] = e[2]
      target[at + 1] = e[3]
    })
  })

  return new LineLayer({
    id: 'wind-arrows',
    data: {
      length: segments,
      attributes: {
        getSourcePosition: { value: source, size: 2 },
        getTargetPosition: { value: target, size: 2 },
      },
    },
    positionFormat: 'XY',
    getColor: ARROW_RGB,
    getWidth: ARROW_WIDTH_PX,
    widthUnits: 'pixels',
  })
}

/**
 * A DOM summary of the overlay, for the browser tests.
 *
 * brief section 42 asks browser tests to check numeric state rather than
 * pixels wherever practical, and a WebGL arrow is not inspectable from a
 * Playwright assertion. This publishes the middle arrow's screen-space vector
 * so `wind.spec.ts` can prove the arrows point east in a westerly — the second
 * guard against the from/toward flip.
 */
export function ArrowProbe({ field }: { field: ArrowField | null }) {
  const middle = field === null ? null : (field.samples[field.samples.length >> 1] ?? null)
  return (
    <div
      data-testid="wind-arrows"
      data-count={field === null ? 0 : field.samples.length}
      data-dx={middle === null ? '' : middle.dx}
      data-dy={middle === null ? '' : middle.dy}
      hidden
    />
  )
}
