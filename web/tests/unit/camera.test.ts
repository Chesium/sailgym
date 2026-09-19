import { describe, expect, it } from 'vitest'

import {
  createCamera,
  worldTransform,
  PIXELS_PER_METRE,
  type CameraMode,
} from '../../src/render/Camera'
import { hullExtent } from '../../src/render/geometry'

const VIEWPORT = { width: 780, height: 520 }

/** Deterministic generator, so a failure is reproducible. */
function lcg(seed: number): () => number {
  let s = seed >>> 0
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0
    return s / 4294967296
  }
}

describe('camera', () => {
  it('screenToWorld inverts worldToScreen in both modes', () => {
    const rand = lcg(20250918)
    for (const mode of ['follow', 'northUp'] as CameraMode[]) {
      for (let i = 0; i < 100; i += 1) {
        const zoom = 0.1 + rand() * (20 - 0.1)
        const cam = createCamera({
          mode,
          zoom,
          centre: { x: rand() * 200 - 100, y: rand() * 200 - 100 },
          heading: rand() * 12 - 6,
          viewport: VIEWPORT,
        })
        const p = { x: rand() * 400 - 200, y: rand() * 400 - 200 }
        const back = cam.screenToWorld(cam.worldToScreen(p))
        expect(Math.abs(back.x - p.x)).toBeLessThan(1e-9)
        expect(Math.abs(back.y - p.y)).toBeLessThan(1e-9)
      }
    }
  })

  it('northUp applies no rotation; follow puts the bow up', () => {
    const northUp = createCamera({
      mode: 'northUp',
      zoom: 1,
      centre: { x: 0, y: 0 },
      heading: 1.1,
      viewport: VIEWPORT,
    })
    expect(northUp.rotation).toBe(0)
    expect(worldTransform(northUp)).not.toContain('rotate')

    const heading = 1.1
    const follow = createCamera({
      mode: 'follow',
      zoom: 1,
      centre: { x: 0, y: 0 },
      heading,
      viewport: VIEWPORT,
    })
    expect(worldTransform(follow)).toContain('rotate')
    // A point 10 m ahead of the boat is 10 m straight up the screen.
    const ahead = follow.worldToScreen({ x: 10 * Math.cos(heading), y: 10 * Math.sin(heading) })
    expect(Math.abs(ahead.x - VIEWPORT.width / 2)).toBeLessThan(1e-9)
    expect(ahead.y).toBeLessThan(VIEWPORT.height / 2)
    // ... and the boat drawing itself stops spinning: its screen rotation is
    // a constant quarter turn, whatever the heading.
    expect(follow.rotation - heading).toBeCloseTo(-Math.PI / 2, 12)
  })

  it('draws the hull at loa x pixelsPerMetre at zoom 1', () => {
    // Deliberately not the ILCA numbers: the renderer must work from whatever
    // `parameters_json()` reports (F7).
    const hull = { loa: 5.5, beam: 1.9 }
    const extent = hullExtent(hull)
    expect(Math.abs(extent.length - hull.loa)).toBeLessThan(1e-12)

    const cam = createCamera({
      mode: 'northUp',
      zoom: 1,
      centre: { x: 0, y: 0 },
      heading: 0,
      viewport: VIEWPORT,
    })
    const bow = cam.worldToScreen({ x: hull.loa / 2, y: 0 })
    const stern = cam.worldToScreen({ x: -hull.loa / 2, y: 0 })
    const onScreen = Math.hypot(bow.x - stern.x, bow.y - stern.y)
    expect(Math.abs(onScreen - hull.loa * PIXELS_PER_METRE)).toBeLessThan(1)
  })

  it('clamps zoom to the supported range', () => {
    const at = (zoom: number) =>
      createCamera({
        mode: 'northUp',
        zoom,
        centre: { x: 0, y: 0 },
        heading: 0,
        viewport: VIEWPORT,
      }).zoom
    expect(at(1e-6)).toBe(0.1)
    expect(at(1e6)).toBe(20)
  })
})
