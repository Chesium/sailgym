import type { SimHandle } from './loadWasm'

/** Current-snapshot record; Rust owns all derived physical quantities. */
export interface Diagnostics {
  t: number
  steps: number
  apparent_wind_body: { x: number; y: number; z: number }
  apparent_wind_speed: number
  apparent_wind_angle: number
  speed_over_ground: number
  alpha_sail: number
  cl_sail: number
  cd_sail: number
  /** N, mainsheet tension; never negative (brief §11). */
  sheet_tension: number
  /** m, geometric rope path length `ℓ(β)` (F6.8). */
  rope_length: number
  /** m, `e = ℓ − L`; negative when the rope is slack. */
  sheet_extension: number
  /** rad, roll. Not wrapped (F3): an inversion reads past `±π`. */
  heel: number
  /** m, the righting arm `GZ(φ)` (F6.7). */
  gz: number
  /** N·m, `K_restore = −Δ·g·GZ(φ)`; negative for starboard-down heel. */
  k_restore: number
  /**
   * The capsize report (F6.10), `stability::capsize::CapsizeState` as the
   * diagnostics JSON carries it. Informational; nothing acts on it.
   *
   * Written inline rather than as a named interface so that this file's
   * top-level field list stays exactly the Rust record's — which is what
   * `tests/unit/sail.test.ts` compares across the boundary.
   */
  capsize: {
    /** `|φ| > phi_capsize` held for `t_capsize`. */
    capsized: boolean
    /** s, when the current capsize's threshold crossing happened. */
    since: number
    /** rad, the largest `|φ|` since the last reset; monotone in an episode. */
    max_heel: number
  }
}

/** The capsize report on its own, for components that only need it. */
export type CapsizeReport = Diagnostics['capsize']

export function readDiagnostics(sim: SimHandle): Diagnostics {
  return JSON.parse(sim.diagnostics() as string) as Diagnostics
}
