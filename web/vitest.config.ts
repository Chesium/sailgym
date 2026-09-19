import { defineConfig } from 'vitest/config'

/**
 * Unit tests for the pure TypeScript modules — the clock, the input mapping,
 * the snapshot accessor and the camera. None of them touches the DOM or the
 * WASM module; browser-level behaviour is Playwright's job (`test:e2e`).
 *
 * Kept separate from `vite.config.ts` so the app build never loads the Vite
 * plugins' test-only configuration, and so `playwright.config.ts`'s own
 * `testDir` (tests/e2e) stays disjoint from this one.
 */
export default defineConfig({
  test: {
    include: ['tests/unit/**/*.test.ts'],
    environment: 'node',
  },
})
