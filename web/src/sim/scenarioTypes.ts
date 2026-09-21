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
 * file is types, four mirrored constants and one discriminant check
 * ({@link recordedValue}, which reads a tag and returns what is behind it).
 * No physics, and no second version check (F8).
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

/** Mirrors `recording::EPISODE_SCHEMA_VERSION` — the schema this build writes. */
export const EPISODE_SCHEMA_VERSION = 2

/** Mirrors `recording::SUPPORTED_SCHEMA_VERSIONS` — the schemas it reads. */
export const SUPPORTED_SCHEMA_VERSIONS = [1, 2] as const

/** Mirrors `recording::IDENTITY_VERSION`. `0` means "no canonical record". */
export const IDENTITY_VERSION = 1

/** Mirrors `recording::PRACTICE_ENVELOPE_VERSION` (section 11). */
export const PRACTICE_ENVELOPE_VERSION = 1

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

/**
 * `recording::Recorded<T>` — a value that may be recorded, unrecorded, or
 * inapplicable (v2 F18.3).
 *
 * `'unknown'` and `'not_applicable'` are **not** the same thing and must not
 * be collapsed: the first says this document does not say, the second says the
 * concept does not exist for this episode. Only `{ value }` licenses a
 * comparison.
 */
export type Recorded<T> = { value: T } | 'unknown' | 'not_applicable'

/** The value, or `null` when the field is unknown or inapplicable. */
export function recordedValue<T>(field: Recorded<T> | undefined): T | null {
  return field !== undefined && typeof field === 'object' && 'value' in field
    ? field.value
    : null
}

/** `recording::identity::ModelIdentity` (v2 F18.1d). */
export interface ModelIdentity {
  model_version: number
  source: { tree: string; state: 'clean' | 'dirty' | 'unknown' }
}

/** `recording::TaskIdentity` (section 11). */
export interface TaskIdentity { id: string; version: number; thresholds: Record<string, number> }

/** `recording::ActionIdentity` (v2 F14.2, F14.6). */
export interface ActionIdentity { adapter: string; version: number; period_steps: number }

/** `recording::ObservationField` (v2 F14.3). */
export interface ObservationField { name: string; unit: string; normalisation: string; noise: number; privileged: boolean }

/** `recording::ObservationIdentity` (v2 F14.3). */
export interface ObservationIdentity { layout_version: number; fields: ObservationField[] }

/** `recording::PracticeEvent` — keyed by the physics step it was decided on. */
export interface PracticeEvent { id: string; step: number; t: number; value: number }

/** `recording::PracticeEnvelope` — section 11's reserved, typed envelope. */
export interface PracticeEnvelope { envelope_version: number; task: TaskIdentity; events: PracticeEvent[] }

// ---------------------------------------------------------------------------
// Guided practice (v2 section 11, F18.4)
// ---------------------------------------------------------------------------

/**
 * The shapes `Sim.practice_tasks_json` and `Sim.practice_state_json` return.
 *
 * They are **not** recording-document types, so the field-parity test above
 * does not cover them: the challenge list is assembled in
 * `crates/sailgym-wasm/src/lib.rs` from `sailgym_task::TaskSpec`'s public
 * accessors, and the report is `sailgym_task::TaskReport`. What the parity
 * test *does* cover is {@link TaskIdentity} and {@link PracticeEvent}, which
 * are the two things that travel inside an episode.
 *
 * Nothing on this side of the boundary decides an outcome, computes an
 * elapsed time or compares a threshold (F8, RV61). The page formats these
 * numbers and nothing more; every one of them was decided in Rust, on a
 * physics step.
 */

/** One shipped challenge, as `Sim.practice_tasks_json` lists it. */
export interface PracticeChallenge {
  /** `get_moving`, `complete_tack` or `recover_from_heel`. */
  id: string
  /** `TASK_VERSION`; bumped when a threshold or an outcome rule changes. */
  version: number
  /** The shipped scenario (brief §32) the challenge is set on. */
  scenario: string
  /** s, the attempt's limit. */
  time_limit_s: number
  /** The event id a result's **Inspect** action jumps to. */
  highlight_event: string
  /** The headline metric's stable id and its F1 unit. */
  metric: { id: string; unit: string }
  /** Every threshold, by name, in the task's own (SI) units. */
  thresholds: Record<string, number>
}

/** `sailgym_task::Outcome`. `reason` is present only on `failed`. */
export interface PracticeOutcome {
  kind: 'running' | 'succeeded' | 'failed' | 'timed_out'
  reason?: string
}

/** `sailgym_task::Progress` — the minimum to show while sailing. */
export interface PracticeProgress {
  phase: string
  value: number
  target: number
  hold_s: number
  hold_target_s: number
}

/** `sailgym_task::TaskReport`. */
export interface PracticeReport {
  task: TaskIdentity
  outcome: PracticeOutcome
  scenario: string
  /** s, simulated time since the attempt began — **not** wall time. */
  elapsed_s: number
  /** Physics steps since the attempt began (v2 F18.4). */
  elapsed_steps: number
  metric: { id: string; unit: string; value: number }
  progress: PracticeProgress
  events: PracticeEvent[]
  highlight: PracticeEvent | null
}

/**
 * How an attempt ended, when it was not the boat that ended it.
 *
 * `cancelled` — a reset, a restart or a scenario change. `conditions_changed`
 * — a parameter, catalogue or wind edit, which means no result it produced
 * could be compared with another attempt (RV63).
 */
export type PracticeStatus = 'active' | 'finished' | 'cancelled' | 'conditions_changed'

/** `Sim.practice_state_json`. */
export type PracticeState =
  | { active: false }
  | { active: true; status: PracticeStatus; report: PracticeReport }

/** The three challenges, straight from the core. One call (brief §24). */
export function readPracticeChallenges(sim: SimHandle): PracticeChallenge[] {
  return JSON.parse(sim.practice_tasks_json() as string) as PracticeChallenge[]
}

/** The attempt in progress, or `{ active: false }`. */
export function readPracticeState(sim: SimHandle): PracticeState {
  return JSON.parse(sim.practice_state_json() as string) as PracticeState
}

/** The last event with `id`, or `null`. Used for **Inspect** and for results. */
export function lastEvent(events: readonly PracticeEvent[], id: string): PracticeEvent | null {
  for (let i = events.length - 1; i >= 0; i -= 1) {
    if (events[i].id === id) {
      return events[i]
    }
  }
  return null
}

/**
 * `recording::EpisodeHeader`.
 *
 * The first seven fields are schema 1's. The rest arrived with schema 2 and
 * are `'unknown'` in a schema-1 document — never zero and never invented
 * (v2 F18.3).
 */
export interface EpisodeHeader {
  schema_version: number
  scenario: Scenario
  /** The fully resolved F7 catalogue, not the sparse overrides. */
  parameters: Record<string, unknown>
  /** s, the fixed physics timestep the episode was produced at. */
  dt: number
  /** Hz, the logging rate — not the physics rate (brief §33). Metadata. */
  log_hz: number
  toolchain: ToolchainInfo
  /** Metadata only; never read by physics (F9.1). */
  created_utc: string
  /** {@link IDENTITY_VERSION}, or `0` in a pre-identity document. */
  identity_version: number
  /** F18.1d's model and physics-source identity. */
  model: Recorded<ModelIdentity>
  /** The complete F3 state at the first recorded sample, in F8.3 order. */
  initial_state: Recorded<Record<string, number>>
  initial_controls: Recorded<Record<string, unknown>>
  /** Section 11's envelope; `null` in a hand-flown episode. */
  practice: PracticeEnvelope | null
  action: Recorded<ActionIdentity>
  observation: Recorded<ObservationIdentity>
}

/**
 * `recording::FrameDiagnostics` — the diagnostic subset recorded beside every
 * schema-2 sample. **Field order is normative** — it is the binary layout.
 *
 * `recording.rs`'s own doc comment lists, by name, the
 * `diagnostics::Diagnostics` fields that are deliberately **not** here; see
 * `docs/v2/recording-format.md` for the table.
 */
export interface FrameDiagnostics {
  /** m/s, true wind speed at the boat (`environment::wind_to_bearing`). */
  wind_speed: number
  /** deg, meteorological FROM bearing, clockwise from north. */
  wind_bearing_deg: number
  /** m/s, true wind in the horizontal body frame `H`; 2 values. */
  true_wind_body: number[]
  /** m/s, apparent wind at the CG, in `B`; 3 values (F6.2). */
  apparent_wind_body: number[]
  apparent_wind_speed: number
  /** rad, FROM angle off the bow, positive to starboard. */
  apparent_wind_angle: number
  /** m/s, `hypot(u, v)`. */
  speed_over_ground: number
  /** m/s², `(u̇, v̇)` in `H`; 2 values (F4.2). */
  acceleration_body: number[]
  /** N, `(ΣX, ΣY)` in `H`; 2 values. */
  total_force_h: number[]
  /** N, the pull on the boom at `P_b`, in `B`; 3 values (F6.8). */
  sheet_force: number[]
  /** m, the sail's centre of effort in `B`; 3 values. */
  sail_ce_b: number[]
  /** m, the centreboard's centre in `B`; 3 values. */
  board_centre_b: number[]
  /** m, the rudder's centre in `B`; 3 values. */
  rudder_centre_b: number[]
  /** m, `P_b(β)`, the mainsheet's boom attachment; 3 values (F6.8). */
  sheet_attach_b: number[]
  /** m, `P_k`, the block on the hull; 3 values (F6.8). */
  sheet_block_b: number[]
  alpha_sail: number
  alpha_board: number
  alpha_rudder: number
  /** m, `ℓ(β)`, the geometric rope path length (F6.8). */
  sheet_rope_length: number
  /** m, `e = ℓ − L`; negative when the rope is slack. */
  sheet_extension: number
  /** m, the righting arm `GZ(φ)` (F6.7). */
  gz: number
  /** s, `CapsizeState::since`. */
  capsize_since: number
  /** rad, `CapsizeState::max_heel`. */
  capsize_max_heel: number
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
  /** N·m: yaw, heel, righting, boom (the boom's total, F6.9). */
  moments: number[]
  /** N, mainsheet tension. */
  sheet_tension: number
  /** Placeholder, always 0 (brief §33). */
  reward: number
  capsized: boolean
  /**
   * Schema 2's diagnostic subset, captured at this sample's own state and
   * time. **Absent in a schema-1 frame**, and a replay must then show the
   * fields it would have carried as unavailable rather than evaluating the
   * model currently loaded (v2 F18.3, RV59).
   */
  diag?: FrameDiagnostics | null
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
