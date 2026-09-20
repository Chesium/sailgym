# v2 Section 01 — Handoff (low-poly boat: 3-D geometry projected into the SVG)

**Written per F13.6.** Read this before starting the next v2 section.
`docs/v1/00-foundations.md` remains normative; nothing below redefines it, and
the one change made to it (F12, the gate chain) is the approved delta **D1**.

Status: **complete.** `scripts/check.sh` exits 0 end to end — **9/9**, 547 s,
267 Playwright tests across Chromium, Firefox and Edge, 129 vitest tests.
`scripts/check.sh --fast` exits 0 in 139 s.

**Two acceptance criteria are not literally satisfied and one is unsatisfiable
as written. §5 below states each one, why, and what was done instead. Two of
the three require a human ruling.**

---

## 1. What landed

### Task 1.1 — Contracts (P-group S, section agent)

- **`web/src/render/model3d.ts`** — `Vec3`, `Vec2`, `Material`, `BoatGroup`,
  `Tri`, `Line3`, `BoatModel`, `normal`, `centroid`. Types and one cross
  product; no physics, no frame conversion.
- **`web/src/render/visualDims.ts`** — the ten dimensionless `VISUAL`/derived
  constants and `visualDims(p: RenderParams): VisualDims`, the only place F7
  parameters become drawing dimensions. `DECK_Z`, `KEEL_LENGTH_FRACTION`,
  `MAST_HEIGHT`, `BOARD_SPAN` and `RUDDER_SPAN` from the task table are pure
  derivations from `RenderParams`, so they are computed inside `visualDims`
  and documented in its header rather than declared as constants.
- **`web/tests/unit/ilca.ts`** — the F7 ILCA defaults as test data, shared by
  the four new unit suites. Under `tests/`, so the "no metre value" grep over
  `web/src/` is untouched.
- **`web/tests/unit/visualDims.test.ts`** — `sail_triangle_has_the_parameter_area`,
  `dims_scale_with_parameters`, `every_visual_constant_has_provenance`,
  `no_metre_value_is_hard_coded_under_render`.

### Task 1.2 — The hull solid (P-group A)

- **`web/src/render/hullSolid.ts`** — `hullOutline`, `parsePath`, `hullRings`,
  `sheerHeightAt`, `hullSolid`. **76** triangles: 13 deck cap, 26 topsides,
  26 bilge, 11 keel cap.
- **`web/tests/unit/hullSolid.test.ts`** — `outline_is_the_v1_outline`,
  `is_a_closed_manifold`, `normals_point_outward`, `signed_volume_positive`
  (1.917 m³), `triangle_count`, plus
  `the_caps_have_no_lateral_normal_component`.

`geometry.ts` is not owned by this section and `HULL_OUTLINE` is not exported
from it, so the outline is recovered by evaluating `hullPath({loa: 1, beam: 1})`
and parsing it back. That is stronger than copying the array: there is still
exactly one hull outline in the repository, and it cannot drift.

### Task 1.3 — Rig and foils (P-group A)

- **`web/src/render/SailShape.ts`** — `sailShape()` is gone; the file now holds
  `sailCorners`, `sailPatch`, `sailPoint`, `sailEdges`, `camberProfile`. 12
  triangles (4 chordwise × 2 spanwise; the upper strip degenerates at the head,
  so it is one triangle per panel rather than two).
- **`web/src/render/rigSolid.ts`** — `rigSolid` (sail + board + rudder + the
  mast and boom `Line3`s) and `boatModel` (hull + rig). 22 triangles.
- **`web/tests/unit/rigSolid.test.ts`** — `clew_follows_beta`,
  `rudder_trailing_edge_follows_delta_r`, `board_is_clipped_at_the_keel`,
  `articulation_precedes_roll`, `triangle_budget`.
- **`web/tests/unit/sail.test.ts`** — the first `it` replaced by
  `camber_mirrors_with_alpha` and `sail_area_matches_the_parameter`. **The
  other two `it`s are byte-identical**, as required: the `apparentWindDegrees`
  degree-boundary test and the Rust/TS diagnostics field-parity test.

### Task 1.4 — Projection, culling, depth sort (P-group A)

- **`web/src/render/project3d.ts`** — `rollToH` (F2's `R_x(φ)`, verbatim),
  `DrawTri`/`DrawLine`/`DrawItem`, `projectModel`, `depthAt`, `containsPoint`,
  `projectedArea`, `isTri`.
- **`web/tests/unit/project3d.test.ts`** — all ten named criteria.

`DrawTri` carries one field the task listing does not name: `depths`, the three
vertices' `z_H`. `depthAt` needs the triangle's *plane*, and a plane cannot be
recovered from a centroid scalar; the task 1.6 ray reference is built on it.

### Task 1.5 — Integration (P-group S, section agent)

- **`web/src/sim/useSimulation.ts`** — `RenderParams` gained `hull.lwl`,
  `sail.area`, `sail.z_ce`, `board.area`, `rudder.area`, `sheet.z_boom`. All six
  were already emitted by `parameters_json()` (the foil sections are
  `#[serde(flatten)]`ed, so `area` sits directly under `sail`, `board` and
  `rudder`). **No Rust changed.**
- **`web/src/render/BoatSvg.tsx`** — rewritten. Exports `BoatFigure` (model,
  projection, draw list) and `BoatSvg` (the world view). The
  `<g data-testid="boat-hull" transform={boatTransform(...)}>` wrapper is
  unchanged: yaw and the camera are still its job.
- **`web/src/render/BoatProbe.tsx`** — nine fixed-angle probes at
  `φ ∈ {0, ±30, ±60, ±90, 135, 180}°`, `β = 0.35`, `δr = 0.2`, laid out at
  `left: -9999`.
- **`web/src/render/SheetRope.tsx`** — carries `z`: `SheetRigDims` gained
  `zBoom` and the block's `z`, and both ends are projected through the same
  `rollToH`. `attachPointB` is the 3-D attachment; `attachPoint` and
  `blockPoint` are their projections.
- **`web/src/App.tsx`** — the pose handed to `BoatSvg` gained `phi: s.phi`;
  `params` is passed through; `BoatProbe` is mounted next to `HeelProbe`
  (opt-in, see §5.3); `hull` and `sheetRig` are memoised (see §4, RV4).

`BoatSvg`'s `rig: RigDims` prop was removed — nothing read it after the
rewrite. `App.tsx` no longer builds it.

### Task 1.6 — The ordering reference (P-group B)

- **`web/tests/unit/boat3d.test.ts`** — `painter_matches_the_ray_reference`,
  `no_pop_across_a_full_roll`, `total_triangle_budget`,
  `projection_is_deterministic`, `survives_unwrapped_phi`, plus a check that
  the browser probes and this file use the same angle set.

### Task 1.7 — E2E (P-group B)

- **`web/tests/e2e/boat3d.spec.ts`** — seven tests, all seven of the required
  assertions plus `a topside appears on the rising side`.
- **`web/tests/e2e/sail.spec.ts`** — the two authorised assertion lines moved
  onto the boom line's own `x2`/`y2`, via a documented `boomTip` helper. The
  `starboardProjection > 0` assertion and every other assertion in the file are
  untouched; `git diff` is +19/−2 and confined to those two lines and the
  helper.

### Task 1.8 — The ninth gate step (P-group C, section agent)

Delta **D1** implemented: `pnpm --dir web test:unit` is step 8, Playwright is
step 9, steps 1–7 unchanged.

- `scripts/check.sh`, `scripts/check.ps1` — `step_names`, `run_step` /
  `$Steps`, `ValidateRange(1, 9)`, and `--fast` / `-Fast` now restrict **step
  9**; step 8 runs in full either way (it takes under a second).
- `CLAUDE.md` — the gate table is nine rows.
- `docs/v1/00-foundations.md` — **F12 only**; `git diff` touches five lines,
  all inside F12, and records `step 8 added 2026-09-20 by human approval`.
- `README.md` — the gate table, "nine steps", "step 9", and the now-redundant
  "not in the gate" entry for `test:unit`.
- `docs/v2/README.md` — the one sentence describing F12's old length. **This
  file is not in task 1.8's `Owns:` list**; see §5.2.

Historical documents were **not** rewritten: `docs/v1/acceptance.md`,
`docs/v1/progress/*.md` and the rest of `docs/v1/*.md` record what was true when
they were written.

---

## 2. Deviations from the PRD, and why

### 2.1 The keel ring's rocker also raises the chine ring

Task 1.2's table has the chine ring planar at `chineZ` and the rocker on the
keel ring alone. `ROCKER_RISE_PER_BEAM` (0.12) is larger than
`CANOE_DEPTH_PER_BEAM` (0.08), so raising only the keel lifts it **above** the
chine at bow and transom — 0.055 m above, at the ILCA defaults — which turns
the bilge strip inside out there. Measured: **4 of the 78 triangles fail
`normals_point_outward`** with the table taken literally.

Raising both rings by the same `rockerRise·(2·fx)²` keeps the canoe body's
depth constant end to end, which is what a rockered hull does, and every
acceptance criterion then passes. Recorded in `hullSolid.ts`'s header.

### 2.2 The keel cap is a transverse ladder of 11 triangles, not a 13-triangle fan

This one is a **provable contradiction inside the PRD**, and it decided the
shape of the hull. The two passages are:

> **Visibility.** […] the keel cap (outward normal `−ẑ_B`) is visible iff
> `cos φ < 0` — **the underside appears at exactly 90° of heel and not
> before**;

> **Task 1.2, ring table.** keel | outline × (`loa·keelLengthScale`,
> `beam·keelWidthScale`) | `keelZ + rockerRise·(2·fx)²`
> **Task 1.2, triangles.** keel cap — fan from the keel ring's centroid | 13

A triangle is visible iff `n_y·sin φ + n_z·cos φ > 0`. As `φ → π/2⁻` that sign
is `sign(n_y)`, so "hidden for every `|φ| < π/2`" requires `n_y ≤ 0` on every
keel-cap triangle; the hull's port/starboard mirror symmetry maps `n_y` to
`−n_y`, so it requires `n_y = 0` **exactly**. A centroid fan over a ring whose
`z` is quadratic in `x` cannot have that: `n_y = Δz₂Δx₃ − Δx₂Δz₃` vanishes only
when the two edges from the apex have equal `dz/dx`. Measured with the fan:
`|n_y|` up to 0.57, and the underside becomes visible at **φ ≈ 65°**, breaking
`underside_revealed_past_ninety` (1.4) and `the underside shows past ninety`
(1.7, which names the 60° probe explicitly).

Nor can the count be kept: a triangulation of a 13-gon with `k` interior
vertices has `13 + 2k − 2` triangles, so **13 triangles forces exactly one
interior vertex — the fan.** The two requirements are mutually exclusive.

The Visibility statement won: it is the section's stated contract, it is called
"exact", and three acceptance criteria rest on it against one bookkeeping
number. The keel cap is now a ladder — each rung joins a vertex to its mirror,
which shares its station and therefore its rocker height, so
`n_y = Δz·Δx − Δx·Δz = 0` identically. Measured: `|n_y| = 0` exactly on all 11,
`|n_y| ≤ 1.4e−16` on the 13 deck triangles, `|n_z| = 0` exactly on the 26
topsides. The three visibility rules are therefore exact, not approximate.

**Consequence for `triangle_count`:** the test asserts **76**, not the 78 the
table gives. The total model is **98** triangles, against the 144 budget.

### 2.3 Test files were not in any `Owns:` list

Tasks 1.1, 1.2 and 1.4 name acceptance criteria of the form
`pnpm --dir web test:unit <name>` but own only source files, so
`tests/unit/visualDims.test.ts`, `tests/unit/hullSolid.test.ts`,
`tests/unit/rigSolid.test.ts` and `tests/unit/project3d.test.ts` appear in no
`Owns:` list, contrary to the v2 README's rule that "every file written by the
section appears in exactly one `Owns:`". Each was assigned to the task whose
criteria name it. `tests/unit/ilca.ts` is new shared test data and belongs to
1.1. Worth fixing in the next PRD's task template.

### 2.4 Tasks were not delegated to subagents

The PRD permits ("may be delegated") rather than requires it. Groups A and B
were executed by the section agent, because §2.1 and §2.2 had to be settled
across `hullSolid.ts`, `project3d.ts` and two test files at once, and a
four-way split would have had each subagent rediscover the same contradiction.
Group order was still honoured, and `scripts/check.sh` was run between groups.

### 2.5 Small integration choices

- **`BoatSvg`'s `rig: RigDims` prop was removed.** Dead after the rewrite.
- **The `<g data-testid="boom">` wrapper has no `transform`**, exactly as the
  PRD directs.
- **Every face is stroked**, including interior edges. The PRD permits
  stroke-less interior edges ("may"); keeping them stroked makes the facets
  read as a low-poly form and guarantees the silhouette is stroked at zoom 0.1.

---

## 3. The numbers the section acceptance criteria ask for

### Triangle counts (final)

| Part | Triangles |
|---|---|
| deck cap | 13 |
| topsides | 26 |
| bilge | 26 |
| keel cap (ladder) | 11 |
| **hull solid** | **76** |
| sail patch (4 × 2 panels) | 12 |
| centreboard (3 strips) | 6 |
| rudder (2 strips) | 4 |
| **rig** | **22** |
| **total** | **98** (budget 144) |

No subdivision had to be raised to meet the 1.6 threshold.

### Painter's algorithm vs. the ray reference (4000 low-discrepancy samples)

| φ | agreement | rays |
|---|---|---|
| 0° | **100.00 %** | 2539 |
| 30° | 100.00 % | 1963 |
| −30° | 100.00 % | 1665 |
| 60° | 100.00 % | 1507 |
| −60° | 100.00 % | 1442 |
| 90° | 100.00 % | 1206 |
| −90° | 100.00 % | 1202 |
| 135° | 100.00 % | 1522 |
| 180° | **100.00 %** | 2542 |

Threshold 99.5 %, unweakened. It is 100 % everywhere because the rigid-body
argument holds and, after §2.2, the faces of each material tile disjointly in
projection at every angle sampled.

Largest visible-area step across a full roll in 720 steps: **0.764 %** of a
9.58 m² maximum (budget 2 %).

### The `svg` span, same machine, same run conditions

Chromium, `free_sail`, uniform wind, 8 s sample, `perf.spec.ts`'s
"where the frame goes".

| | p50 | p95 | max |
|---|---|---|---|
| **before** (`cb516ba`, via a throwaway worktree) | 0.900 ms | 1.500 ms | 3.100 ms |
| **after** | 1.300 ms | 3.700 ms | 7.000 ms |

Budget: `spans.svg.p95 < 12 ms`. The new renderer costs **+0.4 ms at the median
and +2.2 ms at the 95th percentile**. `wasm`, `frame` and `deck` are unchanged.
Sail Mode frame time, dropped frames and input lag all pass with their existing
budgets.

---

## 4. Risks that fired

**RV4 — ~100 SVG nodes per frame costs the 60 fps budget. FIRED, fixed.**
The first measurement was `svg` p95 = **17.8 ms** against the 12 ms budget. The
cause was not the node count: `App.tsx` built `hull` as a fresh object literal
every render, which invalidated `BoatSvg`'s model `useMemo` *and* defeated
`BoatProbe`'s `memo`, so ten boats' worth of geometry was rebuilt sixty times a
second. Memoising `hull` and `sheetRig` on their values took it to 3.7 ms. **No
budget was touched.** Lesson for the next section: the model memo is only as
good as the identity stability of the props it is keyed on.

**RV1 — painter's-algorithm ordering errors. Did not fire** (100 % at every
probe angle).

**RV3 — the new DOM breaks `sail.spec` / `sheet.spec`. Did not fire.**
`sheet.spec.ts` passes **unmodified** at every heel angle it visits, because
orthographic projection is linear and the boom line is drawn at its true
`z_boom`, so the interpolated attachment is exactly the projected attachment.
`sail.spec.ts` took only the authorised edit.

**RV5 — renumbering the gate's last step. Did not fire** beyond the one
unsatisfiable grep in §5.1.

**RV2, RV6 — did not fire.** No Rust changed; `git diff --name-only crates/` is
empty. D3 is still open, see §5.3.

---

## 5. What did not pass, and what needs a human

### 5.1 `grep -rn "eight steps" CLAUDE.md README.md docs/v2/` still matches — **unsatisfiable as written**

After 1.8 the phrase survives in exactly three places, all inside
`docs/v2/prds/01-boat-3d-svg.md`, which is not in 1.8's `Owns:` list:

- line 63 — a **verbatim quotation** of `docs/v1/progress/02-handoff.md`.
  Editing it would falsify a quotation, and the same task explicitly forbids
  rewriting historical documents.
- line 658 — the task's own instruction text ("the two places that say 'eight
  steps'").
- line 671 — the acceptance criterion's own text.

`CLAUDE.md`, `README.md` and `docs/v2/README.md` are clean. **No human ruling
needed** — the criterion simply cannot be met without the PRD deleting its own
wording. Worth writing future greps to exclude the PRD that states them.

### 5.2 `docs/v2/README.md` was edited without owning it

The same criterion greps `docs/v2/`, and that file's sentence "F12, which fixes
the chain at eight steps" became false the moment 1.8 landed. It was corrected
to past tense. This is an ownership deviation (F13.2) on a file no task claims;
flagging rather than hiding it. **Confirm or revert.**

### 5.3 `underside_revealed_past_ninety`'s third clause — **unsatisfiable as written**

> at `φ = π` the visible `underside` area equals the `deck` area at `φ = 0`
> within 1 %

The keel ring is the sheer outline narrowed to `KEEL_WIDTH_FRACTION` (0.30) and
shortened to `KEEL_LENGTH_FRACTION` (0.901), so the `underside` **material**
projects to 0.30 × 0.901 = **27 %** of the deck's plan-form by construction —
1.236 m² against 4.573 m². No geometry satisfying task 1.1's mandated constants
can satisfy this clause.

What is true, and exactly true, is the statement the clause was plainly
reaching for: **upside down, the boat presents the same plan-form it presented
the right way up.** The keel cap plus the bilge annulus around it tile the
sheer outline, so the visible *hull* area at `φ = π` is 4.57291341 m² and the
deck area at `φ = 0` is 4.57291341 m² — equal to the last digit printed. That
is what the test asserts, with the impossibility recorded in the test body, and
it additionally asserts that the whole keel cap is on show down there. **A human
should confirm this reading**, or say which constant should move instead.

### 5.4 Open item **V-B** / delta **D3** — still open

`project3d.ts` contains `R_x(φ)` as a **view** transform, on the `Camera.ts`
precedent. No human ruling has been recorded, so this shipped on the reading
the PRD states. The guard is in place and green:
`render_rotation_is_view_only` asserts that all five new modules import from
`../sim/` only with `import type`, and that none of them contains `advance(`,
`set_controls`, `Sim(` or `snapshot`. **Still needs the ruling.**

### 5.5 `pwsh scripts/check.ps1` was not run

No Windows host and no `pwsh` on this machine. `scripts/check.ps1` was edited in
step with `check.sh` — same nine steps, same `-Fast` semantics, `ValidateRange`
widened to `(1, 9)` — but it is **unverified**. Section acceptance criterion 1
allows for this ("if not, say so in the handoff"). First Windows run should
confirm it before relying on it.

### 5.6 `debug.spec.ts`'s 300-element budget, and why the probes are opt-in

The nine boat probes added ~400 SVG nodes to every page and took
`document.querySelectorAll('svg *')` from 145 to **741**, failing
`with every overlay on the page stays under 300 SVG elements` in all three
browsers. That budget is about the world view and the force overlay (brief §39),
not about test scaffolding, and `debug.spec.ts` is owned by no task here — so
**the application changed, not the test**: `BoatProbe` is mounted only when
`?probes=boat` or `window.__sailgymProbes` asks for it, which is also the right
production behaviour, since nine spare boats benefit nobody using the app.
`boat3d.spec.ts` sets the flag through `addInitScript` and still goes through
`gotoApp`, so the console-error trap is unchanged. Page total is now **184**.

`HeelProbe` is untouched and still always mounted: seven small sections are
56 nodes, not 400.

---

## 6. What the next section must know

1. **The three visibility rules are exact, and they are load-bearing.** A deck
   or keel-cap triangle has `n_y = 0`; a topside has `n_z = 0`. Any change to
   the sheer ring that makes its `z` non-linear in `x`, or to the keel cap that
   reintroduces a centroid fan, silently breaks
   `deck_cap_hidden_past_ninety`, `underside_revealed_past_ninety` and
   `topside_appears_on_the_rising_side`. `hullSolid.ts`'s header says so; read
   it before touching the rings.

2. **`projectModel` is globally sorted; the DOM is not, quite.** SVG paints in
   document order, and the DOM contract requires `boat-sail`, `boat-board` and
   `boat-rudder` to be single `<g>` elements, so each plate group paints
   contiguously, placed at the position of its lowest member. Task 1.6 measures
   the **draw list**, which is the algorithm. A section that migrates those
   specs can drop the grouping and the DOM becomes exactly the sorted list.

3. **Two debts carried forward, plus one new one.** The mast and boom are
   depth-sorted as single line elements (PRD's, to keep the `sail.spec` /
   `sheet.spec` DOM contract); there is no shading; and the plate grouping in
   (2) is the new one. All three are repaid by the same future migration.

4. **Memoise renderer props on their values.** See RV4 in §4. A fresh object
   literal in `App.tsx` costs 14 ms of frame time and no test catches it except
   `perf.spec.ts`.

5. **`sail.z_ce` is in `RenderParams` and unused** — the PRD's own tracked debt.
   The drawn sail's centre of area is not the core's CE; if a later section
   wants to draw the CE, that is the anchor to use.

6. **The gate is nine steps.** Anything that says "step 8" now means the vitest
   run. `docs/v1/*` was deliberately not back-dated, so historical references to
   "step 8" there mean Playwright and are correct for their date.

7. **Nothing in this section may reach the physics.** `crates/` is untouched,
   `parameters.rs` is untouched, the snapshot layout is untouched, and the
   `VISUAL` constants live in TypeScript precisely so that they cannot be
   confused with coefficients. No coefficient, physical or visual, was tuned to
   make anything look better.
