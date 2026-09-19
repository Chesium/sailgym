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
