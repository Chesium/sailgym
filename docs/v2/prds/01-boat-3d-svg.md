# v2 Section 01 — Low-poly boat: 3-D geometry projected into the SVG

**Prerequisite reading:** `docs/v1/00-foundations.md` (all — **F2**, F7, F8, F12,
F13 especially), `docs/v2/README.md`, `docs/v1/brief.md` §25, §26, §27, §42,
§46, `docs/v1/progress/02-handoff.md` (the SVG renderer as built),
`docs/v1/progress/07-handoff.md` (roll and capsize), `docs/v1/performance.md`
(the frame budget this must stay inside). Background, **not** instructions:
`docs/v2/discussions/boat-rendering-suggestions.md`.

## Goal

The boat is drawn as a small set of 3-D triangles defined in the boat-fixed
frame `B`, rolled into the horizontal body frame `H` by the live `φ`, and
projected orthographically into the existing top-down SVG. Heel becomes
continuously visible — the masthead swings out, the deck narrows, a topside
appears, the sail gains area — and past 90° the underside and the centreboard
come into view, so a capsize reads as a capsize.

Nothing about the camera, the controls, the physics or the WASM boundary
changes. **This section touches no Rust at all.**

## Why this shape

The boat is a **rigid body**. Every part of the model rotates by the same
`R_x(φ)`, so two triangles that do not interpenetrate in `B` cannot
interpenetrate at any heel angle. That single observation removes the expensive
half of the problem the discussion note flagged: there is **no runtime face
splitting and no BSP tree**. The model is built once, non-self-intersecting, in
`B`; per frame the renderer rotates, culls and depth-sorts it. What remains —
painter's-algorithm ordering error between disjoint faces — is handled by
*static* subdivision of the long thin parts, and it is measured rather than
asserted by eye (task 1.6).

## What this section does **not** do

Out of scope, and no agent may add them:

- deck.gl `SimpleMeshLayer`, or any new rendering dependency;
- pre-rendered roll sprites;
- directional lighting, shading, gradients or ambient occlusion — flat material
  colours only (a later section may add shading; it is easier to add to a
  correct depth sort than to debug through one);
- a sailor figure, wake, spray, sail cloth detail, roach, battens, telltales;
- any change to `HeelIndicator.tsx`, which stays exactly as it is (brief §26
  asks for it, and it remains the numeric read of `φ`);
- any change to the physics crate, the WASM surface, `parameters.rs` or the
  snapshot layout.

---

## Normative deltas

Two, both already ruled on by the human on **2026-09-20**, plus one open
reading recorded for confirmation.

### D1 — F12 gains a ninth step. **Approved 2026-09-20.**

`pnpm --dir web test:unit` becomes **step 8**; the Playwright run moves from
step 8 to **step 9**. Steps 1–7 keep their numbers. This closes the open item
raised in `docs/v1/progress/02-handoff.md`:

> **Open item — `pnpm --dir web test:unit` is not in the F12 gate.** F12 fixes
> the gate at eight steps and I did not alter it. […] Section 10 should decide
> whether F12 gains a ninth step; that is a foundations change and needs the
> human.

Task 1.8 implements it and records the approval and the date in F12 itself.

### D2 — Visual-only dimensions live in the web layer, as dimensionless ratios. **Approved 2026-09-20.**

Hull depth, freeboard, sheer rise, mast height and the sail's planform are not
in the F7 catalogue, because nothing in the physics needs them. They are
therefore **not** added to `BoatParameters`. They live in a new
`web/src/render/visualDims.ts` as *ratios* of quantities that F7 does define,
exactly as `geometry.ts` already holds the hull outline. Consequences, all
enforced by acceptance criteria:

- no metre value is hard-coded under `web/src/render/` (the section 02 rule,
  extended to the new files);
- every visual constant carries a provenance comment — its value, the metres it
  yields at the ILCA defaults, and whether it is *derived* from an F7 parameter
  or *assumed*, tagged `VISUAL`;
- a `VISUAL` constant may be chosen because it looks right. A physical
  coefficient may not (brief §43). The two must never be confused, which is why
  they live in different languages in different crates.

### D3 — A 3-D rotation in the render layer. **Open; proceed, confirm.**

F2 says frame conversions live in `physics/frames.rs` "and nowhere else"; the
section 02 PRD calls `frames.rs` "the only file in the repository containing a
rotation matrix". `web/src/render/Camera.ts` has contained a 2-D view rotation
since M1 and passed every gate since, so the established reading is that
**view** transforms are exempt and the prohibition is on *physics* frame
conversions. This section needs `R_x(φ)` as a view transform and proceeds on
that reading. Guarding it is task 1.4's `render_rotation_is_view_only`
acceptance criterion: the new modules import nothing from `web/src/sim/` except
types, and nothing they compute is ever passed back across the WASM boundary.
Recorded in `docs/v2/README.md` as open item **V-B**.

---

## The transform, stated once

Model vertices are defined in `B` (F2: `+x` forward, `+y` **to port**, `+z`
up), in metres, and are **constant**. Articulation — the boom's `β` and the
rudder's `δr`, both right-handed about `+z_B` (F2.1, F2.2) — is applied when
the model is built, i.e. **before** roll. Then, per frame:

```
v_H = R_x(φ) · v_B          F2's matrix, verbatim:

x_H = x_B
y_H = y_B·cos φ − z_B·sin φ
z_H = y_B·sin φ + z_B·cos φ
```

The SVG draws `(x_H, y_H)`; `z_H` is depth. The existing
`Camera.boatTransform(camera, {x, y}, psi)` then applies yaw and the camera
exactly as it does today — **unchanged**, and still the only place heading and
the camera are applied.

`φ` is never wrapped (F3). Everything below must hold for `φ` of ±4 rad, not
just ±π.

### Visibility

The view is orthographic from `+z_H`. For a triangle with outward normal `n_H`:

| Kind | Rule |
|---|---|
| Part of the closed hull solid (`closed: true`) | visible iff `n_H·ẑ > 0` |
| A thin plate — sail, centreboard, rudder (`closed: false`) | always drawn, both sides |

Three consequences are exact, and three acceptance criteria assert them:

- the deck cap (outward normal `+ẑ_B`) is visible iff `cos φ > 0`;
- the keel cap (outward normal `−ẑ_B`) is visible iff `cos φ < 0` — **the
  underside appears at exactly 90° of heel and not before**;
- a port topside (outward normal ≈ `+ŷ_B`) is visible iff `sin φ > 0`, i.e.
  when the boat heels to starboard, which is `φ > 0` (F2).

### Ordering

Painter's algorithm: draw ascending `z_H` of the triangle centroid, so higher
faces paint last. Its known failure mode — a long triangle whose centroid ranks
badly against a small neighbour — is bounded by *static* subdivision of the
long thin parts (the centreboard and rudder plates, the sail), decided once in
`B`, never per frame. Task 1.6 measures the residual error against an
independent sampled-ray reference and the acceptance criterion is numeric.

---

## Tasks

### 1.1 — Contracts: the model type and the visual dimensions
**P-group: S**
**Owns:** `web/src/render/model3d.ts`, `web/src/render/visualDims.ts`

Solo, and first: every other task in the section consumes these two files.

`model3d.ts` — types only, no geometry, no arithmetic beyond `crossProduct`:

```ts
export interface Vec3 { x: number; y: number; z: number }

/** Flat material colours. No lighting term exists anywhere in this section. */
export type Material = 'deck' | 'topsides' | 'bilge' | 'underside' | 'sail' | 'foil'

/** Which `data-testid` group the primitive is rendered into. */
export type BoatGroup = 'hull' | 'sail' | 'board' | 'rudder'

export interface Tri {
  /** Vertices in B, metres, wound counter-clockwise seen from outside. */
  a: Vec3; b: Vec3; c: Vec3
  material: Material
  group: BoatGroup
  /** True only for triangles of the closed hull solid — the only ones culled. */
  closed: boolean
}

/** The mast and the boom stay line elements; see task 1.5 on why. */
export interface Line3 { a: Vec3; b: Vec3; testId: string; widthPx: number; colour: string }

export interface BoatModel { tris: readonly Tri[]; lines: readonly Line3[] }

/** Outward normal of a triangle, in whatever frame its vertices are in. */
export function normal(t: { a: Vec3; b: Vec3; c: Vec3 }): Vec3
```

`visualDims.ts` — the ratios, and one function turning `RenderParams` into
metres. **Every exported constant is dimensionless**; the metre values appear
only in the provenance comments. Required contents, values as given:

| Constant | Value | Metres at ILCA defaults | Provenance |
|---|---|---|---|
| `DECK_Z` | `= sheet.block_pos_b.z` | 0.10 | **derived (F7)** — the transom block sits on the deck, so its height *is* the deck height aft |
| `SHEER_RISE_PER_LOA` | 0.055 | 0.233 | `VISUAL` — the deck rises forward; chosen so the bow deck clears the water in the drawing |
| `CHINE_DROP_PER_BEAM` | 0.22 | 0.301 | `VISUAL` — topside height, sheer to chine |
| `CANOE_DEPTH_PER_BEAM` | 0.08 | 0.110 | `VISUAL` — chine to keel amidships |
| `ROCKER_RISE_PER_BEAM` | 0.12 | 0.164 | `VISUAL` — the keel line rises at bow and transom, `∝ (2·fx)²` |
| `KEEL_WIDTH_FRACTION` | 0.30 | — | `VISUAL` — the keel ring is the sheer outline narrowed in `y` |
| `KEEL_LENGTH_FRACTION` | `= hull.lwl / hull.loa` | 0.901 | **derived (F7)** |
| `MAST_HEIGHT` | `= sheet.z_boom + 2·sail.area / sail.boom_length` | 5.891 | **derived (F7)** — the drawn sail triangle then has *exactly* `sail.area`. See below. |
| `MASTHEAD_EXTENSION_FRACTION` | 0.02 | 0.104 | `VISUAL` — mast above the head |
| `SAIL_CAMBER_FRACTION` | 0.12 | — | **derived** — parity with the existing `SailShape.ts` |
| `SAIL_CAMBER_POSITION` | 0.40 | — | `VISUAL` — chordwise position of maximum camber |
| `BOARD_CHORD_FRACTION` | 0.10 | 0.423 | **derived** — the existing `geometry.ts` constant, moved, not changed |
| `RUDDER_CHORD_FRACTION` | 0.08 | 0.338 | **derived** — likewise |
| `BOARD_SPAN` | `= board.area / boardChord` | 0.473 | **derived (F7)** |
| `RUDDER_SPAN` | `= rudder.area / rudderChord` | 0.310 | **derived (F7)** |

The mast height is worth reading twice, because it is the one place a `VISUAL`
number was avoided. The drawn sail is the triangle tack → head → clew with the
tack and clew both at `z_boom` and the head at the masthead, so its area is
`½ · (MAST_HEIGHT − z_boom) · boom_length`. Setting that equal to `sail.area`
gives the expression above, and at the ILCA defaults it yields 5.89 m — which
is, to a centimetre, the real ILCA mast height. It is a derivation, not a
fit; do not replace it with a constant.

`board.area` and `rudder.area` are not in `RenderParams` yet; task 1.5 adds
them along with `hull.lwl`, `sail.z_ce` and `sheet.z_boom`. Until then, write
`visualDims.ts` against the extended interface and let the typecheck fail —
1.5 is in the next group.

```ts
export interface VisualDims {
  deckZ: number; sheerRise: number; chineZ: number; keelZ: number
  rockerRise: number; keelWidthScale: number; keelLengthScale: number
  mastHeight: number; mastTopZ: number
  boardChord: number; boardSpan: number; rudderChord: number; rudderSpan: number
}
/** Pure. The only place F7 parameters become drawing dimensions. */
export function visualDims(p: RenderParams): VisualDims
```

**Acceptance criteria**
- `pnpm --dir web test:unit visualDims`, by name:
  - `sail_triangle_has_the_parameter_area`: `½·(mastHeight − z_boom)·boom_length`
    equals `sail.area` within 1e-12, for the ILCA defaults **and** for
    `sail.area = 5.0`, `boom_length = 3.4`.
  - `dims_scale_with_parameters`: doubling `hull.beam` doubles `chineZ − deckZ`
    and `keelZ − chineZ` exactly.
  - `every_visual_constant_has_provenance`: the test reads
    `src/render/visualDims.ts` and asserts every exported `const` is preceded by
    a comment containing `VISUAL` or `derived (F7)` or `derived`. Count must
    equal the number of exported constants.
- `grep -rnE "4\.23|3\.81|1\.37|2\.72|7\.06|2\.40|5\.89" --include=*.ts --include=*.tsx web/src/`
  matches nothing.

---

### 1.2 — The hull solid
**P-group: A**
**Owns:** `web/src/render/hullSolid.ts`
**Depends:** 1.1

Three rings of 13 vertices each, all built from the **existing**
`HULL_OUTLINE` in `geometry.ts` (read it; do not copy it, and do not change
it):

| Ring | `x`, `y` | `z` |
|---|---|---|
| sheer | outline × `(loa, beam)` | `deckZ + sheerRise·(fx + 0.5)` |
| chine | outline × `(loa, beam)` | `chineZ` (planar) |
| keel | outline × `(loa·keelLengthScale, beam·keelWidthScale)` | `keelZ + rockerRise·(2·fx)²` |

Triangles, all wound counter-clockwise seen from outside the solid:

| Part | Triangles | Material |
|---|---|---|
| deck cap — fan from the sheer ring's centroid | 13 | `deck` |
| topsides — sheer ↔ chine strip | 26 | `topsides` |
| bilge — chine ↔ keel strip | 26 | `bilge` |
| keel cap — fan from the keel ring's centroid | 13 | `underside` |

78 triangles, all `closed: true`, all `group: 'hull'`.

```ts
export function hullSolid(hull: HullDims, dims: VisualDims): Tri[]
```

**Acceptance criteria**
- `pnpm --dir web test:unit hullSolid`, by name:
  - `outline_is_the_v1_outline`: the sheer ring's `(x, y)` pairs equal
    `hullPath(hull)`'s points, parsed from the string, within 1e-12. The boat
    at `φ = 0` is the boat v1 drew.
  - `is_a_closed_manifold`: every edge of the 78 triangles appears exactly
    twice, and the two appearances have opposite winding.
  - `normals_point_outward`: for all 78, `normal(t)·(centroid(t) − c) > 0`,
    where `c` is the solid's volumetric centroid.
  - `signed_volume_positive`: the divergence-theorem volume of the closed mesh
    is positive and lies in `[0.25, 2.5] m³` at the ILCA defaults (a real ILCA
    canoe body is ≈ 0.9 m³ to the sheer; the band is wide on purpose — it is a
    sanity check on winding and scale, not a hull-form claim).
  - `triangle_count`: exactly 78.

---

### 1.3 — Rig and foils
**P-group: A**
**Owns:** `web/src/render/rigSolid.ts`, `web/src/render/SailShape.ts`,
`web/tests/unit/sail.test.ts`
**Depends:** 1.1

```ts
export interface RigPose { beta: number; deltaR: number; alpha: number }
export function rigSolid(p: RenderParams, dims: VisualDims, pose: RigPose): BoatModel
```

Everything here is built in `B` with articulation already applied, so the
caller only has to roll it.

**Sail.** The patch tack `(mastX, 0, z_boom)` → head `(mastX, 0, mastHeight)` →
clew `(mastX, 0, z_boom) + boom_length·b̂(β)`, with `b̂(β) = (−cos β, −sin β, 0)`
(F2.1) — take it from the existing `geometry.boomDirection`, do not rewrite it.
Camber: each point is offset along the patch's local normal by
`SAIL_CAMBER_FRACTION · chord · shape(s) · sign(alpha)`, with `shape` peaking at
`SAIL_CAMBER_POSITION` and zero at luff and leech, so the sign matches the
existing 2-D bulge. 4 chordwise × 2 spanwise panels, ≤ 16 triangles,
`closed: false`, `group: 'sail'`.

`SailShape.ts` loses `sailShape()` and becomes the sail patch's parameterisation
(`sailPatch`), which `rigSolid` consumes. `web/tests/unit/sail.test.ts`'s first
`it` — the one asserting the 2-D path string — is replaced by the camber-sign
test below. **Its other two `it`s (`apparentWindDegrees` at the degree
boundary, and the Rust/TS diagnostics field-parity check) are preserved
verbatim.** They belong to sections 05 and 08 and this section has no business
touching them.

**Centreboard.** A plate in the `x–z` plane at `y = 0`, chord
`boardChord` centred on `board.pos_b.x`, running from `board.pos_b.z −
boardSpan/2` up to **`min(board.pos_b.z + boardSpan/2, keelZ)`** — clipped at
the keel so the board never pokes through the deck. Subdivided into 3 spanwise
strips (6 triangles) so the depth sort stays local. `closed: false`,
`group: 'board'`.

**Rudder.** The same, chord `rudderChord` along `ĉ_r(δr) = (−cos δr, −sin δr, 0)`
(F2.2) from the stock at `rudder.pos_b`, span `rudderSpan` centred on
`rudder.pos_b.z`, 2 spanwise strips (4 triangles). No clipping: the stock is
aft of the transom. `closed: false`, `group: 'rudder'`.

**Mast and boom** are `Line3`, not triangles — see task 1.5 for the reason.
Mast: from the sheer-ring height at `mastX` up to `mastTopZ`, `testId:
'boat-mast'`. Boom: from the gooseneck `(mastX, 0, z_boom)` to the clew,
`testId: 'boat-boom'`.

**Acceptance criteria**
- `pnpm --dir web test:unit sail`, by name:
  - `camber_mirrors_with_alpha`: the patch built with `alpha = +0.2` and
    `alpha = −0.2` are reflections in the sail's chord plane, within 1e-12; at
    `alpha = 0` the patch is planar within 1e-12.
  - `sail_area_matches_the_parameter`: the summed triangle area of the
    **uncambered** patch equals `sail.area` within 0.5 %.
  - the two preserved `it`s still pass, unchanged.
- `pnpm --dir web test:unit rigSolid`, by name:
  - `clew_follows_beta`: the clew equals `mast + boom_length·b̂(β)` within
    1e-12 for `β ∈ {−1.2, −0.3, 0, 0.3, 1.2}`, and `β = +0.3` puts it at
    `y < 0` — **to starboard** (F2.1).
  - `rudder_trailing_edge_follows_delta_r`: the plate's aft edge is at `y < 0`
    for `δr = +0.3` (F2.2).
  - `board_is_clipped_at_the_keel`: no board vertex has `z > keelZ`.
  - `articulation_precedes_roll`: `rigSolid(…, β).clew` rolled by `φ` equals
    `R_x(φ)·(mast + L·b̂(β))` within 1e-12, for 20 random `(β, φ)`. This is the
    guard against applying roll first and articulating in `H`.
  - `triangle_budget`: `rigSolid` returns ≤ 26 triangles.

---

### 1.4 — Projection, culling, depth sort
**P-group: A**
**Owns:** `web/src/render/project3d.ts`
**Depends:** 1.1

The one file that contains `R_x(φ)`. Pure; no React, no DOM, no `sim` import
that is not `import type`.

```ts
export function rollToH(v: Vec3, phi: number): Vec3        // F2's R_x(φ)

export interface DrawTri {
  points: readonly [Vec2, Vec2, Vec2]   // (x_H, y_H), metres
  depth: number                          // centroid z_H
  material: Material
  group: BoatGroup
}
export interface DrawLine { a: Vec2; b: Vec2; depth: number; testId: string; widthPx: number; colour: string }
export type DrawItem = DrawTri | DrawLine

/** Rolled, culled, sorted back to front. The whole per-frame pipeline. */
export function projectModel(model: BoatModel, phi: number): readonly DrawItem[]

/** Depth of the triangle's plane above a projected point. For the 1.6 reference. */
export function depthAt(t: DrawTri, p: Vec2): number
```

Rules, in order: roll every vertex; cull `closed` triangles whose rolled
outward normal has `n_z ≤ 0`; sort ascending by `depth`; `DrawLine`s take the
`z_H` of their **higher** endpoint, which puts a tipped-up mast in front of the
sail rather than behind it (a deliberate simplification — see 1.5).

The sort must be **stable and total**: equal depths break ties by the item's
index in the model, so the output is a pure function of `(model, phi)` and two
renders of the same frame produce byte-identical SVG.

**Acceptance criteria**
- `pnpm --dir web test:unit project3d`, by name:
  - `roll_matches_F2`: `rollToH` equals the F2 matrix product, within 1e-15,
    for 200 random `(v, φ)` with `φ ∈ [−4, 4]`.
  - `roll_is_an_isometry`: lengths and angles are preserved to 1e-12; `φ = 0`
    is the identity **exactly** (`===` on all three components).
  - `masthead_swings_to_starboard`: the projected masthead `y_H` equals
    `−mastHeight·sin φ` within 1e-12, and is **negative** (starboard, F2) for
    `φ = +0.3`.
  - `deck_narrows_as_cos_phi`: the `y_H` separation of the two widest sheer
    vertices equals `beam·cos φ` within 1e-12, for 50 values of `φ`.
  - `deck_cap_hidden_past_ninety`: no `deck` triangle survives the cull for
    `|φ| > π/2`; at least one does for `|φ| < π/2`.
  - `underside_revealed_past_ninety`: no `underside` triangle survives for
    `|φ| < π/2`; at least one does for `|φ| > π/2`; at `φ = π` the visible
    `underside` area equals the `deck` area at `φ = 0` within 1 %.
  - `topside_appears_on_the_rising_side`: at `φ = +0.5` every visible
    `topsides` triangle has its centroid at `y_B > 0` (**port**, the side that
    rises when the boat heels to starboard); at `φ = −0.5`, `y_B < 0`.
  - `sail_area_grows_with_heel`: projected `sail` area at `φ = π/4`, `β = 0` is
    at least 3 × its area at `φ = 0`, and follows `|cos β · sin φ|` within 2 %
    of the uncambered prediction for 10 `(β, φ)` pairs.
  - `sort_is_pure_and_stable`: `projectModel` called twice on the same inputs
    returns deep-equal output, and the order is unchanged when two triangles
    are given identical depths.
  - `render_rotation_is_view_only`: the test reads
    `src/render/{model3d,visualDims,hullSolid,rigSolid,project3d}.ts` and
    asserts (a) every `from '../sim/…'` import line begins `import type`,
    (b) none of them contains `advance(`, `set_controls`, `Sim(` or
    `snapshot`. The view rotation cannot leak into the physics (D3, F8).

---

### 1.5 — Integration: `BoatSvg`, the pose, the parameters
**P-group: S**
**Owns:** `web/src/render/BoatSvg.tsx`, `web/src/render/BoatProbe.tsx`,
`web/src/render/SheetRope.tsx`, `web/src/App.tsx`, `web/src/sim/useSimulation.ts`
**Depends:** 1.2, 1.3, 1.4

Solo: this is where four independently written modules meet the app, and where
the DOM contract that four existing E2E specs read either survives or does not.

**`useSimulation.ts`** — `RenderParams` gains the fields the model needs. All
five are already emitted by `parameters_json()`; **no Rust change is required,
and none is permitted**:

```ts
hull:   { loa: number; beam: number; lwl: number }
sail:   { …; z_ce: number }
board:  { pos_b: …; area: number }
rudder: { pos_b: …; area: number }
sheet:  { …; z_boom: number }
```

**`App.tsx`** — the pose handed to `BoatSvg` gains `phi: s.phi` (today it stops
at `deltaR`; `BoatSvg` has never seen roll). Mount `<BoatProbe />` next to the
existing `<HeelProbe />`. Nothing else in `App.tsx` changes.

**`BoatSvg.tsx`** — build the model with `useMemo` keyed on
`(params, beta, deltaR, alpha)` — the model is geometry, not a frame — then
call `projectModel(model, phi)` per render and emit the draw list. Keep the
existing `<g data-testid="boat-hull" transform={boatTransform(...)}>` wrapper
**exactly as it is**: yaw and camera are still its job, and three specs read
its matrix.

**The DOM contract. Read this before writing any JSX.** Four existing specs
read this subtree, and none of them may be weakened to accommodate the new
renderer:

| Element | Today | After | Why |
|---|---|---|---|
| `boat-hull` | `<g transform=…>` | unchanged | `render.spec`, `sail.spec`, `sheet.spec` read its screen CTM |
| `boat-boom` | `<line x1 y1 x2 y2>` | **still a `<line>`**, endpoints = the *projected* gooseneck and clew | `sail.spec` reads `x1/y1/x2/y2` for its starboard test; `sheet.spec` interpolates the sheet attachment along it |
| `boat-mast` | `<circle>` | `<line>` from deck to masthead | attachment-only assertion; a line makes heel legible |
| `boat-sail` | `<path>` | `<g>` of polygons, plus stroked luff/leech/foot edges | attachment-only assertion |
| `boat-board`, `boat-rudder` | `<line>` | `<g>` of polygons | attachment-only assertions |
| `boom` | `<g transform="rotate(β …)">` | see below | `sail.spec` asserts the attribute *changes* with `β` |

Two consequences follow, and both are deliberate:

1. **The mast and boom stay single line elements** rather than becoming
   prisms. They are therefore depth-sorted as a unit, by their higher endpoint,
   which can order the mast wrongly against the sail at extreme heel. That is
   accepted: it is one thin line against a large surface, and the alternative
   is breaking two specs that independently verify the boom's sign convention —
   the R3 guard the whole project is built around.
2. **`sheet.spec.ts` must pass unmodified.** It computes the sheet attachment
   by linear interpolation along the boom line at `f = d_sheet / boom_length`.
   Orthographic projection is linear, so as long as the boom line is drawn at
   its true height `z_boom`, the interpolated point is exactly the projected
   attachment — the comparison stays honest at any heel. Update `SheetRope.tsx`
   to carry `z`: the attachment at `(mastX − d_sheet·cos β, −d_sheet·sin β,
   z_boom)` and the block at `block_pos_b` including its `z = 0.10`, both
   projected through the same `rollToH`. The spec's 3 px tolerance absorbs the
   block's `z·sin φ` term (0.6 px at 0.3 rad, 2.0 px at 1.4 rad); the
   attachment term cancels exactly, as above.

The `<g data-testid="boom">` wrapper is the one place the contract cannot be
kept. Its `rotate(β …)` transform exists because the boom used to be drawn in
unrotated boat coordinates; once `β` is baked into the projected endpoints
there is no honest transform left to emit, and emitting a decorative
`rotate(0)` so that a spec keeps passing would be exactly the kind of
test-shaped fiction F13.4 forbids. Keep the group — it is a useful handle —
with **no** `transform` attribute, and let task 1.7 make the single authorised
edit to `sail.spec.ts` that moves the assertion onto the boom line's own
endpoints, which is what the spec was really trying to observe.

**Materials**, matching the v1 palette; flat, no lighting:

| Material | Fill | Stroke |
|---|---|---|
| `deck` | `#fdfdfd` | `#2b3a45` |
| `topsides` | `#e8eef2` | `#2b3a45` |
| `bilge` | `#cfdbe3` | `#2b3a45` |
| `underside` | `#b9c7d1` | `#2b3a45` |
| `sail` | `#d64d3f` at 0.22 opacity | `#d64d3f` |
| `foil` | `#7a8a96` | `#5d6b76` |

Strokes keep `vectorEffect="non-scaling-stroke"`, as everything in this SVG
does. Interior edges within one material may be drawn stroke-less to avoid a
faceted look; the silhouette must always be stroked, so the boat reads at
zoom 0.1.

`BoatProbe.tsx` — the fixed-angle probe, the same device `HeelProbe` uses and
for the same reason: E2E checks need the geometry at angles the live boat will
not conveniently visit. Renders the boat subtree at
`φ ∈ {0, ±30, ±60, ±90, 135, 180}°` with `β = 0.35`, `δr = 0.2`, laid out
off-screen at `left: -9999` (laid out, not `display: none`, or there is no
matrix to read), each with `data-testid="boat-probe-<deg>"`.

**Acceptance criteria**
- `pnpm --dir web typecheck` clean.
- `web/tests/e2e/render.spec.ts`, `heel.spec.ts`, `sheet.spec.ts`,
  `demonstrations.spec.ts` pass **with no edit to any of them**.
  `git diff --stat` in the handoff must show those four files untouched.
- `grep -rn "phi" web/src/render/BoatSvg.tsx` matches — the renderer sees roll.
- `HeelIndicator.tsx` is byte-identical to its pre-section state.
- No file under `crates/` is modified. `git diff --name-only crates/` is empty.

---

### 1.6 — Unit tests: the ordering reference
**P-group: B**
**Owns:** `web/tests/unit/boat3d.test.ts`
**Depends:** 1.4, 1.5

The tasks above each assert their own piece. This one asserts the thing no
single module owns: **that the picture is right**, measured against an
independent reference rather than looked at.

The reference is a sampled ray cast. For a given `(φ, β, δr)`, build and
project the model, then for each of 4000 low-discrepancy sample points `q` in
the projected bounding box:

- `containing(q)` = the drawn items whose projected triangle contains `q`;
- `reference(q)` = the member of `containing(q)` with the greatest
  `depthAt(item, q)` — the true topmost surface along that ray;
- `painter(q)` = the **last** member of `containing(q)` in draw order.

`painter(q) === reference(q)` is what the painter's algorithm is trying to
achieve, and the gap between them is exactly its error.

**Acceptance criteria**
- `pnpm --dir web test:unit boat3d`, by name:
  - `painter_matches_the_ray_reference`: agreement ≥ **99.5 %** of sampled
    points at each of `φ ∈ {0, ±30, ±60, ±90, 135, 180}°`, with `β = 0.35`,
    `δr = 0.2`; and **100 %** at `φ = 0` and `φ = 180°`, where the ordering is
    exact by construction.
  - `no_pop_across_a_full_roll`: sweeping `φ` over 720 steps of `2π/720`, the
    total visible projected area changes by less than 2 % of its maximum
    between consecutive steps. A discontinuity here means a face is appearing
    or vanishing abruptly, which is the visual artefact this test exists to
    catch.
  - `total_triangle_budget`: the assembled model has ≤ **144** triangles.
  - `projection_is_deterministic`: 100 repeated `projectModel` calls at the
    same `(model, φ)` produce deep-equal draw lists.
  - `survives_unwrapped_phi`: at `φ = 4.0` and `φ = −4.0` rad (F3: `φ` is never
    wrapped) every projected coordinate is finite and the draw list is
    non-empty.

Never weaken the 99.5 %. If it does not hold, subdivide the offending part
further in `B` — that is what the subdivision counts in 1.3 are for — and
record the new count in the handoff.

---

### 1.7 — E2E: heel reads in the browser
**P-group: B**
**Owns:** `web/tests/e2e/boat3d.spec.ts`, `web/tests/e2e/sail.spec.ts`
**Depends:** 1.5

`boat3d.spec.ts` drives the real app and reads the real DOM, per brief §42.
Use `fixtures.ts`'s `test` and `gotoApp`, like every spec in the repo.

Required tests:

| Test | Assertion |
|---|---|
| `the boat is drawn as polygons` | `[data-testid="boat-hull"] polygon` count is in `[20, 144]` |
| `the masthead swings with heel` | across the probes, the masthead end of `boat-mast`, in hull-frame coordinates via the group CTM, tracks `−mastHeight·sin φ` within 3 % for `φ ∈ {±30, ±60, ±90}°` |
| `the deck narrows with heel` | the `boat-hull` bounding box's across-hull extent at the 60° probe is smaller than at the 0° probe, and at 90° smaller again |
| `the underside shows past ninety` | a polygon with the `underside` fill exists in the 135° and 180° probes, and in **neither** the 0° nor the 60° probe |
| `the centreboard shows when capsized` | `boat-board` has non-zero rendered area in the 180° probe |
| `heel is visible on the live boat` | in `beam_reach_capsize`, the live `boat-hull` subtree's across-hull extent shrinks monotonically (within noise) as `phi` in the snapshot grows past 0.5 rad |
| `the heel indicator still agrees` | at the same instant, `[data-testid="heel-indicator"]`'s `data-phi` and the snapshot's `phi` agree to 1e-9 — the two views of roll never disagree |

The single permitted edit to `sail.spec.ts`: replace

```ts
const before = await page.getByTestId('boom').getAttribute('transform')
…
expect(await page.getByTestId('boom').getAttribute('transform')).not.toBe(before)
```

with the equivalent on the boom line's own endpoints — read `x2`/`y2` of
`boat-boom` before and after and assert they changed. This is strictly
stronger: it asserts the drawn boom moved, not that an attribute string
differs. **The `starboardProjection > 0` assertion, and every other assertion
in that file, must be left exactly as it is.** Any other change to
`sail.spec.ts` is out of scope for this task.

**Acceptance criteria**
- `pnpm --dir web test:e2e boat3d` passes in Chromium, Edge and Firefox.
- `pnpm --dir web test:e2e sail` passes in all three, with `git diff` on
  `sail.spec.ts` limited to the three lines above.
- No console error and no page error in any spec — enforced by `fixtures.ts`.

---

### 1.8 — The ninth gate step, and the docs that name the eighth
**P-group: C**
**Owns:** `scripts/check.sh`, `scripts/check.ps1`, `CLAUDE.md`,
`docs/v1/00-foundations.md` (F12 only), `README.md`
**Depends:** 1.6, 1.7

Implements normative delta **D1**. Insert `pnpm --dir web test:unit` as step 8;
the Playwright run becomes step 9. Steps 1–7 are untouched, so every existing
reference to steps 1–7 stays correct and only the references to step 8 need
revisiting.

- `scripts/check.sh` and `scripts/check.ps1`: add the step to `step_names` and
  to `run_step`, keep the two files in step with each other, and keep `--fast`
  meaning what it means — `--fast` restricts the **E2E** step to Chromium and
  `--grep-invert @slow`; the unit step runs in full under `--fast` too, because
  it takes under a second.
- `CLAUDE.md`: the gate table becomes nine rows. The new row's "Proves" column:
  *The pure TypeScript — projection, camera, clock, controls, schemas — is
  correct without a browser.*
- `docs/v1/00-foundations.md` **F12 only**: the listed chain becomes nine
  steps, with a one-line note naming the approval and the date
  (`step 8 added 2026-09-20 by human approval; see docs/v2/prds/01-boat-3d-svg.md D1`).
  **No other part of foundations may be touched.** A diff that changes any line
  outside F12 is a defect.
- `README.md`: the two places that say "eight steps" / "step 8".

Historical documents are **not** rewritten: `docs/v1/acceptance.md`,
`docs/v1/progress/*.md` and `docs/v1/*.md` record what was true when they were
written, and back-dating them would destroy the audit trail. The handoff note
says so explicitly.

**Acceptance criteria**
- `scripts/check.sh 8` runs the unit tests and nothing else; `scripts/check.sh 9`
  runs the E2E suite and nothing else.
- `scripts/check.sh` and `pwsh scripts/check.ps1` both report nine steps and
  both exit 0.
- `scripts/check.sh --fast` exits 0 and still runs step 8 in full.
- `grep -rn "eight steps" CLAUDE.md README.md docs/v2/` matches nothing.
- `git diff docs/v1/00-foundations.md` touches only lines inside F12.

---

## Section acceptance criteria

1. `scripts/check.sh` exits 0 — all **nine** steps. `pwsh scripts/check.ps1`
   likewise, if a Windows host is available; if not, say so in the handoff.
2. In the browser, on `beam_reach_capsize`, the boat visibly heels: the
   masthead swings to leeward, the deck narrows, a topside appears, and past
   90° the underside and the centreboard come into view. Every one of those
   four is asserted mechanically by 1.6 and 1.7 — this criterion is the human
   reading of the same facts, not a substitute for them.
3. `git diff --name-only crates/` is **empty**. No Rust changed, the WASM
   surface is unchanged, and `cargo test -p sailgym-physics` passes for the
   same reason it did before.
4. `render.spec.ts`, `heel.spec.ts`, `sheet.spec.ts` and `demonstrations.spec.ts`
   pass **unmodified**; `sail.spec.ts` passes with only the three-line edit
   authorised in 1.7.
5. `perf.spec.ts` passes with its existing budgets: Sail Mode median frame time
   ≤ 16.7 ms, `spans.svg.p95 < 12 ms`, dropped frames < 2 %. The handoff
   records the `svg` span's p50/p95 **before and after**, from the same
   machine, so the cost of the new renderer is a number and not an impression.
6. The ordering criterion in 1.6 holds at ≥ 99.5 % at every probe angle,
   unweakened.
7. **No metre value is hard-coded under `web/src/`.** The section 02 grep, with
   the generated WASM package excluded per the section 02 handoff's open item:
   `grep -rnE "4\.23|3\.81|1\.37|2\.72|7\.06|2\.40" --include=*.ts --include=*.tsx web/src/`
   matches nothing.
8. Every visual constant in `visualDims.ts` carries its provenance comment, and
   the count test in 1.1 proves it.
9. `docs/v2/progress/01-handoff.md` written per F13.6, additionally recording:
   the final triangle count and any subdivision raised to meet 1.6; the
   measured ordering agreement at each probe angle; the before/after `svg` span;
   and whether open item **V-B** (D3) was confirmed by the human or is still
   open.

## Risks

| # | Risk | Mitigation | Fires when |
|---|---|---|---|
| **RV1** | Painter's-algorithm ordering errors at some heel angles — the classic failure of sorting by centroid. | Rigidity means nothing interpenetrates, so only *ordering* can be wrong; static subdivision of the thin plates bounds it; 1.6 measures it against a ray reference at a stated threshold. | agreement < 99.5 % at any probe angle |
| **RV2** | Invented visual dimensions drift into the physics, or get "tuned" like coefficients. | D2: ratios only, web-side, provenance-tagged, `render_rotation_is_view_only` and the metre greps. | any `VISUAL` constant appears in `crates/` |
| **RV3** | The new DOM breaks `sail.spec` / `sheet.spec`, which are the project's R3 sign guards. | The boom and mast stay `<line>`s; projection linearity keeps `sheet.spec` exact; exactly one authorised three-line edit to `sail.spec`. | any assertion in those two files is changed or removed |
| **RV4** | ~100 SVG nodes per frame costs the 60 fps budget. | Model memoised on `(params, β, δr, α)`; only projection runs per frame; `perf.spec.ts`'s existing budgets are the gate, unchanged. | `spans.svg.p95 ≥ 12 ms` |
| **RV5** | Renumbering the gate's last step invalidates references scattered through the docs. | Insert at 8 so only one number moves; 1.8 lists every live reference; historical documents are explicitly **not** rewritten. | a doc names "step 8" meaning Playwright after 1.8 lands |
| **RV6** | D3 — a 3-D rotation outside `frames.rs` — is ruled a foundations violation after the fact. | Camera.ts precedent, guard test, recorded as open item V-B in the v2 README before dispatch. | the human rules otherwise |

## Deliberate debts, tracked

| Debt | Created | Repaid |
|---|---|---|
| Mast and boom are depth-sorted as single line elements, so they can order wrongly against the sail at extreme heel | 1.5, to keep the `sail.spec`/`sheet.spec` DOM contract | a later section, if it also migrates those specs |
| No shading: heel reads from silhouette and material alone | scope, by decision | a later section, on top of a depth sort proven correct |
| The hull form is a 3-ring low-poly approximation, not an ILCA lines plan | 1.2 | never, unless the visual goal changes |
| `sail.z_ce` is added to `RenderParams` but only used as a sanity anchor | 1.5 | whenever a sail model needs it |
