import type { Page } from '@playwright/test'
import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * v2 section 09, task 9.5 — the touch controls and the responsive view, in a
 * real browser with a real touch screen.
 *
 * ## What this is, and what it is not
 *
 * It runs in the single `mobile-chromium` project of `playwright.config.ts`:
 * Chromium with `hasTouch`, `isMobile` and a phone viewport. Every touch below
 * is dispatched through the Chrome DevTools Protocol, so the page receives
 * **trusted** `pointerdown`/`pointermove`/`pointerup` events with
 * `pointerType: 'touch'`, real multi-touch ids and real pointer capture —
 * which is what makes "two fingers steer and trim at once" a statement about
 * the browser rather than about a synthetic event this test made up.
 *
 * **It is still emulation.** There is no digitiser, no finger, no palm
 * rejection, no mobile GPU and no mobile browser chrome sliding in and out. A
 * real-device session is outstanding, and `docs/v2/progress/09-handoff.md`
 * says so rather than letting this suite stand in for one.
 *
 * ## Two different tolerances, deliberately kept apart
 *
 * The PRD asks for full-rate travel against `(max − min) / rate` "allowing at
 * most two physics ticks after a directly applied command", and for
 * input-to-render latency to be measured **separately** rather than folded
 * into that budget (RV54). So:
 *
 * - the travel tests work entirely in **simulated** time and allow `2·dt`;
 * - the latency test works entirely in **wall** time and reports milliseconds.
 *
 * Adding the second to the first would let a slow frame look like a physics
 * error, which is exactly the misreading RV54 names.
 */

/** Two physics steps, in simulated seconds — filled in from the page's `dt`. */
async function twoTicks(page: Page): Promise<number> {
  return 2 * (await readSnapshot(page)).dt
}

// ---------------------------------------------------------------------------
// Touch
// ---------------------------------------------------------------------------

interface Finger {
  down(id: number, x: number, y: number): Promise<void>
  move(id: number, x: number, y: number): Promise<void>
  up(id: number): Promise<void>
  cancel(): Promise<void>
  /** Ids still down, so a test can clean up without tracking them. */
  active(): number[]
}

/**
 * A multi-touch session over CDP.
 *
 * Playwright's own `page.touchscreen` can only tap, which is no use here: the
 * whole point is two fingers that move independently.
 *
 * **`Input.dispatchTouchEvent` is not symmetric, and getting it wrong is
 * silent.** For `touchStart` and `touchMove`, `touchPoints` is the whole set of
 * *active* points and Chrome works out which one changed. For `touchEnd` it is
 * the set of points being **released** — measured: sending the remaining point
 * instead lifts the wrong finger, and the test then reads a perfectly sensible
 * page state that answers a different question.
 */
async function fingers(page: Page): Promise<Finger> {
  const cdp = await page.context().newCDPSession(page)
  const held = new Map<number, { x: number; y: number }>()
  const points = () => [...held.entries()].map(([id, p]) => ({ x: p.x, y: p.y, id }))
  const send = (
    type: 'touchStart' | 'touchMove' | 'touchEnd' | 'touchCancel',
    touchPoints: { x: number; y: number; id: number }[],
  ) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints })
  /**
   * A finger cannot leave the screen, and neither may a dispatched touch: a
   * `touchMove` carrying a point outside the viewport is dropped, and the test
   * then reads a pad that never heard about the drag. A real finger dragged to
   * the edge stops at the edge, so this clamps, which is also what makes
   * {@link FULL_SCALE_DRAG_PX} safe to use from any pad at any viewport size.
   */
  const clampToScreen = async (x: number, y: number) => {
    const size = page.viewportSize()
    if (size === null) {
      return { x, y }
    }
    const inset = 4
    return {
      x: Math.max(inset, Math.min(size.width - inset, x)),
      y: Math.max(inset, Math.min(size.height - inset, y)),
    }
  }
  return {
    async down(id, x, y) {
      held.set(id, await clampToScreen(x, y))
      await send('touchStart', points())
    },
    async move(id, x, y) {
      held.set(id, await clampToScreen(x, y))
      await send('touchMove', points())
    },
    async up(id) {
      const at = held.get(id)
      held.delete(id)
      await send('touchEnd', at === undefined ? [] : [{ x: at.x, y: at.y, id }])
    },
    async cancel() {
      held.clear()
      await send('touchCancel', [])
    },
    active: () => [...held.keys()],
  }
}

/** The centre of an element, in viewport CSS pixels. */
async function centreOf(page: Page, testId: string): Promise<{ x: number; y: number }> {
  const box = await page.getByTestId(testId).boundingBox()
  expect(box, `${testId} must be on screen`).not.toBeNull()
  return { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 }
}

/**
 * Far enough past full scale that the command saturates at exactly `±1`
 * whatever the pad's gain is, and short enough that the drag still reaches the
 * edge of the smallest viewport the suite uses rather than being clamped short
 * of full scale.
 *
 * Full scale is 64 px on the helm and 96 px on the sheet
 * (`TOUCH_FULL_SCALE_PX` in `sim/controls.ts`), and the worst case here is a
 * pad centred 96 px from an edge, which still leaves 92 px of travel. The gain
 * itself is unit-tested where it lives; what a browser test adds is that a
 * real drag of an obviously-full-scale distance arrives as a full-scale
 * command.
 */
const FULL_SCALE_DRAG_PX = 200

async function padValue(page: Page, testId: string): Promise<number> {
  return Number(await page.getByTestId(testId).getAttribute('data-value'))
}

async function padOwned(page: Page, testId: string): Promise<boolean> {
  return (await page.getByTestId(testId).getAttribute('data-owned')) === 'true'
}

/**
 * Wait for a command gauge to read **exactly** `value`.
 *
 * A CDP dispatch resolves when the browser has delivered the event, not when
 * React has committed the render it causes, so reading the attribute on the
 * next round trip races one commit. Polling keeps the assertion exact — it is
 * still `toBe`, not a tolerance — while allowing the frame the page needs to
 * draw it. Where the point is that a value has **not** changed, the reads stay
 * immediate: polling would hide a transient that is exactly what the test is
 * looking for.
 */
async function expectPad(page: Page, testId: string, value: number): Promise<void> {
  await expect.poll(() => padValue(page, testId), { message: `${testId} data-value` }).toBe(value)
}

async function expectPadOwned(page: Page, testId: string, owned: boolean): Promise<void> {
  await expect
    .poll(() => padOwned(page, testId), { message: `${testId} data-owned` })
    .toBe(owned)
}

/** The live F7 values the gauges quote, read off the page (task 9.2). */
async function catalogue(page: Page) {
  const el = page.getByTestId('touch-travel')
  const n = async (name: string) => Number(await el.getAttribute(`data-${name}`))
  return {
    rudderMax: await n('rudder-max'),
    rudderRateMax: await n('rudder-rate-max'),
    sheetMin: await n('sheet-min'),
    sheetMax: await n('sheet-max'),
    haulRate: await n('haul-rate'),
    easeRate: await n('ease-rate'),
    releaseRate: await n('release-rate'),
    travelRudderStopToStop: await n('rudder-stop-to-stop'),
    travelSheetHaul: await n('sheet-haul'),
    travelSheetEase: await n('sheet-ease'),
    travelSheetRelease: await n('sheet-release'),
  }
}

// ---------------------------------------------------------------------------
// A per-frame trace, so timing questions are answered in simulated time
// ---------------------------------------------------------------------------

interface TraceSample {
  t: number
  lSheet: number
  deltaR: number
  cmdRudder: number
  cmdSheet: number
  /** Whether the release button was held on this frame. */
  release: boolean
}

declare global {
  interface Window {
    __touchTrace?: { stop(): TraceSample[] }
  }
}

async function startTrace(page: Page): Promise<void> {
  await page.evaluate(() => {
    const samples: {
      t: number
      lSheet: number
      deltaR: number
      cmdRudder: number
      cmdSheet: number
      release: boolean
    }[] = []
    let running = true
    const value = (id: string) =>
      Number(document.querySelector(`[data-testid="${id}"]`)?.getAttribute('data-value'))
    const tick = () => {
      if (!running) {
        return
      }
      requestAnimationFrame(tick)
      const snap = document.querySelector('[data-testid="snapshot"]')
      if (!(snap instanceof HTMLElement)) {
        return
      }
      samples.push({
        t: Number(snap.dataset.t),
        lSheet: Number(snap.dataset.lSheet),
        deltaR: Number(snap.dataset.deltaR),
        cmdRudder: value('touch-command-rudder'),
        cmdSheet: value('touch-command-sheet'),
        release:
          document
            .querySelector('[data-testid="touch-command-sheet"]')
            ?.getAttribute('data-release') === 'true',
      })
    }
    window.__touchTrace = {
      stop() {
        running = false
        return samples
      },
    }
    requestAnimationFrame(tick)
  })
}

async function stopTrace(page: Page): Promise<TraceSample[]> {
  const samples = await page.evaluate(() => {
    const t = window.__touchTrace
    delete window.__touchTrace
    return t === undefined ? [] : t.stop()
  })
  expect(samples.length, 'the trace collected no samples').toBeGreaterThan(20)
  return samples
}

/**
 * Least-squares `dValue/dt` over the samples where `inside` holds.
 *
 * The actuator integrations of F4.3 are exactly linear in `t` while the
 * command is constant and the value is off its stops, so this is a
 * measurement of the core's rate and not a curve fit to something curved.
 * Returned with the fit's own intercept, so the test can extrapolate to a stop
 * instead of hunting for the frame the crossing happened to land in — a frame
 * is 16 ms and two physics ticks are 10.
 */
function rateFit(
  samples: TraceSample[],
  pick: (s: TraceSample) => number,
  inside: (s: TraceSample) => boolean,
): { slope: number; intercept: number; n: number } {
  const used = samples.filter(inside)
  expect(used.length, 'not enough samples inside the linear segment').toBeGreaterThan(5)
  const n = used.length
  const mt = used.reduce((a, s) => a + s.t, 0) / n
  const mv = used.reduce((a, s) => a + pick(s), 0) / n
  let num = 0
  let den = 0
  for (const s of used) {
    num += (s.t - mt) * (pick(s) - mv)
    den += (s.t - mt) ** 2
  }
  const slope = num / den
  return { slope, intercept: mv - slope * mt, n }
}

/** The simulated time at which a linear fit reaches `value`. */
const timeAt = (fit: { slope: number; intercept: number }, value: number) =>
  (value - fit.intercept) / fit.slope

// ---------------------------------------------------------------------------

test.describe('touch controls', () => {
  test('two fingers steer and trim at the same time, on independent channels', async ({
    page,
  }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')
    const sheet = await centreOf(page, 'touch-pad-sheet')
    const before = await readSnapshot(page)

    await touch.down(1, helm.x, helm.y)
    await touch.down(2, sheet.x, sheet.y)
    // A fresh grab commands nothing: relative, and no jump.
    await expectPadOwned(page, 'touch-command-rudder', true)
    await expectPadOwned(page, 'touch-command-sheet', true)
    expect(await padValue(page, 'touch-command-rudder')).toBe(0)
    expect(await padValue(page, 'touch-command-sheet')).toBe(0)

    await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
    await touch.move(2, sheet.x, sheet.y + FULL_SCALE_DRAG_PX)
    await expect(page.getByTestId('touch-controls')).toHaveAttribute('data-pads-held', '2')
    await expectPad(page, 'touch-command-rudder', 1)
    await expectPad(page, 'touch-command-sheet', -1)

    // Both actuators move, at once, in the commanded directions (F2.2, F3).
    await expect
      .poll(async () => (await readSnapshot(page)).deltaR)
      .toBeGreaterThan(before.deltaR + 0.1)
    await expect
      .poll(async () => (await readSnapshot(page)).lSheet)
      .toBeLessThan(before.lSheet - 0.1)

    // Lifting the helm hands that channel back and leaves the sheet alone.
    await touch.up(1)
    await expect(page.getByTestId('touch-controls')).toHaveAttribute('data-pads-held', '1')
    await expectPadOwned(page, 'touch-command-rudder', false)
    expect(await padOwned(page, 'touch-command-sheet')).toBe(true)
    expect(await padValue(page, 'touch-command-sheet')).toBe(-1)

    const stillHauling = await readSnapshot(page)
    await page.waitForTimeout(200)
    expect((await readSnapshot(page)).lSheet).toBeLessThan(stillHauling.lSheet)
    await touch.up(2)
  })

  test('an unrelated pointerup cannot cancel a grab', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')
    const world = await centreOf(page, 'world-view')

    await touch.down(1, helm.x, helm.y)
    await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
    await expectPad(page, 'touch-command-rudder', 1)

    // A second finger lands somewhere else entirely and lifts off again.
    // Read immediately afterwards, and immediately on purpose: the claim is
    // that **nothing** changed, and polling would let a transient go by.
    await touch.down(9, world.x, world.y)
    await touch.up(9)
    expect(await padValue(page, 'touch-command-rudder')).toBe(1)
    expect(await padOwned(page, 'touch-command-rudder')).toBe(true)

    // …and a finger on the boat does not trim: touch on the world view is not
    // a mainsheet drag (task 9.3).
    const before = (await readSnapshot(page)).lSheet
    await touch.down(8, world.x, world.y)
    await touch.move(8, world.x, world.y + FULL_SCALE_DRAG_PX)
    await page.waitForTimeout(250)
    expect(Math.abs((await readSnapshot(page)).lSheet - before)).toBeLessThan(1e-9)
    await touch.up(8)
    await touch.up(1)
  })

  test('a cancelled touch clears the command, and the core takes the helm back', async ({
    page,
  }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')

    await touch.down(1, helm.x, helm.y)
    await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
    await expect.poll(async () => (await readSnapshot(page)).deltaR).toBeGreaterThan(0.2)

    // The browser takes the pointer away — a system gesture, a notification.
    await touch.cancel()
    await expect(page.getByTestId('touch-controls')).toHaveAttribute('data-pads-held', '0')
    await expectPad(page, 'touch-command-rudder', 0)
    await expectPadOwned(page, 'touch-command-rudder', false)

    // With no command the tiller self-centres — in Rust, where that law lives
    // (`dynamics::rudder_rate`). The browser never held a rudder angle.
    const stranded = (await readSnapshot(page)).deltaR
    await expect.poll(async () => (await readSnapshot(page)).deltaR).toBeLessThan(stranded - 0.05)
  })

  test('pausing clears a held command, and it does not come back', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')

    await touch.down(1, helm.x, helm.y)
    await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
    await expectPad(page, 'touch-command-rudder', 1)

    await page.getByTestId('clock-pause').click()
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
    // The finger is still down; the *command* is not (RV53).
    await expect
      .poll(async () => padValue(page, 'touch-command-rudder'))
      .toBe(0)

    await page.getByTestId('clock-pause').click()
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'true')
    const after = await readSnapshot(page)
    await page.waitForTimeout(400)
    // The stale gesture did not resurrect itself when the clock restarted.
    expect(Math.abs((await readSnapshot(page)).deltaR)).toBeLessThanOrEqual(
      Math.abs(after.deltaR) + 1e-9,
    )
    await touch.up(1)
  })

  test('entering replay clears a held command', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')

    await page.getByTestId('record-toggle').click()
    await page.waitForTimeout(700)
    await page.getByTestId('record-toggle').click()
    await expect(page.getByTestId('record-summary')).toBeVisible()

    await touch.down(1, helm.x, helm.y)
    await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
    await expectPad(page, 'touch-command-rudder', 1)

    await page.getByTestId('record-replay').click()
    await expect.poll(async () => padValue(page, 'touch-command-rudder')).toBe(0)
    await touch.up(1)
  })

  test('the release button needs continuous contact, and lifting is not a dump', async ({
    page,
  }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const sheet = await centreOf(page, 'touch-pad-sheet')
    const release = await centreOf(page, 'touch-release')
    const p = await catalogue(page)

    // Lifting off the sheet pad leaves the rope where it is.
    await touch.down(1, sheet.x, sheet.y)
    await touch.move(1, sheet.x, sheet.y + FULL_SCALE_DRAG_PX)
    await page.waitForTimeout(200)
    await touch.up(1)
    await page.waitForTimeout(250)
    const settled = (await readSnapshot(page)).lSheet
    await page.waitForTimeout(400)
    expect(Math.abs((await readSnapshot(page)).lSheet - settled)).toBeLessThan(1e-9)
    expect(await page.getByTestId('touch-release').getAttribute('data-held')).toBe('false')

    // Holding the button pays out, at the core's release rate.
    await startTrace(page)
    await touch.down(2, release.x, release.y)
    await expect(page.getByTestId('touch-release')).toHaveAttribute('data-held', 'true')
    await expect(page.getByTestId('touch-command-sheet')).toHaveAttribute('data-release', 'true')
    await page.waitForTimeout(500)
    await touch.up(2)
    await expect(page.getByTestId('touch-release')).toHaveAttribute('data-held', 'false')
    await page.waitForTimeout(300)
    const samples = await stopTrace(page)

    const span = p.sheetMax - p.sheetMin
    const fit = rateFit(
      samples,
      (s) => s.lSheet,
      // Only the frames the button was held, and only off both stops. Without
      // the first clause the fit averages in the still frames before the press
      // and reports a rate that is nobody's.
      (s) =>
        s.release &&
        s.lSheet > p.sheetMin + 0.05 * span &&
        s.lSheet < p.sheetMax - 0.05 * span,
    )
    // eslint-disable-next-line no-console
    console.log(
      `[touch] release rate measured ${fit.slope.toFixed(6)} m/s over ${fit.n} frames,` +
        ` catalogue ${p.releaseRate}`,
    )
    expect(Math.abs(fit.slope - p.releaseRate) / p.releaseRate).toBeLessThan(1e-3)

    // And it stopped when the finger came off, rather than running on.
    const end = samples[samples.length - 1]
    await page.waitForTimeout(300)
    expect(Math.abs((await readSnapshot(page)).lSheet - end.lSheet)).toBeLessThan(1e-9)
  })

  test('the reset button restarts the run', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    await expect.poll(async () => (await readSnapshot(page)).t).toBeGreaterThan(1)
    await page.getByTestId('touch-reset').click()
    await expect.poll(async () => (await readSnapshot(page)).t).toBeLessThan(0.7)
  })
})

test.describe('full-rate travel', () => {
  /**
   * The headline number of the section: a full-scale drag on a pad runs the
   * actuator end to end in `(max − min) / rate`, with the command taking
   * effect inside two physics ticks.
   */
  test('a full-scale haul moves the sheet at exactly the core rate', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const sheet = await centreOf(page, 'touch-pad-sheet')
    const p = await catalogue(page)
    const dt2 = await twoTicks(page)
    const span = p.sheetMax - p.sheetMin

    // The reference times really are `(max − min) / rate` from the live
    // catalogue, and not a remembered 2.4 s or 0.6 s.
    expect(p.travelSheetHaul).toBeCloseTo(span / p.haulRate, 9)
    expect(p.travelSheetEase).toBeCloseTo(span / p.easeRate, 9)
    expect(p.travelSheetRelease).toBeCloseTo(span / p.releaseRate, 9)
    expect(p.travelRudderStopToStop).toBeCloseTo((2 * p.rudderMax) / p.rudderRateMax, 9)

    // Ease out to the stop first, so the haul has the whole travel to run.
    await touch.down(1, sheet.x, sheet.y)
    await touch.move(1, sheet.x, sheet.y - FULL_SCALE_DRAG_PX)
    await expect
      .poll(async () => (await readSnapshot(page)).lSheet, { timeout: 15_000 })
      .toBeGreaterThan(p.sheetMax - 1e-9)
    await touch.up(1)

    await startTrace(page)
    await touch.down(2, sheet.x, sheet.y)
    await touch.move(2, sheet.x, sheet.y + FULL_SCALE_DRAG_PX)
    await expectPad(page, 'touch-command-sheet', -1)
    await expect
      .poll(async () => (await readSnapshot(page)).lSheet, { timeout: 15_000 })
      .toBeLessThan(p.sheetMin + 1e-9)
    await touch.up(2)
    const samples = await stopTrace(page)

    // The rate, measured over the interior of the travel where `L` is off both
    // stops and the integration is exactly linear.
    const fit = rateFit(
      samples,
      (s) => s.lSheet,
      (s) => s.lSheet > p.sheetMin + 0.08 * span && s.lSheet < p.sheetMax - 0.08 * span,
    )
    const measuredRate = -fit.slope
    // eslint-disable-next-line no-console
    console.log(
      `[touch] haul rate measured ${measuredRate.toFixed(6)} m/s over ${fit.n} frames,` +
        ` catalogue ${p.haulRate}`,
    )
    expect(Math.abs(measuredRate - p.haulRate) / p.haulRate).toBeLessThan(1e-3)

    // Extrapolate the fit to both stops rather than trusting the frame a
    // crossing happened to land in: a frame is ~16 ms and two ticks are 10.
    const departed = timeAt(fit, p.sheetMax)
    const arrived = timeAt(fit, p.sheetMin)
    const travel = arrived - departed
    // eslint-disable-next-line no-console
    console.log(
      `[touch] full haul travel ${travel.toFixed(4)} s, predicted ${p.travelSheetHaul.toFixed(4)} s,` +
        ` budget ${dt2.toFixed(4)} s`,
    )
    expect(Math.abs(travel - p.travelSheetHaul)).toBeLessThanOrEqual(dt2)

    // …and the command was in force within two ticks of being applied: the
    // last frame that saw no haul command, and the first that saw one, bracket
    // the departure.
    const firstCommanded = samples.find((s) => s.cmdSheet <= -1 + 1e-9)
    expect(firstCommanded, 'the trace never saw the full haul command').toBeDefined()
    const lastQuiet = [...samples].reverse().find((s) => s.t < firstCommanded!.t)
    expect(departed).toBeGreaterThanOrEqual((lastQuiet?.t ?? 0) - dt2)
    expect(departed).toBeLessThanOrEqual(firstCommanded!.t + dt2)
  })

  test('a full-scale helm drag moves the rudder at exactly the core rate', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')
    const p = await catalogue(page)
    const dt2 = await twoTicks(page)

    // Put the tiller hard over one way, then run it to the other stop, which
    // is the stop-to-stop travel the gauge quotes.
    await touch.down(1, helm.x, helm.y)
    await touch.move(1, helm.x - FULL_SCALE_DRAG_PX, helm.y)
    await expect
      .poll(async () => (await readSnapshot(page)).deltaR, { timeout: 10_000 })
      .toBeLessThan(-p.rudderMax + 1e-9)

    await startTrace(page)
    await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
    await expectPad(page, 'touch-command-rudder', 1)
    await expect
      .poll(async () => (await readSnapshot(page)).deltaR, { timeout: 10_000 })
      .toBeGreaterThan(p.rudderMax - 1e-9)
    await touch.up(1)
    const samples = await stopTrace(page)

    const fit = rateFit(
      samples,
      (s) => s.deltaR,
      (s) => s.deltaR > -0.85 * p.rudderMax && s.deltaR < 0.85 * p.rudderMax,
    )
    // eslint-disable-next-line no-console
    console.log(
      `[touch] rudder rate measured ${fit.slope.toFixed(6)} rad/s over ${fit.n} frames,` +
        ` catalogue ${p.rudderRateMax}`,
    )
    expect(Math.abs(fit.slope - p.rudderRateMax) / p.rudderRateMax).toBeLessThan(1e-3)

    const travel = timeAt(fit, p.rudderMax) - timeAt(fit, -p.rudderMax)
    // eslint-disable-next-line no-console
    console.log(
      `[touch] rudder stop-to-stop ${travel.toFixed(4)} s, predicted` +
        ` ${p.travelRudderStopToStop.toFixed(4)} s, budget ${dt2.toFixed(4)} s`,
    )
    expect(Math.abs(travel - p.travelRudderStopToStop)).toBeLessThanOrEqual(dt2)
  })

  test('input-to-render latency, measured on its own in wall time', async ({ page }) => {
    // Separate from the physics budget above, on purpose (RV54). This is the
    // section 10 instrument — a helm command in, the drawn rudder out — and it
    // is keyed on the *composed* command, so a touch pad is measured through
    // exactly the pipeline a key press is.
    await gotoApp(page, { scenario: 'sheet' })
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')

    for (let i = 0; i < 6; i += 1) {
      await page.evaluate(() => window.__sailgymPerf?.reset())
      await touch.down(1, helm.x, helm.y)
      await touch.move(1, helm.x + (i % 2 === 0 ? 1 : -1) * FULL_SCALE_DRAG_PX, helm.y)
      await page.waitForTimeout(250)
      await touch.up(1)
      await page.waitForTimeout(150)
      const report = await page.evaluate(() => window.__sailgymPerf?.report())
      if ((report?.inputLagMs.length ?? 0) > 0) {
        const worst = Math.max(...report!.inputLagMs)
        // eslint-disable-next-line no-console
        console.log(
          `[touch] input→render lag, touch pad: n=${report!.inputLagMs.length} worst ${worst.toFixed(1)} ms`,
        )
        expect(worst, 'touch input to rendered rudder').toBeLessThan(50)
        return
      }
    }
    throw new Error('the touch pad produced no measurable input-lag sample')
  })
})

test.describe('one trajectory, whatever the device', () => {
  /**
   * Equal normalised commands produce equal trajectories.
   *
   * Driven from a **paused** clock and advanced one step at a time, so the two
   * runs see exactly the same number of steps under exactly the same
   * `Controls`. Any difference would mean a second path into the core, which
   * is the thing task 9.1 exists to make impossible.
   */
  const STEPS = 60

  async function steppedFrom(
    page: Page,
    apply: () => Promise<void>,
  ): Promise<Record<string, number>> {
    await gotoApp(page, { scenario: 'sheet' })
    await page.getByTestId('clock-pause').click()
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
    await page.getByTestId('clock-reset').click()
    await expect.poll(async () => (await readSnapshot(page)).t).toBeLessThan(1e-9)
    // Pause and reset are clear paths, so the command is applied *after* them.
    await apply()
    for (let i = 0; i < STEPS; i += 1) {
      await page.keyboard.press('.')
    }
    return readSnapshot(page)
  }

  test('a full-scale touch drag and a held arrow key produce the same state', async ({
    page,
  }) => {
    const byKey = await steppedFrom(page, async () => {
      await page.keyboard.down('ArrowRight')
    })
    await page.keyboard.up('ArrowRight')

    const byTouch = await steppedFrom(page, async () => {
      const touch = await fingers(page)
      const helm = await centreOf(page, 'touch-pad-rudder')
      await touch.down(1, helm.x, helm.y)
      await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
      await expectPad(page, 'touch-command-rudder', 1)
    })

    expect(byKey.t).toBeCloseTo(byTouch.t, 12)
    expect(Math.abs(byKey.deltaR), 'the helm must actually have moved').toBeGreaterThan(0.1)
    for (const field of ['x', 'y', 'psi', 'phi', 'u', 'v', 'r', 'p', 'beta', 'deltaR', 'lSheet']) {
      expect(byTouch[field], `${field} differs between the keyboard and the pad`).toBe(
        byKey[field],
      )
    }
  })
})

test.describe('the responsive view', () => {
  /** The four sizes the section's acceptance criteria name, in CSS pixels. */
  const SIZES = [
    { name: '360×640', width: 360, height: 640 },
    { name: '390×844', width: 390, height: 844 },
    { name: '844×390 (landscape)', width: 844, height: 390 },
    { name: '1280×800 (desktop)', width: 1280, height: 800 },
  ]

  /** Every control and every piece of primary feedback. */
  const MUST_BE_ON_SCREEN = [
    'touch-pad-rudder',
    'touch-pad-sheet',
    'touch-release',
    'touch-reset',
    'touch-gauge-rudder',
    'touch-gauge-sheet',
    'touch-command-rudder',
    'touch-command-sheet',
    'world-view',
    'sail-hud',
  ]

  /** The 44 × 44 CSS-pixel minimum the PRD sets for a touch target. */
  const TARGET_MIN_PX = 44
  const TOUCH_TARGETS = ['touch-pad-rudder', 'touch-pad-sheet', 'touch-release', 'touch-reset']

  /** `App.tsx`'s `WORLD_PX.min`. */
  const WORLD_MIN = { width: 260, height: 190 }

  for (const size of SIZES) {
    test(`fits at ${size.name}`, async ({ page }) => {
      await page.setViewportSize({ width: size.width, height: size.height })
      await gotoApp(page, { scenario: 'free_sail' })
      await page.waitForTimeout(400)

      const overflow = await page.evaluate(() => {
        const e = document.scrollingElement!
        return {
          scrollWidth: e.scrollWidth,
          clientWidth: e.clientWidth,
          scrollHeight: e.scrollHeight,
          clientHeight: e.clientHeight,
        }
      })
      expect(overflow.scrollWidth, 'horizontal document overflow').toBeLessThanOrEqual(
        overflow.clientWidth,
      )

      for (const id of MUST_BE_ON_SCREEN) {
        const box = await page.getByTestId(id).boundingBox()
        expect(box, `${id} is not laid out`).not.toBeNull()
        expect(box!.x, `${id} is off the left edge`).toBeGreaterThanOrEqual(-0.5)
        expect(box!.y, `${id} is off the top edge`).toBeGreaterThanOrEqual(-0.5)
        expect(box!.x + box!.width, `${id} is off the right edge`).toBeLessThanOrEqual(
          size.width + 0.5,
        )
        expect(box!.y + box!.height, `${id} is below the fold`).toBeLessThanOrEqual(
          size.height + 0.5,
        )
      }

      for (const id of TOUCH_TARGETS) {
        const box = (await page.getByTestId(id).boundingBox())!
        expect(box.width, `${id} width`).toBeGreaterThanOrEqual(TARGET_MIN_PX)
        expect(box.height, `${id} height`).toBeGreaterThanOrEqual(TARGET_MIN_PX)
      }

      const world = (await page.getByTestId('world-view').boundingBox())!
      expect(world.width, 'world view width').toBeGreaterThanOrEqual(WORLD_MIN.width)
      expect(world.height, 'world view height').toBeGreaterThanOrEqual(WORLD_MIN.height)

      // Debug Mode is reachable without taking space from the boat by default.
      await expect(page.getByTestId('mode-switch')).toHaveAttribute('data-mode', 'sail')
      await expect(page.getByTestId('layout-debug')).toHaveCount(0)
      // eslint-disable-next-line no-console
      console.log(
        `[touch] ${size.name}: world ${Math.round(world.width)}×${Math.round(world.height)},` +
          ` document ${overflow.scrollWidth}×${overflow.scrollHeight}`,
      )
    })
  }

  test('rotating keeps the boat visible and the pointer mapping consistent', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 })
    await gotoApp(page, { scenario: 'free_sail' })
    await page.waitForTimeout(400)

    /**
     * The camera's own idea of the view's centre, against the SVG the page
     * actually drew.
     *
     * `Camera.worldTransform` emits `translate(width/2 height/2)`. If the two
     * disagree after a resize, a tap does not land where the player aimed —
     * which is exactly what "pointer coordinates consistent" means.
     */
    const agreement = async () =>
      page.evaluate(() => {
        const svg = document.querySelector('[data-testid="world-view"]') as SVGSVGElement
        const grid = document.querySelector('[data-testid="world-grid"]') as SVGGElement
        const m = /translate\(([-\d.]+) ([-\d.]+)\)/.exec(grid.getAttribute('transform') ?? '')
        const box = svg.getBoundingClientRect()
        return {
          svgWidth: Number(svg.getAttribute('width')),
          svgHeight: Number(svg.getAttribute('height')),
          cssWidth: box.width,
          cssHeight: box.height,
          cameraCentre: m === null ? null : { x: Number(m[1]), y: Number(m[2]) },
        }
      })

    const check = async (label: string) => {
      const a = await agreement()
      expect(a.cameraCentre, `${label}: no camera transform`).not.toBeNull()
      // One CSS pixel is one SVG unit, so a screen tap is a camera coordinate.
      expect(a.cssWidth, `${label}: SVG width`).toBeCloseTo(a.svgWidth, 0)
      expect(a.cssHeight, `${label}: SVG height`).toBeCloseTo(a.svgHeight, 0)
      expect(a.cameraCentre!.x, `${label}: camera x`).toBeCloseTo(a.svgWidth / 2, 3)
      expect(a.cameraCentre!.y, `${label}: camera y`).toBeCloseTo(a.svgHeight / 2, 3)
      return a
    }

    const portrait = await check('portrait')
    await page.setViewportSize({ width: 844, height: 390 })
    await page.waitForTimeout(500)
    const landscape = await check('landscape')
    expect(landscape.svgWidth).toBeGreaterThan(portrait.svgWidth)

    // The boat is still there, and the controls still work in the new shape.
    await expect(page.getByTestId('boat-hull')).toHaveCount(1)
    const touch = await fingers(page)
    const helm = await centreOf(page, 'touch-pad-rudder')
    await touch.down(1, helm.x, helm.y)
    await touch.move(1, helm.x + FULL_SCALE_DRAG_PX, helm.y)
    await expectPad(page, 'touch-command-rudder', 1)
    await expect.poll(async () => (await readSnapshot(page)).deltaR).toBeGreaterThan(0.1)
    await touch.up(1)

    // …and back again, which is the other half of a rotation.
    await page.setViewportSize({ width: 390, height: 844 })
    await page.waitForTimeout(500)
    await check('portrait again')
  })

  test('Sail Mode draws the wind thinner, and Debug Mode draws all of it', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await page.waitForTimeout(400)
    const stats = page.getByTestId('wind-stats')
    const read = async () => ({
      drawn: Number(await stats.getAttribute('data-particles')),
      total: Number(await stats.getAttribute('data-particles-total')),
      density: Number(await stats.getAttribute('data-wind-density')),
      contrast: Number(await stats.getAttribute('data-wind-contrast')),
    })

    const sail = await read()
    expect(sail.density).toBeLessThan(1)
    expect(sail.contrast).toBeLessThan(1)
    expect(sail.drawn).toBeLessThan(sail.total)
    // brief §39 still holds: the dense field is WebGL, and there are still
    // thousands of it.
    expect(sail.drawn).toBeGreaterThan(1000)

    await page.getByTestId('mode-switch').click()
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
    const debug = await read()
    expect(debug.density).toBe(1)
    expect(debug.contrast).toBe(1)
    expect(debug.drawn).toBe(debug.total)
    // eslint-disable-next-line no-console
    console.log(
      `[touch] wind drawn: sail ${sail.drawn}/${sail.total} at contrast ${sail.contrast},` +
        ` debug ${debug.drawn}/${debug.total}`,
    )

    // The wind the *boat* feels is untouched by either: this is presentation.
    const speed = async () =>
      Number(await page.getByTestId('wind-readout').getAttribute('data-speed'))
    const inDebug = await speed()
    await page.getByTestId('mode-switch').click()
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'sail')
    expect(await speed()).toBeCloseTo(inDebug, 6)
  })

  /**
   * Task 9.2's own acceptance criterion, end to end.
   *
   * brief §31 makes the catalogue editable while the boat sails, and the
   * gauges quote it. If they quoted the catalogue the page loaded with, every
   * number beside a control would go quietly wrong the moment anyone touched
   * the panel — which is the failure the `parameters_json()` poll in
   * `useSimulation` exists to prevent.
   */
  test('a live parameter edit moves the gauges and the reference times', async ({ page }) => {
    await gotoApp(page, { scenario: 'sheet' })
    const travel = page.getByTestId('touch-travel')
    const before = await catalogue(page)

    await page.getByTestId('mode-switch').click()
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
    await page.getByTestId('parameter-panel-toggle').click()
    await expect(page.getByTestId('parameter-groups')).toHaveCount(1)

    const edit = async (path: string, value: string) => {
      const input = page.getByTestId(`param-input-${path}`)
      await input.scrollIntoViewIfNeeded()
      await input.fill(value)
      await input.dispatchEvent('change')
      await expect(page.getByTestId(`param-${path}`)).toHaveAttribute('data-value', value)
    }

    // Halve the haul rate: the sheet's full-travel time must double.
    await edit('sheet.sheet_haul_rate', '0.75')
    await expect(travel).toHaveAttribute('data-haul-rate', '0.75')
    await expect
      .poll(async () => Number(await travel.getAttribute('data-sheet-haul')))
      .toBeCloseTo(2 * before.travelSheetHaul, 6)

    // Halve the rudder's maximum rate: stop to stop must double too, and the
    // rudder gauge's own limit must move with `delta_r_max`.
    await edit('rudder.delta_r_rate_max', '1.045')
    await expect
      .poll(async () => Number(await travel.getAttribute('data-rudder-stop-to-stop')))
      .toBeCloseTo(2 * before.travelRudderStopToStop, 6)

    await edit('rudder.delta_r_max', '0.349')
    await expect(travel).toHaveAttribute('data-rudder-max', '0.349')
    await expect(page.getByTestId('touch-gauge-rudder')).toContainText('of 20°')

    const after = await catalogue(page)
    // eslint-disable-next-line no-console
    console.log(
      `[touch] live edit: sheet haul ${before.travelSheetHaul.toFixed(3)} s →` +
        ` ${after.travelSheetHaul.toFixed(3)} s, rudder stop-to-stop` +
        ` ${before.travelRudderStopToStop.toFixed(3)} s → ${after.travelRudderStopToStop.toFixed(3)} s`,
    )
  })

  test('a chosen Debug Mode survives a reload; a new visitor starts in Sail Mode', async ({
    page,
  }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'sail')
    // Sail Mode because nobody has said otherwise, not because it was picked.
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode-chosen', 'false')

    await page.getByTestId('mode-switch').click()
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode-chosen', 'true')
    await page.reload()
    await expect(page.getByTestId('wasm-status')).toHaveAttribute('data-ready', 'true')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode-chosen', 'true')
    // …and the debug column really is mounted, not merely remembered.
    await expect(page.getByTestId('layout-debug')).toHaveCount(1)

    // A visitor with no stored preference.
    await page.evaluate(() => localStorage.clear())
    await page.reload()
    await expect(page.getByTestId('wasm-status')).toHaveAttribute('data-ready', 'true')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'sail')
    await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode-chosen', 'false')
    await expect(page.getByTestId('layout-debug')).toHaveCount(0)
  })
})
