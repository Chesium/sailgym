import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Keyboard steering, end to end (task 2.6).
 *
 * This is the browser-level guard against R3, sign-convention drift: `D` must
 * put the rudder to positive `delta_r`, and per F2.2 that turns the bow to
 * **starboard**, i.e. `psi` decreases (ψ is CCW-positive, to port).
 */
test.describe('controls', () => {
  test('D steers the bow to starboard', async ({ page }) => {
    // The `coast` scenario gives the boat an initial speed. From section 04 the
    // rudder's authority comes from the water it moves through (F6.5), and the
    // M1 placeholder force model that used to push the boat along is gone; a
    // boat at rest cannot be steered, which is correct physics (brief §14).
    await gotoApp(page, { scenario: 'coast' })
    await page.keyboard.down('d')
    await page.waitForTimeout(1200)
    const held = await readSnapshot(page)
    await page.keyboard.up('d')

    expect(held.deltaR).toBeGreaterThan(0)
    expect(held.psi).toBeLessThan(0)
  })

  test('A mirrors both signs', async ({ page }) => {
    await gotoApp(page, { scenario: 'coast' })
    await page.keyboard.down('a')
    await page.waitForTimeout(1200)
    const held = await readSnapshot(page)
    await page.keyboard.up('a')

    expect(held.deltaR).toBeLessThan(0)
    expect(held.psi).toBeGreaterThan(0)
  })

  test('the arrow keys mirror A and D', async ({ page }) => {
    await gotoApp(page)
    await page.keyboard.down('ArrowRight')
    await page.waitForTimeout(600)
    const right = await readSnapshot(page)
    await page.keyboard.up('ArrowRight')
    expect(right.deltaR).toBeGreaterThan(0)
  })

  test('releasing the key lets Rust re-centre the rudder', async ({ page }) => {
    // Self-centring is `delta_r_self_centre` in Rust (F7); the browser only
    // stops sending a command.
    await gotoApp(page)
    await page.keyboard.down('d')
    await page.waitForTimeout(800)
    const held = await readSnapshot(page)
    await page.keyboard.up('d')
    await page.waitForTimeout(1200)
    const released = await readSnapshot(page)

    expect(held.deltaR).toBeGreaterThan(0.1)
    expect(Math.abs(released.deltaR)).toBeLessThan(held.deltaR)
  })

  test('Space eases the mainsheet', async ({ page }) => {
    // A scenario with rope left to pay out. The page default is `free_sail`
    // from section 09, whose sheet is at `l_sheet_max` so the boom swings
    // free — easing there is a no-op by construction, not a defect.
    // `close_hauled` starts at 2.0 m.
    await gotoApp(page, { scenario: 'close_hauled' })
    const before = await readSnapshot(page)
    await page.keyboard.down(' ')
    await page.waitForTimeout(600)
    const during = await readSnapshot(page)
    await page.keyboard.up(' ')

    expect(during.lSheet).toBeGreaterThan(before.lSheet)
  })

  test('P pauses and R resets', async ({ page }) => {
    await gotoApp(page)
    await page.waitForTimeout(400)
    await page.keyboard.press('p')
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')

    const paused = await readSnapshot(page)
    expect(paused.t).toBeGreaterThan(0)
    await page.waitForTimeout(300)
    expect((await readSnapshot(page)).t).toBe(paused.t)

    await page.keyboard.press('.')
    const stepped = await readSnapshot(page)
    expect(Math.abs(stepped.t - paused.t - paused.dt)).toBeLessThan(1e-9)

    await page.keyboard.press('r')
    expect((await readSnapshot(page)).t).toBe(0)
  })
})
