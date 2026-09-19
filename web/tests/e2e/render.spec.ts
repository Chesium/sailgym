import { expect, gotoApp, test } from './fixtures'

/**
 * SVG boat and camera (task 2.7, brief §25, §27).
 */
test.describe('render', () => {
  test('the boat is drawn and moves while the simulation runs', async ({ page }) => {
    await gotoApp(page)
    const hull = page.getByTestId('boat-hull')
    await expect(hull).toBeAttached()

    const before = await hull.getAttribute('transform')
    await page.waitForTimeout(1000)
    const after = await hull.getAttribute('transform')

    expect(before).not.toBeNull()
    expect(after).not.toBe(before)
  })

  test('the rig and the trajectory are drawn', async ({ page }) => {
    await gotoApp(page)
    for (const id of ['boat-boom', 'boat-sail', 'boat-rudder', 'boat-board', 'boat-mast']) {
      await expect(page.getByTestId(id)).toBeAttached()
    }

    await page.waitForTimeout(1200)
    const points = await page.getByTestId('trajectory').getAttribute('points')
    expect(points).not.toBeNull()
    expect((points ?? '').trim().split(/\s+/).length).toBeGreaterThan(1)
  })

  test('northUp has no rotation, follow does', async ({ page }) => {
    await gotoApp(page)
    const mode = page.getByTestId('camera-mode')
    await expect(mode).toHaveAttribute('data-mode', 'northUp')

    const grid = page.getByTestId('world-grid')
    expect(await grid.getAttribute('transform')).not.toContain('rotate')

    await mode.click()
    await expect(mode).toHaveAttribute('data-mode', 'follow')
    await page.waitForTimeout(300)
    expect(await grid.getAttribute('transform')).toContain('rotate')
  })

  test('the wheel zooms and reset camera restores it', async ({ page }) => {
    await gotoApp(page)
    const view = page.getByTestId('world-view')
    const grid = page.getByTestId('world-grid')

    const scaleOf = (transform: string | null) => {
      const m = /scale\((-?[\d.]+)/.exec(transform ?? '')
      return m === null ? NaN : Number(m[1])
    }

    const before = scaleOf(await grid.getAttribute('transform'))
    await view.hover()
    await page.mouse.wheel(0, -600)
    await page.waitForTimeout(200)
    const zoomed = scaleOf(await grid.getAttribute('transform'))
    expect(zoomed).toBeGreaterThan(before)

    await page.getByTestId('camera-reset').click()
    await page.waitForTimeout(200)
    expect(scaleOf(await grid.getAttribute('transform'))).toBeCloseTo(before, 6)
  })
})
