import { expect, gotoApp, test } from './fixtures'

/**
 * SVG boat and camera (task 2.7, brief §25, §27).
 */
test.describe('render', () => {
  test('the boat is drawn and moves while the simulation runs', async ({ page }) => {
    // `coast` gives the boat the speed it needs to move at all: the M1
    // placeholder thrust is gone (section 04) and the sail arrives in 05.
    await gotoApp(page, { scenario: 'coast' })
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

  test('the zoom buttons zoom and reset camera restores it', async ({ page }) => {
    await gotoApp(page)
    const grid = page.getByTestId('world-grid')

    const scaleOf = (transform: string | null) => {
      const m = /scale\((-?[\d.]+)/.exec(transform ?? '')
      return m === null ? NaN : Number(m[1])
    }
    const scale = async () => scaleOf(await grid.getAttribute('transform'))

    const before = await scale()
    await page.getByTestId('zoom-in-coarse').click()
    await expect.poll(scale).toBeCloseTo(before * 2, 6)
    await page.getByTestId('zoom-in-fine').click()
    await expect.poll(scale).toBeCloseTo(before * 2 * 1.1, 6)
    await page.getByTestId('zoom-out-fine').click()
    await expect.poll(scale).toBeCloseTo(before * 2, 6)
    await page.getByTestId('zoom-out-coarse').click()
    await expect.poll(scale).toBeCloseTo(before, 6)

    await page.getByTestId('zoom-in-coarse').click()
    await page.getByTestId('camera-reset').click()
    await expect.poll(scale).toBeCloseTo(before, 6)
  })

  test('the wheel scrolls the main row and does not zoom', async ({ page }) => {
    await gotoApp(page)
    const grid = page.getByTestId('world-grid')
    const main = page.getByTestId('layout-main')
    const before = await grid.getAttribute('transform')

    await page.getByTestId('world-view').hover()
    await page.mouse.wheel(0, 600)
    await page.waitForTimeout(200)

    const scaleOf = (transform: string | null) =>
      Number(/scale\((-?[\d.]+)/.exec(transform ?? '')?.[1])
    expect(scaleOf(await grid.getAttribute('transform'))).toBeCloseTo(scaleOf(before), 6)
    // Only scrollable when the rows below the world overflow it, which they do
    // at the default test viewport.
    const { scrollTop, overflowing } = await main.evaluate((e) => ({
      scrollTop: e.scrollTop,
      overflowing: e.scrollHeight > e.clientHeight,
    }))
    if (overflowing) {
      expect(scrollTop).toBeGreaterThan(0)
    }
  })

  test('the main row draws no scrollbar, so a press does not resize the world', async ({
    page,
  }) => {
    await gotoApp(page)
    const main = page.getByTestId('layout-main')
    // Headless browsers hide scrollbars whatever the page asks for, so a
    // measured gutter is zero either way; the declared style is what a real
    // window honours.
    expect(await main.evaluate((e) => getComputedStyle(e).scrollbarWidth)).toBe('none')
    const gutter = () =>
      main.evaluate((e) => (e as HTMLElement).offsetWidth - e.clientWidth)
    expect(await gutter()).toBe(0)

    const view = page.getByTestId('world-view')
    const width = await view.getAttribute('width')
    const box = await view.boundingBox()
    await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2)
    await page.mouse.down()
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-dragging', 'true')
    expect(await gutter()).toBe(0)
    expect(await view.getAttribute('width')).toBe(width)
    await page.mouse.up()
  })
})
