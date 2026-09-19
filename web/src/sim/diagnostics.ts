import type { SimHandle } from './loadWasm'

/** Current-snapshot M4 record; Rust owns all derived physical quantities. */
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
}

export function readDiagnostics(sim: SimHandle): Diagnostics {
  return JSON.parse(sim.diagnostics() as string) as Diagnostics
}
