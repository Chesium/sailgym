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
  workers: process.env.CI ? 2 : 4,
  reporter: process.env.CI ? 'line' : [['list']],
  timeout: 30_000,
  expect: { timeout: 5_000 },

  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry',
  },

  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'msedge', use: { ...devices['Desktop Edge'], channel: 'msedge' } },
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
