import type { Page } from '@playwright/test'
import type { Diagnostics } from '../../src/sim/diagnostics'
import type { SimHandle } from '../../src/sim/loadWasm'
import { expect, gotoApp, readSnapshot, test } from './fixtures'

async function endpoint(page: Page) {
  return page.getByTestId('boat-boom').evaluate((element) => {
    const line = element as SVGLineElement
    const hull = document.querySelector('[data-testid="boat-hull"]') as SVGGElement
    const matrix = line.getScreenCTM()!
    const h = hull.getScreenCTM()!
    const mast = new DOMPoint(line.x1.baseVal.value, line.y1.baseVal.value).matrixTransform(matrix)
    const tip = new DOMPoint(line.x2.baseVal.value, line.y2.baseVal.value).matrixTransform(matrix)
    const origin = new DOMPoint(0, 0).matrixTransform(h)
    const starboard = new DOMPoint(0, -1).matrixTransform(h)
    return {
      x: tip.x - mast.x, y: tip.y - mast.y,
      starboardProjection: (tip.x - mast.x) * (starboard.x - origin.x)
        + (tip.y - mast.y) * (starboard.y - origin.y),
    }
  })
}

/**
 * The drawn boom's clew end.
 *
 * v2 section 01 bakes `β` into the boom line's projected endpoints, so the
 * `<g data-testid="boom">` wrapper no longer carries a `rotate(β …)` transform
 * to compare — emitting a decorative one purely so this assertion kept passing
 * would be the test-shaped fiction F13.4 forbids. Reading the line's own `x2`
 * and `y2` is strictly stronger: it asserts that the drawn boom moved, not
 * that an attribute string differs.
 */
async function boomTip(page: Page) {
  return page.getByTestId('boat-boom').evaluate((element) => {
    const line = element as SVGLineElement
    return `${line.x2.baseVal.value},${line.y2.baseVal.value}`
  })
}

async function pausedReset(page: Page) {
  await page.getByTestId('clock-pause').click()
  await page.getByTestId('clock-reset').click()
}

test.describe('sail', () => {
  test('positive beta rotates the rendered endpoint to starboard', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await pausedReset(page)
    const before = await boomTip(page)
    await page.getByTestId('clock-pause').click()
    await expect.poll(async () => (await readSnapshot(page)).beta).toBeGreaterThan(0.4)
    await page.getByTestId('clock-pause').click()
    expect((await endpoint(page)).starboardProjection).toBeGreaterThan(0)
    expect(await boomTip(page)).not.toBe(before)
  })

  test('free sail visibly swings within ten seconds and accelerates under wind', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await pausedReset(page)
    const before = await endpoint(page)
    expect((await readSnapshot(page)).u).toBe(0)
    await page.getByTestId('clock-pause').click()
    await page.waitForTimeout(10_000)
    await page.getByTestId('clock-pause').click()
    const after = await endpoint(page)
    expect(Math.hypot(after.x - before.x, after.y - before.y)).toBeGreaterThan(20)
    const st = await readSnapshot(page)
    expect(st.u).toBeGreaterThan(0)
    expect(Math.hypot(st.x, st.y)).toBeGreaterThan(0.1)
  })

  test('HUD matches WASM diagnostics and the published snapshot', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await page.waitForTimeout(500)
    await page.getByTestId('clock-pause').click()
    const probe = await page.evaluate(async (url) => {
      const mod = await import(/* @vite-ignore */ url) as { loadWasm(): Promise<{ Sim: new (s: string) => SimHandle }> }
      const wasm = await mod.loadWasm()
      const sim = new wasm.Sim('{}')
      try {
        const element = document.querySelector('[data-testid="snapshot"]') as HTMLElement
        const state: Record<string, number> = {}
        for (const [key, value] of Object.entries(element.dataset)) {
          if (key !== 'testid' && key !== 'dt') state[key.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)] = Number(value)
        }
        const wind = { ...JSON.parse(sim.wind_json() as string), mode: 'uniform', speed: 5, bearing_deg: 0 }
        sim.reset(JSON.stringify({ state, wind }))
        const expected = JSON.parse(sim.diagnostics() as string) as Diagnostics
        const hud = document.querySelector('[data-testid="sail-hud"]') as HTMLElement
        const actual = JSON.parse(hud.dataset.diagnostics!) as Diagnostics
        const value = (id: string) => Number((document.querySelector(`[data-testid="${id}"]`) as HTMLElement).dataset.value)
        return { expected, actual, speed: value('apparent-wind-speed'), boat: value('boat-speed'), angle: value('apparent-wind-angle'), state }
      } finally { sim.free() }
    }, '/src/sim/loadWasm.ts')
    expect(Math.abs(probe.speed - probe.expected.apparent_wind_speed)).toBeLessThan(0.1)
    expect(probe.actual.t).toBe(probe.state.t)
    expect(probe.actual.apparent_wind_body).toEqual(probe.expected.apparent_wind_body)
    expect(probe.angle).toBeCloseTo(probe.expected.apparent_wind_angle * 180 / Math.PI, 10)
    expect(probe.boat).toBeCloseTo(Math.hypot(probe.state.u, probe.state.v), 10)
  })

  test('steering changes the point of sail under wind', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await page.waitForTimeout(1000)
    await page.getByTestId('clock-pause').click()
    const initial = await readSnapshot(page)
    const angleBefore = Number(await page.getByTestId('apparent-wind-angle').getAttribute('data-value'))
    await page.keyboard.down('d')
    await page.getByTestId('clock-pause').click()
    await page.waitForTimeout(2000)
    await page.getByTestId('clock-pause').click()
    await page.keyboard.up('d')
    const after = await readSnapshot(page)
    expect(Math.abs(after.psi - initial.psi)).toBeGreaterThan(0.001)
    const angleAfter = Number(await page.getByTestId('apparent-wind-angle').getAttribute('data-value'))
    expect(Math.abs(angleAfter - angleBefore)).toBeGreaterThan(0.1)
  })
})
