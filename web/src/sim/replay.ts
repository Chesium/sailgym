/**
 * The one live-or-replay display selection (brief §33, v1 task 9.4, v2
 * section 10 task 10.3).
 *
 * **A replay consumes stored episode data; it does not recompute the
 * physics.** Nothing in this file touches the `Sim`: it indexes frames the
 * recorder produced, and the renderer reads a replay frame exactly as it reads
 * a live snapshot, so the same components work in both modes.
 *
 * `replay.spec.ts` proves the "does not recompute" claim the only way it can
 * be proved — by editing one frame in memory and watching the render follow
 * the edit. `replay-truth.spec.ts` proves the complementary half: that
 * altering the live run conspicuously changes nothing in the replay view.
 *
 * ## One selection, not one per panel
 *
 * {@link selectInspection} is called **once** per render and its
 * {@link InspectionView} is handed to every consumer — the boat, the HUD, the
 * force overlay, the charts, the debug panel, the wind readout. Before v2
 * section 10, `App.tsx` selected a recorded *pose* while the HUD, the wind,
 * the force overlay, the charts and the diagnostics panel went on reading the
 * live simulator, so a replay showed one episode's boat inside another run's
 * numbers (RV57). The fix is this boundary, not a patch per panel.
 *
 * ## Unavailable is a value, not a zero
 *
 * A schema-1 episode carries no diagnostics block at all, and schema 2 carries
 * a deliberate subset (`docs/v2/recording-format.md` §4). Everything else is
 * **unavailable**: {@link diagnosticsFromFrame} simply does not set the field,
 * {@link PartialDiagnostics} makes the compiler force every consumer to decide
 * what to show, and the answer is `Not recorded` — never a zero, never the
 * live simulation's number, and never a fresh evaluation of the model
 * currently loaded (v2 F18.3, RV59).
 *
 * ## What the interpolation is, and is not
 *
 * {@link ReplaySource.sampleAt} exists so scrubbing looks smooth between the
 * logged samples. It is **display only** and is not physics (F8): no equation
 * of motion, no coefficient and no integration happens here, and an
 * interpolated frame is never fed back into the core. Exactly at a frame's
 * own time it returns that frame's values unchanged.
 *
 * Three kinds of field are deliberately *not* lerped:
 *
 * * `controls` and `capsized` are **commands and reports**, not continuous
 *   signals. Averaging a rudder command of −1 with one of +1 would display a
 *   centred rudder at an instant when the helm was never centred, so both are
 *   held at the earlier frame's value until the later frame's time arrives.
 * * `psi` and `beta` are wrapped to `(−π, π]` (F3), so they are interpolated
 *   along the **shorter arc**: a boat swinging through due west must not
 *   appear to spin the long way round when the recorded value steps from
 *   `+3.14` to `−3.14`. `phi` is not wrapped (F3) and is lerped plainly.
 * * **Diagnostics are not interpolated at all.** A force is the output of an
 *   evaluation at a state, and the average of two evaluations is not an
 *   evaluation; presenting one as an exact calculation is the misreading this
 *   section exists to prevent. {@link InspectionView.diagnostics} therefore
 *   carries the **preceding recorded sample**, with that sample's own
 *   timestamp beside it.
 */

import {
  isRecorded,
  type DiagLoad,
  type DiagVec2,
  type DiagVec3,
  type Diagnostics,
  type DiagnosticsSample,
  type PartialDiagnostics,
} from './diagnostics'
import type { Episode, EpisodeFrame, EpisodeHeader } from './scenarioTypes'
import { SNAPSHOT_FIELDS, type Snapshot } from './snapshot'
import { radiansToDegrees } from './units'
import type { WindReading } from '../ui/WindReadout'

/** Indices into `EpisodeFrame.state` that carry an angle wrapped to (−π, π]. */
const WRAPPED_STATE_INDICES: readonly number[] = [
  SNAPSHOT_FIELDS.indexOf('psi'),
  SNAPSHOT_FIELDS.indexOf('beta'),
]

const PHI = SNAPSHOT_FIELDS.indexOf('phi')
const U = SNAPSHOT_FIELDS.indexOf('u')
const V = SNAPSHOT_FIELDS.indexOf('v')
const R = SNAPSHOT_FIELDS.indexOf('r')
const P = SNAPSHOT_FIELDS.indexOf('p')
const BETA = SNAPSHOT_FIELDS.indexOf('beta')
const BETA_DOT = SNAPSHOT_FIELDS.indexOf('betaDot')

/** Wrap an angle to `(−π, π]`. Bookkeeping for the shorter arc, not physics. */
export function wrapPi(a: number): number {
  const twoPi = 2 * Math.PI
  let x = (a + Math.PI) % twoPi
  if (x <= 0) {
    x += twoPi
  }
  return x - Math.PI
}

/** Linear interpolation, `alpha` in `[0, 1]`. */
function lerp(a: number, b: number, alpha: number): number {
  return a + (b - a) * alpha
}

/** Interpolate along the shorter arc between two wrapped angles. */
function lerpAngle(a: number, b: number, alpha: number): number {
  return wrapPi(a + wrapPi(b - a) * alpha)
}

function lerpArray(a: readonly number[], b: readonly number[], alpha: number): number[] {
  return a.map((v, i) => lerp(v, b[i], alpha))
}

/** A replayable episode. */
export interface ReplaySource {
  header: EpisodeHeader
  frameCount: number
  /** Simulated time of the first frame; `0` for an empty episode. */
  startTime: number
  /** Simulated time of the last frame; `0` for an empty episode. */
  endTime: number
  /** Hz, the logging rate the episode was recorded at — its resolution. */
  logHz: number
  /** The episode's declared schema. `1` carries no diagnostics block. */
  schemaVersion: number
  /**
   * The stored frame at `i`, clamped to the episode, or `null` when it is
   * empty.
   *
   * Returns the **stored object**, not a copy: an episode inspector — and
   * `replay.spec.ts` — must be able to see that the renderer is reading these
   * frames and nothing else.
   */
  frameAt(i: number): EpisodeFrame | null
  /** The index of the last frame at or before `t`; `-1` when empty. */
  indexAt(t: number): number
  /**
   * Linear interpolation between frames for smooth scrubbing. Display only,
   * and `null` when the episode is empty.
   */
  sampleAt(t: number): EpisodeFrame | null
}

export type PlaybackMode = { kind: 'live' } | { kind: 'replay'; source: ReplaySource }

/** `{ kind: 'live' }`, as a shared constant so React sees a stable identity. */
export const LIVE: PlaybackMode = { kind: 'live' }

/**
 * Wrap a decoded episode as a replay source.
 *
 * **An empty episode is a source with `frameCount = 0`, not an exception.** A
 * throw here would have to be caught by every caller and would leave the
 * transport with nothing to render; instead the accessors return `null`, the
 * timeline renders disabled, and the page says the episode has no samples
 * (v2 section 10, task 10.4).
 */
export function createReplaySource(episode: Episode): ReplaySource {
  const frames = episode.frames
  const empty = frames.length === 0
  const first = empty ? null : frames[0]
  const last = empty ? null : frames[frames.length - 1]

  const frameAt = (i: number): EpisodeFrame | null => {
    if (empty) {
      return null
    }
    const clamped = Math.min(frames.length - 1, Math.max(0, Math.trunc(i)))
    return frames[clamped]
  }

  /** The last index whose `t` is at or before `t`. Binary search. */
  const indexAt = (t: number): number => {
    if (first === null || last === null) {
      return -1
    }
    if (!(t > first.t)) {
      return 0
    }
    if (t >= last.t) {
      return frames.length - 1
    }
    let lo = 0
    let hi = frames.length - 1
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1
      if (frames[mid].t <= t) {
        lo = mid
      } else {
        hi = mid
      }
    }
    return lo
  }

  const sampleAt = (t: number): EpisodeFrame | null => {
    const i = indexAt(t)
    if (i < 0) {
      return null
    }
    const a = frames[i]
    if (i === frames.length - 1) {
      return a
    }
    const b = frames[i + 1]
    const span = b.t - a.t
    // Exactly on a frame, or two frames sharing a timestamp: the stored
    // values, unchanged.
    if (!(span > 0) || t <= a.t) {
      return a
    }
    const alpha = Math.min(1, (t - a.t) / span)
    if (alpha === 0) {
      return a
    }

    const state = lerpArray(a.state, b.state, alpha)
    for (const k of WRAPPED_STATE_INDICES) {
      state[k] = lerpAngle(a.state[k], b.state[k], alpha)
    }
    return {
      t: lerp(a.t, b.t, alpha),
      state,
      // Held, not averaged — see the note at the top of this file.
      controls: [...a.controls],
      wind_at_boat: lerpArray(a.wind_at_boat, b.wind_at_boat, alpha),
      forces: lerpArray(a.forces, b.forces, alpha),
      moments: lerpArray(a.moments, b.moments, alpha),
      sheet_tension: lerp(a.sheet_tension, b.sheet_tension, alpha),
      reward: lerp(a.reward, b.reward, alpha),
      capsized: a.capsized,
      // **Not interpolated and not carried.** An interpolated diagnostics
      // block would be an evaluation nobody made; the inspection view takes
      // the preceding recorded sample's block instead, and says which sample
      // it came from.
      diag: null,
    }
  }

  return {
    header: episode.header,
    frameCount: frames.length,
    startTime: first?.t ?? 0,
    endTime: last?.t ?? 0,
    logHz: episode.header.log_hz,
    schemaVersion: episode.header.schema_version,
    frameAt,
    indexAt,
    sampleAt,
  }
}

/**
 * A replay frame's state as the renderer's {@link Snapshot}.
 *
 * The frame carries the F3 state in F8.3 order — the same layout
 * `Sim.snapshot()` returns — so this is the index map and nothing more.
 */
export function snapshotFromFrame(frame: EpisodeFrame): Snapshot {
  const out = {} as Snapshot
  for (let i = 0; i < SNAPSHOT_FIELDS.length; i += 1) {
    out[SNAPSHOT_FIELDS[i]] = frame.state[i]
  }
  return out
}

// ---------------------------------------------------------------------------
// Recorded frame → diagnostics
// ---------------------------------------------------------------------------

function v2(a: readonly number[], at = 0): DiagVec2 {
  return { x: a[at], y: a[at + 1] }
}

function v3(a: readonly number[], at = 0): DiagVec3 {
  return { x: a[at], y: a[at + 1], z: a[at + 2] }
}

/** `F6.6`: the hull force acts at the CG, so its arm is structurally zero. */
const AT_THE_CG: DiagVec3 = { x: 0, y: 0, z: 0 }

function load(f: DiagVec3, r: DiagVec3): DiagLoad {
  return { f, r }
}

/**
 * The diagnostics a recorded sample carries, and only those.
 *
 * Two tiers, exactly as `docs/v2/recording-format.md` §4 describes them:
 *
 * * **Always** — the quantities a schema-1 frame already held. The five that
 *   come straight out of the F3 state are re-read from it, which is an index
 *   and not a recomputation; `heel_deg` is `φ` in degrees, which F1 permits at
 *   a UI boundary and `units.ts` is the one implementation of.
 * * **Only with the block** — everything schema 2 added. A schema-1 frame
 *   leaves every one of them unset, so the compiler makes each consumer say
 *   `Not recorded`.
 *
 * The four component loads need **both** a force and an arm. The forces are
 * schema-1 fields; the three arms are in the block, so sail, board and rudder
 * appear only with it. The hull's arm is `Vec3::ZERO` by F6.6 — a property of
 * the model, not a measurement — and the sheet's arm is its boom attachment,
 * which the block records.
 */
export function diagnosticsFromFrame(frame: EpisodeFrame): PartialDiagnostics {
  const s = frame.state
  const values: PartialDiagnostics = {
    t: frame.t,
    true_wind_world: v2(frame.wind_at_boat),
    velocity_body: { x: s[U], y: s[V] },
    yaw_rate: s[R],
    roll_rate: s[P],
    beta: s[BETA],
    beta_dot: s[BETA_DOT],
    heel_deg: radiansToDegrees(s[PHI]),
    sheet_tension: frame.sheet_tension,
    yaw_moment: frame.moments[0],
    heeling_moment: frame.moments[1],
    righting_moment: frame.moments[2],
  }

  const d = frame.diag
  if (d === undefined || d === null) {
    return values
  }
  return {
    ...values,
    true_wind_body: v2(d.true_wind_body),
    apparent_wind_body: v3(d.apparent_wind_body),
    apparent_wind_speed: d.apparent_wind_speed,
    apparent_wind_angle: d.apparent_wind_angle,
    speed_over_ground: d.speed_over_ground,
    acceleration_body: v2(d.acceleration_body),
    sail: load(v3(frame.forces, 0), v3(d.sail_ce_b)),
    board: load(v3(frame.forces, 3), v3(d.board_centre_b)),
    rudder: load(v3(frame.forces, 6), v3(d.rudder_centre_b)),
    hull: load(v3(frame.forces, 9), AT_THE_CG),
    sheet: load(v3(d.sheet_force), v3(d.sheet_attach_b)),
    total_force_h: v2(d.total_force_h),
    sail_ce_b: v3(d.sail_ce_b),
    board_centre_b: v3(d.board_centre_b),
    rudder_centre_b: v3(d.rudder_centre_b),
    sheet_attach_b: v3(d.sheet_attach_b),
    sheet_block_b: v3(d.sheet_block_b),
    alpha_sail: d.alpha_sail,
    alpha_board: d.alpha_board,
    alpha_rudder: d.alpha_rudder,
    sheet_rope_length: d.sheet_rope_length,
    sheet_extension: d.sheet_extension,
    gz: d.gz,
    capsize: {
      capsized: frame.capsized,
      since: d.capsize_since,
      max_heel: d.capsize_max_heel,
    },
  }
}

/**
 * The wind at the boat, as a recorded sample carries it.
 *
 * The **vector** is a schema-1 field, so it is always there. The speed and the
 * meteorological FROM bearing are not: they come from
 * `environment::wind_to_bearing`, which exists in exactly one place (F6.1),
 * and deriving them here would be that second implementation. A schema-1
 * episode therefore shows its recorded vector and says that the speed and the
 * bearing are not recorded.
 */
export function windFromFrame(frame: EpisodeFrame): WindReading {
  const d = frame.diag
  return {
    source: 'recorded',
    wx: frame.wind_at_boat[0],
    wy: frame.wind_at_boat[1],
    speed: d === undefined || d === null ? null : d.wind_speed,
    bearingDeg: d === undefined || d === null ? null : d.wind_bearing_deg,
  }
}

// ---------------------------------------------------------------------------
// The spatial wind field
// ---------------------------------------------------------------------------

/** Whether the dense wind field may be drawn, and why not when it may not. */
export interface WindFieldAvailability {
  available: boolean
  /** One sentence, shown on the page. Empty when it is available. */
  reason: string
}

/**
 * Whether a replay may draw the spatial wind field.
 *
 * **The recorded vector at the boat does not identify the field.** Two
 * different `WindConfig`s, two seeds or two builds of `environment::wind.rs`
 * can produce the same vector at one point and disagree everywhere else, so
 * drawing the *live* field behind a recorded boat would be exactly the mixed
 * timeline RV57 names. The conditions are checked in order and the first
 * failure is the reason shown:
 *
 * 1. the episode must carry a canonical identity at all — a schema-1 document
 *    does not, so it cannot say which field it was sailed in;
 * 2. the identity's model must name a baseline (F18.1d) — a dirty or unknown
 *    physics source means the wind kernel that produced the episode is not
 *    known to be the one loaded now;
 * 3. and this build must be able to reconstruct the field from that
 *    configuration, which it **cannot**: doing so needs a second wind field
 *    instantiated from the episode's own configuration and seed, and the only
 *    field this page holds belongs to the live simulation. Section 10 does not
 *    build one, and reusing the live one is the defect, not the feature.
 *
 * The third reason is the one a schema-2 episode gets, and it is deliberately
 * a statement about this build rather than about the episode.
 */
export function replayWindField(header: EpisodeHeader): WindFieldAvailability {
  if (header.identity_version === 0) {
    return {
      available: false,
      reason:
        'this episode is schema 1 and records no wind identity, so the field it was sailed in is unknown',
    }
  }
  const model = typeof header.model === 'object' ? header.model.value : null
  if (model === null || model.source.state !== 'clean') {
    return {
      available: false,
      reason:
        'this episode names no physics baseline, so the wind kernel that produced it is unknown',
    }
  }
  return {
    available: false,
    reason:
      'this build does not reconstruct a recorded wind field; the live field belongs to the live run',
  }
}

// ---------------------------------------------------------------------------
// The one display selection
// ---------------------------------------------------------------------------

/** The controls in force, as a frame or the live input holds them. */
export interface InspectedControls {
  rudderRateCmd: number
  sheetRateCmd: number
  release: boolean
}

/**
 * Everything the page draws this frame, chosen once.
 *
 * Every consumer takes its numbers from here. Nothing reads the live
 * simulation directly while `source` is `'replay'`, and the fields that a
 * replay cannot supply are `null` or absent rather than borrowed.
 */
export interface InspectionView {
  source: 'live' | 'replay'
  /** The pose and actuator state the whole page is drawn from. */
  snapshot: Snapshot
  /** s, the time of the pose. In replay, the playhead. */
  t: number
  /** The pose is interpolated between two recorded samples. */
  interpolated: boolean
  /** The recorded sample the diagnostics came from, or `null` when live. */
  sampleIndex: number | null
  /** Live, or the preceding recorded sample. `null` before the first frame. */
  diagnostics: DiagnosticsSample | null
  /** Held from the preceding sample in replay; `null` when unknown. */
  controls: InspectedControls | null
  /** Wind at the boat, recorded or live; `null` when unknown. */
  wind: WindReading | null
  /** Whether the dense wind field may be drawn. */
  windField: WindFieldAvailability
  /** `Diagnostics` keys this view cannot supply, sorted. */
  unavailable: readonly string[]
  /** The episode has no samples at all. */
  emptyEpisode: boolean
}

/** Every `Diagnostics` key, taken from a live record so the list cannot drift. */
function keysOf(values: PartialDiagnostics): string[] {
  return Object.keys(values)
}

/**
 * The keys a live record has and this view does not.
 *
 * Computed against the live key list rather than against a constant, so a
 * field added to `diagnostics.rs` turns up here as unavailable in replay the
 * moment the WASM package is rebuilt — with no edit to this file.
 */
function missingFrom(
  values: PartialDiagnostics,
  reference: readonly string[],
): string[] {
  return reference
    .filter((k) => !isRecorded(values, k as keyof Diagnostics))
    .sort((a, b) => a.localeCompare(b))
}

/** Live: everything the simulation publishes, and the live wind field. */
export function liveInspection(
  snapshot: Snapshot,
  diagnostics: Diagnostics | null,
  wind: WindReading | null,
): InspectionView {
  return {
    source: 'live',
    snapshot,
    t: snapshot.t,
    interpolated: false,
    sampleIndex: null,
    diagnostics:
      diagnostics === null
        ? null
        : { source: 'live', t: diagnostics.t, values: diagnostics },
    controls: null,
    wind,
    windField: { available: true, reason: '' },
    unavailable: [],
    emptyEpisode: false,
  }
}

/**
 * Replay: the pose at the playhead, every number from the episode.
 *
 * The pose is interpolated so scrubbing is smooth; the diagnostics, the
 * controls and the capsize report come from the **preceding recorded sample**
 * and carry its timestamp, because holding a discrete value and averaging a
 * force evaluation are not the same operation.
 *
 * `liveKeys` is the live `Diagnostics` key list, handed in so
 * {@link InspectionView.unavailable} is measured against what a live record
 * actually has rather than against a list written here.
 */
export function replayInspection(
  source: ReplaySource,
  t: number,
  liveKeys: readonly string[],
): InspectionView {
  const windField = replayWindField(source.header)
  const pose = source.sampleAt(t)
  const index = source.indexAt(t)
  const sample = index < 0 ? null : source.frameAt(index)

  if (pose === null || sample === null) {
    return {
      source: 'replay',
      // Nothing recorded, so nothing is drawn from a recording: the caller
      // keeps the pose it already had and the page says the episode is empty.
      snapshot: {} as Snapshot,
      t,
      interpolated: false,
      sampleIndex: null,
      diagnostics: null,
      controls: null,
      wind: null,
      windField,
      unavailable: [...liveKeys].sort((a, b) => a.localeCompare(b)),
      emptyEpisode: true,
    }
  }

  const values = diagnosticsFromFrame(sample)
  return {
    source: 'replay',
    snapshot: snapshotFromFrame(pose),
    t: pose.t,
    // Exactly on a sample the two times agree to the bit, which is what makes
    // this a fact about the playhead rather than a tolerance.
    interpolated: pose.t !== sample.t,
    sampleIndex: index,
    diagnostics: { source: 'recorded', t: sample.t, values },
    controls: {
      rudderRateCmd: sample.controls[0],
      sheetRateCmd: sample.controls[1],
      release: sample.controls[2] !== 0,
    },
    wind: windFromFrame(sample),
    windField,
    unavailable: missingFrom(values, liveKeys),
    emptyEpisode: false,
  }
}

/**
 * The one call `App.tsx` makes.
 *
 * `liveDiagnostics` is used for two things and two only: as the live view's
 * numbers, and — in replay — as the **key list** the unavailable set is
 * measured against. Its values never reach a replay view, which is what
 * `replay-truth.spec.ts` asserts by changing them conspicuously.
 */
export function selectInspection(
  playback: PlaybackMode,
  replayTime: number,
  live: { snapshot: Snapshot; diagnostics: Diagnostics | null; wind: WindReading | null },
): InspectionView {
  if (playback.kind === 'live') {
    return liveInspection(live.snapshot, live.diagnostics, live.wind)
  }
  const keys = live.diagnostics === null ? [] : keysOf(live.diagnostics)
  return replayInspection(playback.source, replayTime, keys)
}

/**
 * The replay probe the E2E suite drives (tasks 9.4, 9.5, 10.5), installed on
 * `window.__sailgym` by `App.tsx`.
 *
 * It exists so `replay.spec.ts` can exercise the export/import path without
 * a real file dialog, and so it can *edit a stored frame* and watch the
 * render follow — which is the only way to prove a replay consumes stored
 * data rather than recomputing it (brief §33). Nothing in the application
 * reads it, and it is `undefined` until the app has mounted.
 */
export interface ReplayProbe {
  /** The episode currently held, as its JSON document. */
  episodeJson(): string | null
  /** The same episode in the core's binary form, as plain bytes. */
  binary(): number[] | null
  /** Import an episode from either form, through the core's own decoders. */
  load(data: number[] | string): void
  /** Simulated time of a stored frame, or `null` when not replaying. */
  frameTime(index: number): number | null
  /** A copy of a stored frame's F3 state, or `null` when not replaying. */
  frameState(index: number): number[] | null
  /** Overwrite one scalar of a stored frame, in place. */
  patchFrame(index: number, field: number, value: number): void
  /** Frames in the episode being replayed, or 0. */
  frameCount(): number
  /** The canonical identity of the episode held, as JSON (task 10.2). */
  identityJson(): string | null
  /** The core's verdict on comparing the held episode with itself. */
  comparabilityJson(): string | null
  /** The `Diagnostics` keys the current inspection view cannot supply. */
  unavailable(): string[]
  /** `live` or `replay`, and the sample the diagnostics came from. */
  inspection(): { source: string; sampleIndex: number | null; sampleT: number | null }
}

declare global {
  interface Window {
    __sailgym?: ReplayProbe
  }
}
