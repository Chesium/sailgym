import type { Page } from '@playwright/test'

import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Recording, replay and episode files in the browser (brief §33, tasks 9.4
 * and 9.5).
 *
 * The load-bearing assertion is the negative one: **a replay consumes stored
 * trajectory data and does not recompute the physics.** That cannot be shown
 * by matching numbers — a recomputation would match too. It is shown by
 * editing a stored frame in memory and watching the render follow the edit.
 *
 * Export and import go through the real paths: the encoders are the core's
 * (`episode_to_binary` / `episode_from_binary`), and loading uses the page's
 * own file input, driven with `setInputFiles` so no dialog is involved.
 */

/** Index of `x` in the F8.3 state layout. */
const X = 0

async function timeline(page: Page): Promise<Record<string, number | string>> {
  return page.getByTestId('timeline').evaluate((el) => ({
    frames: Number(el.getAttribute('data-frames')),
    index: Number(el.getAttribute('data-index')),
    time: Number(el.getAttribute('data-time')),
    start: Number(el.getAttribute('data-start')),
    end: Number(el.getAttribute('data-end')),
    playing: el.getAttribute('data-playing') ?? '',
  }))
}

/**
 * Record about `seconds` of simulated time at 4× and stop.
 *
 * The recording is started immediately after a reset, so frame 0 is at
 * `t = 0` and the frame times are exact multiples of the logging interval.
 */
async function record(page: Page, seconds: number): Promise<void> {
  await page.getByTestId('clock-pause').click()
  await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
  await page.getByTestId('clock-reset').click()
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
 * Scrub to `t`, then snap onto the nearest recorded frame with the frame
 * buttons, so the playhead sits exactly on a sample rather than between two.
 */
async function seekToFrameNear(page: Page, t: number): Promise<number> {
  const scrub = page.getByTestId('timeline-scrub')
  await scrub.fill(String(t))
  await page.getByTestId('timeline-step-forward').click()
  await page.getByTestId('timeline-step-back').click()
  const after = await timeline(page)
  return Number(after.index)
}

test.describe('replay', () => {
  test('records an episode and replays the stored frames', async ({ page }) => {
    test.setTimeout(90_000)
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await record(page, 10.5)

    const frames = Number(
      await page.getByTestId('record-summary').getAttribute('data-frame-count'),
    )
    // 10.5 s at the default 20 Hz.
    expect(frames).toBeGreaterThan(200)

    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'replay')
    await expect(page.getByTestId('timeline')).toHaveAttribute('data-index', '0')

    // The rendered boat at t = 5 s is the recorded frame at t = 5 s, to 1e-6.
    const index = await seekToFrameNear(page, 5)
    const state = await timeline(page)
    expect(Math.abs(Number(state.time) - 5)).toBeLessThan(0.1)

    const recorded = await page.evaluate((i) => window.__sailgym?.frameState(i) ?? null, index)
    expect(recorded).not.toBeNull()
    const rendered = await readSnapshot(page)
    expect(Math.abs(rendered.x - (recorded as number[])[0])).toBeLessThan(1e-6)
    expect(Math.abs(rendered.y - (recorded as number[])[1])).toBeLessThan(1e-6)
    expect(Math.abs(rendered.psi - (recorded as number[])[2])).toBeLessThan(1e-6)
    expect(Math.abs(rendered.phi - (recorded as number[])[3])).toBeLessThan(1e-6)
    // The episode has to be an episode: the boat moved and heeled.
    expect(Math.hypot(rendered.x, rendered.y)).toBeGreaterThan(1)
    expect(Math.abs(rendered.phi)).toBeGreaterThan(0.05)
  })

  test('shows stored states, not recomputed ones — proven by editing a frame', async ({
    page,
  }) => {
    test.setTimeout(90_000)
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await record(page, 10.5)
    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'replay')

    const atEight = await seekToFrameNear(page, 8)
    const before = (await readSnapshot(page)).x

    // Edit the stored frame in memory. A replay that recomputed the physics
    // would be unmoved by this; one that reads stored frames must follow it.
    await page.evaluate(
      ([index, field, value]) => {
        window.__sailgym?.patchFrame(index as number, field as number, value as number)
      },
      [atEight, X, before + 1234],
    )

    // Away to t = 2 s — which must itself show the recorded state — and back.
    const atTwo = await seekToFrameNear(page, 2)
    expect(atTwo).toBeLessThan(atEight)
    const twoRecorded = await page.evaluate(
      (i) => window.__sailgym?.frameState(i) ?? null,
      atTwo,
    )
    const twoRendered = await readSnapshot(page)
    expect(Math.abs(twoRendered.x - (twoRecorded as number[])[0])).toBeLessThan(1e-9)

    const back = await seekToFrameNear(page, 8)
    expect(back).toBe(atEight)
    const after = await readSnapshot(page)
    expect(Math.abs(after.x - (before + 1234))).toBeLessThan(1e-9)
  })

  test('reset to episode start returns to frame 0', async ({ page }) => {
    test.setTimeout(90_000)
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await record(page, 10.5)
    await page.getByTestId('record-replay').click()

    await seekToFrameNear(page, 7)
    expect(Number((await timeline(page)).index)).toBeGreaterThan(0)

    await page.getByTestId('timeline-reset').click()
    const state = await timeline(page)
    expect(state.index).toBe(0)
    expect(Number(state.time)).toBeCloseTo(Number(state.start), 9)

    const zero = await page.evaluate(() => window.__sailgym?.frameState(0) ?? null)
    const rendered = await readSnapshot(page)
    expect(Math.abs(rendered.x - (zero as number[])[0])).toBeLessThan(1e-12)
    expect(Math.abs(rendered.t - (zero as number[])[12])).toBeLessThan(1e-12)
  })

  test('plays back with the physics clock paused', async ({ page }) => {
    test.setTimeout(90_000)
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await record(page, 10.5)
    await page.getByTestId('record-replay').click()

    // Entering replay pauses the simulation, and it stays paused.
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
    const clockBefore = Number(
      await page.getByTestId('clock-time').getAttribute('data-sim-time'),
    )

    await page.getByTestId('timeline-play').click()
    await expect(page.getByTestId('timeline')).toHaveAttribute('data-playing', 'true')
    const started = Number((await timeline(page)).time)
    await page.waitForTimeout(1200)
    const running = Number((await timeline(page)).time)
    await page.getByTestId('timeline-play').click()

    // The playhead moved…
    expect(running).toBeGreaterThan(started + 0.5)
    // …and the physics clock did not.
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
    const clockAfter = Number(
      await page.getByTestId('clock-time').getAttribute('data-sim-time'),
    )
    expect(clockAfter).toBe(clockBefore)

    // Back to live, and the simulation is still where it was.
    await page.getByTestId('timeline-exit').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'live')
  })

  test('exports and re-imports an episode unchanged, in both formats', async ({ page }) => {
    test.setTimeout(90_000)
    await gotoApp(page, { scenario: 'close_hauled' })
    await record(page, 6)

    const original = await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)
    expect(original).not.toBeNull()

    // The exported JSON is a valid `EpisodeHeader`/`EpisodeFrame` document.
    const parsed = JSON.parse(original as string) as {
      header: Record<string, unknown> & { toolchain: Record<string, unknown> }
      frames: Array<Record<string, unknown>>
    }
    expect(Object.keys(parsed.header).sort()).toEqual([
      'created_utc',
      'dt',
      'log_hz',
      'parameters',
      'scenario',
      'schema_version',
      'toolchain',
    ])
    expect(Object.keys(parsed.frames[0]).sort()).toEqual([
      'capsized',
      'controls',
      'forces',
      'moments',
      'reward',
      'sheet_tension',
      'state',
      't',
      'wind_at_boat',
    ])
    expect(parsed.header.schema_version).toBe(1)
    expect(Object.keys(parsed.header.toolchain).sort()).toEqual(['profile', 'rustc', 'target'])
    expect((parsed.frames[0].state as number[]).length).toBe(13)
    // brief §33's placeholder, and it really is zero everywhere.
    expect(parsed.frames.every((f) => f.reward === 0)).toBe(true)

    // --- binary: encode in the core, hand the bytes back through the page's
    // own file input, and compare the frame sequence.
    const bytes = await page.evaluate(() => window.__sailgym?.binary() ?? null)
    expect(bytes).not.toBeNull()
    expect((bytes as number[]).length).toBeGreaterThan(1000)
    await page.getByTestId('record-file').setInputFiles({
      name: 'episode.bin',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(bytes as number[]),
    })
    await expect(page.getByTestId('record-error')).toHaveCount(0)
    const fromBinary = await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)
    expect(fromBinary).toBe(original)

    // --- JSON: the same, through the other format.
    await page.getByTestId('record-file').setInputFiles({
      name: 'episode.json',
      mimeType: 'application/json',
      buffer: Buffer.from(original as string, 'utf8'),
    })
    await expect(page.getByTestId('record-error')).toHaveCount(0)
    const fromJson = await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)
    expect(fromJson).toBe(original)

    // …and the re-imported episode replays, frame for frame.
    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'replay')
    const count = await page.evaluate(() => window.__sailgym?.frameCount() ?? 0)
    expect(count).toBe((JSON.parse(original as string) as { frames: unknown[] }).frames.length)
    for (const index of [0, 5, Math.floor(count / 2), count - 1]) {
      const stored = await page.evaluate((i) => window.__sailgym?.frameState(i) ?? null, index)
      const source = (JSON.parse(original as string) as { frames: Array<{ state: number[] }> })
        .frames[index].state
      expect(stored).toEqual(source)
    }
  })

  test('rejects an episode from another schema version with a clear message', async ({
    page,
  }) => {
    test.setTimeout(90_000)
    await gotoApp(page, { scenario: 'close_hauled' })
    await record(page, 4)

    const original = (await page.evaluate(
      () => window.__sailgym?.episodeJson() ?? null,
    )) as string
    const document = JSON.parse(original) as { header: { schema_version: number } }
    document.header.schema_version = 7

    await page.getByTestId('record-file').setInputFiles({
      name: 'from-the-future.json',
      mimeType: 'application/json',
      buffer: Buffer.from(JSON.stringify(document), 'utf8'),
    })

    // A message, not a crash — and the core's own words, so the check is not
    // duplicated in TypeScript.
    const error = page.getByTestId('record-error')
    await expect(error).toHaveCount(1)
    await expect(error).toContainText('schema_version 7')
    await expect(error).toContainText('this build reads 1')

    // The episode already held is untouched, and the page still works.
    const after = await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)
    expect(after).toBe(original)
    await page.getByTestId('record-replay').click()
    await expect(page.getByTestId('record-controls')).toHaveAttribute('data-playback', 'replay')
  })
})
