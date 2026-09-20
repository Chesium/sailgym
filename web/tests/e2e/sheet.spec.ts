import type { Page } from '@playwright/test'
import type { SimHandle } from '../../src/sim/loadWasm'
import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Mainsheet: the mouse drives `l_sheet` (task 6.3) and the drawn rope follows
 * the sheet state (task 6.4). Numbers, not pixels, wherever the assertion is
 * about physics (brief §42).
 */

const VIEW = { x: 400, y: 300 }

/** The sheet parameters the core actually holds, read across the boundary. */
async function sheetParams(page: Page) {
  return page.evaluate(async (url) => {
    const mod = (await import(/* @vite-ignore */ url)) as {
      loadWasm(): Promise<{ Sim: new (s: string) => SimHandle }>
    }
    const wasm = await mod.loadWasm()
    const sim = new wasm.Sim('{}')
    try {
      const p = JSON.parse(sim.parameters_json() as string) as {
        sail: { boom_length: number; mast_pos_b: { x: number } }
        sheet: {
          d_sheet: number
          block_pos_b: { x: number; y: number }
          l_sheet_min: number
        }
      }
      return {
        dSheet: p.sheet.d_sheet,
        block: p.sheet.block_pos_b,
        mastX: p.sail.mast_pos_b.x,
        boomLength: p.sail.boom_length,
        lSheetMin: p.sheet.l_sheet_min,
      }
    } finally {
      sim.free()
    }
  }, '/src/sim/loadWasm.ts')
}

/**
 * Rope path endpoints, the boom attachment and the block, all in screen pixels.
 *
 * The attachment is computed from the **boom line** that `BoatSvg` draws — a
 * different element, owned by a different task — and the block from the hull
 * transform plus `block_pos_b`. Neither is read off `SheetRope`'s own output,
 * so "the rope ends where the boom and the block are" is a real comparison.
 */
async function ropeGeometry(page: Page, p: Awaited<ReturnType<typeof sheetParams>>) {
  return page.evaluate((params) => {
    const path = document.querySelector('[data-testid="sheet-rope"]') as SVGPathElement
    const boom = document.querySelector('[data-testid="boat-boom"]') as SVGLineElement
    const hull = document.querySelector('[data-testid="boat-hull"]') as SVGGElement
    const pm = path.getScreenCTM()!
    const bm = boom.getScreenCTM()!
    const hm = hull.getScreenCTM()!
    const start = path.getPointAtLength(0).matrixTransform(pm)
    const end = path.getPointAtLength(path.getTotalLength()).matrixTransform(pm)
    const mast = new DOMPoint(boom.x1.baseVal.value, boom.y1.baseVal.value).matrixTransform(bm)
    const clew = new DOMPoint(boom.x2.baseVal.value, boom.y2.baseVal.value).matrixTransform(bm)
    const f = params.dSheet / params.boomLength
    const box = path.getBoundingClientRect()
    return {
      start: { x: start.x, y: start.y },
      end: { x: end.x, y: end.y },
      attach: { x: mast.x + f * (clew.x - mast.x), y: mast.y + f * (clew.y - mast.y) },
      block: (({ x, y }) => ({ x, y }))(
        new DOMPoint(params.block.x, params.block.y).matrixTransform(hm),
      ),
      length: path.getTotalLength(),
      boxHeight: box.height,
      slack: Number(path.dataset.slack),
    }
  }, p)
}

const gap = (a: { x: number; y: number }, b: { x: number; y: number }) =>
  Math.hypot(a.x - b.x, a.y - b.y)

test.describe('mainsheet input', () => {
  test('a downward drag hauls: l_sheet decreases', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const before = (await readSnapshot(page)).lSheet
    expect(before).toBeGreaterThan(1.0)

    await page.mouse.move(VIEW.x, VIEW.y)
    await page.mouse.down()
    await page.mouse.move(VIEW.x, VIEW.y + 150, { steps: 5 })
    await expect
      .poll(async () => (await readSnapshot(page)).lSheet, { timeout: 5_000 })
      .toBeLessThan(before - 0.2)
    await page.mouse.up()

    // Releasing the button stops the haul: the length settles and stays put.
    // The first wait is for the render to catch up — `data-l-sheet` is written
    // by React from the previous frame's snapshot, so reading it the instant
    // after `mouse.up` returns a value one frame stale, and the comparison
    // would see that frame's remaining haul (≈ 15 ms × 0.9 m/s) rather than
    // anything the sheet did afterwards.
    await page.waitForTimeout(300)
    const settled = (await readSnapshot(page)).lSheet
    await page.waitForTimeout(500)
    expect(Math.abs((await readSnapshot(page)).lSheet - settled)).toBeLessThan(1e-9)
  })

  test('Space pays the sheet out rapidly', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    await page.locator('[data-testid="world-view"]').click({ position: { x: 10, y: 10 } })
    const before = (await readSnapshot(page)).lSheet
    await page.keyboard.down(' ')
    await expect
      .poll(async () => (await readSnapshot(page)).lSheet, { timeout: 5_000 })
      .toBeGreaterThan(before + 0.5)
    await page.keyboard.up(' ')
  })

  test('a Shift-drag pans the camera and leaves the sheet alone', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    await page.getByTestId('clock-pause').click()
    const before = await readSnapshot(page)

    await page.keyboard.down('Shift')
    await page.mouse.move(VIEW.x, VIEW.y)
    await page.mouse.down()
    await page.mouse.move(VIEW.x + 40, VIEW.y + 200, { steps: 8 })
    await page.mouse.up()
    await page.keyboard.up('Shift')

    // The camera moved; `l_sheet` did not, to the last bit.
    await page.getByTestId('clock-pause').click()
    await page.waitForTimeout(300)
    await page.getByTestId('clock-pause').click()
    const after = await readSnapshot(page)
    expect(after.lSheet).toBe(before.lSheet)
  })
})

test.describe('mainsheet rope', () => {
  test('the rope is drawn between the boom attachment and the block', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const params = await sheetParams(page)
    const seen: number[] = []
    for (let i = 0; i < 3; i += 1) {
      await page.getByTestId('clock-pause').click()
      const beta = (await readSnapshot(page)).beta
      const g = await ropeGeometry(page, params)
      expect(gap(g.start, g.attach)).toBeLessThan(3)
      expect(gap(g.end, g.block)).toBeLessThan(3)
      seen.push(beta)
      await page.getByTestId('clock-pause').click()
      await page.waitForTimeout(700)
    }
    // Three genuinely different boom angles, not the same one three times.
    expect(Math.max(...seen) - Math.min(...seen)).toBeGreaterThan(0.05)
  })

  test('releasing adds visible sag; hauling fully in takes it away', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const params = await sheetParams(page)
    await page.locator('[data-testid="world-view"]').click({ position: { x: 10, y: 10 } })

    // Haul right in first. v2 F18.1b makes `l_sheet_min` the shortest path the
    // rope can take, so at the stop the rope is straight for every boom angle
    // and no sag is possible — which is what makes this a stable reference.
    // The stop is read from the core rather than named here (F7, F8): v1's
    // 0.90 m was *below* the geometric path and is the defect this section
    // corrected.
    const atTheStop = params.lSheetMin + 1e-9
    const haulRightIn = async () => {
      await page.mouse.move(VIEW.x, VIEW.y)
      await page.mouse.down()
      await page.mouse.move(VIEW.x, VIEW.y + 250, { steps: 5 })
      await expect
        .poll(async () => (await readSnapshot(page)).lSheet, { timeout: 10_000 })
        .toBeLessThan(atTheStop)
      const geometry = await ropeGeometry(page, params)
      await page.mouse.up()
      return geometry
    }

    const taut = await haulRightIn()
    expect(taut.slack).toBe(0)

    // Space, and the rope runs out. Polled rather than timed: the sheet pays
    // out at a fixed rate in *simulated* seconds, and a software-rendered
    // Firefox page reaches a given `l_sheet` several wall-clock frames after
    // Chromium does.
    //
    // The poll waits on the **slack itself**, not on the drawn bounding box.
    // The box grows for two reasons — the rope bows as it goes slack, and the
    // boom swings out and rotates the whole path — and only the first is what
    // this test is about. Waiting on the box let the poll finish while the
    // rope was still taut and the boom merely turning, and the `slack > 0`
    // assertion below then read a 0. That is what failed on Firefox in
    // section 06 and again in section 07's gate run; the box-height
    // assertions are unchanged and still checked, once there is slack to see.
    await page.keyboard.down(' ')
    await expect
      .poll(async () => (await ropeGeometry(page, params)).slack, { timeout: 8_000 })
      .toBeGreaterThan(0)
    await expect
      .poll(async () => (await ropeGeometry(page, params)).boxHeight, { timeout: 8_000 })
      .toBeGreaterThan(taut.boxHeight + 5)
    // Still held, so the rope is still running out: nothing read here can
    // have shrunk back since the poll saw it.
    const slack = await ropeGeometry(page, params)
    await page.keyboard.up(' ')
    expect(slack.slack).toBeGreaterThan(0)
    expect(slack.length).toBeGreaterThan(taut.length)
    expect(slack.boxHeight).toBeGreaterThan(taut.boxHeight + 5)

    // Haul it back in and the sag goes.
    expect((await haulRightIn()).slack).toBe(0)
  })
})
