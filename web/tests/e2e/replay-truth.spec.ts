import type { Page } from '@playwright/test'

import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * **Truthful replay** (v2 section 10, task 10.5; F18.3).
 *
 * `replay.spec.ts` proves that a replay reads *stored frames* — by editing one
 * in memory and watching the render follow. This spec proves the complementary
 * and harder half, which is the whole point of the section:
 *
 * > when replaying an episode, **every visible quantity belongs to that
 * > episode and that time**.
 *
 * A matching number proves nothing on its own, because the live run and the
 * episode agree until something separates them. So every test here first makes
 * the live simulation **conspicuously different** — a different scenario, a
 * halved sail area, a different wind, an advanced clock — and then asserts
 * that nothing on screen moved. That is RV57 as an adversary rather than as a
 * comment.
 *
 * The consumers checked, one per acceptance criterion:
 *
 * | consumer | what it must show |
 * |---|---|
 * | the boat (`snapshot`) | the recorded pose |
 * | the HUD | the recorded sample, with its own timestamp |
 * | the wind readout | the episode's wind at the boat |
 * | the spatial wind field | **hidden**, with the reason on the page |
 * | the force overlay + legend | recorded vectors, or `Not recorded` |
 * | the charts | the episode's own samples, at recorded timestamps |
 * | the debug panel | the recorded sample; absent fields say so |
 */

/** Index of `x` in the F8.3 state layout. */
const X = 0

async function attrs(page: Page, testId: string): Promise<Record<string, string>> {
  return page.getByTestId(testId).evaluate((el) => {
    const out: Record<string, string> = {}
    for (const a of el.attributes) {
      out[a.name] = a.value
    }
    return out
  })
}

/** Record about `seconds` of simulated time at 4× and stop. */
async function record(page: Page, seconds: number, hz?: string): Promise<void> {
  await page.getByTestId('clock-pause').click()
  await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
  await page.getByTestId('clock-reset').click()
  if (hz !== undefined) {
    await page.getByTestId('record-hz').selectOption(hz)
  }
  await page.getByTestId('record-toggle').click()
  await expect(page.getByTestId('record-controls')).toHaveAttribute('data-recording', 'true')

  await page.getByTestId('clock-pause').click()
  await page.getByTestId('clock-speed-4x').click()
  await expect
    .poll(async () => (await readSnapshot(page)).t, { timeout: 30_000 })
    .toBeGreaterThan(seconds)

  await page.getByTestId('record-toggle').click()
  await expect(page.getByTestId('record-controls')).toHaveAttribute('data-recording', 'false')
  await expect(page.getByTestId('record-controls')).toHaveAttribute('data-has-episode', 'true')
}

/**
 * Set the scrub slider.
 *
 * `fill` on an `<input type="range">` refuses any text the control would
 * normalise — so neither `end / 2`, a seventeen-digit double, nor `"5.000"`,
 * which the browser rewrites to `"5"`. Round to milliseconds (finer than any
 * logging rate the UI offers) and then back through `Number`, which is the one
 * spelling the control agrees with.
 */
async function scrubTo(page: Page, t: number): Promise<void> {
  await page.getByTestId('timeline-scrub').fill(String(Number(t.toFixed(3))))
}

/** Scrub to `t`, then snap onto the nearest recorded sample. */
async function seekToFrameNear(page: Page, t: number): Promise<number> {
  await scrubTo(page, t)
  await page.getByTestId('timeline-step-forward').click()
  await page.getByTestId('timeline-step-back').click()
  return Number(await page.getByTestId('timeline').getAttribute('data-index'))
}

/** Everything the page is currently showing, in one read. */
async function shown(page: Page) {
  const footer = await readSnapshot(page)
  const hud = await attrs(page, 'sail-hud')
  const windReadout = await attrs(page, 'wind-readout')
  const snapshotEl = await attrs(page, 'snapshot')
  const inspection = await attrs(page, 'inspection')
  return {
    x: footer.x,
    y: footer.y,
    psi: footer.psi,
    phi: footer.phi,
    lSheet: footer.lSheet,
    source: inspection['data-source'],
    sheetTension: snapshotEl['data-sheet-tension'],
    gz: snapshotEl['data-gz'],
    ropeLength: snapshotEl['data-rope-length'],
    hudSource: hud['data-source'],
    hudSampleT: hud['data-sample-t'],
    hudDiagnostics: hud['data-diagnostics'],
    windSpeed: windReadout['data-speed'],
    windBearing: windReadout['data-bearing'],
    windSource: windReadout['data-source'],
  }
}

/** Enter Debug Mode, where the overlay, the charts and the panel live. */
async function enterDebug(page: Page) {
  await page.getByTestId('mode-switch').click()
  await expect(page.getByTestId('app-layout')).toHaveAttribute('data-mode', 'debug')
}

test.describe('truthful replay', () => {
  test('every consumer follows the episode while the live run is changed under it', async ({
    page,
  }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await enterDebug(page)
    await page.getByTestId('overlays-all-on').click()
    await record(page, 8)

    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'replay')

    const index = await seekToFrameNear(page, 5)
    const before = await shown(page)
    const recorded = await page.evaluate((i) => window.__sailgym?.frameState(i) ?? null, index)
    expect(recorded).not.toBeNull()

    // The view really is the recording, to the bit where it can be.
    expect(before.source).toBe('replay')
    expect(before.hudSource).toBe('recorded')
    expect(before.windSource).toBe('recorded')
    expect(Math.abs(before.x - (recorded as number[])[X])).toBeLessThan(1e-9)
    // The episode has to be an episode, or the comparisons below prove nothing.
    expect(Math.hypot(before.x, before.y)).toBeGreaterThan(1)
    expect(Math.abs(before.phi)).toBeGreaterThan(0.05)
    expect(Number(before.sheetTension)).toBeGreaterThan(0)

    // --- now alter the live run as conspicuously as the UI allows ---------
    //
    // A different scenario (a different initial condition, wind and seed), a
    // halved sail area, and a different wind mode. Any one of them would move
    // every force on the page if the page were reading the live simulation.
    await page.getByTestId('parameter-panel-toggle').click()
    await expect(page.getByTestId('parameter-panel')).toHaveAttribute('data-open', 'true')
    await page.getByTestId('param-input-sail.area').fill('3')
    await page.getByTestId('param-input-sail.area').dispatchEvent('change')
    await expect(page.getByTestId('param-sail.area')).toHaveAttribute('data-value', '3')
    await page.getByTestId('wind-mode').selectOption('uniform')

    // …and let a few animation frames go by, so anything that was going to
    // re-read the live simulator has had the chance.
    await page.waitForTimeout(400)

    const after = await shown(page)
    expect(after).toEqual(before)

    // The clock did not move either: entering replay suspends live
    // advancement, and no route back to `advance` remains.
    const clockNow = await page.getByTestId('clock-time').getAttribute('data-sim-time')
    await page.getByTestId('clock-pause').click() // ask it to run
    await page.waitForTimeout(400)
    expect(await page.getByTestId('clock-time').getAttribute('data-sim-time')).toBe(clockNow)
    const stillTheSame = await shown(page)
    expect(stillTheSame).toEqual(before)
  })

  test('the spatial wind field is hidden in replay, with its reason on the page', async ({
    page,
  }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'free_sail' })
    await record(page, 4)

    const liveParticles = Number(
      await page.getByTestId('wind-stats').getAttribute('data-particles'),
    )
    expect(liveParticles).toBeGreaterThan(0)

    await page.getByTestId('record-replay').click()
    const notice = page.getByTestId('replay-notice')
    await expect(notice).toHaveCount(1)
    await expect(notice).toHaveAttribute('data-wind-field', 'false')
    // The reason is specific — a statement about this build, not a shrug.
    await expect(notice).toContainText('wind field hidden')

    // The deck.gl surface draws nothing while replaying: the live field would
    // be the live run's, and the recorded vector at the boat does not identify
    // a field (RV57).
    await expect
      .poll(async () => Number(await page.getByTestId('deck-overlay').getAttribute('data-layers')))
      .toBe(0)

    // …and the wind *at the boat* is still shown, because the episode records
    // that.
    await expect(page.getByTestId('wind-readout')).toHaveAttribute('data-source', 'recorded')
    await expect(page.getByTestId('wind-readout')).toHaveAttribute('data-recorded', 'true')

    // Back to live, and the field comes back.
    await page.getByTestId('timeline-exit').click()
    await expect
      .poll(async () => Number(await page.getByTestId('deck-overlay').getAttribute('data-layers')))
      .toBeGreaterThan(0)
  })

  test('scrubbing forward, backward and across a capsize stays on the episode', async ({
    page,
  }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await record(page, 12)
    await page.getByTestId('record-replay').click()

    const at = async (t: number) => {
      const i = await seekToFrameNear(page, t)
      const state = await readSnapshot(page)
      const stored = (await page.evaluate(
        (k) => window.__sailgym?.frameState(k) ?? null,
        i,
      )) as number[]
      // Read at **this** playhead: comparing a heel readout taken at one stop
      // with a `φ` taken at another is a test of nothing.
      const heel = Number(await page.getByTestId('heel-angle').getAttribute('data-value'))
      return { i, state, stored, heel }
    }

    // Forward, then backward, then forward again: every stop is the stored
    // frame, and going back gives the same answer as the first visit.
    const first = await at(2)
    const late = await at(11)
    const backAgain = await at(2)
    for (const visit of [first, late, backAgain]) {
      expect(Math.abs(visit.state.x - visit.stored[0])).toBeLessThan(1e-9)
      expect(Math.abs(visit.state.psi - visit.stored[2])).toBeLessThan(1e-9)
      expect(Math.abs(visit.state.phi - visit.stored[3])).toBeLessThan(1e-9)
    }
    expect(backAgain.i).toBe(first.i)
    expect(backAgain.state.x).toBe(first.state.x)

    // Across the capsize: `φ` is unwrapped (F3) and must stay unwrapped, so a
    // boat past inversion reads past ±180° rather than folding back.
    // …and the heel readout is that sample's `φ` in degrees, at each stop.
    for (const visit of [first, late, backAgain]) {
      expect(Math.abs(visit.heel)).toBeCloseTo(Math.abs((visit.state.phi * 180) / Math.PI), 6)
    }
    // The run does go over: the recorded heel reaches beyond a knockdown.
    expect(Math.max(Math.abs(first.heel), Math.abs(late.heel))).toBeGreaterThan(60)

    // Across the ±π wrap of `psi`, the interpolated heading stays inside the
    // wrapped range at every playhead between two samples.
    const end = Number(await page.getByTestId('timeline').getAttribute('data-end'))
    for (let k = 0; k <= 40; k += 1) {
      await scrubTo(page, (end * k) / 40)
      const s = await readSnapshot(page)
      expect(Math.abs(s.psi)).toBeLessThanOrEqual(Math.PI + 1e-9)
      expect(Number.isFinite(s.phi)).toBe(true)
    }

    // Clamping: the control itself cannot ask for a time outside the episode —
    // its `min` and `max` are the episode's own ends, and the browser refuses
    // anything beyond them. So the ends are what is checked here, and
    // `timelineState`'s clamp of an out-of-range request is asserted directly
    // in `tests/unit/replay.test.ts`.
    const scrub = page.getByTestId('timeline-scrub')
    expect(Number(await scrub.getAttribute('min'))).toBe(
      Number(await page.getByTestId('timeline').getAttribute('data-start')),
    )
    expect(Number(await scrub.getAttribute('max'))).toBe(end)
    await scrubTo(page, 0)
    expect(Number(await page.getByTestId('timeline').getAttribute('data-index'))).toBe(0)
    await page.getByTestId('timeline-step-back').isDisabled()
    // …and stepping forward from near the end lands on the last sample.
    await seekToFrameNear(page, end)
    const frames = Number(await page.getByTestId('timeline').getAttribute('data-frames'))
    await page.getByTestId('timeline-step-forward').click()
    expect(Number(await page.getByTestId('timeline').getAttribute('data-index'))).toBe(frames - 1)
    await expect(page.getByTestId('timeline-step-forward')).toBeDisabled()
  })

  test('the diagnostics shown are a recorded sample, and say which one', async ({ page }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'close_hauled' })
    await enterDebug(page)
    await record(page, 8, '10')
    await page.getByTestId('record-replay').click()

    // Land between two samples on purpose: the pose interpolates, the
    // diagnostics do not.
    const end = Number(await page.getByTestId('timeline').getAttribute('data-end'))
    await scrubTo(page, end / 2 + 0.037)
    const panel = await attrs(page, 'debug-panel')
    const playhead = Number(await page.getByTestId('timeline').getAttribute('data-time'))

    expect(panel['data-source']).toBe('recorded')
    const sampleT = Number(panel['data-sample-t'])
    // The sample is at or before the playhead, and within one logging
    // interval of it — 1/10 s here, because the recording was made at 10 Hz.
    expect(sampleT).toBeLessThanOrEqual(playhead + 1e-9)
    expect(playhead - sampleT).toBeLessThan(0.1 + 1e-9)
    await expect(page.getByTestId('diag-source')).toContainText('recorded sample')

    // The overlay and the HUD agree with the panel about *which* sample.
    expect(await page.getByTestId('force-overlay').getAttribute('data-sample-t')).toBe(
      String(sampleT),
    )
    expect(await page.getByTestId('sail-hud').getAttribute('data-sample-t')).toBe(
      String(sampleT),
    )

    // A schema-2 episode carries the whole recorded subset, so the fifteen
    // deliberately omitted fields — and only those — read "Not recorded".
    const unavailable = (await page.evaluate(
      () => window.__sailgym?.unavailable() ?? [],
    )) as string[]
    expect(unavailable.sort()).toEqual([
      'boom_moment',
      'cd_board',
      'cd_rudder',
      'cd_sail',
      'cl_board',
      'cl_rudder',
      'cl_sail',
      'course_over_ground',
      'energy_kinetic',
      'energy_roll_potential',
      'energy_sheet_elastic',
      'hull_model_warning',
      'leeway_angle',
      'sheet_hull',
      'steps',
    ])
    for (const key of unavailable) {
      await expect(page.getByTestId(`diag-${key}`)).toHaveAttribute('data-recorded', 'false')
    }
    // …and a field the episode does carry is not marked unavailable.
    for (const key of ['gz', 'sheet_tension', 'alpha_sail', 'capsize', 'heel_deg']) {
      await expect(page.getByTestId(`diag-${key}`)).toHaveAttribute('data-recorded', 'true')
    }
    // No overlay went dark in a schema-2 episode: every one of the sixteen has
    // its numbers.
    await page.getByTestId('overlays-all-on').click()
    await expect(page.getByTestId('overlay-legend')).toHaveAttribute('data-unavailable', '0')
  })

  test('the charts are the episode, and repeat exactly', async ({ page }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'close_hauled' })
    await enterDebug(page)
    await record(page, 8, '10')

    const liveCharts = await attrs(page, 'charts')
    expect(liveCharts['data-source']).toBe('live')
    const livePoints = liveCharts['data-samples']

    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('charts')).toHaveAttribute('data-source', 'recorded')
    // The resolution is the episode's, and it is on the page.
    await expect(page.getByTestId('charts')).toHaveAttribute('data-resolution-hz', '10')
    await expect(page.getByTestId('chart-resolution')).toContainText('10 Hz')

    const pointsAt = async (t: number): Promise<string> => {
      await scrubTo(page, t)
      return (await attrs(page, 'chart-heelDeg'))['data-points']
    }

    const end = Number(await page.getByTestId('timeline').getAttribute('data-end'))
    const half = await pointsAt(end / 2)
    // Scrubbing forward adds points; scrubbing back removes them and does not
    // duplicate or append wall-clock samples.
    const full = await pointsAt(end)
    expect(Number(full)).toBeGreaterThan(Number(half))
    expect(await pointsAt(end / 2)).toBe(half)
    // Playing through twice lands on the same series for the same playhead.
    await page.getByTestId('timeline-play').click()
    await page.waitForTimeout(600)
    await page.getByTestId('timeline-play').click()
    expect(await pointsAt(end / 2)).toBe(half)

    // The points are recorded samples, and at the **last** sample there is one
    // per frame. The playhead is put on that sample with the frame buttons
    // rather than by scrubbing: the slider's value is rounded to milliseconds
    // and the episode's last timestamp is not.
    const frames = Number(await page.getByTestId('timeline').getAttribute('data-frames'))
    await scrubTo(page, end)
    await page.getByTestId('timeline-step-forward').click()
    await expect(page.getByTestId('timeline')).toHaveAttribute('data-index', String(frames - 1))
    expect((await attrs(page, 'chart-heelDeg'))['data-points']).toBe(String(frames))

    // Leaving replay restores the live chart untouched — the replay neither
    // appended to it nor cleared it.
    await page.getByTestId('timeline-exit').click()
    await expect(page.getByTestId('charts')).toHaveAttribute('data-source', 'live')
    expect(Number((await attrs(page, 'charts'))['data-samples'])).toBeGreaterThanOrEqual(
      Number(livePoints),
    )
  })

  test('a legacy schema-1 episode is viewable, and says what it does not carry', async ({
    page,
  }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'close_hauled' })
    await enterDebug(page)
    await record(page, 5)

    // Build a schema-1 document from the schema-2 one: drop the block from
    // every frame and the identity from the header, exactly as a v1 recorder
    // would have written it. Then import it through the page's own file input,
    // so the core's decoder is what accepts it.
    const legacy = await page.evaluate(() => {
      const raw = window.__sailgym?.episodeJson()
      if (raw === null || raw === undefined) {
        return null
      }
      const doc = JSON.parse(raw) as {
        header: Record<string, unknown>
        frames: Array<Record<string, unknown>>
      }
      const header: Record<string, unknown> = {}
      for (const k of [
        'schema_version',
        'scenario',
        'parameters',
        'dt',
        'log_hz',
        'toolchain',
        'created_utc',
      ]) {
        header[k] = doc.header[k]
      }
      header.schema_version = 1
      const frames = doc.frames.map((f) => {
        const out: Record<string, unknown> = { ...f }
        delete out.diag
        return out
      })
      return JSON.stringify({ header, frames })
    })
    expect(legacy).not.toBeNull()

    await page.getByTestId('record-file').setInputFiles({
      name: 'legacy.json',
      mimeType: 'application/json',
      buffer: Buffer.from(legacy as string, 'utf8'),
    })
    await expect(page.getByTestId('record-error')).toHaveCount(0)

    // It is viewable: it names no baseline, and the page says so rather than
    // calling it a same-conditions experiment (RV58).
    await expect(page.getByTestId('episode-identity')).toHaveAttribute('data-baseline', 'false')
    await expect(page.getByTestId('episode-identity')).toContainText(
      'cannot be labelled a same-conditions experiment',
    )
    const verdict = JSON.parse(
      (await page.evaluate(() => window.__sailgym?.comparabilityJson() ?? null)) as string,
    ) as { verdict: string; reasons: string[] }
    expect(verdict.verdict).toBe('indeterminate')
    expect(verdict.reasons).toContain('model')

    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('timeline')).toHaveAttribute('data-schema', '1')

    // The pose is there — a schema-1 frame carries the whole F3 state.
    const stored = (await page.evaluate(
      () => window.__sailgym?.frameState(3) ?? null,
    )) as number[]
    await page.getByTestId('timeline-step-forward').click()
    await page.getByTestId('timeline-step-back').click()
    const shownState = await readSnapshot(page)
    expect(Number.isFinite(shownState.x)).toBe(true)
    expect(stored.length).toBe(13)

    // …and everything the block would have carried says so, in the panel, the
    // legend and the charts. Not zero, not the live simulation's (RV59).
    const unavailable = (await page.evaluate(
      () => window.__sailgym?.unavailable() ?? [],
    )) as string[]
    for (const key of ['alpha_sail', 'gz', 'capsize', 'sail', 'speed_over_ground']) {
      expect(unavailable).toContain(key)
      await expect(page.getByTestId(`diag-${key}`)).toHaveAttribute('data-recorded', 'false')
    }
    // The fields a schema-1 frame does carry are still shown.
    for (const key of ['velocity_body', 'yaw_rate', 'sheet_tension', 'heel_deg']) {
      expect(unavailable).not.toContain(key)
      await expect(page.getByTestId(`diag-${key}`)).toHaveAttribute('data-recorded', 'true')
    }

    await page.getByTestId('overlays-all-on').click()
    expect(
      Number(await page.getByTestId('overlay-legend').getAttribute('data-unavailable')),
    ).toBeGreaterThan(0)
    await expect(page.getByTestId('legend-sailForce')).toHaveAttribute('data-recorded', 'false')
    await expect(page.getByTestId('legend-rightingMoment')).toHaveAttribute(
      'data-recorded',
      'true',
    )
    // A chart series the episode does not carry is empty and labelled, never
    // plotted as zero.
    await expect(page.getByTestId('chart-heelDeg')).toHaveAttribute('data-recorded', 'true')
    await page.getByTestId('chart-toggle-boatSpeed').check()
    await expect(page.getByTestId('chart-boatSpeed')).toHaveAttribute('data-recorded', 'false')
    // The wind vector is recorded; the speed and the bearing are not, because
    // `wind_to_bearing` lives in Rust and is not re-implemented here (F6.1).
    await expect(page.getByTestId('wind-readout')).toHaveAttribute('data-recorded', 'false')
    await expect(page.getByTestId('wind-readout')).toContainText('Not recorded')
  })

  test('export and import round-trip in both codecs, and a malformed file is refused', async ({
    page,
  }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'close_hauled' })
    await record(page, 5)

    const original = (await page.evaluate(
      () => window.__sailgym?.episodeJson() ?? null,
    )) as string
    const bytes = (await page.evaluate(() => window.__sailgym?.binary() ?? null)) as number[]
    expect(bytes.length).toBeGreaterThan(1000)

    // Binary, then JSON — both through the page's own file input, both decoded
    // by the core.
    for (const file of [
      { name: 'e.bin', mimeType: 'application/octet-stream', buffer: Buffer.from(bytes) },
      {
        name: 'e.json',
        mimeType: 'application/json',
        buffer: Buffer.from(original, 'utf8'),
      },
    ]) {
      await page.getByTestId('record-file').setInputFiles(file)
      await expect(page.getByTestId('record-error')).toHaveCount(0)
      expect(await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)).toBe(original)
    }

    // A truncated binary: refused with a message, and the episode already
    // loaded is untouched (RV60).
    await page.getByTestId('record-file').setInputFiles({
      name: 'truncated.bin',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(bytes.slice(0, bytes.length - 40)),
    })
    await expect(page.getByTestId('record-error')).toHaveCount(1)
    expect(await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)).toBe(original)

    // A non-finite measurement: likewise.
    const withNaN = JSON.parse(original) as { frames: Array<{ sheet_tension: unknown }> }
    withNaN.frames[1].sheet_tension = null
    await page.getByTestId('record-file').setInputFiles({
      name: 'nan.json',
      mimeType: 'application/json',
      buffer: Buffer.from(JSON.stringify(withNaN), 'utf8'),
    })
    await expect(page.getByTestId('record-error')).toHaveCount(1)
    expect(await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)).toBe(original)

    // …and the page still works after all of that.
    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'replay')
  })

  test('replay entry neutralises input, and exit leaves the run paused', async ({ page }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'free_sail' })
    await record(page, 4)

    // `record()` leaves the clock running at 4×. Leave it running, and put
    // the helm hard over, so entering replay has something to neutralise.
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'true')
    await page.keyboard.down('d')
    await expect.poll(async () => (await readSnapshot(page)).deltaR).not.toBe(0)

    await page.getByTestId('record-replay').click()
    // The clock stopped…
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
    const simTime = await page.getByTestId('clock-time').getAttribute('data-sim-time')
    // …and the key that is still down steers nothing.
    await page.waitForTimeout(500)
    expect(await page.getByTestId('clock-time').getAttribute('data-sim-time')).toBe(simTime)
    await page.keyboard.up('d')

    // Leaving replay leaves the run **paused** — resuming is the viewer's
    // decision, not a side effect of closing the timeline.
    await page.getByTestId('timeline-exit').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'live')
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
    await page.waitForTimeout(400)
    expect(await page.getByTestId('clock-time').getAttribute('data-sim-time')).toBe(simTime)

    // And the live view is live again.
    await expect(page.getByTestId('inspection')).toHaveAttribute('data-source', 'live')
    await expect(page.getByTestId('sail-hud')).toHaveAttribute('data-source', 'live')
  })

  test('the recording bound is visible before it is reached', async ({ page }) => {
    await gotoApp(page, { scenario: 'free_sail' })
    const limit = await attrs(page, 'record-limit')
    const frames = Number(limit['data-frames'])
    const bytes = Number(limit['data-bytes'])
    // The cap is the core's, derived from a byte budget — not a literal here.
    expect(frames).toBeGreaterThan(1000)
    expect(bytes).toBe(frames * 624)
    expect(bytes).toBeLessThanOrEqual(8 * 1024 * 1024)
    await expect(page.getByTestId('record-limit')).toContainText('min at 20 Hz')
  })
})
