import type { SimHandle } from './loadWasm'

/**
 * The TypeScript mirror of `crates/sailgym-physics/src/diagnostics.rs`.
 *
 * The two field lists must match exactly, in both directions:
 * `tests/unit/diagnostics.test.ts` parses both files and compares them, so a
 * field added in Rust and forgotten here is a failing test rather than a
 * silently missing readout (task 8.1).
 *
 * Rust owns every derived physical quantity. Nothing in this file computes
 * one, and nothing may (F8) — the only conversions on this side of the
 * boundary are radians → degrees for display, in `units.ts`.
 *
 * The helper shapes below are written on one line each on purpose: the
 * parity tests read top-level `  name:` declarations out of the `Diagnostics`
 * interface, and a multi-line helper would look like one of its fields.
 */
export interface DiagVec2 { x: number; y: number }
export interface DiagVec3 { x: number; y: number; z: number }
/** `dynamics::Load` — a force in `B` and the point it acts at, in `B`. */
export interface DiagLoad { f: DiagVec3; r: DiagVec3 }
/** `rigging::boom::BoomMoments` — the four F6.9 terms about `+z_B`. */
export interface DiagBoomMoments { aero: number; sheet: number; damping: number; limit: number }
/** `stability::capsize::CapsizeState` (F6.10). Informational; nothing acts on it. */
export interface DiagCapsize { capsized: boolean; since: number; max_heel: number }

/** Everything brief §30 asks to be inspectable, for one published state. */
export interface Diagnostics {
  t: number
  steps: number

  /** m/s, true wind at the boat in the world frame — the direction the air blows *toward*. */
  true_wind_world: DiagVec2
  /** m/s, the same vector in the horizontal body frame `H`. */
  true_wind_body: DiagVec2
  /** m/s, apparent wind at the CG, in `B` (F6.2). */
  apparent_wind_body: DiagVec3
  apparent_wind_speed: number
  /** rad, FROM angle off the bow, positive to starboard. */
  apparent_wind_angle: number

  /** m/s, `(u, v)`: surge forward, sway to port (F3). */
  velocity_body: DiagVec2
  speed_over_ground: number
  /** rad, direction of travel in the world frame, CCW from world `+x`. */
  course_over_ground: number
  /** m/s², `(u̇, v̇)` (F4.2). */
  acceleration_body: DiagVec2
  yaw_rate: number
  roll_rate: number
  /** rad, drift angle; positive when the boat slides to starboard. */
  leeway_angle: number

  sail: DiagLoad
  board: DiagLoad
  rudder: DiagLoad
  hull: DiagLoad
  /** The pull on the boom at `P_b` (F6.8). */
  sheet: DiagLoad
  /** The reaction `−F_b` on the hull at the block `P_k` (F6.8). */
  sheet_hull: DiagLoad
  /** N, `(ΣX, ΣY)` in `H`. */
  total_force_h: DiagVec2

  /** N·m, `ΣN`. */
  yaw_moment: number
  /** N·m, everything in `ΣK` that is not the hydrostatic righting. */
  heeling_moment: number
  /** N·m, `K_restore = −Δ·g·GZ(φ)`; negative for starboard-down heel. */
  righting_moment: number
  boom_moment: DiagBoomMoments

  sail_ce_b: DiagVec3
  board_centre_b: DiagVec3
  rudder_centre_b: DiagVec3
  sheet_attach_b: DiagVec3
  sheet_block_b: DiagVec3

  alpha_sail: number
  cl_sail: number
  cd_sail: number
  alpha_board: number
  cl_board: number
  cd_board: number
  alpha_rudder: number
  cl_rudder: number
  cd_rudder: number

  /** rad, boom angle, positive to starboard (F2.1). */
  beta: number
  beta_dot: number
  /** N, mainsheet tension; never negative (brief §11). */
  sheet_tension: number
  /** m, geometric rope path length `ℓ(β)` (F6.8). */
  sheet_rope_length: number
  /** m, `e = ℓ − L`; negative when the rope is slack. */
  sheet_extension: number

  /** m, the righting arm `GZ(φ)` (F6.7). */
  gz: number
  /** deg, roll. Not wrapped (F3): an inversion reads past `±180°`. */
  heel_deg: number
  capsize: DiagCapsize

  /** J, translational + rotational + boom kinetic energy. */
  energy_kinetic: number
  /** J, `Δ·g·∫₀^φ GZ`. */
  energy_roll_potential: number
  /** J, `½k_sheet·max(0, e)²`. */
  energy_sheet_elastic: number
  /** R6: the hull resistance shown is an extrapolation above ≈ 5 m/s. */
  hull_model_warning: boolean
}

/** The capsize report on its own, for components that only need it. */
export type CapsizeReport = Diagnostics['capsize']

export function readDiagnostics(sim: SimHandle): Diagnostics {
  return JSON.parse(sim.diagnostics() as string) as Diagnostics
}

// ---------------------------------------------------------------------------
// Availability — the replay half (v2 section 10, F18.3)
// ---------------------------------------------------------------------------

/**
 * A diagnostics record that may be missing fields.
 *
 * A **live** record has every one; a record reconstructed from a recorded
 * sample has the subset that episode's schema carried, and a schema-1 episode
 * carries none of the diagnostics block at all
 * (`docs/v2/recording-format.md` §4).
 *
 * The type is what enforces the rule. Every field reads `T | undefined`, so a
 * consumer cannot use one without deciding what to show when it is absent, and
 * it cannot quietly substitute a live value or a zero — RV59 is the defect
 * this shape exists to prevent, and the compiler is the guard.
 */
export type PartialDiagnostics = Partial<Diagnostics>

/**
 * The one set of diagnostic numbers the whole page is drawn from this frame.
 *
 * `source` and `t` travel with the values because a replay's diagnostics come
 * from the **preceding recorded sample**, not from the playhead: interpolating
 * a force is not a force calculation, so the sample's own timestamp is shown
 * rather than implied (v2 section 10, task 10.3).
 */
// Written on one line, by the same convention as the helper shapes at the top
// of this file: the two parity tests read top-level `  name:` declarations out
// of this file, and a multi-line shape here would look like a `Diagnostics`
// field. `source` is 'live' or 'recorded'; `t` is the simulated time the
// values describe; `values` are the fields present.
export interface DiagnosticsSample { source: 'live' | 'recorded'; t: number; values: PartialDiagnostics }

/** What a readout shows in place of a value the episode does not carry. */
export const NOT_RECORDED = 'Not recorded'

/** The live record, as the shared sample shape. Nothing is dropped. */
export function liveDiagnosticsSample(d: Diagnostics): DiagnosticsSample {
  return { source: 'live', t: d.t, values: d }
}

/**
 * Whether a field is present.
 *
 * A separate helper rather than `!== undefined` at each call site, so "is this
 * recorded?" reads the same everywhere and `null` — which a JSON document can
 * produce — counts as absent too.
 */
export function isRecorded<K extends keyof Diagnostics>(values: PartialDiagnostics, key: K): values is PartialDiagnostics & Required<Pick<Diagnostics, K>> {
  return values[key] !== undefined && values[key] !== null
}
