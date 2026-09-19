import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Hydrodynamics in the browser (task 4.6).
 *
 * The `coast` fixture supplies initial surge speed and explicitly zero wind.
 * With M4, the sail also experiences the still-air flow caused by boat motion;
 * no component supplies propulsion.
 *
 * What is asserted is what the real model now produces and the placeholder
 * could not: a rudder whose authority comes from the water it moves through, a
 * hull that slows the boat down on its own, and a tiller that re-centres in
 * Rust rather than in TypeScript.
 *
 * Numbers rather than pixels (brief §42). `psi` is CCW-positive and therefore
 * negative to starboard (F2), and `r` never wraps, so the polls below watch
 * `r` and read `psi` once the turn is established.
 */
test.describe('hydro', () => {
  test('the coasting boat slows down under hull and foil drag alone', async ({ page }) => {
    await gotoApp(page, { scenario: 'coast' })
    const start = await readSnapshot(page)
    expect(start.u).toBeGreaterThan(0)

    await expect
      .poll(async () => (await readSnapshot(page)).u, { timeout: 20_000 })
      .toBeLessThan(start.u * 0.8)

    const later = await readSnapshot(page)
    // Hull plus foil drag only: it decelerates, and it never reverses.
    expect(later.u).toBeGreaterThan(0)
    expect(later.x).toBeGreaterThan(start.x)
  })

  test('holding D curves the trajectory to starboard', async ({ page }) => {
    await gotoApp(page, { scenario: 'coast' })
    await page.keyboard.down('d')
    // A rightward turn is `r < 0` (yaw rate is positive to port, F2).
    await expect
      .poll(async () => (await readSnapshot(page)).r, { timeout: 20_000 })
      .toBeLessThan(-0.1)

    const turning = await readSnapshot(page)
    await page.keyboard.up('d')

    expect(turning.deltaR).toBeGreaterThan(0)
    expect(turning.psi).toBeLessThan(0)
    // Starting on a heading of due east, a sustained turn to starboard bends
    // the track south. The rudder's own side force is to port (F2.2), so the
    // boat is nudged north for the first fraction of a second before the yaw
    // takes over; the poll waits for the turn, it does not assume it.
    await expect
      .poll(async () => (await readSnapshot(page)).y, { timeout: 20_000 })
      .toBeLessThan(0)

    // Releasing every key: Rust re-centres the tiller (F4.3), and the hull and
    // rudder between them damp the rotation out.
    await expect
      .poll(async () => Math.abs((await readSnapshot(page)).deltaR), { timeout: 20_000 })
      .toBeLessThan(0.02)
    await expect
      .poll(async () => Math.abs((await readSnapshot(page)).r), { timeout: 20_000 })
      .toBeLessThan(Math.abs(turning.r) * 0.5)
  })
})
