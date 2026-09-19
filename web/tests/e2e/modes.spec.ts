import type { Page } from '@playwright/test'
import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * brief §29's two modes (task 8.2).
 *
 * The headline assertion is negative: Sail Mode's value is what it omits, so
 * the test that matters is the one counting what is *not* there.
 */

/** brief §29's Sail Mode list, one wrapper per item. */
const SAIL_READOUTS = [
  'sail-readout-wind',
  'sail-readout-speed',
  'sail-readout-heading',
  'sail-readout-heel',
  'sail-readout-rudder',
  'sail-readout-sheet',
  'sail-readout-capsize',
] as const

async function enterDebug(page: Page) {
  await page.getByTestId('mode-switch').click()
  await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
}

test.describe('modes', () => {
  test('Sail Mode is the default and shows exactly the seven brief §29 readouts', async ({
    page,
  }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'sail')

    // Exactly the seven, each present exactly once.
    for (const id of SAIL_READOUTS) {
      await expect(page.getByTestId(id)).toHaveCount(1)
    }
    expect(await page.locator('[data-testid^="sail-readout-"]').count()).toBe(
      SAIL_READOUTS.length,
    )
    expect(SAIL_READOUTS).toHaveLength(7)

    // And nothing from the debug set: no diagnostics rows, no panels.
    expect(await page.locator('[data-testid^="diag-"]').count()).toBe(0)
    await expect(page.getByTestId('layout-debug')).toHaveCount(0)
    await expect(page.getByTestId('debug-panel')).toHaveCount(0)
    await expect(page.getByTestId('force-overlay')).toHaveCount(0)
    await expect(page.getByTestId('charts')).toHaveCount(0)
    await expect(page.getByTestId('parameter-panel')).toHaveCount(0)
    await expect(page.getByTestId('overlay-controls')).toHaveCount(0)
  })

  test('each Sail Mode readout carries a live value', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await page.waitForTimeout(600)

    // Read in **one** round trip. The readouts and the snapshot element come
    // from the same React render, so comparing them is exact — but only if
    // they are sampled together; two `getAttribute` calls a few milliseconds
    // apart straddle a frame and the boat has moved in between.
    const probe = await page.evaluate(() => {
      const value = (id: string) =>
        Number(document.querySelector(`[data-testid="${id}"]`)?.getAttribute('data-value'))
      const snapshot = document.querySelector('[data-testid="snapshot"]') as HTMLElement
      return {
        heading: value('sail-readout-heading'),
        rudder: value('sail-readout-rudder'),
        sheet: value('sail-readout-sheet'),
        psi: Number(snapshot.dataset.psi),
        deltaR: Number(snapshot.dataset.deltaR),
        lSheet: Number(snapshot.dataset.lSheet),
      }
    })

    // The three that come straight off the snapshot are checked against it,
    // so "the readout is present" also means "the readout is right".
    expect(probe.heading).toBeCloseTo((probe.psi * 180) / Math.PI, 9)
    expect(probe.rudder).toBeCloseTo((probe.deltaR * 180) / Math.PI, 9)
    expect(probe.sheet).toBeCloseTo(probe.lSheet, 12)

    for (const id of ['boat-speed', 'heel-angle', 'capsize-state', 'wind-readout']) {
      await expect(page.getByTestId(id)).toHaveCount(1)
    }
  })

  test('Debug Mode renders a diagnostics row for every field group', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await enterDebug(page)

    await expect(page.getByTestId('debug-panel')).toHaveCount(1)
    for (const group of [
      'clock',
      'environment',
      'motion',
      'forces-b',
      'moments',
      'geometry-b',
      'coefficients',
      'rig',
      'stability',
      'health',
    ]) {
      await expect(page.getByTestId(`diag-group-${group}`)).toHaveCount(1)
    }
    // Every group contributes rows, and there are a lot of them.
    expect(await page.locator('[data-testid^="diag-"]').count()).toBeGreaterThan(40)

    // The rest of the debug column is mounted too.
    await expect(page.getByTestId('overlay-controls')).toHaveCount(1)
    await expect(page.getByTestId('force-overlay')).toHaveCount(1)
    await expect(page.getByTestId('charts')).toHaveCount(1)
    await expect(page.getByTestId('parameter-panel')).toHaveCount(1)

    // …and Sail Mode's seven are still there: Debug Mode adds, it does not
    // take away.
    expect(await page.locator('[data-testid^="sail-readout-"]').count()).toBe(7)
  })

  test('the M key switches modes, and the mode survives a reload', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await expect(page.getByTestId('mode-switch')).toHaveAttribute('data-mode', 'sail')

    await page.keyboard.press('m')
    await expect(page.getByTestId('mode-switch')).toHaveAttribute('data-mode', 'debug')

    await page.reload()
    await expect(page.getByTestId('wasm-status')).toHaveAttribute('data-ready', 'true')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
    await expect(page.getByTestId('debug-panel')).toHaveCount(1)

    // …and back, which must also survive.
    await page.locator('body').click({ position: { x: 2, y: 2 } })
    await page.keyboard.press('m')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'sail')
    await page.reload()
    await expect(page.getByTestId('wasm-status')).toHaveAttribute('data-ready', 'true')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'sail')
  })

  test('switching modes neither pauses the simulation nor changes the rate of t', async ({
    page,
  }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'true')

    /** Simulated seconds per wall-clock second over a 1.2 s window. */
    const rate = async () => {
      const before = (await readSnapshot(page)).t
      const wallBefore = Date.now()
      await page.waitForTimeout(1_200)
      const after = (await readSnapshot(page)).t
      return (after - before) / ((Date.now() - wallBefore) / 1000)
    }

    const inSail = await rate()
    await enterDebug(page)
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'true')
    const inDebug = await rate()

    // Both are real-time (1x), and the clock never stopped.
    expect(inSail).toBeGreaterThan(0.8)
    expect(inDebug).toBeGreaterThan(0.8)
    expect(Math.abs(inDebug - inSail)).toBeLessThan(0.25)

    // `t` did not jump, restart or rewind across the switch.
    const before = (await readSnapshot(page)).t
    await page.getByTestId('mode-switch').click()
    const after = (await readSnapshot(page)).t
    expect(after).toBeGreaterThanOrEqual(before)
    expect(after - before).toBeLessThan(0.5)
  })
})
