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
