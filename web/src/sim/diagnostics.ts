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
}

export function readDiagnostics(sim: SimHandle): Diagnostics {
  return JSON.parse(sim.diagnostics() as string) as Diagnostics
}
