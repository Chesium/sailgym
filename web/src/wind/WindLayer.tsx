/**
 * deck.gl layers for the animated particle field (brief section 19).
 *
 * brief section 39 forbids representing the dense field as SVG DOM elements;
 * four thousand particles are four thousand `<line>` nodes, which is why this
 * is WebGL. `render.spec.ts` and `wind.spec.ts` both count SVG nodes to keep
 * it that way.
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
import type { ParticleSystem } from './particles'

/** Trail colour. A desaturated slate that reads on the pale sea. */
const TRAIL_RGBA: [number, number, number, number] = [60, 90, 120, 190]
const HEAD_RGBA: [number, number, number, number] = [30, 55, 85, 225]
const TRAIL_WIDTH_PX = 1.4
const HEAD_RADIUS_PX = 0.9

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
 * The count is stable — `wind.spec.ts` asserts that toggling the arrow overlay
 * changes the total by exactly one, which only means anything if this half
 * does not vary.
 */
export function particleLayers(input: ParticleLayerInput): Layer[] {
  const { particles, heads, tails } = input
  const length = particles.count

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
      getColor: TRAIL_RGBA,
      getWidth: TRAIL_WIDTH_PX,
      widthUnits: 'pixels',
    }),
    new ScatterplotLayer({
      id: 'wind-heads',
      data: {
        length,
        attributes: { getPosition: { value: heads, size: 2 } },
      },
      positionFormat: 'XY',
      getFillColor: HEAD_RGBA,
      getRadius: HEAD_RADIUS_PX,
      radiusUnits: 'pixels',
    }),
  ]
}
