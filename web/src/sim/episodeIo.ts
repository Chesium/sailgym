/**
 * Getting an episode out of the page and back in again (task 9.5).
 *
 * Two shipped serialisations, both the core's (brief §33): JSON for
 * inspection and a typed-array binary form for size. **Neither codec is
 * implemented here** — `Sim.episode_to_binary`, `Sim.episode_from_binary` and
 * `Sim.episode_from_json` are thin marshalling calls into
 * `sailgym_physics::recording`, so the browser and the native headless
 * simulator read the same bytes by construction (F8).
 *
 * The schema-version check is the core's too. An episode from another
 * `schema_version` therefore fails with the core's own message — "episode
 * schema_version 7 is not supported; this build reads 1 and 2" — rather than
 * with a second version check written on this side that could drift. This
 * build reads **schema 1 and schema 2** and re-encodes each in its own schema;
 * `docs/v2/recording-format.md` is the contract.
 *
 * The same rule covers the two v2 section 10 additions below. Whether two
 * episodes describe the same conditions is decided by
 * `recording::ExperimentIdentity::compare` in Rust and only *displayed* here:
 * a comparison rule written in TypeScript is how a changed equation comes to
 * be called the same experiment (RV58).
 *
 * `download` is separated from the blob builders so tests can exercise the
 * whole encode path without a file dialog, which is what `replay.spec.ts`
 * does.
 */

import type { SimHandle } from './loadWasm'
import type {
  ActionIdentity,
  Episode,
  ObservationIdentity,
  Recorded,
  TaskIdentity,
} from './scenarioTypes'
import { recordedValue } from './scenarioTypes'

/** The MIME types the two forms are offered as. */
export const JSON_MEDIA_TYPE = 'application/json'
export const BINARY_MEDIA_TYPE = 'application/octet-stream'

/** `sailgym-<scenario>-<created>.<ext>`, with nothing a file system dislikes. */
export function episodeFilename(episode: Episode, extension: 'json' | 'bin'): string {
  const scenario = episode.header.scenario.name.replace(/[^a-zA-Z0-9_-]+/g, '-')
  const stamp = episode.header.created_utc.replace(/[:.]/g, '-').replace(/Z$/, '')
  return `sailgym-${scenario}-${stamp || 'episode'}.${extension}`
}

/** Turn a `stop_recording()` document into a downloadable JSON blob. */
export function jsonBlob(episodeJson: string): Blob {
  return new Blob([episodeJson], { type: JSON_MEDIA_TYPE })
}

/**
 * Turn a `stop_recording()` document into the binary form.
 *
 * The encoding happens in Rust; this only wraps the returned bytes.
 */
export function binaryBlob(sim: SimHandle, episodeJson: string): Blob {
  const bytes = sim.episode_to_binary(episodeJson)
  // Copy into a plain `ArrayBuffer`: the view `wasm-bindgen` returns is
  // already a copy out of linear memory, but `Blob` keeps a reference and a
  // `SharedArrayBuffer`-backed view would be refused in some browsers.
  return new Blob([new Uint8Array(bytes)], { type: BINARY_MEDIA_TYPE })
}

/** Hand a blob to the browser's download path. Separated so tests can skip it. */
export function download(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.style.display = 'none'
  document.body.appendChild(anchor)
  anchor.click()
  anchor.remove()
  // The object URL holds the blob alive until it is revoked, and the click
  // has already been queued by the time this runs.
  URL.revokeObjectURL(url)
}

/** The four bytes a binary episode opens with (`recording::MAGIC`). */
const MAGIC = 'SGEP'

function looksBinary(bytes: Uint8Array): boolean {
  if (bytes.length < MAGIC.length) {
    return false
  }
  for (let i = 0; i < MAGIC.length; i += 1) {
    if (bytes[i] !== MAGIC.charCodeAt(i)) {
      return false
    }
  }
  return true
}

/**
 * Parse an episode file of either form, through the core.
 *
 * `data` is whatever a `<input type="file">` or a test hands over: the file's
 * bytes. The form is decided by the magic, so the user does not have to say
 * which they picked, and a mistyped extension is not an error.
 *
 * Throws an `Error` carrying the core's message. A wrong `schema_version` is
 * the case that matters: it must be a clear message, not a crash.
 */
export function importEpisode(sim: SimHandle, data: ArrayBuffer | Uint8Array | string): Episode {
  let json: string
  try {
    if (typeof data === 'string') {
      json = sim.episode_from_json(data) as string
    } else {
      const bytes = data instanceof Uint8Array ? data : new Uint8Array(data)
      json = looksBinary(bytes)
        ? (sim.episode_from_binary(bytes) as string)
        : (sim.episode_from_json(new TextDecoder().decode(bytes)) as string)
    }
  } catch (cause: unknown) {
    const message = typeof cause === 'string' ? cause : String(cause)
    throw new Error(`could not load that episode — ${message}`)
  }
  return JSON.parse(json) as Episode
}

// ---------------------------------------------------------------------------
// Identity and comparability (v2 F18.3, task 10.2)
// ---------------------------------------------------------------------------

/**
 * `recording::ExperimentIdentity`, as `Sim.episode_identity_json` returns it.
 *
 * Declared here rather than in `scenarioTypes.ts` because it is **not** a
 * document field list: the core *derives* it from an episode header, so there
 * is no Rust struct in the recording document for the parity test to compare
 * it against. The records it is built from — {@link TaskIdentity},
 * {@link ActionIdentity}, {@link ObservationIdentity} — are mirrored there and
 * are parity-checked.
 *
 * `parameters`, `initial_state` and `initial_controls` are left opaque for the
 * same reason `EpisodeHeader.parameters` is: the browser displays the F7
 * catalogue by walking it, never by naming its fields (F7, F8).
 */
export interface ExperimentIdentity {
  identity_version: number
  model: Recorded<{ model_version: number; source: { tree: string; state: string } }>
  parameters: Recorded<Record<string, unknown>>
  integrator: Recorded<string>
  dt: Recorded<number>
  initial_state: Recorded<Record<string, number>>
  initial_controls: Recorded<Record<string, unknown>>
  scenario: Recorded<string>
  wind: Recorded<Record<string, unknown>>
  seed: Recorded<number>
  task: Recorded<TaskIdentity>
  action: Recorded<ActionIdentity>
  observation: Recorded<ObservationIdentity>
}

/** `Comparability`, as `Sim.episode_comparability_json` shapes it. */
export interface ComparabilityVerdict {
  verdict: 'same_conditions' | 'different' | 'indeterminate'
  /** The identity fields behind a negative verdict, in the core's order. */
  reasons: string[]
  /** The core's one-line form, for a badge or a log. */
  describe: string
}

/** The canonical identity of an episode, straight from the core. */
export function readEpisodeIdentity(sim: SimHandle, episode: Episode): ExperimentIdentity {
  return JSON.parse(
    sim.episode_identity_json(JSON.stringify(episode)) as string,
  ) as ExperimentIdentity
}

/** Whether two episodes may be compared, decided by the core. */
export function compareEpisodes(
  sim: SimHandle,
  a: Episode,
  b: Episode,
): ComparabilityVerdict {
  return JSON.parse(
    sim.episode_comparability_json(JSON.stringify(a), JSON.stringify(b)) as string,
  ) as ComparabilityVerdict
}

/**
 * Whether this episode could ever be one half of a same-conditions
 * comparison — before a second episode exists to compare it with.
 *
 * It is a **display rule over recorded data**, not a second comparison: an
 * episode whose identity record is missing, or whose physics source was dirty
 * or unknown when it was made, names no baseline and must never be labelled a
 * same-conditions experiment (v2 F18.1d, F18.3). Whether two *particular*
 * episodes agree is {@link compareEpisodes}'s question and the core's answer.
 */
export function identityNamesABaseline(identity: ExperimentIdentity): boolean {
  if (identity.identity_version === 0) {
    return false
  }
  const model = recordedValue(identity.model)
  return model !== null && model.source.state === 'clean'
}

/** One line for a badge: what this episode's identity does and does not say. */
export function describeIdentity(identity: ExperimentIdentity): string {
  const model = recordedValue(identity.model)
  if (identity.identity_version === 0) {
    return 'identity unknown — a schema-1 episode records no model identity'
  }
  if (model === null) {
    return 'identity unknown — this episode records no model identity'
  }
  if (model.source.state !== 'clean') {
    return `model v${model.model_version}, physics source ${model.source.state} — names no baseline`
  }
  return `model v${model.model_version}, physics source ${model.source.tree.slice(0, 12)}`
}

// ---------------------------------------------------------------------------
// The recording bound (task 10.2)
// ---------------------------------------------------------------------------

/** The cap and the byte budget, both read from the core. */
export interface RecordingLimit {
  /** Frames a recording will accept in total. */
  frames: number
  /** Bytes one sample occupies in the binary form. */
  bytesPerFrame: number
  /** `frames × bytesPerFrame`. */
  bytes: number
}

/**
 * The recording bound, from the core.
 *
 * Read rather than restated: the cap is derived in Rust from a stated byte
 * budget (`docs/v2/recording-format.md` §8) and a literal here would be one
 * more number to keep in step (F7, F8, RV56).
 */
export function readRecordingLimit(sim: SimHandle): RecordingLimit {
  const frames = sim.recording_capacity()
  const bytesPerFrame = sim.recording_bytes_per_frame()
  return { frames, bytesPerFrame, bytes: frames * bytesPerFrame }
}

/** Simulated seconds a full-length recording covers at `logHz`. */
export function recordingSeconds(limit: RecordingLimit, logHz: number): number {
  return logHz > 0 ? limit.frames / logHz : 0
}

/** `7.5 MB`, for a readout. Binary megabytes, as the budget is stated. */
export function megabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
