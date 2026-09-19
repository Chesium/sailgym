import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * The simulation clock in the browser (brief §22, task 2.5).
 *
 * Every assertion is on numeric simulation state read out of the snapshot
 * attributes, never on pixels (brief §42).
 */
test.describe('clock', () => {
  test('pause freezes the simulation clock', async ({ page }) => {
    await gotoApp(page)
    await page.getByTestId('clock-pause').click()
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')

    const before = await readSnapshot(page)
    await page.waitForTimeout(600)
    const after = await readSnapshot(page)
    expect(after.t).toBe(before.t)
  })

  test('single-step advances the clock by exactly one dt', async ({ page }) => {
    await gotoApp(page)
    await page.getByTestId('clock-pause').click()

    const before = await readSnapshot(page)
    expect(before.dt).toBeGreaterThan(0)
    await page.getByTestId('clock-step').click()
    const after = await readSnapshot(page)

    expect(Math.abs(after.t - before.t - before.dt)).toBeLessThan(1e-9)

    // Three more steps, three more dt.
    for (let i = 0; i < 3; i += 1) {
      await page.getByTestId('clock-step').click()
    }
    const later = await readSnapshot(page)
    expect(Math.abs(later.t - before.t - 4 * before.dt)).toBeLessThan(1e-9)
  })

  test('single-step is disabled while running', async ({ page }) => {
    await gotoApp(page)
    await expect(page.getByTestId('clock-step')).toBeDisabled()
    await page.getByTestId('clock-pause').click()
    await expect(page.getByTestId('clock-step')).toBeEnabled()
  })

  test('2x advances simulated time about twice as fast as 1x', async ({ page }) => {
    await gotoApp(page)

    const elapsedOver = async (windowMs: number) => {
      const before = await readSnapshot(page)
      await page.waitForTimeout(windowMs)
      const after = await readSnapshot(page)
      return after.t - before.t
    }

    await page.getByTestId('clock-speed-1x').click()
    const single = await elapsedOver(2000)

    await page.getByTestId('clock-speed-2x').click()
    const double = await elapsedOver(2000)

    expect(single).toBeGreaterThan(1.0)
    const ratio = double / single
    expect(ratio).toBeGreaterThanOrEqual(1.7)
    expect(ratio).toBeLessThanOrEqual(2.3)
  })

  test('all four speeds are selectable and keep the simulator running', async ({ page }) => {
    await gotoApp(page)
    for (const speed of ['0.25', '1', '2', '4']) {
      const button = page.getByTestId(`clock-speed-${speed}x`)
      await button.click()
      await expect(button).toHaveAttribute('data-active', 'true')

      const before = await readSnapshot(page)
      await page.waitForTimeout(500)
      const after = await readSnapshot(page)
      expect(after.t).toBeGreaterThan(before.t)
    }
  })

  test('reset restores the deterministic initial state', async ({ page }) => {
    await gotoApp(page)
    await page.waitForTimeout(500)
    expect((await readSnapshot(page)).t).toBeGreaterThan(0)

    await page.getByTestId('clock-pause').click()
    await page.getByTestId('clock-reset').click()
    const after = await readSnapshot(page)
    expect(after.t).toBe(0)
    expect(after.x).toBe(0)
    expect(after.y).toBe(0)
    expect(after.u).toBe(0)
    expect(after.psi).toBe(0)
  })
})
