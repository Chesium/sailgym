import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import type { Page } from '@playwright/test'

import { expect, gotoApp, test } from './fixtures'

/**
 * Section 03 (M2): the wind field in the browser.
 *
 * Two guarantees are load-bearing and both are asserted here rather than
 * described:
 *
 *  - **One WASM call per frame** (brief §19). The application counts its own
 *    `sample_wind_grid` calls and publishes the total, so the property is
 *    observable rather than instrumented from outside.
 *  - **The dense field is not in the DOM** (brief §39). Four thousand
 *    particles must be WebGL; an SVG node count proves they are.
 */

async function stats(page: Page): Promise<Record<string, number>> {
  return page.evaluate(() => {
    const el = document.querySelector('[data-testid="wind-stats"]')
    if (!(el instanceof HTMLElement)) {
      throw new Error('no [data-testid="wind-stats"] element on the page')
    }
    const out: Record<string, number> = {}
    for (const [key, value] of Object.entries(el.dataset)) {
      out[key] = Number(value)
    }
    return out
  })
}

/** Wait until the application has rendered `n` more frames. */
async function advanceFrames(page: Page, n: number): Promise<void> {
  const before = (await stats(page)).frames
  await expect
    .poll(async () => (await stats(page)).frames, { timeout: 20_000 })
    .toBeGreaterThanOrEqual(before + n)
}

async function deckLayers(page: Page): Promise<number> {
  const value = await page.getByTestId('deck-overlay').getAttribute('data-layers')
  return Number(value)
}

test.describe('wind', () => {
  test('samples the field once per frame, and never per point', async ({ page }) => {
    await gotoApp(page)
    await expect(page.getByTestId('deck-overlay')).toBeVisible()

    const before = await stats(page)
    await advanceFrames(page, 60)
    const after = await stats(page)

    const frames = after.frames - before.frames
    const calls = after.gridCalls - before.gridCalls
    expect(frames).toBeGreaterThanOrEqual(60)
    // brief §19: one batched call per animation frame, never more.
    expect(calls).toBeGreaterThan(0)
    expect(calls).toBeLessThanOrEqual(frames)

    // …and the per-point `sample` is not reachable from JavaScript at all, so
    // no future code *can* make thousands of crossings a frame.
    const declarations = readFileSync(
      fileURLToPath(new URL('../../src/wasm/sailgym_wasm.d.ts', import.meta.url)),
      'utf8',
    )
    expect(declarations).toContain('sample_wind_grid(')
    expect(declarations).not.toMatch(/^\s+sample\(/m)
  })

  test('draws the dense field in WebGL, not in the DOM', async ({ page }) => {
    await gotoApp(page)

    const canvases = page.locator('[data-testid="deck-overlay"] canvas')
    await expect(canvases).toHaveCount(1)
    await advanceFrames(page, 5)

    // brief §39: the dense field must never be thousands of SVG elements.
    const svgNodes = await page.evaluate(
      () => document.querySelectorAll('svg line, svg circle').length,
    )
    expect(svgNodes).toBeLessThan(400)

    // …while there really are thousands of particles being drawn.
    expect((await stats(page)).particles).toBeGreaterThan(1000)
  })

  test('survives ten seconds and every mode switch without losing the GL context', async ({
    page,
  }) => {
    // A deliberate soak (R5), plus four mode switches. On a browser rendering
    // four thousand particles in software that does not fit in the default
    // per-test budget, and the ten seconds are the point of the test.
    test.setTimeout(75_000)
    await gotoApp(page)
    await advanceFrames(page, 5)

    // R5. Count the events rather than trusting that they cannot happen.
    await page.evaluate(() => {
      const canvas = document.querySelector('[data-testid="deck-overlay"] canvas')
      if (!(canvas instanceof HTMLCanvasElement)) {
        throw new Error('no deck.gl canvas')
      }
      const w = window as unknown as { __glLost: number }
      w.__glLost = 0
      canvas.addEventListener('webglcontextlost', () => {
        w.__glLost += 1
      })
    })

    // Section acceptance criterion 5: every mode, at runtime, no reload.
    for (const mode of ['uniform', 'spatial', 'gust', 'uniform']) {
      await page.getByTestId('wind-mode').selectOption(mode)
      await expect(page.getByTestId('wind-mode')).toHaveValue(mode)
      await advanceFrames(page, 10)
      await expect(page.locator('[data-testid="deck-overlay"] canvas')).toHaveCount(1)
    }

    await page.waitForTimeout(10_000)

    const lost = await page.evaluate(() => (window as unknown as { __glLost: number }).__glLost)
    expect(lost).toBe(0)
    await expect(page.locator('[data-testid="deck-overlay"] canvas')).toHaveCount(1)
    // Still running, still sampling.
    await advanceFrames(page, 5)
  })

  test('the arrow overlay is exactly one deck layer, on and off', async ({ page }) => {
    await gotoApp(page)
    await advanceFrames(page, 5)

    const off = await deckLayers(page)
    expect(off).toBeGreaterThan(0)

    await page.getByTestId('toggle-wind-arrows').click()
    await expect(page.getByTestId('toggle-wind-arrows')).toHaveAttribute('data-on', 'true')
    await advanceFrames(page, 3)
    expect(await deckLayers(page)).toBe(off + 1)

    await page.getByTestId('toggle-wind-arrows').click()
    await expect(page.getByTestId('toggle-wind-arrows')).toHaveAttribute('data-on', 'false')
    await advanceFrames(page, 3)
    expect(await deckLayers(page)).toBe(off)
  })

  test('a westerly reads 270 degrees and its arrows point east', async ({ page }) => {
    // `close_hauled` is the shipped scenario with a westerly (section 09):
    // uniform 3.5 m/s from bearing 270. Before section 09 this test used the
    // page default, whose bearing was 270 because `WindConfig::default()`
    // said so; the default is now the `free_sail` scenario, which is a
    // northerly, so the westerly has to be asked for by name.
    await gotoApp(page, { scenario: 'close_hauled' })
    // Uniform: the wind at the boat is exactly the configured vector, so the
    // readout is an exact statement about the from/toward convention rather
    // than a gusty approximation of one.
    await page.getByTestId('wind-mode').selectOption('uniform')
    await page.getByTestId('toggle-wind-arrows').click()
    await advanceFrames(page, 5)

    const readout = page.getByTestId('wind-readout')
    const bearing = Number(await readout.getAttribute('data-bearing'))
    const speed = Number(await readout.getAttribute('data-speed'))
    const wx = Number(await readout.getAttribute('data-wx'))
    const wy = Number(await readout.getAttribute('data-wy'))

    // From the west…
    expect(Math.abs(bearing - 270)).toBeLessThanOrEqual(1)
    await expect(readout).toContainText('270°')
    expect(speed).toBeGreaterThan(0)
    // …means blowing toward the east: world +x, and nothing north or south.
    expect(wx).toBeGreaterThan(0)
    expect(Math.abs(wy)).toBeLessThan(1e-9)

    // And the glyphs agree. The camera is northUp, so screen +x is east and
    // screen +y is south.
    const arrows = page.getByTestId('wind-arrows')
    expect(Number(await arrows.getAttribute('data-count'))).toBeGreaterThan(10)
    const dx = Number(await arrows.getAttribute('data-dx'))
    const dy = Number(await arrows.getAttribute('data-dy'))
    expect(dx).toBeGreaterThan(0)
    expect(Math.abs(dy)).toBeLessThan(Math.abs(dx) * 0.05)
  })
})
