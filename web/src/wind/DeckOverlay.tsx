/**
 * The application's single deck.gl surface (brief section 19, section 39).
 *
 * R5 in `docs/00-foundations.md` is about exactly this file: one `Deck`
 * instance, created lazily, never torn down and rebuilt. Toggling the wind
 * layers changes the `layers` prop; it does not mount or unmount `DeckGL`, so
 * no WebGL context is ever created more than once.
 *
 * ## Why the layers are drawn in pixel space
 *
 * `render/Camera.ts` is the only world→screen mapping in the application
 * (section 02). deck.gl's `OrthographicView` has a target and a zoom but no
 * rotation, so it cannot reproduce `follow` mode, where the world turns under
 * the boat. Rather than add a second, subtly different projection, the layer
 * builders convert world metres to screen pixels with `camera.worldToScreen`
 * and this view is left as an identity pixel-space view. The particles then
 * stay registered with the SVG boat in both camera modes, by construction.
 */

import { OrthographicView } from '@deck.gl/core'
import type { Layer } from '@deck.gl/core'
import DeckGL from '@deck.gl/react'

import type { Viewport } from '../render/Camera'

export interface DeckOverlayProps {
  viewport: Viewport
  layers: Layer[]
}

/**
 * Identity pixel-space view: one unit is one CSS pixel and `+y` runs down the
 * screen, matching what `Camera.worldToScreen` returns.
 */
const PIXEL_VIEW = new OrthographicView({ id: 'pixels', flipY: true })

export function DeckOverlay({ viewport, layers }: DeckOverlayProps) {
  const viewState = {
    target: [viewport.width / 2, viewport.height / 2, 0] as [number, number, number],
    zoom: 0,
  }

  return (
    <div
      data-testid="deck-overlay"
      data-layers={layers.length}
      style={{
        position: 'absolute',
        inset: 0,
        // The SVG above owns every pointer interaction (pan, zoom, and the
        // mainsheet drag section 06 reserves). The canvas must never swallow
        // one.
        pointerEvents: 'none',
      }}
    >
      <DeckGL
        views={PIXEL_VIEW}
        viewState={viewState}
        controller={false}
        layers={layers}
        width={`${viewport.width}px`}
        height={`${viewport.height}px`}
        style={{ position: 'absolute', inset: '0' }}
      />
    </div>
  )
}
