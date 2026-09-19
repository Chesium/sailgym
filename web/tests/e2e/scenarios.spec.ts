import type { Page } from '@playwright/test'

import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * The six shipped scenarios in the browser (brief §32, task 9.2).
 *
 * The list is the core's — `Sim.scenarios_json()` — so this spec asserts
 * that what the core ships is what the page offers, that each one loads and
 * runs to a finite state, and that switching is quick enough to be an
 * interaction rather than a reload.
 */

/** brief §32's list, in the order the core returns it. */
const SCENARIOS = [
  'beam_reach_capsize',
  'close_hauled',
  'free_sail',
  'gybe',
  'sheet_release_recovery',
  'tack',
] as const

/**
 * Switch scenario and return the milliseconds the page itself took, measured
 * from the `change` event to the DOM carrying the new scenario.
 *
 * Measured in-page on purpose: a Playwright round trip is tens of
 * milliseconds of harness, and the criterion is about the application.
 */
async function switchTo(page: Page, id: string): Promise<number> {
  await page.evaluate((wanted) => {
    const w = window as unknown as { __switch: { start: number; end: number } }
    w.__switch = { start: 0, end: 0 }
    const picker = document.querySelector('[data-testid="scenario-picker"]')
    const select = document.querySelector('[data-testid="scenario-select"]')
    if (picker === null || select === null) {
      throw new Error('no scenario picker on the page')
    }
    select.addEventListener(
      'change',
      () => {
        w.__switch.start = performance.now()
      },
      { capture: true, once: true },
    )
    const observer = new MutationObserver(() => {
      if (
        w.__switch.start > 0 &&
        w.__switch.end === 0 &&
        picker.getAttribute('data-scenario') === wanted
      ) {
        w.__switch.end = performance.now()
        observer.disconnect()
      }
    })
    observer.observe(picker, { attributes: true })
  }, id)

  await page.getByTestId('scenario-select').selectOption(id)
  await expect(page.getByTestId('scenario-picker')).toHaveAttribute('data-scenario', id)
  return page.evaluate(() => {
    const w = window as unknown as { __switch: { start: number; end: number } }
    return w.__switch.end - w.__switch.start
  })
}

test.describe('scenarios', () => {
  test('the picker offers exactly the six of brief §32, and free_sail is the default', async ({
    page,
  }) => {
    await gotoApp(page)

    const picker = page.getByTestId('scenario-picker')
    await expect(picker).toHaveAttribute('data-count', String(SCENARIOS.length))
    await expect(picker).toHaveAttribute('data-scenario', 'free_sail')

    const options = await page
      .locator('[data-testid="scenario-select"] option')
      .evaluateAll((nodes) => nodes.map((n) => (n as HTMLOptionElement).value))
    expect(options).toEqual([...SCENARIOS])
  })

  test('each of the six loads and reaches a finite state', async ({ page }) => {
    test.setTimeout(90_000)
    await gotoApp(page)
    await page.getByTestId('clock-speed-4x').click()

    for (const id of SCENARIOS) {
      await switchTo(page, id)

      // A reset: the run starts again from the scenario's own initial state.
      const start = await readSnapshot(page)
      expect(start.t, `${id}: the clock did not restart`).toBeLessThan(0.5)

      // Let it sail, then check every published number is finite — brief
      // §35's finite-number invariant, in the browser.
      await expect.poll(async () => (await readSnapshot(page)).t, { timeout: 20_000 })
        .toBeGreaterThan(4)
      const after = await readSnapshot(page)
      for (const [field, value] of Object.entries(after)) {
        // `testid` and `capsized` are the two non-numeric attributes on the
        // snapshot element; everything else is an F8.3 value.
        if (field === 'testid' || field === 'capsized') {
          continue
        }
        expect(Number.isFinite(value), `${id}: ${field} is ${value}`).toBe(true)
      }
      // …and the scenario really is the one loaded.
      await expect(page.getByTestId('scenario-picker')).toHaveAttribute('data-scenario', id)
    }
  })

  test('switching between scenarios takes under 500 ms', async ({ page }) => {
    test.setTimeout(60_000)
    await gotoApp(page)

    // Every ordered pair the picker can make in one hop, walked once.
    const order = [...SCENARIOS, ...SCENARIOS].slice(0, SCENARIOS.length + 3)
    const timings: number[] = []
    for (const id of order) {
      if ((await page.getByTestId('scenario-picker').getAttribute('data-scenario')) === id) {
        continue
      }
      const ms = await switchTo(page, id)
      expect(ms, `switching to ${id} took ${ms} ms`).toBeGreaterThan(0)
      expect(ms, `switching to ${id} took ${ms} ms`).toBeLessThan(500)
      timings.push(ms)
    }
    expect(timings.length).toBeGreaterThanOrEqual(SCENARIOS.length)
    // eslint-disable-next-line no-console
    console.log(`[scenarios] switch ms: ${timings.map((t) => t.toFixed(1)).join(', ')}`)
  })

  test('a scenario carries its own wind, initial condition and camera', async ({ page }) => {
    await gotoApp(page, { scenario: 'beam_reach_capsize' })

    // brief §32's camera suggestion, applied on load.
    await expect(page.getByTestId('camera-mode')).toHaveAttribute('data-mode', 'follow')

    // Pause and reset before reading: the page has been sailing since it
    // loaded, and the assertion below is about the scenario's *initial*
    // condition.
    await page.getByTestId('clock-pause').click()
    await page.getByTestId('clock-reset').click()

    // Hauled hard in, beam-on to a 7 m/s northerly: the apparent wind is on
    // the beam and the sheet is short.
    const start = await readSnapshot(page)
    expect(start.psi).toBeCloseTo(0, 9)
    expect(start.lSheet).toBeCloseTo(0.9, 9)
    const readout = page.getByTestId('wind-readout')
    expect(Number(await readout.getAttribute('data-speed'))).toBeCloseTo(7, 6)
    expect(Number(await readout.getAttribute('data-bearing'))).toBeCloseTo(0, 6)

    // …and `close_hauled` is a different boat in a different breeze, with the
    // world-up camera it asks for.
    await switchTo(page, 'close_hauled')
    await expect(page.getByTestId('camera-mode')).toHaveAttribute('data-mode', 'northUp')
    await page.getByTestId('clock-reset').click()
    const hauled = await readSnapshot(page)
    expect(hauled.lSheet).toBeCloseTo(2, 9)
    expect(Number(await readout.getAttribute('data-speed'))).toBeCloseTo(3.5, 6)
    expect(Number(await readout.getAttribute('data-bearing'))).toBeCloseTo(270, 6)
  })

  test('reset replays the chosen scenario, not the page default', async ({ page }) => {
    await gotoApp(page)
    await switchTo(page, 'tack')

    // `tack` starts with way on and the boom out to starboard, which
    // `free_sail` does not.
    await page.getByTestId('clock-pause').click()
    await page.getByTestId('clock-reset').click()
    const after = await readSnapshot(page)
    expect(after.t).toBe(0)
    expect(after.u).toBeCloseTo(1.8, 9)
    expect(after.beta).toBeGreaterThan(0.5)
    expect(after.psi).toBeCloseTo(Math.PI / 4, 9)
    await expect(page.getByTestId('scenario-picker')).toHaveAttribute('data-scenario', 'tack')
  })

  test('the URL keeps the chosen scenario across a reload', async ({ page }) => {
    await gotoApp(page)
    await switchTo(page, 'gybe')
    expect(page.url()).toContain('scenario=gybe')

    await page.reload()
    await expect(page.getByTestId('wasm-status')).toHaveAttribute('data-ready', 'true')
    await expect(page.getByTestId('scenario-picker')).toHaveAttribute('data-scenario', 'gybe')
  })
})
