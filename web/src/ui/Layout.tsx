import type { ReactNode } from 'react'

import type { UiMode } from './store'

/**
 * The two arrangements of brief §29 (task 8.2).
 *
 * Sail Mode renders the controls, the seven §29 readouts and the world. Debug
 * Mode renders all of that plus the debug column: the overlay toggles, the
 * full diagnostic set, the charts and the parameter panel.
 *
 * The mode decides **what is mounted**, not what is hidden with CSS. A hidden
 * `[data-testid^="diag-"]` subtree would still be in the DOM, would still be
 * re-rendered every frame, and would make "Sail Mode shows exactly the §29 set
 * and nothing more" a statement about styling rather than about the page.
 *
 * Switching modes touches nothing in the core. The simulation, the clock and
 * the one animation frame loop live in `useSimulation`, above this component,
 * so a mode change cannot pause the clock or change the rate `t` advances at —
 * which `modes.spec.ts` asserts.
 */
export interface LayoutProps {
  mode: UiMode
  /** Always shown: the status line, the clock, the camera and mode controls. */
  header: ReactNode
  /** Always shown: the brief §29 readouts. */
  readouts: ReactNode
  /** Always shown: the world view and everything drawn over it. */
  world: ReactNode
  /** Always shown: the heel indicator (brief §26) and the raw snapshot. */
  instruments: ReactNode
  /** Debug Mode only. */
  debug: ReactNode
  /** Always shown, and last: help text and off-screen test probes. */
  footer: ReactNode
}

export function Layout({ mode, header, readouts, world, instruments, debug, footer }: LayoutProps) {
  return (
    <div
      data-testid="app-layout"
      data-mode={mode}
      style={{ font: '13px system-ui, sans-serif', padding: 12, display: 'grid', gap: 8 }}
    >
      <div
        data-testid="layout-header"
        style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}
      >
        {header}
      </div>

      <div
        data-testid="layout-readouts"
        style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}
      >
        {readouts}
      </div>

      <div
        style={{
          display: 'flex',
          gap: 12,
          alignItems: 'flex-start',
          flexWrap: 'wrap',
        }}
      >
        <div data-testid="layout-world">{world}</div>
        {mode === 'debug' && (
          <div
            data-testid="layout-debug"
            style={{
              display: 'grid',
              gap: 8,
              alignContent: 'start',
              maxHeight: 520,
              overflowY: 'auto',
              minWidth: 330,
              flex: '1 1 330px',
            }}
          >
            {debug}
          </div>
        )}
      </div>

      <div
        data-testid="layout-instruments"
        style={{ display: 'flex', gap: 12, alignItems: 'flex-start', flexWrap: 'wrap' }}
      >
        {instruments}
      </div>

      {footer}
    </div>
  )
}
