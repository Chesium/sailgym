import type { Page } from '@playwright/test'
import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Heel indicator and capsize state (task 7.5, brief §26 and §29).
 *
 * Numbers, not pixels (brief §42). The rendering checks read the section
 * view's own transform matrix and the screen position of its masthead, so they
 * assert geometry without comparing a single pixel.
 */

/** The live indicator and the snapshot, read in the **same** frame. */
async function sample(page: Page) {
  return page.evaluate(() => {
    const snapshot = document.querySelector('[data-testid="snapshot"]') as HTMLElement
    const indicator = document.querySelector('[data-testid="heel-indicator"]') as HTMLElement
    const readout = document.querySelector(
      '[data-testid="heel-indicator-readout"]',
    ) as HTMLElement
    const hull = document.querySelector('[data-testid="heel-indicator-hull"]') as SVGGElement
    const m = hull.getScreenCTM()!
    return {
      t: Number(snapshot.dataset.t),
      phi: Number(snapshot.dataset.phi),
      indicatorPhi: Number(indicator.dataset.phi),
      heelDeg: Number(indicator.dataset.heelDeg),
      readoutDeg: Number(readout.dataset.value),
      // `rotate(θ)` about the origin: the matrix is `[s·cosθ, s·sinθ; …]`
      // once the uniform viewBox scale is folded in, so the angle comes back
      // out of `atan2(b, a)` with no knowledge of the scale at all.
      transformDeg: (Math.atan2(m.b, m.a) * 180) / Math.PI,
    }
  })
}

/** Degrees folded into `(−180, 180]`, to compare against a matrix angle. */
function wrap(degrees: number): number {
  const w = (((degrees + 180) % 360) + 360) % 360 - 180
  return w === -180 ? 180 : w
}

test('the readout and the section view follow phi through a capsize', async ({ page }) => {
  await gotoApp(page, { scenario: 'capsize' })
  await page.getByTestId('clock-speed-4x').click()

  let samples = 0
  let extreme = 0
  const deadline = Date.now() + 25_000
  for (;;) {
    const s = await sample(page)
    // The indicator is driven by the same snapshot the page publishes.
    expect(s.indicatorPhi).toBe(s.phi)
    const expected = (s.phi * 180) / Math.PI
    expect(Math.abs(s.heelDeg - expected)).toBeLessThan(0.5)
    expect(Math.abs(s.readoutDeg - expected)).toBeLessThan(0.5)
    // ...and the drawn hull is rotated by exactly that angle.
    expect(Math.abs(wrap(s.transformDeg - expected))).toBeLessThan(0.5)
    samples += 1
    extreme = Math.max(extreme, Math.abs(expected))
    if (s.t >= 20 || Date.now() > deadline) {
      break
    }
    await page.waitForTimeout(120)
  }
  expect(samples).toBeGreaterThan(20)
  // Worth nothing unless the boat actually went over on this run.
  expect(extreme).toBeGreaterThan(45)
})

test('capsize state flips and the simulation keeps advancing', async ({ page }) => {
  await gotoApp(page, { scenario: 'capsize' })
  await page.getByTestId('clock-speed-4x').click()

  const state = page.getByTestId('capsize-state')
  await expect(state).toHaveAttribute('data-capsized', 'false')
  await expect(state).toHaveAttribute('data-capsized', 'true', { timeout: 20_000 })
  await expect(state).toHaveText(/CAPSIZED/)

  // The threshold crossing is recorded, and the clock has not stopped.
  const crossed = Number(await state.getAttribute('data-since'))
  expect(crossed).toBeGreaterThan(0)
  const before = (await readSnapshot(page)).t
  await page.waitForTimeout(1500)
  const after = (await readSnapshot(page)).t
  expect(after).toBeGreaterThan(before)
  // Still capsized, still integrating: nothing terminated (brief §17).
  await expect(state).toHaveAttribute('data-capsized', 'true')
})

test('the boat is driven past 100 degrees and the readout follows it', async ({ page }) => {
  await gotoApp(page, { scenario: 'knockdown' })

  // Pause, then reset back to the scenario's initial state, then walk the
  // knockdown one physics step at a time. The excursion past 100° lasts about
  // 0.3 s, which no frame-rate poll can be relied on to catch; stepping makes
  // the whole of it observable and removes the race entirely.
  await page.getByTestId('clock-pause').click()
  await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
  await page.getByTestId('clock-reset').click()

  let peak = 0
  for (let i = 0; i < 60; i += 1) {
    const s = await sample(page)
    peak = Math.max(peak, Math.abs(s.readoutDeg))
    expect(Math.abs(s.readoutDeg - (s.phi * 180) / Math.PI)).toBeLessThan(0.5)
    await page.getByTestId('clock-step').click()
  }
  expect(peak).toBeGreaterThan(100)

  // The core's own per-step high-water mark agrees, and the simulation is
  // still running afterwards rather than having stopped at the threshold.
  const snapshot = await readSnapshot(page)
  expect((snapshot.maxHeel * 180) / Math.PI).toBeGreaterThan(100)
  await page.getByTestId('clock-pause').click()
  const before = (await readSnapshot(page)).t
  await page.waitForTimeout(800)
  expect((await readSnapshot(page)).t).toBeGreaterThan(before)
})

test('the section view is legible at every regime', async ({ page }) => {
  await gotoApp(page)

  const probes = await page.evaluate(() => {
    const angles = [0, 45, -45, 95, -95, 185, -185]
    return angles.map((deg) => {
      const root = document.querySelector(`[data-testid="heel-probe-${deg}"]`) as HTMLElement
      const hull = document.querySelector(
        `[data-testid="heel-probe-${deg}-hull"]`,
      ) as SVGGElement
      const head = document.querySelector(
        `[data-testid="heel-probe-${deg}-masthead"]`,
      ) as SVGCircleElement
      const svg = hull.ownerSVGElement!
      const hm = hull.getScreenCTM()!
      const sm = svg.getScreenCTM()!
      const masthead = new DOMPoint(0, -1.5).matrixTransform(head.getScreenCTM()!)
      const origin = new DOMPoint(0, 0).matrixTransform(sm)
      const mastTop = new DOMPoint(0, -1.5).matrixTransform(sm)
      return {
        deg,
        regime: root.dataset.regime ?? '',
        transformDeg: (Math.atan2(hm.b, hm.a) * 180) / Math.PI,
        // Screen pixels: `+y` is down, so "above the waterline" is `y < 0`.
        mastheadDx: masthead.x - origin.x,
        mastheadDy: masthead.y - origin.y,
        // How far the mast reaches when the boat is upright, for scale.
        uprightDy: mastTop.y - origin.y,
      }
    })
  })

  const at = (deg: number) => probes.find((p) => p.deg === deg)!

  // 1. Every section is rotated by exactly its heel angle.
  for (const probe of probes) {
    expect(Math.abs(wrap(probe.transformDeg - probe.deg))).toBeLessThan(0.5)
  }

  // 2. The mast is above the waterline up to severe heel and below it past 90°
  //    and past inversion — which is what "the hull past horizontal" means in
  //    a section view.
  for (const deg of [0, 45, -45]) {
    expect(at(deg).mastheadDy).toBeLessThan(0)
  }
  for (const deg of [95, -95, 185, -185]) {
    expect(at(deg).mastheadDy).toBeGreaterThan(0)
  }

  // 3. Positive heel is starboard down, and the mast leans out over the side
  //    that is going down — to starboard, the right of a view looking forward
  //    from astern. Negative heel mirrors it exactly.
  expect(at(45).mastheadDx).toBeGreaterThan(0)
  expect(at(-45).mastheadDx).toBeLessThan(0)
  expect(at(45).mastheadDx).toBeCloseTo(-at(-45).mastheadDx, 6)
  expect(at(95).mastheadDx).toBeCloseTo(-at(-95).mastheadDx, 6)

  // 4. Inverted: the masthead is very nearly straight down, as far below the
  //    waterline as it is above it when upright. And the four regimes are
  //    labelled.
  const reach = Math.abs(at(0).uprightDy)
  expect(at(185).mastheadDy).toBeGreaterThan(0.95 * reach)
  expect(Math.abs(at(185).mastheadDx)).toBeLessThan(0.15 * reach)
  expect(at(0).regime).toBe('upright')
  expect(at(45).regime).toBe('moderate')
  expect(at(95).regime).toBe('severe')
  expect(at(185).regime).toBe('inverted')
})
