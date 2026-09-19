import type { Page } from '@playwright/test'
import { expect, gotoApp, readSnapshot, test } from './fixtures'
import type { Diagnostics } from '../../src/sim/diagnostics'
import { BRIEF_31_PARAMETERS, BRIEF_31_WIND } from '../../src/ui/parameterSchema'

/**
 * Live parameter editing in the browser (brief §31, task 8.5).
 */

async function openPanel(page: Page) {
  await page.getByTestId('mode-switch').click()
  await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
  await page.getByTestId('parameter-panel-toggle').click()
  await expect(page.getByTestId('parameter-panel')).toHaveAttribute('data-open', 'true')
}

async function diagnostics(page: Page): Promise<Diagnostics> {
  const raw = await page.getByTestId('sail-hud').getAttribute('data-diagnostics')
  return JSON.parse(raw ?? '{}') as Diagnostics
}

/**
 * Every control's path and value.
 *
 * This **is** `parameters_json()`: the panel rebuilds its controls from that
 * call on every edit (`ParameterPanel.refresh`), so reading the controls back
 * reads the catalogue back, leaf for leaf, without adding a debug-only hook
 * to the page for the test's benefit.
 */
async function catalogue(page: Page): Promise<Record<string, number>> {
  return page.evaluate(() => {
    const out: Record<string, number> = {}
    for (const el of document.querySelectorAll('[data-testid^="param-"][data-value]')) {
      const id = (el as HTMLElement).dataset.testid ?? el.getAttribute('data-testid') ?? ''
      out[id.replace(/^param-/, '')] = Number((el as HTMLElement).getAttribute('data-value'))
    }
    return out
  })
}

async function setParam(page: Page, path: string, value: string) {
  await page.getByTestId(`param-input-${path}`).fill(value)
  await page.getByTestId(`param-input-${path}`).dispatchEvent('change')
}

test.describe('parameter panel', () => {
  test('is generated from the catalogue, tagged, and collapsible', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await page.getByTestId('mode-switch').click()

    // Collapsed by default (brief §31 asks for a collapsible panel), and the
    // control count is the whole F7 catalogue even while it is shut.
    await expect(page.getByTestId('parameter-panel')).toHaveAttribute('data-open', 'false')
    await expect(page.getByTestId('parameter-groups')).toHaveCount(0)
    const count = Number(await page.getByTestId('parameter-panel').getAttribute('data-controls'))
    expect(count).toBeGreaterThan(55)

    await page.getByTestId('parameter-panel-toggle').click()
    await expect(page.getByTestId('parameter-groups')).toHaveCount(1)

    // Every group of F7, and every control carrying one of the four tags.
    for (const group of [
      'hull',
      'inertia',
      'resistance',
      'sail',
      'board',
      'rudder',
      'sheet',
      'stability',
      'sim',
    ]) {
      await expect(page.getByTestId(`param-group-${group}`)).toHaveCount(1)
    }
    const tags = await page.evaluate(() =>
      [...document.querySelectorAll('[data-testid^="param-"][data-tag]')].map(
        (el) => (el as HTMLElement).getAttribute('data-tag') ?? '',
      ),
    )
    expect(tags.length).toBeGreaterThan(55)
    for (const tag of tags) {
      expect(['KNOWN', 'ASSUMED', 'TUNABLE', 'DEFERRED']).toContain(tag)
    }
    // KNOWN is present and visually marked, per task 8.5.
    expect(tags.filter((t) => t === 'KNOWN').length).toBeGreaterThan(0)
    expect(await page.locator('[data-testid="tag-KNOWN"]').count()).toBeGreaterThan(0)

    // The section 06/07 caution about the timestep is in front of whoever
    // reaches for it.
    await expect(page.getByTestId('param-note-sim')).toHaveCount(1)
  })

  test('every brief §31 example parameter is reachable', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await openPanel(page)

    for (const { example, path } of BRIEF_31_PARAMETERS) {
      await expect(page.getByTestId(`param-${path}`), `brief §31: ${example}`).toHaveCount(1)
      await expect(page.getByTestId(`param-input-${path}`), `brief §31: ${example}`).toHaveCount(1)
    }
    for (const { example, field } of BRIEF_31_WIND) {
      await expect(page.getByTestId(`param-wind.${field}`), `brief §31: ${example}`).toHaveCount(1)
      await expect(
        page.getByTestId(`param-input-wind.${field}`),
        `brief §31: ${example}`,
      ).toHaveCount(1)
    }
    expect(BRIEF_31_PARAMETERS.length + BRIEF_31_WIND.length).toBeGreaterThanOrEqual(13)
  })

  test('cutting sail.area from 7.06 to 3.0 reduces the sail force', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await openPanel(page)
    await page.waitForTimeout(1_500)

    const force = async () => {
      const d = await diagnostics(page)
      return Math.hypot(d.sail.f.x, d.sail.f.y)
    }

    // Paused, so the only thing that changes between the two readings is the
    // sail area: same state, same wind, same boom angle. Comparing across a
    // stretch of *running* simulation would not be a test of the edit — the
    // boom swings and the apparent wind changes, and a smaller sail can end
    // up more loaded at a different boom angle.
    await page.getByTestId('clock-pause').click()
    await expect(page.getByTestId('param-sail.area')).toHaveAttribute('data-value', '7.06')
    const before = await force()
    expect(before).toBeGreaterThan(1)

    await setParam(page, 'sail.area', '3')
    await expect(page.getByTestId('param-sail.area')).toHaveAttribute('data-value', '3')
    await expect.poll(force).toBeLessThan(before * 0.6)

    // …and the edit is still in force two seconds of simulation later, which
    // is what the acceptance criterion asks for. Checked the same controlled
    // way: run, pause, then put the area back and watch the force return.
    await page.getByTestId('clock-pause').click()
    await page.waitForTimeout(2_000)
    await page.getByTestId('clock-pause').click()
    const small = await force()
    expect(small).toBeGreaterThan(0.1)

    await setParam(page, 'sail.area', '7.06')
    await expect(page.getByTestId('param-sail.area')).toHaveAttribute('data-value', '7.06')
    // Force is linear in area at a fixed state, so 7.06/3 = 2.35x; asserted
    // loosely because the poll may land a frame later.
    await expect.poll(force).toBeGreaterThan(small * 1.5)
    await expect(page.getByTestId('parameter-error')).toHaveCount(0)
  })

  test('an impossible stability group is refused, visibly, without a NaN', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await openPanel(page)
    await page.waitForTimeout(500)

    await expect(page.getByTestId('parameter-error')).toHaveCount(0)
    // GM = 3 m with the F7 `gz_max`, `phi_peak` and `phi_vanish` makes the
    // fitted GZ curve cross zero long before the vanishing angle, which
    // `GzCurve::fit` rejects.
    await setParam(page, 'stability.gm', '3')

    const error = page.getByTestId('parameter-error')
    await expect(error).toHaveCount(1)
    await expect(error).toContainText(/GZ|phi_vanish|stability/)

    // The edit was refused, not half-applied: the catalogue still holds the
    // old value and the control has snapped back to it.
    await expect(page.getByTestId('param-stability.gm')).toHaveAttribute('data-value', '1')

    // And the simulation is still a simulation.
    await page.waitForTimeout(1_000)
    const st = await readSnapshot(page)
    // `data-capsized` is a flag, not a number; everything else on the
    // snapshot element is an F8.3 value and must still be finite.
    for (const [key, value] of Object.entries(st)) {
      if (key === 'capsized' || key === 'testid') {
        continue
      }
      expect(Number.isFinite(value), `${key} = ${value}`).toBe(true)
    }
    expect(Number.isFinite(st.phi)).toBe(true)
    expect(Number.isFinite(st.u)).toBe(true)
    const d = await diagnostics(page)
    expect(Number.isFinite(d.gz)).toBe(true)
    expect(Number.isFinite(d.righting_moment)).toBe(true)
    expect(Number.isFinite(d.energy_kinetic)).toBe(true)
  })

  test('"Reset to ILCA defaults" restores every edited value', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await openPanel(page)
    await page.waitForTimeout(400)

    const before = await catalogue(page)
    expect(Object.keys(before).length).toBeGreaterThan(55)

    // One edit per group shape: a scalar, a flattened foil coefficient, a
    // vector component and a flag.
    await setParam(page, 'sail.area', '3')
    await setParam(page, 'resistance.x_uu', '4')
    await setParam(page, 'sheet.block_pos_b.x', '-1.5')
    await page.getByTestId('param-input-rudder.delta_r_self_centre').uncheck()
    await expect(page.getByTestId('parameter-error')).toHaveCount(0)

    const edited = await catalogue(page)
    expect(edited).not.toEqual(before)
    expect(edited['sail.area']).toBe(3)
    expect(edited['rudder.delta_r_self_centre']).toBe(0)

    await page.getByTestId('parameters-reset-defaults').click()
    await expect(page.getByTestId('param-sail.area')).toHaveAttribute('data-value', '7.06')
    expect(await catalogue(page)).toEqual(before)
  })

  test('a reset-required edit shows the badge, and the reset works', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await openPanel(page)
    await page.waitForTimeout(1_200)

    await expect(page.getByTestId('reset-required-badge')).toHaveCount(0)
    // `sim.*` is the one group that invalidates continuity (F9, brief §33).
    await expect(page.getByTestId('param-sim.dt')).toHaveAttribute('data-reset-required', 'true')
    await expect(page.getByTestId('param-sail.area')).toHaveAttribute('data-reset-required', 'false')

    // An edit that does *not* require a reset must not raise the badge.
    await setParam(page, 'sail.area', '6')
    await expect(page.getByTestId('reset-required-badge')).toHaveCount(0)

    const beforeT = (await readSnapshot(page)).t
    expect(beforeT).toBeGreaterThan(0.5)
    await setParam(page, 'sim.dt', '0.0075')
    await expect(page.getByTestId('reset-required-badge')).toHaveCount(1)
    await expect(page.getByTestId('param-sim.dt')).toHaveAttribute('data-value', '0.0075')

    await page.getByTestId('reset-required-reset').click()
    await expect(page.getByTestId('reset-required-badge')).toHaveCount(0)
    // The run really restarted: `t` went back to (near) zero and the new
    // timestep is what it is now advancing by.
    await expect.poll(async () => (await readSnapshot(page)).t).toBeLessThan(beforeT)
    expect((await readSnapshot(page)).dt).toBeCloseTo(0.0075, 12)
  })
})
