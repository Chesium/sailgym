import type { Page } from '@playwright/test'

import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Waypoint courses, end to end (v2 section 12, task 12.9).
 *
 * The section's flow is **choose → sail → pass → race → finish → watch →
 * inspect → retry**, and this file walks the browser half of it: the numbered
 * overlay, the baseline, the ghost, **Watch baseline**, **Show course** and the
 * timeline markers, in a real browser against the real `Sim`.
 *
 * ## What is measured here and what is measured elsewhere
 *
 * | claim | where |
 * |---|---|
 * | what a waypoint *is*, and what a cut is | `crates/sailgym-course/tests/waypoints.rs`, `crates/sailgym-env/tests/course.rs` |
 * | whether the rule sailor can sail | `crates/sailgym-env/tests/rule_sailor.rs`, `docs/v2/baseline-validation.md` |
 * | whether a course challenge scores correctly | `crates/sailgym-task/tests/course.rs` |
 * | **that the page is wired to those and to nothing else** | here |
 *
 * Nothing in this file decides a passage, a state or a split. Every assertion
 * reads a `data-*` the page published from a number Rust produced, which is
 * RV73 as a test rather than as a promise.
 *
 * ## RV71: the baseline must not freeze the page
 *
 * `run_baseline` runs a whole episode inside one call. The budget is **2 s**
 * for the longest shipped course, and
 * {@link it-measures-the-baseline-wall-time} measures it in the browser rather
 * than asserting it from the Rust side.
 */

/** Longer than the default: a course attempt is up to 320 simulated seconds. */
const COURSE_TIMEOUT_MS = 180_000

/** The three shipped courses, in the order the panel offers them. */
const COURSES = ['course_reach', 'course_triangle', 'course_windward_leeward'] as const

/** RV71's budget for one `run_baseline`, in wall milliseconds. */
const BASELINE_BUDGET_MS = 2_000

async function setSpeed(page: Page, speed: '1x' | '2x' | '4x'): Promise<void> {
  await page.getByTestId(`clock-speed-${speed}`).click()
}

/** Select a course and start it. */
async function startCourse(page: Page, id: string): Promise<void> {
  await page.getByTestId(`practice-challenge-${id}`).click()
  await expect(page.getByTestId(`practice-challenge-${id}`)).toHaveAttribute(
    'data-selected',
    'true',
  )
  await page.getByTestId(`practice-start-${id}`).click()
  await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')
  await expect(page.getByTestId('practice')).toHaveAttribute('data-task', id)
  await expect(page.getByTestId('practice')).toHaveAttribute('data-kind', 'course')
}

/** Wait for the baseline to arrive (or to be refused). */
async function waitForBaseline(page: Page): Promise<void> {
  await expect(page.getByTestId('practice')).toHaveAttribute(
    'data-baseline-computing',
    'false',
    { timeout: 30_000 },
  )
  await expect(page.getByTestId('practice-baseline')).toBeVisible()
}

/** Every `data-*` on one element, as strings. */
async function dataset(page: Page, testId: string): Promise<Record<string, string>> {
  return page.evaluate((id) => {
    const el = document.querySelector(`[data-testid="${id}"]`)
    if (!(el instanceof HTMLElement)) {
      throw new Error(`no ${id} on the page`)
    }
    return { ...el.dataset } as Record<string, string>
  }, testId)
}

// ---------------------------------------------------------------------------
// 1. Choosing a course, and what it draws
// ---------------------------------------------------------------------------

test.describe('v2 §12 — the course overlay', () => {
  test.setTimeout(COURSE_TIMEOUT_MS)

  test('the chooser offers the three courses in their own group', async ({ page }) => {
    await gotoApp(page)
    await expect(page.getByTestId('practice-group-courses')).toBeVisible()
    await expect(page.getByTestId('practice-group-skills')).toBeVisible()
    for (const id of COURSES) {
      const card = page.getByTestId(`practice-challenge-${id}`)
      await expect(card).toBeVisible()
      await expect(card).toHaveAttribute('data-kind', 'course')
      // Every shipped course is sailed in a shipped scenario (brief §32).
      await expect(card).toHaveAttribute('data-scenario', 'free_sail')
      // The waypoint count is the course document's, across the boundary.
      const n = Number(await card.getAttribute('data-waypoints'))
      expect(n, `${id} has waypoints`).toBeGreaterThanOrEqual(2)
      await card.click()
      // One short description and one measurable goal, with numbers in it.
      await expect(page.getByTestId(`practice-about-${id}`)).toBeVisible()
      const goal = await page.getByTestId(`practice-goal-${id}`).textContent()
      expect(goal ?? '', `${id} states a measurable goal`).toMatch(/\d/)
    }
  })

  test('choosing `reach` draws three numbered waypoints with 1 next', async ({ page }) => {
    await gotoApp(page)
    await startCourse(page, 'course_reach')

    // The overlay is on the page, with one labelled waypoint per waypoint.
    await expect(page.getByTestId('course-layer')).toBeVisible()
    for (const n of [1, 2, 3]) {
      await expect(page.getByTestId(`waypoint-${n}`)).toHaveCount(1)
    }
    // Exactly three waypoint groups: each one also emits its mark, its gate
    // bar and its label, so the group is the element that carries `data-state`.
    await expect(page.locator('[data-testid^="waypoint-"][data-state]')).toHaveCount(3)
    // Rust's states, read: the first is next and the rest are pending.
    await expect(page.getByTestId('waypoint-1')).toHaveAttribute('data-state', 'next')
    await expect(page.getByTestId('waypoint-2')).toHaveAttribute('data-state', 'pending')
    await expect(page.getByTestId('waypoint-3')).toHaveAttribute('data-state', 'pending')

    // The panel says which waypoint and how far, from the same block.
    const progress = await dataset(page, 'practice-course-progress')
    expect(progress.next).toBe('1')
    expect(Number(progress.distance)).toBeGreaterThan(0)

    // Either the waypoint's own label is in view, or the edge chevron points
    // at it. One of the two must be true, which is what the chevron is for.
    const label = await page.getByTestId('waypoint-1-label').count()
    const chevron = await page.getByTestId('course-chevron').count()
    expect(label + chevron, 'waypoint 1 is drawn or pointed at').toBeGreaterThan(0)
  })

  test('Show course fits the whole course, and the camera stays usable', async ({ page }) => {
    await gotoApp(page)
    await startCourse(page, 'course_triangle')
    await page.getByTestId('world-view').scrollIntoViewIfNeeded()
    await page.getByTestId('practice-show-course').click()
    // A fit is a `northUp` operation (`fitBounds` is axis-aligned).
    await expect(page.getByTestId('camera-mode')).toHaveAttribute('data-mode', 'northUp')
    // Every waypoint's label is now on screen, which is what "show the course"
    // means.
    for (const n of [1, 2, 3]) {
      await expect(page.getByTestId(`waypoint-${n}-label`)).toBeVisible()
    }
  })
})

// ---------------------------------------------------------------------------
// 2. The baseline, the ghost and the replay
// ---------------------------------------------------------------------------

test.describe('v2 §12 — the baseline', () => {
  test.setTimeout(COURSE_TIMEOUT_MS)

  test('the baseline sails the same course under the same conditions', async ({ page }) => {
    await gotoApp(page)
    await startCourse(page, 'course_reach')
    await waitForBaseline(page)

    const baseline = await dataset(page, 'practice-baseline')
    // D4/RV70: only this verdict licenses the ghost and the splits, and it is
    // `ExperimentIdentity::compare_conditions`'s, in Rust.
    expect(baseline.verdict, `reasons: ${baseline.reasons}`).toBe('same_conditions')
    expect(baseline.state).toBe('ready')
    expect(baseline.outcome).toBe('finished')
    expect(Number(baseline.time)).toBeGreaterThan(10)
    expect(Number(baseline.splits)).toBe(3)
    expect(Number(baseline.narration)).toBeGreaterThan(0)
    // One split line per waypoint, numbered.
    await expect(page.getByTestId('practice-baseline-splits').locator('li')).toHaveCount(3)
  })

  test('the ghost is visible and moves as the clock runs', async ({ page }) => {
    await gotoApp(page)
    await startCourse(page, 'course_reach')
    await waitForBaseline(page)
    await setSpeed(page, '4x')

    const ghost = page.getByTestId('ghost-boat')
    await expect(ghost).toBeVisible()
    // The pose is in the hull's own transform — `Camera.boatTransform`, the
    // same one the player's boat is placed by — so that is what moving means.
    const hull = page.getByTestId('ghost-hull')
    const first = await hull.getAttribute('transform')
    expect(first, 'the ghost is placed by a transform').not.toBeNull()
    // Let the clock run; the ghost is a **recording**, replayed at the live
    // run's own time, so it moves because `t` moves.
    await expect
      .poll(async () => (await readSnapshot(page)).t, { timeout: 60_000, intervals: [100] })
      .toBeGreaterThan(6)
    expect(await hull.getAttribute('transform'), 'the ghost moved').not.toBe(first)
    // …and it is a ghost: a track behind it, no rig, no forces.
    await expect(page.getByTestId('ghost-track')).toHaveCount(1)
    const points = await page.getByTestId('ghost-track').getAttribute('points')
    expect((points ?? '').split(' ').length, 'the ghost leaves a track').toBeGreaterThan(5)
  })

  test('Watch baseline opens its episode, and Back to live returns', async ({ page }) => {
    await gotoApp(page)
    await startCourse(page, 'course_reach')
    await waitForBaseline(page)

    await page.getByTestId('practice-watch-baseline').click()
    await expect(page.getByTestId('inspection')).toHaveAttribute('data-source', 'replay')
    // Captioned with what the controller was doing (D5's narration).
    await expect(page.getByTestId('replay-baseline-caption')).toBeVisible()

    // The timeline carries one marker per recorded passage, labelled with the
    // waypoint number.
    const markers = page.locator('[data-testid="timeline-event-waypoint_passed"]')
    await expect(markers).toHaveCount(3)
    // Jump to the last one and read the overlay: everything up to here is
    // passed, which is the episode's own events and not a recomputation.
    await markers.nth(2).click()
    await expect(page.getByTestId('waypoint-1')).toHaveAttribute('data-state', 'passed')
    await expect(page.getByTestId('waypoint-3')).toHaveAttribute('data-state', 'passed')

    // Jump back to the first: waypoint 3 has not happened yet.
    await markers.nth(0).click()
    await expect(page.getByTestId('waypoint-1')).toHaveAttribute('data-state', 'passed')
    await expect(page.getByTestId('waypoint-3')).toHaveAttribute('data-state', 'pending')

    // The ghost is not drawn over a replay: the replay **is** the baseline.
    await expect(page.getByTestId('ghost-boat')).toHaveCount(0)

    await page.getByTestId('timeline-exit').click()
    await expect(page.getByTestId('inspection')).toHaveAttribute('data-source', 'live')
    await expect(page.getByTestId('ghost-boat')).toBeVisible()
  })

  test('Retry restores the same conditions and recomputes the baseline', async ({ page }) => {
    await gotoApp(page)
    await setSpeed(page, '4x')
    await startCourse(page, 'course_reach')
    await waitForBaseline(page)
    const before = await dataset(page, 'practice-baseline')
    const firstStart = await readSnapshot(page)

    // Sail nothing: the attempt runs out its 130 s limit and the evaluator
    // reports `timed_out`, which is the quickest honest way to a result.
    // **Give up is not that path** — it cancels, and a cancelled attempt has
    // no result by design (section 11).
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'result', {
      timeout: COURSE_TIMEOUT_MS,
    })
    await page.getByTestId('practice-retry').click()
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'sailing')
    await waitForBaseline(page)

    const after = await dataset(page, 'practice-baseline')
    // **The same conditions, so the same baseline, to the last digit.** This is
    // the assertion that matters: `run_baseline` builds its episode from the
    // attempt's frozen contract, the controller draws nothing from its RNG, and
    // a retry freezes the same contract again — so an identical time is the
    // whole chain agreeing (D5, RV63, RV70).
    expect(after.time).toBe(before.time)
    expect(after.splits).toBe(before.splits)
    expect(after.verdict).toBe('same_conditions')
    // …and the course is back at waypoint 1, with the clock restarted. That
    // the *canonical identity* of the retry matches is section 11's own
    // assertion (`practice.spec.ts`), measured on the recorded episode rather
    // than on a snapshot taken whenever a frame happened to land.
    await expect(page.getByTestId('waypoint-1')).toHaveAttribute('data-state', 'next')
    const progress = await dataset(page, 'practice-course-progress')
    expect(progress.next).toBe('1')
    expect(Number(progress.passed)).toBe(0)
    const second = await readSnapshot(page)
    expect(second.t, 'the clock restarted').toBeLessThan(firstStart.t + 2)
  })

  test('it measures the baseline wall time for the longest shipped course (RV71)', async ({
    page,
  }) => {
    await gotoApp(page)
    // `triangle` is the longest: 105.5 s of simulated time against `reach`'s
    // 42.5 s (`docs/v2/baseline-validation.md` §2).
    await startCourse(page, 'course_triangle')
    const started = Date.now()
    await waitForBaseline(page)
    const elapsed = Date.now() - started
    // Reported, then asserted: the number goes in the handoff.
    // eslint-disable-next-line no-console
    console.log(`[course] run_baseline(course_triangle) took ${elapsed} ms`)
    const baseline = await dataset(page, 'practice-baseline')
    expect(baseline.verdict).toBe('same_conditions')
    expect(elapsed, 'RV71: the page must not freeze computing the baseline').toBeLessThan(
      BASELINE_BUDGET_MS,
    )
  })
})

// ---------------------------------------------------------------------------
// 3. The layout, and what section 11 still does
// ---------------------------------------------------------------------------

test.describe('v2 §12 — the layout and the old flows', () => {
  test.setTimeout(COURSE_TIMEOUT_MS)

  test('the whole flow works at 360 × 640', async ({ page }) => {
    await page.setViewportSize({ width: 360, height: 640 })
    await gotoApp(page)
    await startCourse(page, 'course_reach')
    await waitForBaseline(page)

    // The overlay, the panel and the baseline all fit: nothing is wider than
    // the document, which is section 09's own rule for this width.
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    )
    expect(overflow, 'no horizontal overflow at 360 px').toBeLessThanOrEqual(1)
    await expect(page.getByTestId('course-layer')).toBeVisible()
    await expect(page.getByTestId('practice-baseline')).toBeVisible()
    await expect(page.getByTestId('practice-show-course')).toBeVisible()

    // And by keyboard: every control is a native button, so Tab reaches them.
    await page.getByTestId('practice-show-course').focus()
    await expect(page.getByTestId('practice-show-course')).toBeFocused()
    await page.keyboard.press('Enter')
    await expect(page.getByTestId('camera-mode')).toHaveAttribute('data-mode', 'northUp')
    await page.getByTestId('practice-watch-baseline').focus()
    await page.keyboard.press('Enter')
    await expect(page.getByTestId('inspection')).toHaveAttribute('data-source', 'replay')
  })

  test('the three skills and free sail are still reachable and unchanged', async ({ page }) => {
    await gotoApp(page)
    // A skill: no course block, no overlay and no baseline.
    await page.getByTestId('practice-challenge-get_moving').click()
    await page.getByTestId('practice-start-get_moving').click()
    await expect(page.getByTestId('practice')).toHaveAttribute('data-kind', 'skill')
    await expect(page.getByTestId('course-layer')).toHaveCount(0)
    await expect(page.getByTestId('practice-baseline')).toHaveCount(0)
    await expect(page.getByTestId('practice-progress')).toBeVisible()

    // Free sail: back to the chooser, with the recording controls available.
    await page.getByTestId('practice-free-sail').click()
    await expect(page.getByTestId('practice')).toHaveAttribute('data-phase', 'choose')
    await expect(page.getByTestId('course-layer')).toHaveCount(0)
    await expect(page.getByTestId('record-controls')).toBeVisible()
  })
})
