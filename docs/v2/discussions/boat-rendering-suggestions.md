**Updated 2026-09-20: this direction already shipped in section 01.** App now passes `phi` to BoatSvg. Retain the low-poly geometry projected into SVG; section 09 improves viewport sizing, wind visibility and controls. Full 3-D scenery and photorealism are outside the product direction.

## Historical pre-01 design rationale

The inspection below describes the earlier implementation, not today's tree. The chosen approach gives continuous roll, retains the camera and avoids rendered-image sets.

From inspecting the code:

- The main boat **doesn’t receive `phi`**, the roll angle: [App.tsx:379](../../../web/src/App.tsx). Its hull, sail and rudder remain flat regardless of heel.
- Roll appears only in the separate [HeelIndicator.tsx:98](../../../web/src/render/HeelIndicator.tsx).
- The existing [geometry.ts](../../../web/src/render/geometry.ts) already defines a simple, dimension-scaled hull outline—a useful starting point.

| Approach | Main advantage | Main cost | Fit here |
|---|---|---|---|
| **Low-poly geometry → SVG polygons** | Smooth roll, sharp at any zoom, no new rendering dependency | Implement face visibility and overlap | **Best first step** |
| **Live 3D mesh in deck.gl** | Depth testing handles overlapping hull and rig | More integration and asset work | Best for richer visuals later |
| **Pre-rendered roll sprites** | Very cheap drawing; polished baked appearance | Angle stepping, image memory, moving rig complications | Good for a mostly fixed boat design |

**How I’d implement the SVG version**

Give the hull outline some depth: deck, sides and underside, with distinct flat colours. Add a mast, a simple triangular sail surface, and centreboard geometry. A few dozen faces should be a reasonable starting budget; actual performance would need measuring.

For each boat-local vertex, roll around the longitudinal axis, then use the existing heading/camera transform:

```text
x′ = x
y′ = y cos(phi) − z sin(phi)
z′ = y sin(phi) + z cos(phi)
```

The top-down drawing uses `(x′, y′)`; `z′` determines visibility and drawing order. This matches the simulator’s convention: positive roll is starboard down.

The useful visual cues would be:

- The **masthead moves sideways**, making even modest heel visible.
- The deck narrows while a hull side becomes visible.
- The sail gains visible area as it leans.
- Beyond 90°, the underside and centreboard reveal the capsize.

Simply squeezing the existing SVG by `cos(phi)` would miss most of these cues and collapse the boat at 90°.

Boom and rudder articulation should happen **before** applying roll. The sheet attachments also need their heights carried through; the current rendering discards them. Use existing physical dimensions where available and explicitly defined visual dimensions for missing hull depth/mast height. Intersecting sail/hull faces may need splitting rather than relying solely on average-depth sorting.

**Your pre-captured-view idea is viable—with the hull baked separately**

I’d render a consistent low-poly hull from a fixed orthographic camera, sampling the full 360° of roll, initially every 5°: **72 frames**. Every frame needs identical scale, bounds and roll-axis anchoring. Heading can then be applied as a normal 2D rotation.

Keep the sail, boom, rudder and sheet procedural. Baking those into the images multiplies the frame count across independently changing controls. Other tradeoffs:

- Crossfading adjacent frames can produce double edges.
- At 256×256 RGBA, 72 frames occupy roughly **18 MiB decoded**, before padding or mipmaps.
- Baked directional lighting rotates with the image; neutral lighting is easier to integrate.
- Capsize still requires deliberate rig/hull occlusion.

**If you prefer actual 3D**, reuse the existing deck.gl canvas. Its [SimpleMeshLayer](https://deck.gl/docs/api-reference/mesh-layers/simple-mesh-layer) supports mesh geometry and per-instance transforms. The main integration work would be matching the existing camera and reorganising the SVG/grid layers around the mesh.

My proposed first milestone is the projected SVG boat at **0°, ±30°, ±60°, ±90° and 180°**, with working boom/rudder controls and the existing heel indicator retained. That would establish whether the roll reads clearly before investing in detailed assets. No files changed.
