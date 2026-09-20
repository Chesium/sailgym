/**
 * The F7 ILCA 7 defaults, as `parameters_json()` shapes them, for the unit
 * tests of v2 section 01.
 *
 * This is **test data**, not a parameter catalogue: nothing under `web/src/`
 * may contain a metre value (the section 02 rule, extended by D2), and nothing
 * here is imported by the application. The numbers mirror
 * `crates/sailgym-physics/src/parameters.rs`; if F7 changes, this fixture has
 * to follow, and the tests that assert scaling relations rather than absolute
 * metres are the ones that will survive that.
 *
 * The browser half of the section reads the same values across the F8 boundary
 * instead of restating them — see `web/tests/e2e/boat3d.spec.ts`.
 */

import type { RenderParams } from '../../src/sim/useSimulation'

/** The F7 defaults. `overrides` is merged one level deep, per group. */
export function ilcaParams(overrides: DeepPartial<RenderParams> = {}): RenderParams {
  const base: RenderParams = {
    hull: { loa: 4.23, beam: 1.37, lwl: 3.81 },
    sail: {
      area: 7.06,
      boom_length: 2.72,
      z_ce: 2.4,
      mast_pos_b: { x: 1.2, y: 0, z: 0 },
    },
    rudder: { pos_b: { x: -2.0, y: 0, z: -0.28 }, area: 0.105 },
    board: { pos_b: { x: 0.45, y: 0, z: -0.45 }, area: 0.2 },
    sheet: {
      d_sheet: 2.45,
      z_boom: 0.7,
      block_pos_b: { x: -2.1, y: 0, z: 0.1 },
      l_sheet_min: 1.0404326023342405,
      l_sheet_max: 4.5,
    },
  }
  return {
    hull: { ...base.hull, ...overrides.hull },
    sail: { ...base.sail, ...overrides.sail },
    rudder: { ...base.rudder, ...overrides.rudder },
    board: { ...base.board, ...overrides.board },
    sheet: { ...base.sheet, ...overrides.sheet },
  }
}

type DeepPartial<T> = { [K in keyof T]?: Partial<T[K]> }
