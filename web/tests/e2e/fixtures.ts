import { test as base, expect, type Page } from '@playwright/test'

/**
 * Shared E2E harness. **Every spec in this repo must use it** (task 1.4).
 *
 * brief section 42 requires the suite to prove there are no obvious JS/WASM
 * errors. That is only worth anything if it is enforced automatically, so the
 * trap lives here rather than being re-implemented (and forgotten) per spec:
 *
 *  - `gotoApp` fails as soon as the app finishes loading if anything went to
 *    `console.error` or raised a page error on the way;
 *  - the `test` exported below additionally asserts an empty log at teardown,
 *    catching errors raised *after* load, during interaction.
 */

/** Errors collected per page, so the listeners are attached exactly once. */
const errorLog = new WeakMap<Page, string[]>()

function watch(page: Page): string[] {
  const existing = errorLog.get(page)
  if (existing !== undefined) {
    return existing
  }
  const log: string[] = []
  errorLog.set(page, log)
  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      log.push(`console.error: ${msg.text()}`)
    }
  })
  page.on('pageerror', (error) => {
    log.push(`pageerror: ${error.message}`)
  })
  return log
}

/**
 * `test` with an automatic console-error / page-error trap.
 *
 * Import this instead of `@playwright/test`'s `test`.
 */
export const test = base.extend<{ noBrowserErrors: void }>({
  noBrowserErrors: [
    async ({ page }, use) => {
      const log = watch(page)
      await use()
      expect(log, 'the page must raise no console errors and no page errors').toEqual([])
    },
    { auto: true },
  ],
})

export { expect }

/**
 * Navigates, waits for WASM ready, and fails the test on any console error
 * or pageerror. Every spec in this repo must use it.
 *
 * `opts.scenario` is accepted now so the signature is stable for section 09;
 * section 01's app has no scenarios and ignores it.
 */
export async function gotoApp(page: Page, opts?: { scenario?: string }): Promise<void> {
  const log = watch(page)

  const query = opts?.scenario === undefined ? '' : `?scenario=${encodeURIComponent(opts.scenario)}`
  await page.goto(`/${query}`)

  // The `data-testid`/`data-ready` pair is the contract from task 1.3.
  await expect(page.getByTestId('wasm-status')).toHaveAttribute('data-ready', 'true', {
    timeout: 5_000,
  })

  // Give the page keyboard focus before any spec starts typing.
  //
  // The key listeners are on `window` (`sim/useSimulation.ts`), so
  // `page.keyboard` only reaches them while the document holds focus, and
  // nothing established it. The first key press of a spec was therefore
  // swallowed about once in three full-suite runs — section 05's **defect B**,
  // diagnosed there, re-confirmed in section 06, and observed again in section
  // 07 as `determinism.spec.ts` finding `clock-pause` still running after
  // pressing `p`. The click is on the outer padding of the page body: there is
  // no control there, and the sheet reducer only sees pointer events on the
  // boat SVG.
  await page.locator('body').click({ position: { x: 2, y: 2 } })

  expect(log, 'the app must load with no console errors and no page errors').toEqual([])
}

/**
 * The F8.3 snapshot, read from the `data-*` attributes of
 * `[data-testid="snapshot"]`.
 *
 * brief §42 asks browser tests to check numeric state rather than pixels where
 * practical. JavaScript renders a `number` to its shortest round-tripping
 * decimal, so `Number(attribute)` recovers the exact double the simulation
 * produced — which is what makes the determinism spec a 0-ULP comparison.
 *
 * Keys are the camel-cased attribute names: `t`, `x`, `y`, `psi`, `phi`, `u`,
 * `v`, `r`, `p`, `beta`, `betaDot`, `deltaR`, `lSheet`, plus `dt`.
 */
export async function readSnapshot(page: Page): Promise<Record<string, number>> {
  return page.evaluate(() => {
    const el = document.querySelector('[data-testid="snapshot"]')
    if (!(el instanceof HTMLElement)) {
      throw new Error('no [data-testid="snapshot"] element on the page')
    }
    const out: Record<string, number> = {}
    for (const [key, value] of Object.entries(el.dataset)) {
      out[key] = Number(value)
    }
    return out
  })
}
