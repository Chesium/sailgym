import react from '@vitejs/plugin-react'
import topLevelAwait from 'vite-plugin-top-level-await'
import wasm from 'vite-plugin-wasm'
import { defineConfig } from 'vite'

// Stack pinned by F12: Vite + React + TypeScript, `vite-plugin-wasm` and
// `vite-plugin-top-level-await`.
export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
  server: {
    // Fixed so `playwright.config.ts` (task 1.4) can point `webServer` at a
    // known URL instead of racing Vite's port fallback.
    port: 5173,
    strictPort: true,
  },
  build: {
    // WebAssembly instantiation and top-level await both need a modern target.
    target: 'esnext',
  },
})
