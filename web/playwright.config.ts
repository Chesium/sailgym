import { defineConfig, devices } from '@playwright/test'

/**
 * Browser targets are brief section 38: current Chrome, Edge and Firefox,
 * desktop, mouse + keyboard. Safari is explicitly not a v1 acceptance
 * criterion, so there is no webkit project.
 *
 * `webServer` runs the Vite dev server the specs talk to. It does **not**
 * build the WASM package — that is gate step 6 (`scripts/build-wasm.*`),
 * which the gate chain always runs before step 8.
 */

/**
 * Headless Chromium defaults to SwiftShader, a software rasteriser. Section 03
 * put a deck.gl canvas on the page, and rendering four thousand particles in
 * software costs about 85 ms a frame — twelve frames a second, on every page,
 * in every worker. Asking for the real GPU brings it back to the vsync limit
 * and keeps the suite from starving the host (see the worker note below).
 *
 * On a machine or a CI container with no usable GPU these flags change
 * nothing: Chromium falls back to SwiftShader exactly as before. Firefox has
 * no equivalent switch and stays on its software path.
 */
const GPU = { args: ['--enable-gpu', '--use-angle=default', '--ignore-gpu-blocklist'] }

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  // Retries hide flakes. Zero locally, per task 1.4.
  retries: process.env.CI ? 1 : 0,
  // Section 02 turned every page into a live simulator: a WASM instance and an
  // animation-frame loop each, and the 2x/4x speeds double or quadruple the
  // physics work per page. Playwright's default worker count oversubscribed
  // the host badly enough to produce two distinct failures — Firefox pages
  // missing `data-ready` inside gotoApp's 5 s window, and multi-hundred-
  // millisecond frame stalls that trip `MAX_STEPS_PER_TICK` and deflate the
  // measured 2x/1x ratio. Both are a starved host, not a slow app, so the cap
  // is the honest fix; neither the readiness timeout nor the ratio bounds were
  // touched.
  //
  // Section 03 added a WebGL canvas to every page, and had to lower the cap
  // again. Chromium and Edge get the real GPU (see `GPU` above) and cost
  // little, but Firefox has no headless GPU path here and renders the particle
  // field in software at roughly 80 ms a frame. Four such workers starved the
  // host enough to fail the 2x/1x ratio and to leave a page having advanced no
  // simulated time at all in half a second. Two workers, again without
  // touching a single assertion.
  workers: 2,
  reporter: process.env.CI ? 'line' : [['list']],
  timeout: 30_000,
  expect: { timeout: 5_000 },

  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
  },

  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'], launchOptions: GPU } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    {
      name: 'msedge',
      use: { ...devices['Desktop Edge'], channel: 'msedge', launchOptions: GPU },
    },
  ],

  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    stdout: 'ignore',
    stderr: 'pipe',
  },
})
