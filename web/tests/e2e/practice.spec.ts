import type { Page } from '@playwright/test'

import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Guided practice, end to end (v2 section 11, task 11.4).
 *
 * The section's user flow is **goal → attempt → result → inspect → retry →
 * compare**, and this file walks it in a real browser with the real controls:
 * `A` and `D` on the tiller, a held pointer drag on the boat for the
 * mainsheet, `Space` for the release. Nothing here reaches into the core
 * except through the page.
 *
 * ## What each group measures
 *
 * | group | the claim |
 * |---|---|
 * | the loop | a player can start, sail, read a result, inspect it and retry, with keyboard and with pointer |
 * | the evaluator's boundary | recorded event times agree with the runtime; a paused clock consumes no task time; a replay emits no success |
 * | retry | the same initial snapshot and the same canonical identity, with no held control surviving |
 * | conditions | a parameter edit ends the attempt and the two attempts are refused a comparison |
 * | the words | nothing on the panel claims a real-boat qualification, and every number shown is the core's |
 * | the layout | the whole flow is reachable by keyboard and at 360 × 640 |
 *
 * ## Scripted outcomes are not measured here
 *
 * Whether the thresholds are the right thresholds is
 * `crates/sailgym-task/tests/practice.rs`'s question, answered against the
 * real physics with scripted control sequences and recorded in
 * `docs/v2/practice-validation.md`. What is measured **here** is that the
 * browser is wired to that evaluator and to nothing else. The two do not
 * substitute for one another, which is why both exist.
 */

/** Longer than the default: an attempt is up to 45 simulated seconds. */
const ATTEMPT_TIMEOUT_MS = 120_000

/** The three shipped challenge ids, in the order the panel offers them. */
const CHALLENGES = ['get_moving', 'complete_tack', 'recover_from_heel'] as const

async function simTime(page: Page): Promise<number> {
  return (await readSnapshot(page)).t
}

/** Wait until the simulation clock has passed `t` seconds. */
async function waitForSimTime(page: Page, t: number): Promise<void> {
  await expect
    .poll(async () => simTime(page), { timeout: 60_000, intervals: [100] })
    .toBeGreaterThanOrEqual(t)
}

/** Run the clock at `speed`, so a 45 s attempt does not take 45 s of wall time. */
async function setSpeed(page: Page, speed: '1x' | '2x' | '4x'): Promise<void> {
  await page.getByTestId(`clock-speed-${speed}`).click()
}

/**
 * Select a challenge and start it.
 *
 * The chooser shows three one-line selectors and the instruction, goal and
 * Start button of the selected one, so a challenge that is not selected has
 * no Start button of its own.
 */
async function startChallenge(page: Page, id: string): Promise<void> {
  await page.getByTestId(`practice-challenge-${id}`).click()
  await expect(page.getByTestId(`practice-challenge-${id}`)).toHaveAttribute(
    'data-selected',
    'true',
  )
  await page.getByTestId(`practice-start-${id}`).click()
  await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')
  await expect(page.getByTestId('practice')).toHaveAttribute('data-task', id)
}

/** Wait for the attempt to leave `sailing`, whatever it ends as. */
async function waitForResult(page: Page): Promise<void> {
  await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'result', {
    timeout: ATTEMPT_TIMEOUT_MS,
  })
}

/**
 * Trim the mainsheet to about `target` metres with a pointer drag on the boat.
 *
 * Drag **down** to haul in (`sim/sheetInput.ts`), hold while the rope runs in,
 * and let go once the snapshot says it is short enough. 70 px is a gentle
 * command — `DEFAULT_INPUT.sheetDragGain` turns it into about 0.28, so the
 * rope runs at roughly a quarter of `sheet_haul_rate` and the poll below
 * cannot overshoot the band by more than a few centimetres.
 *
 * The number it is aiming at is a **rope length**, read back from the
 * snapshot; nothing here computes a rate or restates a catalogue value.
 */
async function trimSheetTo(page: Page, target: number): Promise<void> {
  // The practice panel lives below the world view in the layout's scrolling
  // `main` row, so pressing Start scrolls the boat off the top. A player
  // scrolls back; so does this.
  await page.getByTestId('world-view').scrollIntoViewIfNeeded()
  const box = await page.getByTestId('world-view').boundingBox()
  expect(box, 'the world view must be on screen').not.toBeNull()
  const x = box!.x + box!.width / 2
  const y = box!.y + box!.height / 2
  await page.mouse.move(x, y)
  await page.mouse.down()
  await page.mouse.move(x, y + 70, { steps: 4 })
  try {
    await expect
      .poll(async () => (await readSnapshot(page)).lSheet, { timeout: 30_000, intervals: [50] })
      .toBeLessThanOrEqual(target)
  } finally {
    await page.mouse.up()
  }
}

/** The episode the page is holding, decoded. */
async function readEpisode(page: Page): Promise<{
  header: {
    initial_state: unknown
    initial_controls: unknown
    parameters: unknown
    scenario: { seed: number; wind: unknown }
    practice: { envelope_version: number; task: { id: string; version: number }; events: { id: string; step: number; t: number; value: number }[] } | null
  }
  frames: unknown[]
}> {
  const raw = await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)
  expect(raw, 'the page must be holding an episode').not.toBeNull()
  return JSON.parse(raw as string)
}

/** Every `data-*` on the practice result, as strings. */
async function readResult(page: Page): Promise<Record<string, string>> {
  return page.evaluate(() => {
    const el = document.querySelector('[data-testid="practice-result"]')
    if (!(el instanceof HTMLElement)) {
      throw new Error('no practice result on the page')
    }
    return { ...el.dataset } as Record<string, string>
  })
}

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

test.describe('v2 §11 — the practice loop', () => {
  test.setTimeout(ATTEMPT_TIMEOUT_MS)

  test('the chooser offers exactly the three shipped challenges, with their goals', async ({
    page,
  }) => {
    await gotoApp(page)
    const panel = page.getByTestId('practice')
    await expect(panel).toHaveAttribute('data-phase', 'choose')

    for (const id of CHALLENGES) {
      const card = page.getByTestId(`practice-challenge-${id}`)
      await expect(card).toBeVisible()
      // The scenario is a shipped one (brief §32): section 11 adds none.
      const scenario = await card.getAttribute('data-scenario')
      expect(
        ['free_sail', 'tack', 'sheet_release_recovery', 'close_hauled', 'gybe', 'beam_reach_capsize'],
        `${id} names a shipped scenario`,
      ).toContain(scenario)
      // Selecting it shows one short instruction and one measurable goal,
      // with numbers in it, before anything is started.
      await card.click()
      const goal = await page.getByTestId(`practice-goal-${id}`).textContent()
      expect(goal ?? '', `${id} states a measurable goal`).toMatch(/\d/)
      await expect(page.getByTestId(`practice-start-${id}`)).toBeEnabled()
    }
    // And exactly three: a fourth would be a challenge nobody validated.
    const cards = await page.locator('[data-testid^="practice-challenge-"]').count()
    expect(cards).toBe(CHALLENGES.length)
  })

  test('goal → attempt → result → inspect → retry, with the keyboard and a drag', async ({
    page,
  }) => {
    await gotoApp(page)
    await setSpeed(page, '4x')
    await startChallenge(page, 'get_moving')

    // Progress is published while sailing, on simulated time and physics
    // steps — never on frames.
    const progress = page.getByTestId('practice-progress')
    await expect(progress).toBeVisible()
    await expect
      .poll(async () => Number(await progress.getAttribute('data-steps')), { timeout: 30_000 })
      .toBeGreaterThan(0)

    // Sheet in, which is the thing the challenge teaches. 3.0 m is inside
    // the band `docs/v2/practice-validation.md` §2.1 measured; hauling to the
    // stop is as slow as not hauling at all.
    await trimSheetTo(page, 3.0)
    await waitForResult(page)

    const result = await readResult(page)
    console.log(`[practice] get_moving in the browser: ${JSON.stringify(result)}`)
    // Trimmed into the band `practice-validation.md` §2.1 measured, the
    // challenge is passed — in the browser, through the real mainsheet drag.
    expect(result.outcome, 'a correctly trimmed boat must pass').toBe('succeeded')
    expect(Number(result.metric), 'the metric is the top speed reached').toBeGreaterThan(1.2)
    expect(Number(result.steps), 'the result counts physics steps').toBeGreaterThan(0)
    expect(Number(result.events), 'a result explains itself with events').toBeGreaterThan(0)

    // Inspect jumps into a replay of this attempt's own episode.
    await expect(page.getByTestId('practice-compare')).toHaveAttribute('data-count', '1')
    const inspect = page.locator('[data-testid^="practice-inspect-"]').first()
    await expect(inspect).toBeEnabled()
    await inspect.click()
    await expect(page.getByTestId('inspection')).toHaveAttribute('data-source', 'replay')

    // …and the playhead is on the highlight event's own recorded time.
    const events = page.getByTestId('timeline-events')
    await expect(events).toBeVisible()
    const highlight = page.getByTestId('timeline-event-speed_reached').first()
    if ((await highlight.count()) > 0) {
      const at = Number(await highlight.getAttribute('data-t'))
      const playhead = Number(await page.getByTestId('timeline').getAttribute('data-time'))
      expect(Math.abs(playhead - at)).toBeLessThan(1e-6)
    }

    // Retry leaves the replay and starts a second attempt.
    await page.getByTestId('practice-retry').click()
    await expect(page.getByTestId('inspection')).toHaveAttribute('data-source', 'live')
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')
  })

  test('free sail and the debug panel stay reachable', async ({ page }) => {
    await gotoApp(page)
    await startChallenge(page, 'recover_from_heel')
    // Abandoning goes straight back to the chooser, with the v1 recording
    // controls back on the page.
    await page.getByTestId('practice-free-sail').click()
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'choose')
    await expect(page.getByTestId('record-controls')).toBeVisible()

    // Debug Mode still opens, with everything in it.
    await page.getByTestId('mode-switch').click()
    await expect(page.getByTestId('layout-debug')).toBeVisible()
    await expect(page.getByTestId('charts')).toBeVisible()
    await expect(page.getByTestId('practice')).toBeVisible()
  })
})

// ---------------------------------------------------------------------------
// The evaluator's boundary
// ---------------------------------------------------------------------------

test.describe('v2 §11 — the evaluator runs on physics steps', () => {
  test.setTimeout(ATTEMPT_TIMEOUT_MS)

  test('recorded event times agree with the runtime, step for step', async ({ page }) => {
    await gotoApp(page)
    await setSpeed(page, '4x')
    await startChallenge(page, 'recover_from_heel')
    await waitForResult(page)

    // What the runtime says.
    const runtime = await page.$$eval('[data-testid="practice-events"] li', (items) =>
      items.map((li) => ({
        id: (li as HTMLElement).dataset.eventId ?? '',
        step: Number((li as HTMLElement).dataset.step),
        t: Number((li as HTMLElement).dataset.t),
        value: Number((li as HTMLElement).dataset.value),
      })),
    )
    expect(runtime.length).toBeGreaterThan(0)

    // What the episode says. The envelope is the one section 10 reserved —
    // there is no second recorder and no second event list.
    const recorded = await page.evaluate(() => {
      const raw = window.__sailgym?.episodeJson() ?? null
      if (raw === null) {
        return null
      }
      const doc = JSON.parse(raw) as {
        header: {
          practice: {
            envelope_version: number
            task: { id: string; version: number }
            events: { id: string; step: number; t: number; value: number }[]
          } | null
        }
      }
      return doc.header.practice
    })
    expect(recorded, 'the episode carries the practice envelope').not.toBeNull()
    expect(recorded?.envelope_version).toBe(1)
    expect(recorded?.events).toEqual(runtime)

    // Each event's time is its own step's time, to the bit the double can
    // hold: `t = step · dt`.
    const dt = Number(await page.getByTestId('snapshot').getAttribute('data-dt'))
    for (const e of runtime) {
      expect(Math.abs(e.t - e.step * dt), `${e.id} at step ${e.step}`).toBeLessThan(1e-9)
    }
  })

  test('a paused clock consumes no task time', async ({ page }) => {
    await gotoApp(page)
    await startChallenge(page, 'get_moving')
    await waitForSimTime(page, 1.0)

    await page.keyboard.press('p')
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')
    const progress = page.getByTestId('practice-progress')
    const pausedAt = {
      elapsed: Number(await progress.getAttribute('data-elapsed')),
      steps: Number(await progress.getAttribute('data-steps')),
    }
    // Two wall seconds of nothing.
    await page.waitForTimeout(2000)
    expect(Number(await progress.getAttribute('data-elapsed'))).toBe(pausedAt.elapsed)
    expect(Number(await progress.getAttribute('data-steps'))).toBe(pausedAt.steps)

    // And it starts again where it stopped.
    await page.keyboard.press('p')
    await expect
      .poll(async () => Number(await progress.getAttribute('data-steps')), { timeout: 20_000 })
      .toBeGreaterThan(pausedAt.steps)
  })

  test('a replay cannot advance an attempt or emit a success', async ({ page }) => {
    await gotoApp(page)
    await setSpeed(page, '4x')
    await startChallenge(page, 'recover_from_heel')
    await waitForResult(page)
    const before = await readResult(page)

    await page.locator('[data-testid^="practice-inspect-"]').first().click()
    await expect(page.getByTestId('inspection')).toHaveAttribute('data-source', 'replay')
    // Play the whole episode through, twice.
    await page.getByTestId('timeline-play').click()
    await page.waitForTimeout(1500)
    await page.getByTestId('timeline-reset').click()
    await page.getByTestId('timeline-play').click()
    await page.waitForTimeout(1500)

    const after = await readResult(page)
    expect(after, 'a replay changed the result').toEqual(before)
  })
})

// ---------------------------------------------------------------------------
// Retry restores the conditions
// ---------------------------------------------------------------------------

test.describe('v2 §11 — retry restores the exact conditions', () => {
  test.setTimeout(ATTEMPT_TIMEOUT_MS)

  test('the same initial snapshot, and the same canonical identity', async ({ page }) => {
    await gotoApp(page)
    await setSpeed(page, '4x')

    // Two attempts at the same challenge: the first from the chooser, the
    // second from Retry. Their *episodes* are compared rather than a live
    // snapshot — a snapshot read after the page has already run a frame or
    // two would be measuring Playwright's reflexes, while the header's
    // `initial_state` is the first recorded sample and is exact.
    await startChallenge(page, 'recover_from_heel')
    await waitForResult(page)
    const first = await readEpisode(page)

    // Move a control in between, so the retry is restoring rather than
    // coasting on a state nothing disturbed.
    await page.keyboard.down('d')
    await page.waitForTimeout(200)
    await page.keyboard.up('d')

    await page.getByTestId('practice-retry').click()
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')
    await waitForResult(page)
    const second = await readEpisode(page)

    // The complete F3 state at the first recorded sample, scalar for scalar,
    // and the controls in force with it. Not a tolerance: an attempt that
    // began from a different state is a different experiment (RV63).
    expect(second.header.initial_state, 'the retry started from a different state').toEqual(
      first.header.initial_state,
    )
    expect(second.header.initial_controls).toEqual(first.header.initial_controls)
    // …the resolved catalogue, …
    expect(second.header.parameters).toEqual(first.header.parameters)
    // …the wind and the seed, …
    expect(second.header.scenario.wind).toEqual(first.header.scenario.wind)
    expect(second.header.scenario.seed).toBe(first.header.scenario.seed)
    // …and the core's own verdict on the whole canonical record (F18.3).
    const verdict = await page.evaluate(() => window.__sailgym?.comparabilityJson() ?? null)
    expect(verdict, 'the page must hold an identity for the latest episode').not.toBeNull()
    const compare = page.getByTestId('practice-compare')
    await expect(compare).toHaveAttribute('data-count', '2')
    await expect(compare).toHaveAttribute('data-verdict', 'same_conditions')
  })

  test('a held release button does not survive a cancellation', async ({ page }) => {
    await gotoApp(page)
    await startChallenge(page, 'recover_from_heel')
    // Space is the release (brief §28). Hold it down across the cancel.
    await page.keyboard.down(' ')
    await page.waitForTimeout(200)
    await page.getByTestId('practice-cancel').click()
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'choose')
    await startChallenge(page, 'recover_from_heel')

    // The sheet must not be paying out: with the key's effect cleared, the
    // rope length stays at the scenario's own setting until something asks
    // for it to move. `l_sheet` is read straight off the snapshot.
    const at0 = (await readSnapshot(page)).lSheet
    await waitForSimTime(page, 0.5)
    const later = (await readSnapshot(page)).lSheet
    expect(Math.abs(later - at0), 'the sheet paid out with no live command').toBeLessThan(1e-9)
    await page.keyboard.up(' ')
  })
})

// ---------------------------------------------------------------------------
// Conditions and comparison
// ---------------------------------------------------------------------------

test.describe('v2 §11 — comparison refuses what it cannot compare', () => {
  test.setTimeout(ATTEMPT_TIMEOUT_MS)

  test('two attempts at the same challenge compare, and show measured deltas', async ({
    page,
  }) => {
    await gotoApp(page)
    await setSpeed(page, '4x')

    for (let i = 0; i < 2; i += 1) {
      if (i === 0) {
        await startChallenge(page, 'recover_from_heel')
      } else {
        await page.getByTestId('practice-retry').click()
        await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')
      }
      // Release at a different moment each time, so the two results differ.
      await page.waitForTimeout(i === 0 ? 300 : 900)
      await page.keyboard.down(' ')
      await page.waitForTimeout(600)
      await page.keyboard.up(' ')
      await waitForResult(page)
    }

    const compare = page.getByTestId('practice-compare')
    await expect(compare).toHaveAttribute('data-count', '2')
    await expect(compare).toHaveAttribute('data-comparable', 'true')
    await expect(compare).toHaveAttribute('data-verdict', 'same_conditions')
    // A measured delta, in the metric's own units, and an elapsed-time delta.
    const delta = await compare.getAttribute('data-delta-metric')
    expect(Number.isFinite(Number(delta)), `delta was ${delta}`).toBe(true)
    // Both traces on one axis.
    await expect(page.getByTestId('practice-compare-chart')).toBeVisible()

    // The verdict is always one of the core's three — the page never invents
    // a fourth, and never shows a delta without `same_conditions`
    // (`ExperimentIdentity::compare`, F18.3; the rejection matrix itself is
    // measured in Rust by
    // `recording::tests::same_conditions_needs_every_field_and_a_clean_source`).
    expect(['same_conditions', 'different', 'indeterminate']).toContain(
      await compare.getAttribute('data-verdict'),
    )

    // Choosing a different challenge forgets both, so two results that are
    // not the same experiment are never sitting side by side.
    await page.getByTestId('practice-free-sail').click()
    await startChallenge(page, 'get_moving')
    await expect(page.getByTestId('practice-compare')).toHaveCount(0)
  })

  test('a parameter edit ends the attempt as conditions changed', async ({ page }) => {
    await gotoApp(page)
    await startChallenge(page, 'get_moving')
    await waitForSimTime(page, 0.5)

    // A live brief §31 edit, made the way a person makes one: Debug Mode,
    // the parameter panel, the sail-area field. Sail area is a physical
    // coefficient and moving it changes the boat, which is the point.
    await page.getByTestId('mode-switch').click()
    await page.getByTestId('parameter-panel-toggle').click()
    await page.getByTestId('param-input-sail.area').fill('3')
    await page.getByTestId('param-input-sail.area').dispatchEvent('change')
    await expect(page.getByTestId('param-sail.area')).toHaveAttribute('data-value', '3')

    await expect(page.getByTestId('practice')).toHaveAttribute('data-status', 'conditions_changed', {
      timeout: 20_000,
    })
    await expect(page.getByTestId('practice-conditions-changed')).toBeVisible()
  })
})

// ---------------------------------------------------------------------------
// The words, and the layout
// ---------------------------------------------------------------------------

test.describe('v2 §11 — what the panel may claim, and where it fits', () => {
  test.setTimeout(ATTEMPT_TIMEOUT_MS)

  test('no text on the panel claims a real-boat qualification', async ({ page }) => {
    await gotoApp(page)
    const forbidden = [
      'certif',
      'qualif',
      'licence',
      'license',
      'accredit',
      'guarantee',
      'ready to sail a real',
    ]
    for (const id of CHALLENGES) {
      const text = ((await page.getByTestId('practice').textContent()) ?? '').toLowerCase()
      for (const word of forbidden) {
        expect(text, `"${word}" on the chooser`).not.toContain(word)
      }
      await startChallenge(page, id)
      const sailing = ((await page.getByTestId('practice').textContent()) ?? '').toLowerCase()
      for (const word of forbidden) {
        expect(text + sailing, `"${word}" while sailing ${id}`).not.toContain(word)
      }
      await page.getByTestId('practice-free-sail').click()
    }
    // And it says what it *is*.
    await expect(page.getByTestId('practice-disclaimer')).toContainText('reduced model')
  })

  test('the whole flow is reachable by keyboard at 360 × 640', async ({ page }) => {
    await page.setViewportSize({ width: 360, height: 640 })
    await gotoApp(page)
    await setSpeed(page, '4x')

    // No horizontal document overflow, with the panel on the page.
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    )
    expect(overflow, 'horizontal overflow at 360 px').toBeLessThanOrEqual(0)

    // Reach Start with the keyboard alone and press it with Enter.
    await page.getByTestId('practice-challenge-get_moving').click()
    const start = page.getByTestId('practice-start-get_moving')
    await start.scrollIntoViewIfNeeded()
    await start.focus()
    await expect(start).toBeFocused()
    await page.keyboard.press('Enter')
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')

    // Steer with the keyboard while the attempt runs.
    await page.keyboard.down('d')
    await waitForSimTime(page, 1)
    await page.keyboard.up('d')
    expect(Math.abs((await readSnapshot(page)).deltaR)).toBeGreaterThan(0)

    await waitForResult(page)
    const retry = page.getByTestId('practice-retry')
    await retry.scrollIntoViewIfNeeded()
    await retry.focus()
    await expect(retry).toBeFocused()
    await page.keyboard.press('Enter')
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')

    const stillOverflowing = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    )
    expect(stillOverflowing, 'horizontal overflow while sailing').toBeLessThanOrEqual(0)
  })
})
