# Parameter provenance

Every editable leaf of the F7 catalogue, with its shipped value, its unit, its
honest tag and the reason it holds that value. The table is **generated from
`crates/sailgym-physics/src/parameters.rs`** — from the struct declarations and
the doc comments, by the same `parameters::catalogue()` the brief §31 panel is
built from — so a parameter added, retagged or re-documented in the source
appears here on the next run, and a parameter deleted disappears.

`crates/sailgym-physics/tests/provenance.rs::docs_match_source` compares the
generated region below with a fresh render and fails if the two differ. To
regenerate after a source change:

```
SAILGYM_UPDATE_DOCS=1 cargo test -p sailgym-physics --test provenance docs_match_source
```

---

## This prototype does not claim validated ILCA performance

**The numbers below do not constitute a validated model of an ILCA 7, and no
figure this simulator produces should be read as a prediction of how a real
boat behaves.** brief §36 says so directly — "Do not claim quantitative ILCA
performance accuracy yet" — and the tag column is where that statement is made
parameter by parameter rather than in general.

What the prototype *does* claim is what brief §36 asks of a v1:

1. public ILCA/Laser specifications for the basic dimensions;
2. physically motivated coefficients and deliberately simplified models;
3. analytical and symmetry invariants, enforced — `docs/v1/invariants.md`;
4. convergence tests — `docs/v1/convergence.md`;
5. behaviour inspected interactively — brief §29's Debug Mode;
6. every approximate coefficient isolated and configurable — this file, and the
   live panel it is generated alongside.

### The tags

| Tag | Means |
|---|---|
| **KNOWN** | From published ILCA/Laser class data or a physical constant. Changing it means you are no longer simulating an ILCA 7. |
| **ASSUMED** | A physically motivated estimate. Plausible, internally consistent, and **unvalidated**. Most of the catalogue is here, and that is the honest position for a prototype. |
| **TUNABLE** | Expected to be adjusted by playtesting or by fitting to data. Its present value is a starting point, not a result. |
| **DEFERRED** | The hook exists and is inert in v1. |

### Deferred validation routes (brief §36)

None of these has been done. Each is listed with what in this repository it
would replace, because brief §36's last requirement is that the architecture
make the replacement straightforward:

- **towing-tank resistance data** → the eight `resistance.*` coefficients of
  F6.6, which are one `hull_loads` function behind a named-parameter interface.
  The known limit is recorded as R6 and measured in
  `docs/v1/progress/04-handoff.md` §4: no planing regime, so resistance is
  over-predicted above ≈ 5 m/s, which `diagnostics.hull_model_warning` surfaces
  in the browser.
- **published Laser VPPs** → target boat speeds by true wind angle, which the
  native headless benchmark could sweep directly
  (`cargo run --release -p sailgym-bench --bin bench`).
- **CFD** → `sail.*`, `board.*` and `rudder.*` section coefficients, all nine of
  which feed the single F5 foil model and none of which is duplicated anywhere.
- **IMU / GNSS tack datasets** → the recorded-episode format of section 09,
  which already stores everything a tack comparison needs at 5–50 Hz and reads
  back bit-for-bit.
- **independent sailing simulators** → the six shipped scenarios and their
  golden trajectories.
- **instrumented real-boat experiments** → `stability.*`, which is the group
  with the largest known problem: see below.

### What is most wrong, in the authors' own estimation

Recorded here rather than left to be discovered, because an honest provenance
table has to include the parts that are not right yet:

1. **`stability.gm = 1.00 m` is inconsistent with `gz_max = 0.30 m` at
   `phi_peak = 45°`.** The fitted `GZ` peaks at 32.5° instead of 45° and then
   climbs back to +0.78 m at 140°, so past about 82° of heel the shipped boat is
   pushed back upright: it can be knocked down but cannot be sailed over and
   cannot be inverted by wind at any speed. `gm ≈ 0.55 m` makes the group
   self-consistent. Measured in full in `docs/v1/progress/07-handoff.md` §4.
   **Not changed** — it is a physical coefficient with a scenario effect and
   changing it invalidates all six golden trajectories, so it is the human's
   call (brief §43, F13.5).
2. **`sheet.l_sheet_min = 0.90 m` is shorter than the shortest geometric rope
   path, `ℓ(0) = 1.0404 m`,** so a fully hauled sheet carries ≈ 2.8 kN of
   permanent pre-tension and the boom is undamped by the sheet at `β = 0`.
   `docs/v1/progress/06-handoff.md` §4. **Not changed**, same reason.
3. **The hull has no planing regime** (R6, above).

## Parameters changed since F7 — the brief §43 record

**No F7 coefficient has been changed by any section.** That is not a claim
resting on the handoff notes: `provenance.rs::shipped_values_match_the_f7_table`
parses the F7 tables out of `docs/v1/00-foundations.md` and compares every numeric
row against the value `BoatParameters::ilca7()` actually ships. A coefficient
cannot move without `00-foundations.md` moving with it, and F13.1 puts that
edit with the human.

Two entries in the catalogue differ from the F7 *table*, and both are additions
or deletions rather than changes of value:

| Change | Section | Why | Recorded in |
|---|---|---|---|
| `sail.camber_blend` **added**, DEFERRED, 0.2 rad | 02 | F5.3's `FoilParams` requires `α_b` and the F7 table omits it. With `alpha_camber = 0` the camber hook is inert, so the value has no physical effect in v1; it is non-zero only so `tanh(α / α_b)` is defined at `α = 0`. Not a tuned coefficient — no scenario, test or visual depends on it. | `docs/v1/progress/02-handoff.md` §2.7 |
| `sim.scaffold_thrust` **deleted** | 04 | The M1 placeholder force model's constant thrust. Never physics, never in F7; task 4.5 mandates its deletion together with `forces/scaffold.rs`. R4 closed. | `docs/v1/progress/04-handoff.md` §2.9, §6 |

One tag changed, and no value with it:

| Change | Section | Why | Recorded in |
|---|---|---|---|
| `board.area` and `rudder.area` **retagged KNOWN → ASSUMED** | 10 | The three surfaces share one `FoilSection` declaration, whose `area` doc comment named two tags; `catalogue()` took the first and displayed KNOWN for all three. F7 tags `sail.area` KNOWN and the other two ASSUMED. `FoilSection`'s field list is pinned by F5.3, so `area` cannot be split out (F13.1); each owner now states the tag on its own `section` field and `catalogue()` resolves it. The values are untouched. | `docs/v1/progress/08-handoff.md` §5 (the defect), `docs/v1/progress/10-handoff.md` (the fix) |

## Constants that are not parameters

Three physical constants live in `constants.rs`, verbatim from F1, and are
KNOWN: `G = 9.80665 m/s²`, `RHO_AIR = 1.225 kg/m³` (ISA sea level, 15 °C) and
`RHO_WATER = 1025 kg/m³` (seawater; fresh water is 998.0).

A handful of named constants elsewhere in the crate are **not** physical
coefficients and deliberately do not appear below — degeneracy guards
(`foil::EPS_FLOW`, `mainsheet::EPS_ROPE`, `hydrostatics::EPS_DET`), resolutions
(`hydrostatics::SHAPE_SAMPLES`), the documented validity limit of the hull model
(`diagnostics::HULL_MODEL_VALID_TO`), and the wind field's own definitional
constants (`wind::MODE_BAND`, `wind::RMS_PER_VARIATION`). Each carries a doc
comment at its declaration, which `provenance.rs::no_stray_constants` requires,
and that test is what keeps any *other* loose number out of the source.

The **wind** is scenario configuration, not a boat parameter: F7 is the
catalogue of the boat, and each of the six shipped scenarios carries its own
field (`scenarios/*.json`). Wind defaults are in
`environment/wind.rs::WindConfig::default()`.

---

## The catalogue

<!-- BEGIN GENERATED — regenerate with `cargo test -p sailgym-physics --test provenance` -->

| Path | Value | Unit | Tag | Source or rationale |
|---|---|---|---|---|
| `hull.loa` | 4.23 | m | KNOWN | length overall, brief §3. |
| `hull.lwl` | 3.81 | m | KNOWN | waterline length, brief §3. |
| `hull.beam` | 1.37 | m | KNOWN | maximum beam, brief §3. |
| `hull.m_hull` | 58 | kg | KNOWN | brief §3; class minimum varies 56.7–59.0 by era/source. |
| `hull.m_sailor` | 80 | kg | KNOWN | brief §4, by definition. |
| `hull.sailor_pos_b.x` | 0 | m, in B | ASSUMED | amidships, on centreline (brief §4). The sailor does not hike and does not move; see R2. |
| `hull.sailor_pos_b.y` | 0 | m, in B | ASSUMED | amidships, on centreline (brief §4). The sailor does not hike and does not move; see R2. |
| `hull.sailor_pos_b.z` | 0.35 | m, in B | ASSUMED | amidships, on centreline (brief §4). The sailor does not hike and does not move; see R2. |
| `inertia.i_zz` | 155 | kg·m² | ASSUMED | `m·(0.25·LOA)²`. |
| `inertia.i_xx` | 28 | kg·m² | ASSUMED | roll radius of gyration 0.45 m about the CG. |
| `inertia.a_x` | 7 | kg | ASSUMED | surge added mass ≈ 5 % of Δ. |
| `inertia.a_y` | 100 | kg | ASSUMED | sway added mass, slender-body estimate. |
| `inertia.a_psi` | 60 | kg·m² | ASSUMED | yaw added inertia. |
| `inertia.a_phi` | 15 | kg·m² | ASSUMED | roll added inertia. |
| `resistance.x_u` | 5 | N·s/m | TUNABLE | linear surge resistance. |
| `resistance.x_uu` | 9 | N·s²/m² | TUNABLE | quadratic surge resistance. |
| `resistance.y_v` | 40 | N·s/m | TUNABLE | linear sway resistance. |
| `resistance.y_vv` | 512 | N·s²/m² | TUNABLE | quadratic sway resistance. |
| `resistance.n_r` | 250 | N·m·s | TUNABLE | linear yaw damping. |
| `resistance.n_rr` | 180 | N·m·s² | TUNABLE | quadratic yaw damping. |
| `resistance.k_p` | 60 | N·m·s | TUNABLE | linear roll damping. |
| `resistance.k_pp` | 40 | N·m·s² | TUNABLE | quadratic roll damping. |
| `sail.area` | 7.06 | m² | KNOWN | the ILCA 7 sail is 7.06 m² by class rule (brief §3). m². The reference area the F5.3 coefficients act on. |
| `sail.ar` | 3.7 | dimensionless | ASSUMED | aspect ratio; the board's is doubled for the free-surface mirror. |
| `sail.alpha_stall` | 0.262 | rad | TUNABLE | onset of the stall blend, F5.2 `α_s`. |
| `sail.stall_blend` | 0.105 | rad | TUNABLE | width of the stall blend, F5.2 `Δ_s`. |
| `sail.cn_max` | 1.8 | dimensionless | ASSUMED | flat-plate normal force, F5.2 `C_N,max`. |
| `sail.cd0` | 0.06 | dimensionless | ASSUMED | zero-lift profile drag. |
| `sail.oswald` | 0.85 | dimensionless | ASSUMED | Oswald span efficiency. |
| `sail.alpha_camber` | 0 | rad | DEFERRED | camber hook `α_0` of F5.2; zero in v1. |
| `sail.camber_blend` | 0.2 | rad | DEFERRED | camber blend width `α_b` of F5.2. Not tabulated in F7 because the hook is inert while `alpha_camber = 0`; it is non-zero only so `tanh(α / α_b)` stays defined at `α = 0`. |
| `sail.boom_length` | 2.72 | m | KNOWN | boom length. |
| `sail.d_ce` | 1.05 | m | ASSUMED | centre of effort along the boom from the mast, ≈ 0.38 × boom length. |
| `sail.z_ce` | 2.4 | m | ASSUMED | CE height above the CG. |
| `sail.mast_pos_b.x` | 1.2 | m, in B | ASSUMED | mast foot relative to the CG. |
| `sail.mast_pos_b.y` | 0 | m, in B | ASSUMED | mast foot relative to the CG. |
| `sail.mast_pos_b.z` | 0 | m, in B | ASSUMED | mast foot relative to the CG. |
| `sail.i_boom` | 12 | kg·m² | ASSUMED | boom 3 kg + sail 3 kg about the mast axis. |
| `sail.c_beta` | 2 | N·m·s/rad | TUNABLE | gooseneck friction, F6.9 `c_β`. |
| `sail.beta_max` | 1.745 | rad | ASSUMED | boom swing limit (100°), F6.9. |
| `sail.k_lim` | 400 | N·m/rad | TUNABLE | soft limit stiffness beyond `beta_max`. |
| `sail.c_lim` | 40 | N·m·s/rad | TUNABLE | soft limit damping beyond `beta_max`. |
| `board.area` | 0.2 | m² | ASSUMED | 0.20 m² is a plan-form estimate for the ILCA centreboard, not published class data. m². The reference area the F5.3 coefficients act on. |
| `board.ar` | 4.9 | dimensionless | ASSUMED | aspect ratio; the board's is doubled for the free-surface mirror. |
| `board.alpha_stall` | 0.209 | rad | TUNABLE | onset of the stall blend, F5.2 `α_s`. |
| `board.stall_blend` | 0.087 | rad | TUNABLE | width of the stall blend, F5.2 `Δ_s`. |
| `board.cn_max` | 1.9 | dimensionless | ASSUMED | flat-plate normal force, F5.2 `C_N,max`. |
| `board.cd0` | 0.012 | dimensionless | ASSUMED | zero-lift profile drag. |
| `board.oswald` | 0.9 | dimensionless | ASSUMED | Oswald span efficiency. |
| `board.alpha_camber` | 0 | rad | DEFERRED | camber hook `α_0` of F5.2; zero in v1. |
| `board.camber_blend` | 0.2 | rad | DEFERRED | camber blend width `α_b` of F5.2. Not tabulated in F7 because the hook is inert while `alpha_camber = 0`; it is non-zero only so `tanh(α / α_b)` stays defined at `α = 0`. |
| `board.pos_b.x` | 0.45 | m, in B | ASSUMED | centre of effort relative to the CG. |
| `board.pos_b.y` | 0 | m, in B | ASSUMED | centre of effort relative to the CG. |
| `board.pos_b.z` | -0.45 | m, in B | ASSUMED | centre of effort relative to the CG. |
| `rudder.area` | 0.105 | m² | ASSUMED | 0.105 m² is a plan-form estimate for the ILCA rudder blade, not published class data. m². The reference area the F5.3 coefficients act on. |
| `rudder.ar` | 3.9 | dimensionless | ASSUMED | aspect ratio; the board's is doubled for the free-surface mirror. |
| `rudder.alpha_stall` | 0.209 | rad | TUNABLE | onset of the stall blend, F5.2 `α_s`. |
| `rudder.stall_blend` | 0.087 | rad | TUNABLE | width of the stall blend, F5.2 `Δ_s`. |
| `rudder.cn_max` | 1.9 | dimensionless | ASSUMED | flat-plate normal force, F5.2 `C_N,max`. |
| `rudder.cd0` | 0.012 | dimensionless | ASSUMED | zero-lift profile drag. |
| `rudder.oswald` | 0.9 | dimensionless | ASSUMED | Oswald span efficiency. |
| `rudder.alpha_camber` | 0 | rad | DEFERRED | camber hook `α_0` of F5.2; zero in v1. |
| `rudder.camber_blend` | 0.2 | rad | DEFERRED | camber blend width `α_b` of F5.2. Not tabulated in F7 because the hook is inert while `alpha_camber = 0`; it is non-zero only so `tanh(α / α_b)` stays defined at `α = 0`. |
| `rudder.pos_b.x` | -2 | m, in B | ASSUMED | rudder centre of effort relative to the CG. |
| `rudder.pos_b.y` | 0 | m, in B | ASSUMED | rudder centre of effort relative to the CG. |
| `rudder.pos_b.z` | -0.28 | m, in B | ASSUMED | rudder centre of effort relative to the CG. |
| `rudder.delta_r_max` | 0.698 | rad | ASSUMED | maximum rudder deflection (40°), brief §13. |
| `rudder.delta_r_rate_max` | 2.09 | rad/s | TUNABLE | maximum commanded rudder rate (120°/s), brief §13. |
| `rudder.delta_r_return_rate` | 1.57 | rad/s | TUNABLE | rate at which the tiller returns to neutral (90°/s) when no steering key is held. |
| `rudder.delta_r_self_centre` | 1 | — | TUNABLE | whether the tiller self-centres at all (brief §13 leaves the choice to playtesting). Non-zero through `set_path` means `true`. |
| `sheet.k_sheet` | 20000 | N/m | TUNABLE | rope stiffness. R1: raising this above 3e4 is the most likely cause of a blow-up at `dt = 0.005`. |
| `sheet.c_sheet` | 300 | N·s/m | TUNABLE | rope damping. |
| `sheet.d_sheet` | 2.45 | m | ASSUMED | boom attachment distance from the mast, near the clew. |
| `sheet.z_boom` | 0.7 | m | ASSUMED | boom height above the CG. |
| `sheet.block_pos_b.x` | -2.1 | m, in B | ASSUMED | transom block position. |
| `sheet.block_pos_b.y` | 0 | m, in B | ASSUMED | transom block position. |
| `sheet.block_pos_b.z` | 0.1 | m, in B | ASSUMED | transom block position. |
| `sheet.l_sheet_min` | 0.9 | m | ASSUMED | shortest available sheet length. |
| `sheet.l_sheet_max` | 4.5 | m | ASSUMED | longest available sheet length. |
| `sheet.sheet_haul_rate` | 1.5 | m/s | TUNABLE | hauling rate under player command. |
| `sheet.sheet_ease_rate` | 3 | m/s | TUNABLE | easing rate under player command. |
| `sheet.sheet_release_rate` | 6 | m/s | TUNABLE | emergency release rate (Space), brief §12. |
| `stability.gm` | 1 | m | ASSUMED | metacentric height, the slope of `GZ` at `φ = 0`. |
| `stability.phi_peak` | 0.785 | rad | ASSUMED | angle of maximum righting arm (45°). |
| `stability.gz_max` | 0.3 | m | ASSUMED | maximum righting arm. |
| `stability.phi_vanish` | 1.396 | rad | ASSUMED | angle of vanishing stability (80°). |
| `stability.phi_capsize` | 1.396 | rad | TUNABLE | heel beyond which the capsize timer runs (80°). |
| `stability.t_capsize` | 1 | s | TUNABLE | how long `\|φ\| > phi_capsize` must hold before the boat is *reported* capsized. Reported, never acted on (F6.10). |
| `sim.dt` | 0.005 | s | TUNABLE | fixed physics timestep; brief §21 range 0.005–0.01. |
<!-- END GENERATED -->
