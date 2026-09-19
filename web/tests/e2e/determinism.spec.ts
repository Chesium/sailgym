import type { Page } from '@playwright/test'

import { expect, gotoApp, readSnapshot, test } from './fixtures'

/**
 * Browser-side determinism (F9, brief §34), the counterpart of
 * `crates/sailgym-physics/tests/determinism.rs`.
 *
 * The scripted sequence is replayed in **step space**, not wall-clock space:
 * the clock is paused and advanced one `dt` at a time with `.`, so the number
 * of physics steps between key events is fixed by the script rather than by
 * how fast the machine happened to be. Everything still goes through
 * `page.keyboard` — the same path a player uses.
 */

/** Steering key held during each phase, and how many single steps to take. */
const SCRIPT: ReadonlyArray<{ key: string | null; steps: number }> = [
  { key: 'd', steps: 15 },
  { key: null, steps: 15 },
  { key: 'a', steps: 15 },
]

/** Sample the snapshot every this many steps. */
const SAMPLE_EVERY = 5

async function replay(page: Page): Promise<Record<string, number>[]> {
  // Reset to the deterministic initial state, then pause.
  await page.keyboard.press('r')
  await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')

  const samples: Record<string, number>[] = []
  for (const phase of SCRIPT) {
    if (phase.key !== null) {
      await page.keyboard.down(phase.key)
      // Let one frame carry the new held-key set into `set_controls`, so the
      // controls are already settled before the first step of this phase.
      await page.waitForTimeout(120)
    }
    for (let i = 1; i <= phase.steps; i += 1) {
      await page.keyboard.press('.')
      if (i % SAMPLE_EVERY === 0) {
        samples.push(await readSnapshot(page))
      }
    }
    if (phase.key !== null) {
      await page.keyboard.up(phase.key)
      await page.waitForTimeout(120)
    }
  }
  return samples
}

test.describe('determinism', () => {
  test('the same scripted key sequence reproduces the same trajectory', async ({ page }) => {
    // An initial speed, so the replay has a trajectory to reproduce: there is
    // no sail until section 05 and the M1 placeholder force model was deleted
    // in section 04.
    await gotoApp(page, { scenario: 'coast' })
    // Pause first; `replay` resets and then expects a paused clock.
    await page.keyboard.press('p')

    const first = await replay(page)
    const second = await replay(page)

    expect(first).toHaveLength(SCRIPT.reduce((n, p) => n + Math.floor(p.steps / SAMPLE_EVERY), 0))
    expect(second).toHaveLength(first.length)

    for (let i = 0; i < first.length; i += 1) {
      const a = first[i]
      const b = second[i]
      for (const key of Object.keys(a)) {
        // 0 ULP: these are the exact doubles the simulation produced.
        expect(
          Object.is(a[key], b[key]),
          `sample ${i}, field ${key}: ${a[key]} vs ${b[key]}`,
        ).toBe(true)
      }
    }

    // The replay must actually have gone somewhere.
    const last = first[first.length - 1]
    expect(last.t).toBeGreaterThan(0)
    expect(Math.abs(last.x) + Math.abs(last.psi)).toBeGreaterThan(0)
  })
})

/**
 * The same thing again, through the recorder (section 09, task 9.7).
 *
 * `determinism.rs`'s `identical_seed_identical_trajectory` and the invariant
 * `deterministic_replay` prove this in Rust. This is the browser half: the
 * whole chain — key events, the clock, `set_controls`, `advance`, the
 * recorder — has to be deterministic too, because that chain is what an
 * episode file is a record of (brief §34, §33).
 */

/** Steering key held during each phase, and how many single steps to take. */
const EPISODE_SCRIPT: ReadonlyArray<{ key: string | null; steps: number }> = [
  { key: 'd', steps: 20 },
  { key: null, steps: 20 },
  { key: 'a', steps: 20 },
]

/**
 * Reset, record the scripted sequence at 50 Hz, stop, and return the episode
 * document.
 *
 * Every step is a single `.` press, so the number of physics steps between
 * key events is fixed by the script and not by how fast the machine is.
 */
async function recordScriptedEpisode(page: Page): Promise<string> {
  await page.keyboard.press('r')
  await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')

  await page.getByTestId('record-hz').selectOption('50')
  await page.getByTestId('record-toggle').click()
  await expect(page.getByTestId('record-controls')).toHaveAttribute('data-recording', 'true')

  for (const phase of EPISODE_SCRIPT) {
    if (phase.key !== null) {
      await page.keyboard.down(phase.key)
      // Let one frame carry the new held-key set into `set_controls`, so the
      // controls are settled before the first step of this phase.
      await page.waitForTimeout(120)
    }
    for (let i = 0; i < phase.steps; i += 1) {
      await page.keyboard.press('.')
    }
    if (phase.key !== null) {
      await page.keyboard.up(phase.key)
      await page.waitForTimeout(120)
    }
  }

  await page.getByTestId('record-toggle').click()
  await expect(page.getByTestId('record-controls')).toHaveAttribute('data-recording', 'false')
  const json = await page.evaluate(() => window.__sailgym?.episodeJson() ?? null)
  expect(json).not.toBeNull()
  return json as string
}

test.describe('determinism', () => {
  test('two recordings of the same scripted episode are frame-identical', async ({ page }) => {
    test.setTimeout(120_000)
    await gotoApp(page, { scenario: 'beam_reach_capsize' })
    await page.keyboard.press('p')
    await expect(page.getByTestId('clock-pause')).toHaveAttribute('data-running', 'false')

    const first = await recordScriptedEpisode(page)
    const second = await recordScriptedEpisode(page)

    const a = JSON.parse(first) as {
      header: { created_utc: string; scenario: { name: string } }
      frames: Array<{ t: number; state: number[]; controls: number[] }>
    }
    const b = JSON.parse(second) as typeof a

    expect(a.header.scenario.name).toBe('beam_reach_capsize')
    // 60 steps at dt = 0.005 is 0.3 s; at 50 Hz that is 15 samples plus the
    // one taken when recording started.
    expect(a.frames.length).toBeGreaterThanOrEqual(10)
    expect(b.frames.length).toBe(a.frames.length)

    for (let i = 0; i < a.frames.length; i += 1) {
      // 0 ULP: `Object.is` on the exact doubles the core produced, which
      // survive the JSON round trip bit for bit.
      expect(Object.is(a.frames[i].t, b.frames[i].t), `frame ${i}: t`).toBe(true)
      for (let k = 0; k < a.frames[i].state.length; k += 1) {
        expect(
          Object.is(a.frames[i].state[k], b.frames[i].state[k]),
          `frame ${i}, state[${k}]: ${a.frames[i].state[k]} vs ${b.frames[i].state[k]}`,
        ).toBe(true)
      }
      expect(a.frames[i].controls).toEqual(b.frames[i].controls)
    }

    // The two documents differ in nothing but the wall-clock stamp, which is
    // metadata and never reaches the physics (F9.1).
    const strip = (doc: string) =>
      doc.replace(/"created_utc":"[^"]*"/, '"created_utc":""')
    expect(strip(second)).toBe(strip(first))
    expect(a.header.created_utc).not.toBe('')

    // …and the episode has to have been an episode.
    const last = a.frames[a.frames.length - 1]
    expect(last.t).toBeGreaterThan(0)
    expect(Math.abs(last.state[2])).toBeGreaterThan(0)
  })
})
