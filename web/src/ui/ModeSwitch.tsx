import { useEffect } from 'react'

import { useUiStore, type UiMode } from './store'

/**
 * The brief §29 mode control: a visible button and a key (task 8.2).
 *
 * The key is handled here rather than through `sim/keymap.ts`, which maps keys
 * to *simulation* actions that `useSimulation` pushes across the WASM
 * boundary. Switching modes changes nothing in the core — it must not pause
 * the clock, reset anything or touch `t` — so it has no business in that
 * table. `m` is unused by section 02's mapping (`a`, `d`, the arrows, space,
 * `r`, `p`, `.`).
 */
export const MODE_KEY = 'm'

const LABEL: Record<UiMode, string> = { sail: 'Sail', debug: 'Debug' }

export function ModeSwitch() {
  const mode = useUiStore((s) => s.mode)
  const toggleMode = useUiStore((s) => s.toggleMode)

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() !== MODE_KEY || e.repeat || e.metaKey || e.ctrlKey || e.altKey) {
        return
      }
      // Typing an `m` into the parameter panel is not a mode switch.
      const target = e.target as HTMLElement | null
      if (target !== null && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) {
        return
      }
      e.preventDefault()
      toggleMode()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [toggleMode])

  return (
    <button
      type="button"
      data-testid="mode-switch"
      data-mode={mode}
      onClick={toggleMode}
      title={`Press ${MODE_KEY.toUpperCase()} to switch`}
    >
      Mode: {LABEL[mode]}
    </button>
  )
}
