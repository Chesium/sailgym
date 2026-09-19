import { readFileSync } from 'node:fs'
import type { Page } from '@playwright/test'
import { expect, gotoApp, readSnapshot, test } from './fixtures'
import type { Diagnostics } from '../../src/sim/diagnostics'
import { OVERLAY_KEYS as STORE_OVERLAY_KEYS } from '../../src/ui/store'

/**
 * Debug Mode's instrumentation, in a real browser (tasks 8.3, 8.4, 8.6).
 *
 * brief §30 calls this tooling a core prototype feature, so the tests treat a
 * missing diagnostic as a missing requirement: the field list is enumerated
 * from `src/sim/diagnostics.ts` rather than written out here, and every one of
 * the sixteen overlays is toggled in both directions.
 */

/**
 * brief §30's sixteen visual items, taken from the store rather than written
 * out here: a copy would let the test and the application drift apart, and
 * "each of the 16" is the criterion.
 */
const OVERLAY_KEYS = STORE_OVERLAY_KEYS

/** The top-level field names of the TypeScript `Diagnostics` interface. */
function diagnosticFields(): string[] {
  const source = readFileSync('src/sim/diagnostics.ts', 'utf8')
  const start = source.indexOf('export interface Diagnostics {')
  const body = source.slice(start, source.indexOf('\n}', start))
  return [...body.matchAll(/^ {2}(\w+):/gm)].map((m) => m[1])
}

async function enterDebug(page: Page) {
  await page.getByTestId('mode-switch').click()
  await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
}

async function setAllOverlays(page: Page, on: boolean) {
  await page.getByTestId(on ? 'overlays-all-on' : 'overlays-all-off').click()
  await expect(page.getByTestId('overlay-controls')).toHaveAttribute(
    'data-on',
    String(on ? OVERLAY_KEYS.length : 0),
  )
}

/** The live diagnostics record, as the HUD published it this frame. */
async function diagnostics(page: Page): Promise<Diagnostics> {
  const raw = await page.getByTestId('sail-hud').getAttribute('data-diagnostics')
  return JSON.parse(raw ?? '{}') as Diagnostics
}

test.describe('debug overlays', () => {
  test('the store and the overlay table agree on sixteen keys', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)
    // One checkbox per key, and no more: the count is brief §30's visual list.
    for (const key of OVERLAY_KEYS) {
      await expect(page.getByTestId(`toggle-${key}`)).toHaveCount(1)
    }
    // Scoped to the overlay panel: section 03's `toggle-wind-arrows` shares
    // the prefix and is a wind-layer control, not one of brief §30's sixteen.
    expect(
      await page.locator('[data-testid="overlay-controls"] [data-testid^="toggle-"]').count(),
    ).toBe(OVERLAY_KEYS.length)
    expect(OVERLAY_KEYS).toHaveLength(16)
  })

  test('toggling each of the sixteen overlays changes the DOM, both ways', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)
    await setAllOverlays(page, false)
    await expect(page.getByTestId('force-overlay')).toHaveAttribute('data-active', '0')

    for (const key of OVERLAY_KEYS) {
      const group = page.getByTestId(`overlay-${key}`)
      const checkbox = page.getByTestId(`toggle-${key}`)

      await expect(group, `${key} must be absent while off`).toHaveCount(0)
      const legendBefore = await page.getByTestId('overlay-legend').getAttribute('data-count')

      await checkbox.check()
      await expect(group, `${key} must appear when switched on`).toHaveCount(1)
      await expect(page.getByTestId(`legend-${key}`)).toHaveCount(1)
      expect(
        Number(await page.getByTestId('overlay-legend').getAttribute('data-count')),
      ).toBe(Number(legendBefore) + 1)

      await checkbox.uncheck()
      await expect(group, `${key} must disappear when switched off`).toHaveCount(0)
      await expect(page.getByTestId(`legend-${key}`)).toHaveCount(0)
      expect(await page.getByTestId('overlay-legend').getAttribute('data-count')).toBe(
        legendBefore,
      )
    }
  })

  test('the sail force vector starts at the rendered sail centre of effort', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)
    await setAllOverlays(page, true)
    // Let the boat load the sail up, so the vector has a direction.
    await expect
      .poll(
        async () => {
          const d = await diagnostics(page)
          return Math.hypot(d.sail.f.x, d.sail.f.y)
        },
        { timeout: 15_000 },
      )
      .toBeGreaterThan(1)
    await page.getByTestId('clock-pause').click()

    const gap = await page.evaluate(() => {
      const line = document.querySelector(
        '[data-testid="overlay-sailForce"] line',
      ) as SVGLineElement
      const dot = document.querySelector(
        '[data-testid="overlay-sailCe"] circle',
      ) as SVGCircleElement
      const lm = line.getScreenCTM()!
      const cm = dot.getScreenCTM()!
      const origin = new DOMPoint(line.x1.baseVal.value, line.y1.baseVal.value).matrixTransform(lm)
      const centre = new DOMPoint(dot.cx.baseVal.value, dot.cy.baseVal.value).matrixTransform(cm)
      return Math.hypot(origin.x - centre.x, origin.y - centre.y)
    })
    expect(gap).toBeLessThan(3)
  })

  test('with every overlay on the page stays under 300 SVG elements', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)
    await setAllOverlays(page, true)
    await page.getByTestId('charts-all-on').click()
    await page.waitForTimeout(1_500)

    const counts = await page.evaluate(() => ({
      total: document.querySelectorAll('svg *').length,
      overlay: document.querySelectorAll('[data-testid="force-overlay"] *').length,
    }))
    // Printed so the figures reach the run log and the handoff; the budget is
    // brief §39's point that SVG is fine for a bounded overlay and wrong for
    // the dense wind field.
    console.log(
      `[svg] every overlay + every chart: overlay=${counts.overlay} page=${counts.total}`,
    )
    expect(counts.overlay).toBeLessThan(120)
    expect(counts.total).toBeLessThan(300)
  })

  test("the legend's sail force matches the diagnostics within 0.5 N", async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)
    await setAllOverlays(page, true)
    await page.waitForTimeout(2_000)
    await page.getByTestId('clock-pause').click()

    const d = await diagnostics(page)
    const shown = Number(await page.getByTestId('legend-sailForce').getAttribute('data-value'))
    const expected = Math.hypot(d.sail.f.x, d.sail.f.y)
    expect(expected).toBeGreaterThan(0.5)
    expect(Math.abs(shown - expected)).toBeLessThan(0.5)
  })
})

test.describe('debug readouts and charts', () => {
  test('every Diagnostics field has a row in the panel', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)

    const fields = diagnosticFields()
    expect(fields.length).toBeGreaterThan(40)
    for (const field of fields) {
      await expect(page.getByTestId(`diag-${field}`), `no readout for ${field}`).toHaveCount(1)
    }
    // …and the panel is showing exactly the record it was given, no more.
    expect(Number(await page.getByTestId('debug-panel').getAttribute('data-fields'))).toBe(
      fields.length,
    )
  })

  test('charts accumulate over five seconds and end on the current value', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)
    await page.getByTestId('charts-all-on').click()
    await page.waitForTimeout(5_000)
    await page.getByTestId('clock-pause').click()

    // Step until the newest sample was taken at the state now on screen. A
    // sample is only taken once `1/hz` of *simulated* time has passed, so
    // without this the chart and the live readout are up to one interval of
    // motion apart and "matches the current value" would be a statement about
    // how fast the boat happens to be changing.
    for (let i = 0; i < 40; i += 1) {
      const t = (await readSnapshot(page)).t
      const sampled = Number(await page.getByTestId('chart-heelDeg').getAttribute('data-latest-t'))
      if (Math.abs(sampled - t) < 1e-12) {
        break
      }
      await page.getByTestId('clock-step').click()
    }
    expect(
      Math.abs(
        Number(await page.getByTestId('chart-heelDeg').getAttribute('data-latest-t')) -
          (await readSnapshot(page)).t,
      ),
    ).toBeLessThan(1e-12)

    const charts = page.getByTestId('charts')
    const samples = Number(await charts.getAttribute('data-samples'))
    const span = Number(await charts.getAttribute('data-span'))
    const hz = Number(await charts.getAttribute('data-sample-hz'))
    expect(hz).toBe(20)
    // Five seconds at 20 Hz is ~100 samples; allow for a slow browser losing
    // frames, but insist the series is a series and not a point.
    expect(samples).toBeGreaterThan(20)
    expect(span).toBeGreaterThan(3)

    const d = await diagnostics(page)
    const latest = async (key: string) =>
      Number(await page.getByTestId(`chart-${key}`).getAttribute('data-latest'))
    const near = (shown: number, actual: number) =>
      Math.abs(shown - actual) <= Math.max(0.01, Math.abs(actual) * 0.01)

    expect(near(await latest('heelDeg'), d.heel_deg)).toBe(true)
    expect(near(await latest('boatSpeed'), d.speed_over_ground)).toBe(true)
    expect(near(await latest('sheetTension'), d.sheet_tension)).toBe(true)
    expect(near(await latest('alphaSail'), d.alpha_sail)).toBe(true)
    expect(near(await latest('yawRate'), d.yaw_rate)).toBe(true)

    // The ring buffer is bounded: a long run cannot grow it without limit.
    expect(samples).toBeLessThanOrEqual(600)
  })

  test('the sample rate is simulated time, not frames', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)
    const charts = page.getByTestId('charts')

    await page.getByTestId('chart-sample-hz').selectOption('5')
    await page.waitForTimeout(4_000)
    const slow = Number(await charts.getAttribute('data-samples'))
    const slowSpan = Number(await charts.getAttribute('data-span'))

    // At 5 Hz, four seconds of simulated time is about twenty samples — far
    // fewer than the ~240 frames the browser drew in the same window.
    expect(slow).toBeGreaterThan(5)
    expect(slow).toBeLessThan(60)
    expect(slowSpan / Math.max(1, slow - 1)).toBeGreaterThan(0.15)
  })

  test('no console errors with every overlay and every chart enabled', async ({ page }) => {
    // The fixture's trap fails the test on any console error or page error;
    // this test exists to drive the page hard while it is armed.
    await gotoApp(page, { scenario: 'capsize' })
    await enterDebug(page)
    await setAllOverlays(page, true)
    await page.getByTestId('charts-all-on').click()
    await page.getByTestId('parameter-panel-toggle').click()
    await page.getByTestId('clock-speed-4x').click()
    await page.waitForTimeout(4_000)

    // Still alive, still advancing, and the overlay is still drawing.
    await expect(page.getByTestId('force-overlay')).toHaveAttribute('data-active', '16')
    expect(Number(await page.getByTestId('charts').getAttribute('data-samples'))).toBeGreaterThan(10)
    await expect(page.getByTestId('wasm-status')).toHaveAttribute('data-ready', 'true')
  })

  test('R6 is surfaced as a warning while the hull model is extrapolating', async ({ page }) => {
    // `fast` starts at u = 6.5 m/s, above the documented validity limit, and
    // the hull drags it back below — so the warning is asserted in both
    // directions on one run, against `u` itself.
    await gotoApp(page, { scenario: 'fast' })
    await enterDebug(page)

    expect(Number((await page.getByTestId('snapshot').getAttribute('data-u')) ?? 0)).toBeGreaterThan(5)
    await expect(page.getByTestId('hull-model-warning')).toHaveCount(1)
    await expect(page.getByTestId('diag-hull_model_warning')).toHaveAttribute('data-value', 'true')

    // Hull resistance alone takes it under 5 m/s well inside this window.
    await expect
      .poll(
        async () => Number((await page.getByTestId('snapshot').getAttribute('data-u')) ?? 0),
        { timeout: 20_000 },
      )
      .toBeLessThan(4.8)
    await expect(page.getByTestId('hull-model-warning')).toHaveCount(0)
    await expect(page.getByTestId('diag-hull_model_warning')).toHaveAttribute('data-value', 'false')
  })
})
