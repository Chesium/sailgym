import type { ReactNode } from 'react'

import type { UiMode } from './store'

/**
 * The two arrangements of brief §29 (task 8.2), made responsive (v2 section
 * 09, task 9.4).
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
 *
 * ## The shape, and why it is a fixed-height grid
 *
 * ```
 * ┌──────────────────────────────┐  auto      header    (scrolls if it wraps)
 * ├──────────────────────────────┤  auto      readouts  (scrolls if it wraps)
 * │                              │  1fr       main      world · debug · instruments
 * ├──────────────────────────────┤  auto      controls  helm, sheet, release
 * └──────────────────────────────┘
 * ```
 *
 * The page itself never scrolls: the root is exactly one viewport tall and the
 * **main** row is the scroll container. Three things follow, and all three are
 * acceptance criteria of task 9.4:
 *
 * - the controls and the primary feedback are inside the viewport at every
 *   size, because they are rows of a grid that is the viewport (RV55);
 * - `main` has a definite height that does not depend on its contents, so the
 *   world view can be sized from a `ResizeObserver` on it without the
 *   measurement feeding back into the layout;
 * - there is no horizontal document overflow, because the root clips and every
 *   flex child carries `min-width: 0`.
 *
 * `100svh` is the *small* viewport height — the one that excludes a mobile
 * browser's retractable chrome — so the controls do not slide under the
 * address bar when it comes back. `100vh` is the fallback for a browser that
 * has never heard of it.
 *
 * ## Safe areas
 *
 * The padding is `max(base, env(safe-area-inset-*))` on all four sides, so a
 * notch, a rounded corner or a home indicator eats margin rather than a
 * control. `env()` is zero unless `index.html`'s viewport meta carries
 * `viewport-fit=cover`, which it now does.
 */
export interface LayoutProps {
  mode: UiMode
  /**
   * Whether `mode` was chosen by a person or is still the default.
   *
   * Published as `data-mode-chosen` and used for nothing else: the layout
   * arranges the mode it is given and never picks one. See
   * {@link UiState.modeChosen}.
   */
  modeChosen: boolean
  /**
   * Lay out for a narrow viewport: the header and the readouts become
   * scrollable strips instead of being allowed to grow without limit.
   *
   * Measured by `App.tsx` and passed in, rather than decided here by a media
   * query, because the same measurement sizes the world view and the two must
   * not disagree about which arrangement is in force.
   */
  compact: boolean
  /**
   * The world view currently owns a pointer drag.
   *
   * While it does, the **main** row stops being a scroll container. A pointer
   * captured by the world view and dragged past the bottom of that row makes
   * the browser scroll it — measured in Firefox: a 250 px mainsheet haul from
   * the middle of a 326 px-tall view scrolled the row by 224 px, which slid
   * the boat off the top and left the next press landing on the instruments
   * instead. The player experiences it as the boat sliding out from under
   * their hand mid-trim.
   *
   * `overflow: hidden` for the duration is the narrowest fix that exists: the
   * row keeps its scroll position, everything in it stays reachable the moment
   * the drag ends, and nothing about the layout changes. It is set from
   * `App.tsx`, which is where a drag is already known to be in progress.
   */
  dragging: boolean
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
  /** Always shown, pinned to the bottom row: the helm, sheet and release. */
  controls: ReactNode
  /**
   * The element whose box sizes the world view.
   *
   * `App.tsx` observes it. It is the `1fr` row, so its size comes from the
   * grid and never from what is inside it.
   */
  mainRef?: React.Ref<HTMLDivElement>
  /** The world cell, observed for its **width**. */
  worldRef?: React.Ref<HTMLDivElement>
}

/**
 * How tall the header and the readouts may grow before they start scrolling
 * instead, in `rem`.
 *
 * **Provenance.** Presentation only. At 360 CSS pixels the header's controls
 * wrap to four or five rows and the seven §29 readouts to five or six; left
 * alone they would take two thirds of a phone screen and leave the boat a
 * letterbox, which is RV55. `3.6rem` is about two rows of 13 px controls with
 * their padding, and `4.4rem` about three lines of readouts — enough that the
 * first and most-read line of each is always in view, with the rest a short
 * scroll away rather than gone.
 */
const COMPACT_MAX_REM = { header: 3.6, readouts: 4.4 }

/**
 * The world view's minimum height, in CSS pixels, as the landscape threshold
 * above reasons about it. It mirrors `App.tsx`'s `WORLD_PX.min.height`, and
 * `tests/e2e/mobile-controls.spec.ts` asserts the view never goes under it.
 */
const WORLD_MIN_HEIGHT_PX = 190

/**
 * The narrowest the landscape control column may be, in CSS pixels.
 *
 * **Provenance.** Two pads side by side at the 44 px minimum plus their
 * padding, borders and the gap between them, with room for the gauge rows'
 * labels beside their bars: 260 px is comfortably above that and still under
 * half of the 620 px width at which this arrangement starts.
 */
const CONTROL_COLUMN_MIN_PX = 260

const LAYOUT_STYLE = `
html, body { margin: 0; padding: 0; }
body { overscroll-behavior: none; }
.sg-layout {
  box-sizing: border-box;
  height: 100vh;
  height: 100svh;
  max-width: 100%;
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr) auto;
  gap: 8px;
  overflow: hidden;
  font: 13px system-ui, sans-serif;
  padding-top: max(8px, env(safe-area-inset-top));
  padding-right: max(8px, env(safe-area-inset-right));
  padding-bottom: max(8px, env(safe-area-inset-bottom));
  padding-left: max(8px, env(safe-area-inset-left));
}
.sg-layout * { box-sizing: border-box; }
.sg-row { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; min-width: 0; }
.sg-layout[data-compact='true'] .sg-header {
  max-height: ${COMPACT_MAX_REM.header}rem;
  overflow-y: auto;
  gap: 6px;
}
.sg-layout[data-compact='true'] .sg-readouts {
  max-height: ${COMPACT_MAX_REM.readouts}rem;
  overflow-y: auto;
  gap: 8px;
}
.sg-layout[data-dragging='true'] .sg-main { overflow: hidden; }
.sg-main {
  min-height: 0;
  min-width: 0;
  overflow: auto;
  overflow-x: hidden;
  overscroll-behavior: contain;
  display: flex;
  gap: 12px;
  align-items: flex-start;
  flex-wrap: wrap;
  align-content: flex-start;
}
.sg-world { flex: 1 1 260px; min-width: 0; }
.sg-debug {
  display: grid;
  gap: 8px;
  align-content: start;
  min-width: 0;
  flex: 1 1 330px;
}
.sg-instruments { flex: 1 1 100%; display: flex; gap: 12px; align-items: flex-start; flex-wrap: wrap; min-width: 0; }
.sg-footer { flex: 1 1 100%; min-width: 0; }
.sg-controls { min-width: 0; overflow-x: hidden; }

/*
 * A short, wide viewport — a phone in landscape — puts the controls **beside**
 * the world instead of under it.
 *
 * Stacked, the deck and the two capped strips leave a 390 px-tall screen
 * nothing at all for the boat: measured, the main row collapsed to 0 px and the
 * world view was clipped behind the pads, which is RV55 in its purest form.
 * Turned ninety degrees, the same deck costs width the landscape view has to
 * spare, and the boat keeps the whole height.
 *
 * 560 px of height is where the stacked arrangement stops leaving the world
 * its ${WORLD_MIN_HEIGHT_PX} px minimum; 620 px of width is where a
 * ${CONTROL_COLUMN_MIN_PX} px control column still leaves the world more than
 * half the screen. Both are presentation thresholds and neither reaches
 * anything physical.
 */
@media (max-height: 560px) and (min-width: 620px) {
  .sg-layout {
    grid-template-columns: minmax(0, 1fr) minmax(${CONTROL_COLUMN_MIN_PX}px, 340px);
    grid-template-rows: auto auto minmax(0, 1fr);
    column-gap: 12px;
  }
  .sg-header, .sg-readouts { grid-column: 1 / -1; }
  .sg-main { grid-column: 1; grid-row: 3; }
  .sg-controls { grid-column: 2; grid-row: 3; min-height: 0; overflow-y: auto; }
}
`

export function Layout({
  mode,
  modeChosen,
  compact,
  dragging,
  header,
  readouts,
  world,
  instruments,
  debug,
  footer,
  controls,
  mainRef,
  worldRef,
}: LayoutProps) {
  return (
    <div
      data-testid="app-layout"
      data-mode={mode}
      data-mode-chosen={modeChosen ? 'true' : 'false'}
      data-compact={compact ? 'true' : 'false'}
      data-dragging={dragging ? 'true' : 'false'}
      className="sg-layout"
    >
      <style>{LAYOUT_STYLE}</style>

      <div data-testid="layout-header" className="sg-row sg-header">
        {header}
      </div>

      <div data-testid="layout-readouts" className="sg-row sg-readouts">
        {readouts}
      </div>

      <div data-testid="layout-main" className="sg-main" ref={mainRef}>
        <div data-testid="layout-world" className="sg-world" ref={worldRef}>
          {world}
        </div>
        {mode === 'debug' && (
          <div data-testid="layout-debug" className="sg-debug">
            {debug}
          </div>
        )}
        <div data-testid="layout-instruments" className="sg-instruments">
          {instruments}
        </div>
        <div className="sg-footer">{footer}</div>
      </div>

      <div data-testid="layout-controls" className="sg-controls">
        {controls}
      </div>
    </div>
  )
}
