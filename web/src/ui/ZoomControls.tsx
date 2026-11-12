/**
 * The camera's zoom buttons, floated over the top-right corner of the world
 * view.
 *
 * They replace wheel zoom. The wheel scrolls the page's **main** row instead,
 * because that row holds the instruments, the debug column and the practice
 * panel underneath the world, and it no longer shows a scrollbar to drag.
 *
 * Two step sizes: a coarse pair that halves or doubles the view, and a fine
 * pair for small adjustments. Every step is a **factor**, so a press in and a
 * press out cancel exactly at any zoom, and the clamp to the camera's range is
 * left to the `onZoom` handler that already clamps.
 *
 * Pointer-down events stop here and never reach the SVG underneath, so a press
 * cannot start a mainsheet haul or a camera pan.
 */

/**
 * **Provenance.** Presentation only. ×2 per coarse press crosses the camera's
 * whole 0.1–20 range in under eight presses; ×1.1 per fine press is a change
 * small enough to frame a mark without overshooting it.
 */
export const ZOOM_STEP = { coarse: 2, fine: 1.1 }

export interface ZoomControlsProps {
  /** `factor` multiplies the current zoom. */
  onZoom: (factor: number) => void
}

const BUTTONS = [
  { testId: 'zoom-in-coarse', label: '+', title: 'Zoom in', factor: ZOOM_STEP.coarse, size: 18 },
  { testId: 'zoom-in-fine', label: '+', title: 'Zoom in a little', factor: ZOOM_STEP.fine, size: 12 },
  { testId: 'zoom-out-fine', label: '−', title: 'Zoom out a little', factor: 1 / ZOOM_STEP.fine, size: 12 },
  { testId: 'zoom-out-coarse', label: '−', title: 'Zoom out', factor: 1 / ZOOM_STEP.coarse, size: 18 },
] as const

export function ZoomControls({ onZoom }: ZoomControlsProps) {
  return (
    <div
      data-testid="zoom-controls"
      style={{
        position: 'absolute',
        right: 8,
        top: 8,
        zIndex: 4,
        display: 'flex',
        flexDirection: 'column',
        gap: 4,
      }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {BUTTONS.map((b) => (
        <button
          key={b.testId}
          type="button"
          data-testid={b.testId}
          title={b.title}
          aria-label={b.title}
          onClick={() => onZoom(b.factor)}
          style={{
            width: 32,
            height: 32,
            padding: 0,
            borderRadius: 4,
            border: '1px solid #cbd',
            background: 'rgba(255, 255, 255, 0.88)',
            color: '#334',
            fontSize: b.size,
            fontWeight: 600,
            lineHeight: 1,
            cursor: 'pointer',
            touchAction: 'manipulation',
          }}
        >
          {b.label}
        </button>
      ))}
    </div>
  )
}
