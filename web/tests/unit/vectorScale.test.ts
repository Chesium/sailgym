import { describe, expect, it } from 'vitest'

import {
  arrowHead,
  autoScale,
  boatPointToScreen,
  boatVectorToScreen,
  MAX_VECTOR_PX,
  MIN_VECTOR_PX,
  momentArc,
  pixelsFor,
} from '../../src/render/vectorScale'

describe('vector scaling', () => {
  it('draws a 200 N force at 2 N/px as 100 px', () => {
    expect(pixelsFor(200, 2)).toBe(100)
  })

  it('is linear in the magnitude and inverse in the scale', () => {
    expect(pixelsFor(400, 2)).toBe(200)
    expect(pixelsFor(200, 4)).toBe(50)
    expect(pixelsFor(0, 2)).toBe(0)
  })

  it('refuses to divide by a scale that is not a scale', () => {
    expect(pixelsFor(200, 0)).toBe(0)
    expect(pixelsFor(200, -1)).toBe(0)
    expect(pixelsFor(Number.NaN, 2)).toBe(0)
  })

  it('keeps the largest active vector inside the window across a 10x range', () => {
    // A decade of force, sampled finely enough to cross every 1-2-5 boundary.
    for (let i = 0; i <= 200; i += 1) {
      const magnitude = 20 * 10 ** (i / 200)
      const px = pixelsFor(magnitude, autoScale(magnitude))
      expect(px, `${magnitude} N`).toBeGreaterThanOrEqual(MIN_VECTOR_PX)
      expect(px, `${magnitude} N`).toBeLessThanOrEqual(MAX_VECTOR_PX)
    }
  })

  it('holds that window over every decade the boat can produce', () => {
    // 0.01 N (a drifting hull) to 100 kN (a hand-edited sheet stiffness).
    for (let decade = -2; decade <= 5; decade += 1) {
      for (const mantissa of [1, 1.5, 2, 3, 5, 7, 9.99]) {
        const magnitude = mantissa * 10 ** decade
        const px = pixelsFor(magnitude, autoScale(magnitude))
        expect(px, `${magnitude}`).toBeGreaterThanOrEqual(MIN_VECTOR_PX)
        expect(px, `${magnitude}`).toBeLessThanOrEqual(MAX_VECTOR_PX)
      }
    }
  })

  it('snaps the scale to a readable 1-2-5 step', () => {
    expect(autoScale(120)).toBe(1)
    expect(autoScale(240)).toBe(2)
    expect(autoScale(600)).toBe(5)
    expect(autoScale(1200)).toBe(10)
    expect(autoScale(0)).toBe(1)
    expect(autoScale(Number.NaN)).toBe(1)
  })
})

describe('boat-fixed to screen', () => {
  it('sends bow to the right and port up with a north-up camera heading east', () => {
    const forward = boatVectorToScreen(1, 0, 0, 10)
    expect(forward.x).toBeCloseTo(10, 12)
    expect(forward.y).toBeCloseTo(0, 12)
    const port = boatVectorToScreen(0, 1, 0, 10)
    expect(port.x).toBeCloseTo(0, 12)
    // Screen y grows downward, so port is up the screen: negative y.
    expect(port.y).toBeCloseTo(-10, 12)
  })

  it('rotates with the boat', () => {
    // Bow pointing north (psi = +90 deg) under a north-up camera: theta = -psi.
    const theta = -Math.PI / 2
    const forward = boatVectorToScreen(1, 0, theta, 10)
    expect(forward.x).toBeCloseTo(0, 12)
    expect(forward.y).toBeCloseTo(-10, 12)
  })

  it('preserves length under any rotation', () => {
    for (let i = 0; i < 32; i += 1) {
      const theta = (i / 32) * 2 * Math.PI
      const v = boatVectorToScreen(3, -4, theta, 2)
      expect(Math.hypot(v.x, v.y)).toBeCloseTo(10, 10)
    }
  })

  it('places a boat-fixed point relative to the boat origin', () => {
    const origin = { x: 100, y: 200 }
    const p = boatPointToScreen(origin, 2, 0, 0, 20)
    expect(p).toEqual({ x: 140, y: 200 })
  })
})

describe('arrowheads and moment arcs', () => {
  it('produces three points for a real direction and nothing for none', () => {
    expect(arrowHead({ x: 0, y: 0 }, 1, 0).split(' ')).toHaveLength(3)
    expect(arrowHead({ x: 0, y: 0 }, 0, 0)).toBe('')
  })

  it('draws nothing for a vanishing moment and clamps a huge one', () => {
    expect(momentArc({ x: 0, y: 0 }, 0, 1)).toBeNull()
    const huge = momentArc({ x: 0, y: 0 }, 1e9, 1)
    expect(huge?.radius).toBe(MAX_VECTOR_PX)
  })

  it('reverses the sweep with the sign of the moment', () => {
    const positive = momentArc({ x: 0, y: 0 }, 100, 1)
    const negative = momentArc({ x: 0, y: 0 }, -100, 1)
    expect(positive).not.toBeNull()
    expect(negative).not.toBeNull()
    expect(positive?.radius).toBe(negative?.radius)
    // Mirror images about the horizontal: same radius, opposite tip.
    expect(positive?.tip.y).toBeCloseTo(-(negative?.tip.y ?? 0), 10)
  })
})
