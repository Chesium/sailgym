/**
 * The TypeScript mirror of `crates/sailgym-physics/src/scenario.rs` and
 * `crates/sailgym-physics/src/recording.rs` (tasks 9.1, 9.3).
 *
 * The field lists must match the Rust declarations exactly, in both
 * directions: `tests/unit/scenarioTypes.test.ts` parses both sides and
 * compares them, exactly as `diagnostics.test.ts` does for the debug record
 * (task 8.1). A field added in Rust and forgotten here is a failing test
 * rather than a silently missing control.
 *
 * **Nothing here computes anything.** No degree is converted, no scenario is
 * authored and no episode is decoded on this side of the boundary: the six
 * documents come from `Sim.scenarios_json()`, the angle conversions live in
 * `Scenario::to_boat_state`, and the binary codec lives in `Episode`. This
 * file is types and one default id (F8).
 *
 * The helper shapes below are written on one line each on purpose: the parity
 * test reads top-level `  name:` declarations out of each interface, and a
 * multi-line helper would look like one of its fields.
 */

import type { SimHandle } from './loadWasm'

/**
 * The scenario the application loads when the URL asks for nothing.
 *
 * Mirrors `scenario::DEFAULT_SCENARIO`; the Rust test
 * `scenario::shipped::free_sail_is_default` reads this very declaration and
 * asserts the two agree.
 */
export const DEFAULT_SCENARIO = 'free_sail'

/** Mirrors `scenario::SCENARIO_SCHEMA_VERSION`. */
export const SCENARIO_SCHEMA_VERSION = 1

/** Mirrors `recording::EPISODE_SCHEMA_VERSION`. */
export const EPISODE_SCHEMA_VERSION = 1

/** `scenario::CameraMode`. Same spelling as `render/Camera.ts`'s own mode. */
export type ScenarioCameraMode = 'follow' | 'northUp'

/** `scenario::CameraSuggestion`. A suggestion: the player may override it. */
export interface CameraSuggestion { mode: ScenarioCameraMode; zoom: number }

/** `scenario::ControlsSpec` — the controls at `t = 0`, never a future script. */
export interface ControlsSpec { rudder_rate: number; sheet_rate: number; release: boolean }

/** `environment::wind::WindConfig`, as the scenario documents carry it. */
export interface ScenarioWind { mode: 'uniform' | 'spatial' | 'gust'; speed: number; bearing_deg: number; variation: number; length_scale: number; time_scale: number; modes: number; spectral_slope: number }

/**
 * `scenario::InitialState`. Degrees, because scenario JSON is a UI boundary
 * (F1); `Scenario::to_boat_state` converts them and nothing here may.
 */
export interface InitialState {
  /** m, world east. */
  x: number
  /** m, world north. */
  y: number
  /** deg, compass: clockwise from north. 90 is due east. */
  heading_deg: number
  /** deg, roll; positive is starboard down. */
  heel_deg: number
  /** m/s, initial forward speed. */
  speed: number
  /** deg, boom angle to port — the human-facing form of `β` (F2.1). */
  boom_deg_to_port: number
  /** m, available mainsheet length at the boom attachment. */
  sheet_length: number
}

/** `scenario::Scenario` — an initial condition and an environment (brief §32). */
export interface Scenario {
  schema_version: number
  /** The scenario id; equal to the file stem for the shipped six. */
  name: string
  description: string
  seed: number
  /** Sparse dotted-path overrides onto the ILCA 7 catalogue. */
  parameter_overrides: Record<string, number>
  initial_state: InitialState
  wind: ScenarioWind
  camera: CameraSuggestion
  initial_controls: ControlsSpec | null
}

/**
 * `recording::ToolchainInfo` (R7). Multi-line, unlike the helper shapes
 * above, because the parity test compares its fields with the Rust struct's.
 */
export interface ToolchainInfo {
  /** `rustc --version` output. */
  rustc: string
  /** The target triple the physics crate was compiled for. */
  target: string
  /** `debug` or `release`. */
  profile: string
}

/** `recording::EpisodeHeader`. */
export interface EpisodeHeader {
  schema_version: number
  scenario: Scenario
  /** The fully resolved F7 catalogue, not the sparse overrides. */
  parameters: Record<string, unknown>
  /** s, the fixed physics timestep the episode was produced at. */
  dt: number
  /** Hz, the logging rate — not the physics rate (brief §33). */
  log_hz: number
  toolchain: ToolchainInfo
  /** Metadata only; never read by physics (F9.1). */
  created_utc: string
}

/**
 * `recording::EpisodeFrame`. **Field order is normative** — it is the binary
 * layout.
 */
export interface EpisodeFrame {
  /** s, simulation time. */
  t: number
  /** The F3 state, in F8.3 order; 13 values. */
  state: number[]
  /** `rudder_rate`, `sheet_rate`, `release` as 0/1. */
  controls: number[]
  /** m/s, true wind at the boat, world frame. */
  wind_at_boat: number[]
  /** N, sail/board/rudder/hull force in `B`, xyz each; 12 values. */
  forces: number[]
  /** N·m: yaw, heel, righting, boom. */
  moments: number[]
  /** N, mainsheet tension. */
  sheet_tension: number
  /** Placeholder, always 0 in v1 (brief §33). */
  reward: number
  capsized: boolean
}

/** `recording::Episode`. */
export interface Episode {
  header: EpisodeHeader
  frames: EpisodeFrame[]
}

/** One row of the scenario picker. */
export interface ScenarioSummary {
  id: string
  description: string
  camera: CameraSuggestion
}

/**
 * The six shipped scenarios, straight from the core.
 *
 * One coarse-grained call for the whole catalogue (brief §24), and the
 * documents are the core's own — the browser never restates what a scenario
 * contains.
 */
export function readScenarios(sim: SimHandle): Scenario[] {
  return JSON.parse(sim.scenarios_json() as string) as Scenario[]
}

/** The scenario the current run started from. */
export function readCurrentScenario(sim: SimHandle): Scenario {
  return JSON.parse(sim.scenario_json() as string) as Scenario
}

/** The picker's rows, in the order the core lists them. */
export function summarise(scenarios: readonly Scenario[]): ScenarioSummary[] {
  return scenarios.map((s) => ({ id: s.name, description: s.description, camera: s.camera }))
}
