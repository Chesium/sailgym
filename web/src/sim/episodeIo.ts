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
 * schema_version 7 is not supported; this build reads 1" — rather than with a
 * second version check written on this side that could drift.
 *
 * `download` is separated from the blob builders so tests can exercise the
 * whole encode path without a file dialog, which is what `replay.spec.ts`
 * does.
 */

import type { SimHandle } from './loadWasm'
import type { Episode } from './scenarioTypes'

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
