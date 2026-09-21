/**
 * deck.gl layers for the animated particle field (brief section 19).
 *
 * brief section 39 forbids representing the dense field as SVG DOM elements;
 * four thousand particles are four thousand `<line>` nodes, which is why this
 * is WebGL. `render.spec.ts` and `wind.spec.ts` both count SVG nodes to keep
 * it that way.
 *
 * These layers draw the **live** field. A replay does not draw them: the
 * caller passes no layers while a recorded episode is being inspected, because
 * an episode records the wind vector at the boat and not the field around it
 * (`sim/replay.ts`'s `replayWindField` says so, with the reason shown on the
 * page). Nothing in this file decides that — it has no idea what is being
 * replayed, and should not.
 *
 * Positions are handed to deck.gl as **binary attributes**: the particle
 * system already keeps them in flat `Float32Array`s, so no per-particle object
 * is allocated on any frame. The `data` descriptor is rebuilt each frame, and
 * that change of identity is what tells deck.gl to re-upload the buffers — the
 * buffers themselves are reused, so an `updateTriggers` on them would never
 * fire.
 */

import { LineLayer, ScatterplotLayer } from '@deck.gl/layers'
import type { Layer } from '@deck.gl/core'

import type { Camera } from '../render/Camera'
import { DEBUG_MODE_WIND, visibleCount, type ParticleSystem, type WindVisual } from './particles'

/** Trail colour. A desaturated slate that reads on the pale sea. */
const TRAIL_RGBA: [number, number, number, number] = [60, 90, 120, 190]
const HEAD_RGBA: [number, number, number, number] = [30, 55, 85, 225]
const TRAIL_WIDTH_PX = 1.4
const HEAD_RADIUS_PX = 0.9

/**
 * The faintest the field may be drawn, as a fraction of full alpha.
 *
 * **Provenance.** A visual floor, not a physical one: below roughly a third of
 * full alpha the trails stop reading as motion on a bright phone screen
 * outdoors, and a wind field nobody can see is worse than a dense one. Sail
 * Mode's 0.55 sits above it with room to spare; the clamp is here so a future
 * preset cannot turn the field off by accident.
 */
const MIN_CONTRAST = 1 / 3

/** `rgba` with its alpha scaled by `contrast`. */
function fade(
  [r, g, b, a]: [number, number, number, number],
  contrast: number,
): [number, number, number, number] {
  return [r, g, b, Math.round(a * clampContrast(contrast))]
}

function clampContrast(contrast: number): number {
  if (!Number.isFinite(contrast)) {
    return 1
  }
  return Math.max(MIN_CONTRAST, Math.min(1, contrast))
}

/**
 * Project a world-space `[x, y]` buffer into screen pixels.
 *
 * `out` is reused between frames by the caller. `Camera.worldToScreen` is the
 * only projection in the application (section 02 handoff, item 10); this does
 * not reimplement it.
 */
export function projectInto(
  world: Float32Array,
  camera: Camera,
  out: Float32Array,
): Float32Array {
  for (let i = 0; i < world.length; i += 2) {
    const p = camera.worldToScreen({ x: world[i], y: world[i + 1] })
    out[i] = p.x
    out[i + 1] = p.y
  }
  return out
}

export interface ParticleLayerInput {
  particles: ParticleSystem
  /** Screen-space `[x, y]` pairs for the head of each trail. */
  heads: Float32Array
  /** Screen-space `[x, y]` pairs for the tail of each trail. */
  tails: Float32Array
}

/**
 * The particle field, as **two** layers: trails and heads.
 *
 * The **layer** count is stable — `wind.spec.ts` asserts that toggling the
 * arrow overlay changes the total by exactly one, which only means anything if
 * this half does not vary. What `visual` changes is how many particles each of
 * the two layers draws and how strongly, never how many layers there are.
 *
 * Reducing the drawn count is a slice of the *front* of the buffers, which is
 * a uniform random sample of the field and not a patch of it
 * ({@link visibleCount}). Every particle is still advected through the same
 * grid at the same speed, so switching modes does not restart the field, shift
 * a streamline or change a single number the boat feels (v2 section 09, RV55).
 */
export function particleLayers(
  input: ParticleLayerInput,
  visual: WindVisual = DEBUG_MODE_WIND,
): Layer[] {
  const { particles, heads, tails } = input
  const length = visibleCount(particles.count, visual.density)
  const contrast = clampContrast(visual.contrast)

  return [
    new LineLayer({
      id: 'wind-trails',
      data: {
        length,
        attributes: {
          getSourcePosition: { value: tails, size: 2 },
          getTargetPosition: { value: heads, size: 2 },
        },
      },
      positionFormat: 'XY',
      getColor: fade(TRAIL_RGBA, contrast),
      getWidth: TRAIL_WIDTH_PX * contrast,
      widthUnits: 'pixels',
    }),
    new ScatterplotLayer({
      id: 'wind-heads',
      data: {
        length,
        attributes: { getPosition: { value: heads, size: 2 } },
      },
      positionFormat: 'XY',
      getFillColor: fade(HEAD_RGBA, contrast),
      getRadius: HEAD_RADIUS_PX,
      radiusUnits: 'pixels',
    }),
  ]
}
