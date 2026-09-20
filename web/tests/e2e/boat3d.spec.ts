import type { Page } from '@playwright/test'
import type { SimHandle } from '../../src/sim/loadWasm'
import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Heel, read in the browser (v2 section 01, task 1.7, brief §42).
 *
 * The boat is 3-D geometry projected into the top-down SVG, so heel is now
 * visible in the world view itself: the masthead swings out, the deck narrows,
 * a topside appears, and past 90° the underside and the centreboard come into
 * view. Every one of those is checked here against the **drawn** DOM, through
 * the same screen transform the viewer sees, and against numbers rather than
 * pixels.
 *
 * The fixed-angle probes are `render/BoatProbe.tsx` — the same device
 * `HeelProbe` has used since section 07, and for the same reason: the live
 * boat does not conveniently sit at 135° of heel while an assertion runs.
 * Their `data-testid`s are prefixed, so the live boat stays the only
 * `boat-mast`, `boat-board` and `boat-sail` on the page.
 */

const HULL_MATERIALS = ['deck', 'topsides', 'bilge', 'underside']

/**
 * `gotoApp`, with the fixed-angle boat probes mounted.
 *
 * They are opt-in (see `boatProbesRequested` in `src/App.tsx`), so the shipped
 * page does not carry nine spare boats. `addInitScript` runs before the
 * document loads, so the flag is set before React mounts — and every other
 * guarantee `gotoApp` provides, including the console-error trap, is unchanged.
 */
async function gotoWithProbes(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.__sailgymProbes = true
  })
  await gotoApp(page)
}

/** The F7 values the drawn geometry is derived from, read across the boundary. */
async function rigParams(page: Page) {
  return page.evaluate(async (url) => {
    const mod = (await import(/* @vite-ignore */ url)) as {
      loadWasm(): Promise<{ Sim: new (s: string) => SimHandle }>
    }
    const wasm = await mod.loadWasm()
    const sim = new wasm.Sim('{}')
    try {
      const p = JSON.parse(sim.parameters_json() as string) as {
        hull: { beam: number }
        sail: { area: number; boom_length: number }
        sheet: { z_boom: number }
      }
      return {
        beam: p.hull.beam,
        // The same derivation `visualDims.mastHeight` makes: the drawn sail is
        // the triangle tack → head → clew, and its area is `sail.area` by
        // construction. Restated here rather than imported, so the browser
        // check is independent of the module it is checking.
        mastHeight: p.sheet.z_boom + (2 * p.sail.area) / p.sail.boom_length,
      }
    } finally {
      sim.free()
    }
  }, '/src/sim/loadWasm.ts')
}

/**
 * Geometry of one drawn boat subtree, in **metres across the hull**.
 *
 * Everything is read through the rendered screen CTMs and projected onto the
 * group's own "to port" direction, so what is measured is what is drawn, at
 * whatever scale and handedness the enclosing SVG happens to use.
 */
async function drawnGeometry(page: Page, root: string, mast: string) {
  return page.evaluate(
    ([rootId, mastId, materials]) => {
      const group = document.querySelector(`[data-testid="${rootId}"]`) as SVGGElement
      const gm = group.getScreenCTM()!
      const origin = new DOMPoint(0, 0).matrixTransform(gm)
      const portPoint = new DOMPoint(0, 1).matrixTransform(gm)
      const port = { x: portPoint.x - origin.x, y: portPoint.y - origin.y }
      const scale = port.x * port.x + port.y * port.y
      /** Screen point → metres to port of the group origin. */
      const across = (p: DOMPoint) =>
        ((p.x - origin.x) * port.x + (p.y - origin.y) * port.y) / scale

      const line = document.querySelector(`[data-testid="${mastId}"]`) as SVGLineElement | null
      const lm = line === null ? null : line.getScreenCTM()!
      const masthead =
        line === null || lm === null
          ? null
          : across(
              new DOMPoint(line.x2.baseVal.value, line.y2.baseVal.value).matrixTransform(lm),
            )

      const hullPoints: number[] = []
      const seen: string[] = []
      for (const polygon of Array.from(group.querySelectorAll('polygon'))) {
        const material = polygon.getAttribute('data-material') ?? ''
        seen.push(material)
        if (!(materials as string[]).includes(material)) {
          continue
        }
        const pm = polygon.getScreenCTM()!
        for (const raw of (polygon.getAttribute('points') ?? '').trim().split(/\s+/)) {
          const [x, y] = raw.split(',').map(Number)
          hullPoints.push(across(new DOMPoint(x, y).matrixTransform(pm)))
        }
      }

      return {
        masthead,
        hullExtent:
          hullPoints.length === 0 ? 0 : Math.max(...hullPoints) - Math.min(...hullPoints),
        polygons: seen.length,
        materials: [...new Set(seen)].sort(),
      }
    },
    [root, mast, HULL_MATERIALS] as const,
  )
}

/** Rendered area of an element, in CSS pixels. */
async function renderedArea(page: Page, testId: string): Promise<number> {
  return page.evaluate((id) => {
    const element = document.querySelector(`[data-testid="${id}"]`)
    if (element === null) {
      return -1
    }
    const box = element.getBoundingClientRect()
    return box.width * box.height
  }, testId)
}

test.describe('boat in 3-D', () => {
  test('the boat is drawn as polygons', async ({ page }) => {
    await gotoApp(page)
    const count = await page.locator('[data-testid="boat-hull"] polygon').count()
    // Twenty is more than the deck cap alone; 144 is the model's whole budget.
    expect(count).toBeGreaterThanOrEqual(20)
    expect(count).toBeLessThanOrEqual(144)
  })

  test('the masthead swings with heel', async ({ page }) => {
    await gotoWithProbes(page)
    const { mastHeight } = await rigParams(page)
    for (const degrees of [30, -30, 60, -60, 90, -90]) {
      const phi = (degrees * Math.PI) / 180
      const g = await drawnGeometry(
        page,
        `boat-probe-${degrees}`,
        `boat-probe-${degrees}-boat-mast`,
      )
      expect(g.masthead, `${degrees}° has no mast`).not.toBeNull()
      // `y_H = y_B cos φ − z_B sin φ` with the mast on the centreline, so the
      // masthead's offset across the hull is `−mastHeight · sin φ`. The drawn
      // mast reaches `MASTHEAD_EXTENSION_FRACTION` of the luff above the head,
      // which is the 1.8 % this 3 % band absorbs.
      const expected = -mastHeight * Math.sin(phi)
      expect(
        Math.abs((g.masthead as number) - expected),
        `${degrees}°: drew ${g.masthead}, expected ${expected}`,
      ).toBeLessThan(0.03 * mastHeight)
    }
  })

  test('the deck narrows with heel', async ({ page }) => {
    await gotoWithProbes(page)
    const upright = await drawnGeometry(page, 'boat-probe-0', 'boat-probe-0-boat-mast')
    const heeled = await drawnGeometry(page, 'boat-probe-60', 'boat-probe-60-boat-mast')
    const knocked = await drawnGeometry(page, 'boat-probe-90', 'boat-probe-90-boat-mast')
    expect(upright.hullExtent).toBeGreaterThan(0)
    expect(heeled.hullExtent).toBeLessThan(upright.hullExtent)
    expect(knocked.hullExtent).toBeLessThan(heeled.hullExtent)

    const { beam } = await rigParams(page)
    // Upright, the hull is exactly its beam across. (The widest sheer vertices
    // share a station, so their heights cancel in the projection.)
    expect(Math.abs(upright.hullExtent - beam)).toBeLessThan(0.02 * beam)
  })

  test('a topside appears on the rising side', async ({ page }) => {
    await gotoWithProbes(page)
    // Upright, every topside is exactly edge-on and none is drawn.
    expect((await drawnGeometry(page, 'boat-probe-0', 'x')).materials).not.toContain('topsides')
    for (const degrees of [30, -30, 60, -60]) {
      expect(
        (await drawnGeometry(page, `boat-probe-${degrees}`, 'x')).materials,
        `${degrees}°`,
      ).toContain('topsides')
    }
  })

  test('the underside shows past ninety', async ({ page }) => {
    await gotoWithProbes(page)
    for (const degrees of [135, 180]) {
      const g = await drawnGeometry(page, `boat-probe-${degrees}`, 'x')
      expect(g.materials, `${degrees}°`).toContain('underside')
      expect(g.materials, `${degrees}°`).not.toContain('deck')
    }
    for (const degrees of [0, 60]) {
      const g = await drawnGeometry(page, `boat-probe-${degrees}`, 'x')
      expect(g.materials, `${degrees}°`).not.toContain('underside')
      expect(g.materials, `${degrees}°`).toContain('deck')
    }
    // And it is drawn in the underside colour, not merely present in the DOM.
    const fill = await page
      .locator('[data-testid="boat-probe-180"] polygon[data-material="underside"]')
      .first()
      .getAttribute('fill')
    expect(fill).toBe('#b9c7d1')
  })

  test('the centreboard shows when capsized', async ({ page }) => {
    await gotoWithProbes(page)
    expect(await renderedArea(page, 'boat-probe-180-boat-board')).toBeGreaterThan(0)
    expect(await renderedArea(page, 'boat-probe-180-boat-rudder')).toBeGreaterThan(0)
  })

  test('heel is visible on the live boat @slow', async ({ page }) => {
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await page.getByTestId('clock-speed-4x').click()

    // Sample the live boat as it lies down. Only the window from 0.5 rad to
    // 90° is asserted on: past horizontal the boat starts showing its bottom,
    // which is *wider* than the deck was, so "narrows" stops being the claim.
    const samples: { phi: number; extent: number }[] = []
    const deadline = Date.now() + 25_000
    for (;;) {
      const [snapshot, g] = await Promise.all([
        readSnapshot(page),
        drawnGeometry(page, 'boat-hull', 'boat-mast'),
      ])
      const phi = Math.abs(snapshot.phi)
      if (phi > 0.5 && phi < Math.PI / 2 && g.hullExtent > 0) {
        samples.push({ phi, extent: g.hullExtent })
      }
      if (phi >= Math.PI / 2 || snapshot.t >= 20 || Date.now() > deadline) {
        break
      }
      await page.waitForTimeout(100)
    }

    expect(samples.length, 'samples between 0.5 rad and 90° of heel').toBeGreaterThan(5)
    samples.sort((a, b) => a.phi - b.phi)
    const first = samples[0]
    const last = samples[samples.length - 1]
    expect(last.phi - first.phi, 'heel range covered').toBeGreaterThan(0.3)
    expect(last.extent, 'the hull is narrower at greater heel').toBeLessThan(first.extent)
    // Monotone within noise: the sail and the rig are excluded from the
    // measurement, so the only thing that can widen the hull is a face coming
    // into view, and those arrive at zero projected area.
    for (let i = 1; i < samples.length; i += 1) {
      expect(
        samples[i].extent - samples[i - 1].extent,
        `extent grew between φ=${samples[i - 1].phi} and φ=${samples[i].phi}`,
      ).toBeLessThan(0.02 * first.extent)
    }
  })

  test('the heel indicator still agrees', async ({ page }) => {
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await page.getByTestId('clock-speed-4x').click()
    await expect
      .poll(async () => Math.abs((await readSnapshot(page)).phi), { timeout: 20_000 })
      .toBeGreaterThan(0.2)

    // The two views of roll never disagree: the world view and the §26
    // indicator are driven by the same published snapshot.
    const both = await page.evaluate(() => {
      const snapshot = document.querySelector('[data-testid="snapshot"]') as HTMLElement
      const indicator = document.querySelector('[data-testid="heel-indicator"]') as HTMLElement
      return {
        snapshot: Number(snapshot.dataset.phi),
        indicator: Number(indicator.dataset.phi),
      }
    })
    expect(Math.abs(both.snapshot - both.indicator)).toBeLessThan(1e-9)
  })
})
