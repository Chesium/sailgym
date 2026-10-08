import { describe, expect, it } from 'vitest'

import {
  createCamera,
  fitBounds,
  worldTransform,
  MAX_ZOOM,
  MIN_ZOOM,
  PIXELS_PER_METRE,
  type Camera,
  type CameraMode,
  type Vec2,
} from '../../src/render/Camera'
import { hullExtent } from '../../src/render/geometry'

const VIEWPORT = { width: 780, height: 520 }

/** A square view, so which axis limits a fit is the course's doing, not the viewport's. */
const SQUARE = { width: 520, height: 520 }

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

/**
 * `fitBounds` — the **Show course** camera (v2 section 12, task 12.5).
 *
 * Pure, and asserted as a *containment* claim rather than by recomputing the
 * arithmetic: for every point, the corners of its `margin` box must project
 * inside the viewport. The one case the function documents it cannot satisfy —
 * a course too large for `MIN_ZOOM` — is asserted as the clamp it is, not
 * waived.
 */
describe('fitBounds', () => {
  /** Every point's `margin` box, projected — the containment claim, literally. */
  function contains(camera: Camera, points: readonly Vec2[], margin: number): boolean {
    for (const p of points) {
      for (const dx of [-margin, margin]) {
        for (const dy of [-margin, margin]) {
          const s = camera.worldToScreen({ x: p.x + dx, y: p.y + dy })
          if (
            s.x < -1e-9 ||
            s.y < -1e-9 ||
            s.x > camera.viewport.width + 1e-9 ||
            s.y > camera.viewport.height + 1e-9
          ) {
            return false
          }
        }
      }
    }
    return true
  }

  it('contains_every_point_with_its_margin', () => {
    const courses: readonly Vec2[][] = [
      // `courses/reach.json`, start included.
      [
        { x: 0, y: 0 },
        { x: 30, y: 0 },
        { x: 60, y: -10 },
        { x: 90, y: 0 },
      ],
      // `courses/triangle.json`.
      [
        { x: 0, y: 0 },
        { x: 0, y: 40 },
        { x: 30, y: 10 },
        { x: 0, y: -10 },
      ],
      // `courses/windward_leeward.json`.
      [
        { x: 0, y: 0 },
        { x: 0, y: 40 },
      ],
      // A tall narrow one, so the limiting axis is the other one.
      [
        { x: -1, y: -200 },
        { x: 1, y: 200 },
      ],
    ]
    // The 344 × 202 world view is the one a 360 × 640 phone gets
    // (`progress/09-handoff.md` §3.3), so the fit is asserted at the size the
    // acceptance criteria actually name.
    let contained = 0
    let clamped = 0
    for (const viewport of [VIEWPORT, SQUARE, { width: 344, height: 202 }]) {
      for (const points of courses) {
        for (const margin of [0, 5, 20]) {
          const where = `${viewport.width}×${viewport.height} margin ${margin}`
          const fit = fitBounds(points, viewport, margin)
          expect(fit.zoom, where).toBeGreaterThanOrEqual(MIN_ZOOM)
          expect(fit.zoom, where).toBeLessThanOrEqual(MAX_ZOOM)
          const camera = createCamera({
            mode: 'northUp',
            zoom: fit.zoom,
            centre: fit.centre,
            heading: 0,
            viewport,
          })
          if (fit.zoom > MIN_ZOOM) {
            expect(contains(camera, points, margin), where).toBe(true)
            contained += 1
          } else {
            // The one case the function documents it cannot satisfy: a course
            // too large for the supported zoom range. It is reported by the
            // returned zoom being exactly `MIN_ZOOM` — not by silently going
            // outside the range the rest of the camera accepts — and the
            // camera is centred on the bounds.
            expect(fit.zoom, where).toBe(MIN_ZOOM)
            expect(contains(camera, points, margin), where).toBe(false)
            clamped += 1
          }
        }
      }
    }
    // Both branches are live, so neither assertion above is vacuous.
    expect(contained).toBeGreaterThan(0)
    expect(clamped).toBeGreaterThan(0)
  })

  it('clamps_to_MIN_ZOOM_and_MAX_ZOOM', () => {
    // Far too large for the zoom range: clamped up to `MIN_ZOOM`, which is
    // the honest report that it cannot be contained.
    const huge = fitBounds(
      [
        { x: -5e5, y: 0 },
        { x: 5e5, y: 0 },
      ],
      VIEWPORT,
      0,
    )
    expect(huge.zoom).toBe(MIN_ZOOM)
    expect(huge.centre).toEqual({ x: 0, y: 0 })

    // Far too small: clamped down to `MAX_ZOOM`, and clamping *down* only
    // shows more, so the margin box is still contained.
    const tiny = fitBounds([{ x: 3, y: -4 }], VIEWPORT, 1e-6)
    expect(tiny.zoom).toBe(MAX_ZOOM)
    const camera = createCamera({
      mode: 'northUp',
      zoom: tiny.zoom,
      centre: tiny.centre,
      heading: 0,
      viewport: VIEWPORT,
    })
    expect(contains(camera, [{ x: 3, y: -4 }], 1e-6)).toBe(true)
  })

  it('handles_the_degenerate_cases_explicitly', () => {
    // No points: no bounds. The world origin at the default zoom, and no NaN.
    expect(fitBounds([], VIEWPORT, 10)).toEqual({ centre: { x: 0, y: 0 }, zoom: 1 })

    // One point, with a margin: a finite fit centred on it.
    const one = fitBounds([{ x: 12, y: -3 }], VIEWPORT, 10)
    expect(one.centre).toEqual({ x: 12, y: -3 })
    expect(one.zoom).toBe(VIEWPORT.height / (PIXELS_PER_METRE * 20))
    expect(Number.isFinite(one.zoom)).toBe(true)

    // All points coincident: the same answer as one point.
    const same = fitBounds(
      [
        { x: 12, y: -3 },
        { x: 12, y: -3 },
        { x: 12, y: -3 },
      ],
      VIEWPORT,
      10,
    )
    expect(same).toEqual(one)

    // A point and a zero margin: zero extent, which would be a division by
    // zero. `MAX_ZOOM`, not `Infinity` and not `NaN`.
    const point = fitBounds([{ x: 1, y: 2 }], VIEWPORT, 0)
    expect(point.zoom).toBe(MAX_ZOOM)
    expect(point.centre).toEqual({ x: 1, y: 2 })

    // A non-finite point is dropped rather than poisoning the camera.
    const dirty = fitBounds(
      [
        { x: 0, y: 0 },
        { x: Number.NaN, y: 1 },
        { x: 10, y: 0 },
        { x: Number.POSITIVE_INFINITY, y: 0 },
      ],
      VIEWPORT,
      0,
    )
    expect(dirty.centre).toEqual({ x: 5, y: 0 })
    expect(Number.isFinite(dirty.zoom)).toBe(true)

    // A viewport measured at zero (a hidden element) gives `MIN_ZOOM`.
    const hidden = fitBounds([{ x: 0, y: 0 }, { x: 10, y: 0 }], { width: 0, height: 0 }, 0)
    expect(hidden.zoom).toBe(MIN_ZOOM)

    // A nonsense margin is treated as none, not as `NaN`.
    const nan = fitBounds([{ x: 0, y: 0 }, { x: 10, y: 0 }], VIEWPORT, Number.NaN)
    expect(Number.isFinite(nan.zoom)).toBe(true)
    expect(fitBounds([{ x: 0, y: 0 }, { x: 10, y: 0 }], VIEWPORT, -5)).toEqual(nan)
  })
})
