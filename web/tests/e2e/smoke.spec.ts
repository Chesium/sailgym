import { expect, gotoApp, test } from './fixtures'

/** Dev-server URL of the loader module, imported inside the page. */
const LOAD_WASM_MODULE = '/src/sim/loadWasm.ts'

interface IdempotencyProbe {
  sequentialSame: boolean
  concurrentSame: boolean
  crossSame: boolean
  simConstructorSame: boolean
}

test.describe('smoke', () => {
  test('the app loads and the WASM module reports itself ready', async ({ page }) => {
    // gotoApp covers assertions 1-3: the app loads, `wasm-status` reaches
    // data-ready="true", and there were no console errors or page errors.
    await gotoApp(page)

    const status = page.getByTestId('wasm-status')
    await expect(status).toBeVisible()
    // The version string comes from the Rust crate via `Sim::version()`.
    await expect(status).toHaveText(/^sailgym \d+\.\d+\.\d+$/)
  })

  test('loadWasm is idempotent', async ({ page }) => {
    await gotoApp(page)

    const probe = await page.evaluate(async (moduleUrl): Promise<IdempotencyProbe> => {
      const mod = (await import(/* @vite-ignore */ moduleUrl)) as {
        loadWasm: () => Promise<{ Sim: unknown }>
      }
      const first = await mod.loadWasm()
      const second = await mod.loadWasm()
      const [concurrentA, concurrentB] = await Promise.all([mod.loadWasm(), mod.loadWasm()])
      return {
        sequentialSame: first === second,
        concurrentSame: concurrentA === concurrentB,
        crossSame: first === concurrentA,
        simConstructorSame: first.Sim === second.Sim,
      }
    }, LOAD_WASM_MODULE)

    expect(probe).toEqual({
      sequentialSame: true,
      concurrentSame: true,
      crossSame: true,
      simConstructorSame: true,
    })
  })
})
